use crate::image::{clamp01, Image};
use crate::palette::Palette;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Diffusion {
    FloydSteinberg,
    JarvisJudiceNinke,
    Stucki,
    Atkinson,
    Burkes,
    Sierra,
    SierraTwoRow,
    SierraLite,
    ShiauFan,
    ShiauFan2,
}

type Taps = &'static [(i32, i32, f32)];

impl Diffusion {
    pub const ALL: [Diffusion; 10] = [
        Diffusion::FloydSteinberg,
        Diffusion::JarvisJudiceNinke,
        Diffusion::Stucki,
        Diffusion::Atkinson,
        Diffusion::Burkes,
        Diffusion::Sierra,
        Diffusion::SierraTwoRow,
        Diffusion::SierraLite,
        Diffusion::ShiauFan,
        Diffusion::ShiauFan2,
    ];

    /// Returns (divisor, taps as (dx, dy, weight)).
    pub fn kernel(self) -> (f32, Taps) {
        match self {
            Diffusion::FloydSteinberg => (16.0, &[(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)]),
            Diffusion::JarvisJudiceNinke => (
                48.0,
                &[
                    (1, 0, 7.0), (2, 0, 5.0),
                    (-2, 1, 3.0), (-1, 1, 5.0), (0, 1, 7.0), (1, 1, 5.0), (2, 1, 3.0),
                    (-2, 2, 1.0), (-1, 2, 3.0), (0, 2, 5.0), (1, 2, 3.0), (2, 2, 1.0),
                ],
            ),
            Diffusion::Stucki => (
                42.0,
                &[
                    (1, 0, 8.0), (2, 0, 4.0),
                    (-2, 1, 2.0), (-1, 1, 4.0), (0, 1, 8.0), (1, 1, 4.0), (2, 1, 2.0),
                    (-2, 2, 1.0), (-1, 2, 2.0), (0, 2, 4.0), (1, 2, 2.0), (2, 2, 1.0),
                ],
            ),
            // Atkinson intentionally propagates only 6/8 of the error.
            Diffusion::Atkinson => (
                8.0,
                &[(1, 0, 1.0), (2, 0, 1.0), (-1, 1, 1.0), (0, 1, 1.0), (1, 1, 1.0), (0, 2, 1.0)],
            ),
            Diffusion::Burkes => (
                32.0,
                &[
                    (1, 0, 8.0), (2, 0, 4.0),
                    (-2, 1, 2.0), (-1, 1, 4.0), (0, 1, 8.0), (1, 1, 4.0), (2, 1, 2.0),
                ],
            ),
            Diffusion::Sierra => (
                32.0,
                &[
                    (1, 0, 5.0), (2, 0, 3.0),
                    (-2, 1, 2.0), (-1, 1, 4.0), (0, 1, 5.0), (1, 1, 4.0), (2, 1, 2.0),
                    (-1, 2, 2.0), (0, 2, 3.0), (1, 2, 2.0),
                ],
            ),
            Diffusion::SierraTwoRow => (
                16.0,
                &[
                    (1, 0, 4.0), (2, 0, 3.0),
                    (-2, 1, 1.0), (-1, 1, 2.0), (0, 1, 3.0), (1, 1, 2.0), (2, 1, 1.0),
                ],
            ),
            Diffusion::SierraLite => (4.0, &[(1, 0, 2.0), (-1, 1, 1.0), (0, 1, 1.0)]),
            Diffusion::ShiauFan => (8.0, &[(1, 0, 4.0), (-2, 1, 1.0), (-1, 1, 1.0), (0, 1, 2.0)]),
            Diffusion::ShiauFan2 => (16.0, &[(1, 0, 8.0), (-3, 1, 1.0), (-2, 1, 1.0), (-1, 1, 2.0), (0, 1, 4.0)]),
        }
    }
}

