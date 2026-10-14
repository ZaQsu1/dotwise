//! Dotwise core: image dithering engine with a stackable effects pipeline.
pub mod bluenoise;
pub mod dither;
pub mod effects;
pub mod image;
pub mod palette;
pub mod pipeline;
pub mod presets;
pub mod svg;

pub use image::Image;
pub use palette::{Palette, PaletteSpec};
pub use pipeline::{BlendMode, Composite, KeyMode, Layer, Pipeline, Step};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dither::{bayer_matrix, Diffusion, Method};

    fn gradient(w: usize, h: usize) -> Image {
        let mut img = Image::new(w, h);
        for y in 0..h { for x in 0..w { img.px[y * w + x] = [x as f32 / (w - 1) as f32; 3]; } }
        img
    }

    #[test]
    fn mono_output_is_binary_for_all_methods() {
        let img = gradient(64, 32);
        let methods = vec![
            Method::Diffusion { kernel: Diffusion::Atkinson, serpentine: true, strength: 1.0 },
            Method::Bayer { size: 8, spread: None },
            Method::Halftone { cell: 6.0, angle: 30.0, spread: None },
            Method::Lines { period: 4.0, angle: 0.0, spread: None },
            Method::WhiteNoise { seed: 1, spread: None },
            Method::Ign { spread: None },
        ];
        for m in methods {
            let out = Pipeline::new(vec![Step::Dither { method: m, palette: PaletteSpec::Mono }]).run(&img);
            assert!(out.px.iter().all(|p| p[0] == 0.0 || p[0] == 1.0));
        }
    }

    #[test]
    fn mid_gray_gives_about_half_white() {
        let mut img = Image::new(128, 128);
        for p in img.px.iter_mut() { *p = [0.5; 3]; }
        for k in Diffusion::ALL {
            let out = Pipeline::new(vec![Step::Dither {
                method: Method::Diffusion { kernel: k, serpentine: true, strength: 1.0 },
                palette: PaletteSpec::Mono,
            }]).run(&img);
            let white = out.px.iter().filter(|p| p[0] > 0.5).count() as f32 / out.px.len() as f32;
            assert!((white - 0.5).abs() < 0.05, "{:?} gave {}", k, white);
        }
    }

    #[test]
    fn kernels_sum_to_divisor_except_atkinson() {
        for k in Diffusion::ALL {
            let (div, taps) = k.kernel();
            let s: f32 = taps.iter().map(|t| t.2).sum();
            if k == Diffusion::Atkinson { assert_eq!(s, 6.0); } else { assert_eq!(s, div, "{:?}", k); }
        }
    }

    #[test]
    fn colour_input_with_mono_palette_follows_luma() {
        let mut img = Image::new(96, 96);
        for p in img.px.iter_mut() { *p = [1.0, 0.0, 0.0]; }
        for k in Diffusion::ALL {
            let out = Pipeline::new(vec![Step::Dither {
                method: Method::Diffusion { kernel: k, serpentine: true, strength: 1.0 },
                palette: PaletteSpec::Mono,
            }]).run(&img);
            let white = out.px.iter().filter(|p| p[0] > 0.5).count() as f32 / out.px.len() as f32;
            if k == Diffusion::Atkinson {
                // Atkinson drops a quarter of the error on purpose, so shadows get crushed.
                assert!(white > 0.08 && white < 0.2126, "{:?} gave {}", k, white);
            } else {
                assert!((white - 0.2126).abs() < 0.06, "{:?} gave {}", k, white);
            }
        }
    }

    #[test]
    fn animate_noise_changes_frames_only_when_enabled() {
        let img = gradient(32, 32);
        let mut p = Pipeline::new(vec![Step::Noise { amount: 0.5, seed: 3 }]);
        assert_eq!(p.run_frame(&img, 0).px, p.run_frame(&img, 5).px);
        p.animate_noise = true;
        assert_ne!(p.run_frame(&img, 0).px, p.run_frame(&img, 5).px);
        assert_eq!(p.run_frame(&img, 5).px, p.run_frame(&img, 5).px);
    }

    #[test]
    fn svg_merges_runs_and_rejects_photos() {
        let mut img = Image::new(4, 4);
        for p in img.px.iter_mut() { *p = [1.0; 3]; }
        for y in 0..2 { for x in 0..3 { img.px[y * 4 + x] = [0.0; 3]; } }
        let svg = svg::to_svg(&img, 1).unwrap();
        assert!(svg.contains("viewBox=\"0 0 4 4\""));
        assert!(svg.contains("M0 0h3v2h-3z"), "{}", svg);
        let mut photo = Image::new(64, 64);
        for (i, p) in photo.px.iter_mut().enumerate() { *p = [(i % 256) as f32 / 255.0, (i / 7 % 256) as f32 / 255.0, 0.3]; }
        assert!(svg::to_svg(&photo, 1).is_err());
    }

    #[test]
    fn blue_noise_is_a_permutation_and_evenly_spread() {
        let n = 32;
        let t = bluenoise::generate(n, 4);
        let mut ranks: Vec<usize> = t.iter().map(|v| (v * (n * n) as f32) as usize).collect();
        ranks.sort();
        assert_eq!(ranks, (0..n * n).collect::<Vec<_>>());
        // the 10 percent lowest cells should be far apart, clearly more than random placement
        let pts: Vec<(i32, i32)> = (0..n * n).filter(|i| t[*i] < 0.1).map(|i| ((i % n) as i32, (i / n) as i32)).collect();
        let mut sum = 0.0;
        for (i, a) in pts.iter().enumerate() {
            let mut best = f32::MAX;
            for (j, b) in pts.iter().enumerate() {
                if i == j { continue; }
                let dx = (a.0 - b.0).abs().min(n as i32 - (a.0 - b.0).abs()) as f32;
                let dy = (a.1 - b.1).abs().min(n as i32 - (a.1 - b.1).abs()) as f32;
                best = best.min((dx * dx + dy * dy).sqrt());
            }
            sum += best;
        }
        let mean = sum / pts.len() as f32;
        assert!(mean > 2.2, "mean nearest neighbour {}", mean);
    }

    #[test]
    fn jpeg_glitch_keeps_size_and_opacity_blends() {
        let img = gradient(37, 21);
        let g = effects::jpeg_glitch(&img, 90, 0.0, 1);
        assert_eq!((g.w, g.h), (37, 21));
        let err: f32 = g.px.iter().zip(img.px.iter()).map(|(a, b)| (a[0] - b[0]).abs()).sum::<f32>() / g.px.len() as f32;
        assert!(err < 0.05, "high quality should stay close, got {}", err);
        let mut p = Pipeline::new(vec![Step::Invert]);
        p.layers[0].opacity = 0.5;
        let out = p.run(&img);
        assert!(out.px.iter().all(|px| (px[0] - 0.5).abs() < 1e-5));
    }

    #[test]
    fn overlay_composites_over_the_original() {
        let mut base = Image::new(8, 8);
        for p in base.px.iter_mut() { *p = [0.2; 3]; }
        // layers produce a white image; overlay at 50 percent must land halfway between
        let mut p = Pipeline::new(vec![Step::Adjust { brightness: 1.0, contrast: 1.0, gamma: 1.0, saturation: 1.0 }]);
        p.composite = Composite { enabled: true, opacity: 0.5, ..Composite::default() };
        assert!(p.run(&base).px.iter().all(|px| (px[0] - 0.6).abs() < 1e-5));
        // keying out light pixels leaves the original untouched
        p.composite = Composite { enabled: true, key: KeyMode::DropLight, key_threshold: 0.5, ..Composite::default() };
        assert!(p.run(&base).px.iter().all(|px| (px[0] - 0.2).abs() < 1e-5));
        // multiply of 0.2 with a 0.5 gray
        let mut q = Pipeline::new(vec![Step::Adjust { brightness: 0.0, contrast: 0.0, gamma: 1.0, saturation: 1.0 }]);
        q.composite = Composite { enabled: true, mode: BlendMode::Multiply, ..Composite::default() };
        assert!(q.run(&base).px.iter().all(|px| (px[0] - 0.1).abs() < 1e-4), "{:?}", q.run(&base).px[0]);
        // disabled overlay and old JSON without the field keep the plain behaviour
        let old: Pipeline = serde_json::from_str("{\"layers\":[{\"type\":\"Invert\"}]}").unwrap();
        assert!(!old.composite.enabled);
    }

    #[test]
    fn overlay_scales_results_of_a_different_size() {
        let img = gradient(10, 10);
        let mut p = Pipeline::new(vec![Step::Downscale { factor: 2 }]);
        p.composite.enabled = true;
        let out = p.run(&img);
        assert_eq!((out.w, out.h), (10, 10));
    }

    #[test]
    fn new_layers_behave() {
        let img = gradient(16, 4);
        let post = effects::posterize(&img, 2);
        assert!(post.px.iter().all(|p| p[0] == 0.0 || p[0] == 1.0));
        let g = effects::gradient_map(&img, &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);
        assert!(g.px.iter().all(|p| p[1] == 0.0 && p[2] == 0.0));
        assert!((g.px[8][0] - img.px[8][0]).abs() < 0.02);
        let sh = effects::sharpen(&img, 1.0, 2);
        assert_eq!((sh.w, sh.h), (16, 4));
        let steps = Pipeline::new(vec![
            Step::Posterize { levels: 4 },
            Step::GradientMap { colors: vec!["#000000".into(), "#ff8800".into(), "#ffffff".into()] },
            Step::Sharpen { amount: 0.5, radius: 1 },
        ]);
        assert_eq!(Pipeline::from_json(&steps.to_json()).unwrap(), steps);
    }

    #[test]
    fn bayer_is_a_permutation() {
        for n in [2usize, 4, 8, 16] {
            let mut m = bayer_matrix(n);
            m.sort();
            assert_eq!(m, (0..(n * n) as u32).collect::<Vec<_>>());
        }
    }

    #[test]
    fn presets_roundtrip_json_and_run() {
        let img = gradient(48, 48);
        for (name, p) in presets::builtin() {
            let back = Pipeline::from_json(&p.to_json()).unwrap_or_else(|e| panic!("{}: {}", name, e));
            assert_eq!(back, p, "{}", name);
            let out = p.run(&img);
            assert!(out.w > 0 && out.h > 0);
        }
    }

    #[test]
    fn extract_returns_requested_colors_and_disabled_layers_skip() {
        let img = gradient(32, 32);
        assert_eq!(Palette::extract(&img, 8).colors.len(), 8);
        let mut p = Pipeline::new(vec![Step::Invert]);
        p.layers[0].enabled = false;
        assert_eq!(p.run(&img).px, img.px);
    }
}
