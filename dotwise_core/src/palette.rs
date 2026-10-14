use crate::image::{clamp01, luma, Image};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub colors: Vec<[f32; 3]>,
    /// True for ramps (mono, gray, duo, gameboy): dithering then works on luma only,
    /// which keeps error diffusion stable on colour images.
    pub luma_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum PaletteSpec {
    Mono,
    Gray { levels: u32 },
    Duo { dark: String, light: String },
    Gameboy,
    Cga,
    Custom { colors: Vec<String> },
    /// Median cut palette extracted from the image reaching this step.
    Extract { count: u32 },
}

pub fn parse_hex(s: &str) -> Option<[f32; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0])
}

impl Palette {
    pub fn from_hex(list: &[&str], luma_only: bool) -> Self {
        Self { colors: list.iter().filter_map(|h| parse_hex(h)).collect(), luma_only }
    }

    pub fn nearest(&self, c: [f32; 3]) -> usize {
        let mut best = 0;
        let mut bd = f32::MAX;
        if self.luma_only {
            let l = luma(c);
            for (i, p) in self.colors.iter().enumerate() {
                let d = (l - luma(*p)).abs();
                if d < bd {
                    bd = d;
                    best = i;
                }
            }
            return best;
        }
        for (i, p) in self.colors.iter().enumerate() {
            let d0 = c[0] - p[0];
            let d1 = c[1] - p[1];
            let d2 = c[2] - p[2];
            let d = 0.299 * d0 * d0 + 0.587 * d1 * d1 + 0.114 * d2 * d2;
            if d < bd {
                bd = d;
                best = i;
            }
        }
        best
    }

    /// Median cut: split the box with the widest channel range at its median.
    pub fn extract(img: &Image, count: usize) -> Self {
        let count = count.max(2).min(256);
        let step = (img.px.len() / 65536).max(1);
        let sample: Vec<[f32; 3]> = img.px.iter().step_by(step).cloned().collect();
        let mut boxes = vec![sample];
        while boxes.len() < count {
            let mut pick = None;
            let mut widest = 0.0f32;
            for (i, b) in boxes.iter().enumerate() {
                if b.len() < 2 {
                    continue;
                }
                for c in 0..3 {
                    let lo = b.iter().map(|p| p[c]).fold(f32::MAX, f32::min);
                    let hi = b.iter().map(|p| p[c]).fold(f32::MIN, f32::max);
                    if hi - lo > widest {
                        widest = hi - lo;
                        pick = Some((i, c));
                    }
                }
            }
            let Some((i, c)) = pick else { break };
            let mut b = boxes.swap_remove(i);
            b.sort_by(|a, z| a[c].partial_cmp(&z[c]).unwrap());
            let right = b.split_off(b.len() / 2);
            boxes.push(b);
            boxes.push(right);
        }
        let mut colors: Vec<[f32; 3]> = boxes
            .iter()
            .filter(|b| !b.is_empty())
            .map(|b| {
                let n = b.len() as f32;
                let mut s = [0.0; 3];
                for p in b {
                    for c in 0..3 {
                        s[c] += p[c];
                    }
                }
                [s[0] / n, s[1] / n, s[2] / n]
            })
            .collect();
        colors.sort_by(|a, b| luma(*a).partial_cmp(&luma(*b)).unwrap());
        Self { colors, luma_only: false }
    }
}

impl PaletteSpec {
    pub fn resolve(&self, img: &Image) -> Palette {
        match self {
            PaletteSpec::Mono => Palette { colors: vec![[0.0; 3], [1.0; 3]], luma_only: true },
            PaletteSpec::Gray { levels } => {
                let n = (*levels).max(2) as usize;
                Palette { colors: (0..n).map(|i| [clamp01(i as f32 / (n - 1) as f32); 3]).collect(), luma_only: true }
            }
            PaletteSpec::Duo { dark, light } => Palette {
                colors: vec![parse_hex(dark).unwrap_or([0.0; 3]), parse_hex(light).unwrap_or([1.0; 3])],
                luma_only: true,
            },
            PaletteSpec::Gameboy => Palette::from_hex(&["#0f380f", "#306230", "#8bac0f", "#9bbc0f"], true),
            PaletteSpec::Cga => Palette::from_hex(&["#000000", "#55ffff", "#ff55ff", "#ffffff"], false),
            PaletteSpec::Custom { colors } => {
                let mut c: Vec<[f32; 3]> = colors.iter().filter_map(|h| parse_hex(h)).collect();
                if c.len() < 2 {
                    c = vec![[0.0; 3], [1.0; 3]];
                }
                let luma_only = c.len() == 2;
                Palette { colors: c, luma_only }
            }
            PaletteSpec::Extract { count } => Palette::extract(img, *count as usize),
        }
    }
}