fn one() -> f32 { 1.0 }
fn yes() -> bool { true }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "algo")]
pub enum Method {
    Diffusion {
        kernel: Diffusion,
        #[serde(default = "yes")]
        serpentine: bool,
        #[serde(default = "one")]
        strength: f32,
    },
    Bayer { size: u32, #[serde(default)] spread: Option<f32> },
    Halftone { cell: f32, angle: f32, #[serde(default)] spread: Option<f32> },
    Lines { period: f32, angle: f32, #[serde(default)] spread: Option<f32> },
    WhiteNoise { seed: u32, #[serde(default)] spread: Option<f32> },
    Ign { #[serde(default)] spread: Option<f32> },
    BlueNoise { seed: u32, #[serde(default)] spread: Option<f32> },
}

pub fn apply(method: &Method, img: &Image, pal: &Palette) -> Image {
    let gray;
    let img = if pal.luma_only {
        gray = Image { w: img.w, h: img.h, px: img.px.iter().map(|p| [crate::image::luma(*p); 3]).collect() };
        &gray
    } else {
        img
    };
    match method {
        Method::Diffusion { kernel, serpentine, strength } => diffuse(img, pal, *kernel, *serpentine, *strength),
        Method::Bayer { size, spread } => {
            let n = match size { 2 | 4 | 8 | 16 => *size as usize, _ => 4 };
            let m = bayer_matrix(n);
            let norm = (n * n) as f32;
            threshold(img, pal, auto_spread(*spread, pal), |x, y| (m[(y % n) * n + (x % n)] as f32 + 0.5) / norm)
        }
        Method::Halftone { cell, angle, spread } => {
            let (s, c) = angle.to_radians().sin_cos();
            let cell = cell.max(2.0);
            threshold(img, pal, auto_spread(*spread, pal), move |x, y| {
                let (xf, yf) = (x as f32, y as f32);
                let u = fract((xf * c + yf * s) / cell) - 0.5;
                let v = fract((-xf * s + yf * c) / cell) - 0.5;
                clamp01((u * u + v * v).sqrt() / 0.707_106_8)
            })
        }
        Method::Lines { period, angle, spread } => {
            let (s, c) = angle.to_radians().sin_cos();
            let period = period.max(2.0);
            threshold(img, pal, auto_spread(*spread, pal), move |x, y| {
                let p = (-(x as f32) * s + (y as f32) * c) / period;
                (fract(p) - 0.5).abs() * 2.0
            })
        }
        Method::WhiteNoise { seed, spread } => {
            let seed = *seed;
            threshold(img, pal, auto_spread(*spread, pal), move |x, y| crate::effects::rand01(x as u32, y as u32, seed))
        }
        Method::BlueNoise { seed, spread } => {
            let tex = crate::bluenoise::texture(*seed);
            let n = crate::bluenoise::SIZE;
            threshold(img, pal, auto_spread(*spread, pal), move |x, y| tex[(y % n) * n + (x % n)])
        }
        Method::Ign { spread } => threshold(img, pal, auto_spread(*spread, pal), |x, y| {
            fract(52.982_918 * fract(0.067_110_56 * x as f32 + 0.005_837_15 * y as f32))
        }),
    }
}

fn fract(v: f32) -> f32 { v - v.floor() }

fn auto_spread(user: Option<f32>, pal: &Palette) -> f32 {
    if let Some(s) = user { return s; }
    let n = pal.colors.len() as f32;
    if n <= 2.0 { 1.0 } else if n <= 8.0 { 1.0 / (n - 1.0) } else { 1.0 / n.cbrt() }
}

pub fn bayer_matrix(n: usize) -> Vec<u32> {
    let mut m = vec![0u32];
    let mut size = 1;
    while size < n {
        let ns = size * 2;
        let mut nm = vec![0u32; ns * ns];
        for y in 0..size {
            for x in 0..size {
                let v = m[y * size + x] * 4;
                nm[y * ns + x] = v;
                nm[y * ns + x + size] = v + 2;
                nm[(y + size) * ns + x] = v + 3;
                nm[(y + size) * ns + x + size] = v + 1;
            }
        }
        m = nm;
        size = ns;
    }
    m
}

fn threshold<F: Fn(usize, usize) -> f32 + Sync>(img: &Image, pal: &Palette, spread: f32, t: F) -> Image {
    let mut out = img.clone();
    let (w, h) = (img.w, img.h);
    if w == 0 || h == 0 { return out; }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(h).max(1);
    let rows = (h + threads - 1) / threads;
    std::thread::scope(|s| {
        for (ci, chunk) in out.px.chunks_mut(rows * w).enumerate() {
            let t = &t;
            s.spawn(move || {
                for (k, px) in chunk.iter_mut().enumerate() {
                    let i = ci * rows * w + k;
                    let (x, y) = (i % w, i / w);
                    let o = (t(x, y) - 0.5) * spread;
                    let p = img.px[i];
                    *px = pal.colors[pal.nearest([p[0] + o, p[1] + o, p[2] + o])];
                }
            });
        }
    });
    out
}

fn diffuse(img: &Image, pal: &Palette, kernel: Diffusion, serpentine: bool, strength: f32) -> Image {
    let (w, h) = (img.w as isize, img.h as isize);
    let (div, taps) = kernel.kernel();
    let mut buf = img.px.clone();
    let mut out = img.clone();
    for y in 0..h {
        let rev = serpentine && y % 2 == 1;
        for k in 0..w {
            let x = if rev { w - 1 - k } else { k };
            let i = (y * w + x) as usize;
            let old = [clamp01(buf[i][0]), clamp01(buf[i][1]), clamp01(buf[i][2])];
            let q = pal.colors[pal.nearest(old)];
            out.px[i] = q;
            let err = [(old[0] - q[0]) * strength, (old[1] - q[1]) * strength, (old[2] - q[2]) * strength];
            for &(dx, dy, wt) in taps {
                let dx = if rev { -dx } else { dx } as isize;
                let (nx, ny) = (x + dx, y + dy as isize);
                if nx < 0 || nx >= w || ny >= h { continue; }
                let j = (ny * w + nx) as usize;
                let f = wt / div;
                for c in 0..3 { buf[j][c] += err[c] * f; }
            }
        }
    }
    out
}
