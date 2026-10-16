#[derive(Clone, Debug)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 3]>,
}

pub fn luma(p: [f32; 3]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

pub fn clamp01(v: f32) -> f32 {
    v.max(0.0).min(1.0)
}

impl Image {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![[0.0; 3]; w * h] }
    }

    pub fn from_rgb8(w: usize, h: usize, data: &[u8]) -> Self {
        assert_eq!(data.len(), w * h * 3);
        let px = data
            .chunks_exact(3)
            .map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
            .collect();
        Self { w, h, px }
    }

    pub fn to_rgb8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 3);
        for p in &self.px {
            for c in p {
                out.push((clamp01(*c) * 255.0 + 0.5) as u8);
            }
        }
        out
    }

    pub fn at_clamped(&self, x: isize, y: isize) -> [f32; 3] {
        let xx = x.max(0).min(self.w as isize - 1) as usize;
        let yy = y.max(0).min(self.h as isize - 1) as usize;
        self.px[yy * self.w + xx]
    }
}
