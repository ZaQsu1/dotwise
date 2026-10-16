use crate::dither::{Diffusion, Method};
use crate::palette::PaletteSpec;
use crate::pipeline::{BlendMode, Composite, KeyMode, Pipeline, Step};

fn adjust(contrast: f32) -> Step {
    Step::Adjust { brightness: 0.0, contrast, gamma: 1.0, saturation: 1.0 }
}

pub fn builtin() -> Vec<(&'static str, Pipeline)> {
    let diff = |k| Method::Diffusion { kernel: k, serpentine: true, strength: 1.0 };
    vec![
        ("Classic mono", Pipeline::new(vec![adjust(1.1), Step::Dither { method: diff(Diffusion::FloydSteinberg), palette: PaletteSpec::Mono }])),
        ("Compact Mac", Pipeline::new(vec![
            adjust(1.15), Step::Downscale { factor: 2 },
            Step::Dither { method: diff(Diffusion::Atkinson), palette: PaletteSpec::Mono },
            Step::Upscale { factor: 2 },
        ])),
        ("Handheld green", Pipeline::new(vec![
            Step::Downscale { factor: 3 },
            Step::Dither { method: Method::Bayer { size: 4, spread: None }, palette: PaletteSpec::Gameboy },
            Step::Upscale { factor: 3 },
        ])),
        ("Newsprint", Pipeline::new(vec![
            adjust(1.2),
            Step::Dither { method: Method::Halftone { cell: 6.0, angle: 45.0, spread: None }, palette: PaletteSpec::Mono },
        ])),
        ("Engraving lines", Pipeline::new(vec![
            Step::Dither { method: Method::Lines { period: 5.0, angle: 20.0, spread: None }, palette: PaletteSpec::Mono },
        ])),
        ("Extracted 8", Pipeline::new(vec![
            Step::Dither { method: diff(Diffusion::Sierra), palette: PaletteSpec::Extract { count: 8 } },
        ])),
        ("Halftone overlay", {
            let mut p = Pipeline::new(vec![
                adjust(1.2),
                Step::Dither { method: Method::Halftone { cell: 5.0, angle: 45.0, spread: None }, palette: PaletteSpec::Mono },
            ]);
            p.composite = Composite { enabled: true, mode: BlendMode::Multiply, opacity: 0.85, key: KeyMode::DropLight, key_threshold: 0.5, key_softness: 0.1 };
            p
        }),
        ("Duotone", Pipeline::new(vec![
            Step::Sharpen { amount: 0.6, radius: 1 },
            Step::GradientMap { colors: vec!["#14123a".into(), "#c2386b".into(), "#fbe9c6".into()] },
            Step::Dither { method: Method::Bayer { size: 8, spread: None }, palette: PaletteSpec::Extract { count: 6 } },
        ])),
        ("Blue noise", Pipeline::new(vec![
            adjust(1.1),
            Step::Dither { method: Method::BlueNoise { seed: 1, spread: None }, palette: PaletteSpec::Mono },
        ])),
        ("Corrupted scan", Pipeline::new(vec![
            Step::JpegGlitch { quality: 20, corrupt: 0.08, seed: 3 },
            Step::Dither { method: diff(Diffusion::ShiauFan2), palette: PaletteSpec::Duo { dark: "#1b1b3a".into(), light: "#f2e9c9".into() } },
        ])),
        ("Glow glitch", Pipeline::new(vec![
            Step::Downscale { factor: 2 },
            Step::Dither { method: diff(Diffusion::SierraLite), palette: PaletteSpec::Cga },
            Step::Upscale { factor: 2 },
            Step::RowShift { max_shift: 12, chance: 0.04, seed: 7 },
            Step::ChromaticAberration { shift: 3 },
            Step::Glow { threshold: 0.6, radius: 6, intensity: 0.5 },
            Step::Scanlines { period: 3, strength: 0.25 },
        ])),
        ("Pixel arcade", Pipeline::new(vec![
            Step::Downscale { factor: 3 },
            Step::Dither { method: Method::Bayer { size: 4, spread: None }, palette: PaletteSpec::Cga },
            Step::Upscale { factor: 3 },
            Step::Scanlines { period: 3, strength: 0.2 },
        ])),
        ("Terminal lime", Pipeline::new(vec![
            adjust(1.15),
            Step::Dither { method: diff(Diffusion::Atkinson), palette: PaletteSpec::Gameboy },
        ])),
        ("Risograph coral", Pipeline::new(vec![
            Step::Downscale { factor: 2 },
            Step::GradientMap { colors: vec!["#182d4a".into(), "#e55245".into(), "#f2c879".into()] },
            Step::Dither { method: Method::Halftone { cell: 7.0, angle: 35.0, spread: None }, palette: PaletteSpec::Extract { count: 5 } },
            Step::Upscale { factor: 2 },
        ])),
        ("VHS tracking", Pipeline::new(vec![
            Step::Adjust { brightness: 0.0, contrast: 1.08, gamma: 1.0, saturation: 1.15 },
            Step::Noise { amount: 0.035, seed: 9 },
            Step::ChromaticAberration { shift: 4 },
            Step::RowShift { max_shift: 14, chance: 0.035, seed: 4 },
            Step::JpegGlitch { quality: 28, corrupt: 0.025, seed: 6 },
            Step::Scanlines { period: 4, strength: 0.16 },
        ])),
        ("Blueprint", Pipeline::new(vec![
            Step::GradientMap { colors: vec!["#071b32".into(), "#23698a".into(), "#d6edea".into()] },
            Step::Dither { method: Method::Bayer { size: 4, spread: None }, palette: PaletteSpec::Extract { count: 6 } },
        ])),
        ("Newspaper color", Pipeline::new(vec![
            Step::Adjust { brightness: 0.02, contrast: 1.12, gamma: 1.0, saturation: 0.82 },
            Step::Dither { method: Method::Halftone { cell: 7.0, angle: 15.0, spread: None }, palette: PaletteSpec::Extract { count: 6 } },
        ])),
        ("Hard-light poster", Pipeline::new(vec![
            adjust(1.3),
            Step::Posterize { levels: 5 },
            Step::Dither { method: Method::Bayer { size: 4, spread: None }, palette: PaletteSpec::Cga },
        ])),
    ]
}

pub fn find(name: &str) -> Option<Pipeline> {
    builtin().into_iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, p)| p)
}
