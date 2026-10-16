use crate::dither::{self, Method};
use crate::effects;
use crate::image::Image;
use crate::palette::PaletteSpec;
use serde::{Deserialize, Serialize};

fn yes() -> bool { true }
fn one() -> f32 { 1.0 }
fn normal() -> BlendMode { BlendMode::Normal }
fn no_key() -> KeyMode { KeyMode::None }
fn half() -> f32 { 0.5 }
fn soft() -> f32 { 0.1 }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum BlendMode { Normal, Multiply, Screen, Overlay, Add }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum KeyMode { None, DropLight, DropDark }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Composite {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "normal")]
    pub mode: BlendMode,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "no_key")]
    pub key: KeyMode,
    #[serde(default = "half")]
    pub key_threshold: f32,
    #[serde(default = "soft")]
    pub key_softness: f32,
}

impl Default for Composite {
    fn default() -> Self {
        Self { enabled: false, mode: BlendMode::Normal, opacity: 1.0, key: KeyMode::None, key_threshold: 0.5, key_softness: 0.1 }
    }
}

pub fn composite_over(base: &Image, top: &Image, c: &Composite) -> Image {
    let top = if top.w != base.w || top.h != base.h { effects::resize_nearest(top, base.w, base.h) } else { top.clone() };
    let mut out = base.clone();
    for (o, t) in out.px.iter_mut().zip(top.px.iter()) {
        let l = crate::image::luma(*t);
        let key_alpha = match c.key {
            KeyMode::None => 1.0,
            k => {
                let soft = c.key_softness.max(0.0001);
                let x = ((l - (c.key_threshold - soft)) / (2.0 * soft)).max(0.0).min(1.0);
                let sm = x * x * (3.0 - 2.0 * x);
                if k == KeyMode::DropLight { 1.0 - sm } else { sm }
            }
        };
        let a = key_alpha * c.opacity.max(0.0).min(1.0);
        let base_px = *o;
        for ch in 0..3 {
            let (b, v) = (base_px[ch], t[ch]);
            let mixed = match c.mode {
                BlendMode::Normal => v,
                BlendMode::Multiply => b * v,
                BlendMode::Screen => 1.0 - (1.0 - b) * (1.0 - v),
                BlendMode::Overlay => if b < 0.5 { 2.0 * b * v } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - v) },
                BlendMode::Add => (b + v).min(1.0),
            };
            o[ch] = b + (mixed - b) * a;
        }
    }
    out
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Step {
    Adjust { brightness: f32, contrast: f32, gamma: f32, saturation: f32 },
    Blur { radius: u32 },
    Downscale { factor: u32 },
    Upscale { factor: u32 },
    Dither { method: Method, palette: PaletteSpec },
    ChromaticAberration { shift: i32 },
    Glow { threshold: f32, radius: u32, intensity: f32 },
    Scanlines { period: u32, strength: f32 },
    Invert,
    Noise { amount: f32, seed: u32 },
    RowShift { max_shift: i32, chance: f32, seed: u32 },
    JpegGlitch { quality: u32, corrupt: f32, seed: u32 },
    Posterize { levels: u32 },
    GradientMap { colors: Vec<String> },
    Sharpen { amount: f32, radius: u32 },
}

impl Step {
    pub fn with_frame(&self, frame: u32) -> Step {
        let mut s = self.clone();
        let shift = frame.wrapping_mul(7919);
        match &mut s {
            Step::Noise { seed, .. } | Step::RowShift { seed, .. } | Step::JpegGlitch { seed, .. } => *seed = seed.wrapping_add(shift),
            Step::Dither { method: Method::WhiteNoise { seed, .. }, .. } => *seed = seed.wrapping_add(shift),
            _ => {}
        }
        s
    }

    pub fn apply(&self, img: &Image) -> Image {
        match self {
            Step::Adjust { brightness, contrast, gamma, saturation } => effects::adjust(img, *brightness, *contrast, *gamma, *saturation),
            Step::Blur { radius } => effects::box_blur(img, *radius),
            Step::Downscale { factor } => effects::downscale(img, *factor),
            Step::Upscale { factor } => effects::upscale(img, *factor),
            Step::Dither { method, palette } => dither::apply(method, img, &palette.resolve(img)),
            Step::ChromaticAberration { shift } => effects::chromatic_aberration(img, *shift),
            Step::Glow { threshold, radius, intensity } => effects::glow(img, *threshold, *radius, *intensity),
            Step::Scanlines { period, strength } => effects::scanlines(img, *period, *strength),
            Step::Invert => effects::invert(img),
            Step::Noise { amount, seed } => effects::noise(img, *amount, *seed),
            Step::RowShift { max_shift, chance, seed } => effects::row_shift(img, *max_shift, *chance, *seed),
            Step::JpegGlitch { quality, corrupt, seed } => effects::jpeg_glitch(img, *quality, *corrupt, *seed),
            Step::Posterize { levels } => effects::posterize(img, *levels),
            Step::GradientMap { colors } => {
                let stops: Vec<[f32; 3]> = colors.iter().filter_map(|h| crate::palette::parse_hex(h)).collect();
                effects::gradient_map(img, &stops)
            }
            Step::Sharpen { amount, radius } => effects::sharpen(img, *amount, *radius),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Layer {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(flatten)]
    pub step: Step,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Pipeline {
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub animate_noise: bool,
    #[serde(default)]
    pub composite: Composite,
}

impl Pipeline {
    pub fn new(steps: Vec<Step>) -> Self {
        Self { layers: steps.into_iter().map(|step| Layer { enabled: true, opacity: 1.0, step }).collect(), animate_noise: false, composite: Composite::default() }
    }

    pub fn run(&self, src: &Image) -> Image {
        self.run_frame(src, 0)
    }

    pub fn run_frame(&self, src: &Image, frame: u32) -> Image {
        let mut img = src.clone();
        for l in self.layers.iter().filter(|l| l.enabled) {
            let out = if self.animate_noise { l.step.with_frame(frame).apply(&img) } else { l.step.apply(&img) };
            img = if l.opacity < 0.999 && out.w == img.w && out.h == img.h { effects::blend(&img, &out, l.opacity.max(0.0)) } else { out };
        }
        if self.composite.enabled {
            img = composite_over(src, &img, &self.composite);
        }
        img
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }
}
