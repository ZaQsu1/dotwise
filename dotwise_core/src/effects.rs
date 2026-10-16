use crate::image::{clamp01, luma, Image};

fn hash(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

pub fn rand01(x: u32, y: u32, seed: u32) -> f32 {
    let h = hash(x.wrapping_mul(0x9e37_79b1) ^ hash(y.wrapping_add(seed.wrapping_mul(0x85eb_ca6b))));
    (h >> 8) as f32 / 16_777_216.0
}

pub fn adjust(img: &Image, brightness: f32, contrast: f32, gamma: f32, saturation: f32) -> Image {
    let mut out = img.clone();
    let inv_g = 1.0 / gamma.max(0.01);
    for p in out.px.iter_mut() {
        let l = luma(*p);
        for c in p.iter_mut() {
            let mut v = l + (*c - l) * saturation;
            v = (v - 0.5) * contrast + 0.5 + brightness;
            *c = clamp01(v).powf(inv_g);
        }
    }
    out
}

fn blur_line(src: &[[f32; 3]], dst: &mut [[f32; 3]], r: usize) {
    let n = src.len() as isize;
    let r = r as isize;
    let at = |i: isize| src[i.max(0).min(n - 1) as usize];
    let mut sum = [0.0f32; 3];
    for j in -r..=r {
        let p = at(j);
        for c in 0..3 { sum[c] += p[c]; }
    }
    let k = (2 * r + 1) as f32;
    for i in 0..n {
        dst[i as usize] = [sum[0] / k, sum[1] / k, sum[2] / k];
        let add = at(i + r + 1);
        let rem = at(i - r);
        for c in 0..3 { sum[c] += add[c] - rem[c]; }
    }
}

pub fn box_blur(img: &Image, radius: u32) -> Image {
    if radius == 0 || img.w == 0 || img.h == 0 { return img.clone(); }
    let r = radius as usize;
    let mut tmp = img.clone();
    for y in 0..img.h {
        blur_line(&img.px[y * img.w..(y + 1) * img.w], &mut tmp.px[y * img.w..(y + 1) * img.w], r);
    }
    let mut out = tmp.clone();
    let mut col = vec![[0.0f32; 3]; img.h];
    let mut res = vec![[0.0f32; 3]; img.h];
    for x in 0..img.w {
        for y in 0..img.h { col[y] = tmp.px[y * img.w + x]; }
        blur_line(&col, &mut res, r);
        for y in 0..img.h { out.px[y * img.w + x] = res[y]; }
    }
    out
}

pub fn downscale(img: &Image, factor: u32) -> Image {
    let f = factor.max(1) as usize;
    if f == 1 { return img.clone(); }
    let (nw, nh) = ((img.w / f).max(1), (img.h / f).max(1));
    let mut out = Image::new(nw, nh);
    for y in 0..nh {
        for x in 0..nw {
            let mut s = [0.0f32; 3];
            let mut n = 0.0;
            for yy in (y * f)..((y + 1) * f).min(img.h) {
                for xx in (x * f)..((x + 1) * f).min(img.w) {
                    let p = img.px[yy * img.w + xx];
                    for c in 0..3 { s[c] += p[c]; }
                    n += 1.0;
                }
            }
            out.px[y * nw + x] = [s[0] / n, s[1] / n, s[2] / n];
        }
    }
    out
}

pub fn upscale(img: &Image, factor: u32) -> Image {
    let f = factor.max(1) as usize;
    if f == 1 { return img.clone(); }
    let mut out = Image::new(img.w * f, img.h * f);
    for y in 0..out.h {
        for x in 0..out.w {
            out.px[y * out.w + x] = img.px[(y / f) * img.w + x / f];
        }
    }
    out
}

pub fn chromatic_aberration(img: &Image, shift: i32) -> Image {
    let mut out = img.clone();
    for y in 0..img.h {
        for x in 0..img.w {
            let r = img.at_clamped(x as isize - shift as isize, y as isize)[0];
            let b = img.at_clamped(x as isize + shift as isize, y as isize)[2];
            let p = &mut out.px[y * img.w + x];
            p[0] = r;
            p[2] = b;
        }
    }
    out
}

pub fn glow(img: &Image, threshold: f32, radius: u32, intensity: f32) -> Image {
    let mut bright = img.clone();
    for p in bright.px.iter_mut() {
        let l = luma(*p);
        let k = if l > threshold { (l - threshold) / (1.0 - threshold + 1e-6) } else { 0.0 };
        *p = [p[0] * k, p[1] * k, p[2] * k];
    }
    let b = box_blur(&box_blur(&bright, radius), radius);
    let mut out = img.clone();
    for (o, g) in out.px.iter_mut().zip(b.px.iter()) {
        for c in 0..3 { o[c] = clamp01(o[c] + g[c] * intensity); }
    }
    out
}

pub fn scanlines(img: &Image, period: u32, strength: f32) -> Image {
    let period = period.max(2) as usize;
    let mut out = img.clone();
    for y in 0..img.h {
        if y % period == period - 1 {
            for x in 0..img.w {
                let p = &mut out.px[y * img.w + x];
                for c in 0..3 { p[c] *= 1.0 - strength; }
            }
        }
    }
    out
}

pub fn invert(img: &Image) -> Image {
    let mut out = img.clone();
    for p in out.px.iter_mut() { *p = [1.0 - p[0], 1.0 - p[1], 1.0 - p[2]]; }
    out
}

pub fn noise(img: &Image, amount: f32, seed: u32) -> Image {
    let mut out = img.clone();
    for y in 0..img.h {
        for x in 0..img.w {
            let n = (rand01(x as u32, y as u32, seed) - 0.5) * amount;
            let p = &mut out.px[y * img.w + x];
            for c in 0..3 { p[c] = clamp01(p[c] + n); }
        }
    }
    out
}

pub fn row_shift(img: &Image, max_shift: i32, chance: f32, seed: u32) -> Image {
    let mut out = img.clone();
    for y in 0..img.h {
        if rand01(0, y as u32, seed) < chance {
            let s = ((rand01(1, y as u32, seed) * 2.0 - 1.0) * max_shift as f32) as isize;
            for x in 0..img.w {
                out.px[y * img.w + x] = img.at_clamped(x as isize - s, y as isize);
            }
        }
    }
    out
}

const QY: [f32; 64] = [
    16., 11., 10., 16., 24., 40., 51., 61., 12., 12., 14., 19., 26., 58., 60., 55., 14., 13., 16., 24., 40., 57., 69., 56., 14., 17., 22., 29.,
    51., 87., 80., 62., 18., 22., 37., 56., 68., 109., 103., 77., 24., 35., 55., 64., 81., 104., 113., 92., 49., 64., 78., 87., 103., 121.,
    120., 101., 72., 92., 95., 98., 112., 100., 103., 99.,
];
const QC: [f32; 64] = [
    17., 18., 24., 47., 99., 99., 99., 99., 18., 21., 26., 66., 99., 99., 99., 99., 24., 26., 56., 99., 99., 99., 99., 99., 47., 66., 99., 99.,
    99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99., 99.,
    99., 99., 99., 99., 99., 99., 99., 99.,
];

fn dct_matrix() -> [[f32; 8]; 8] {
    let mut t = [[0f32; 8]; 8];
    for u in 0..8 {
        let c = if u == 0 { 0.5 / std::f32::consts::SQRT_2 } else { 0.5 };
        for x in 0..8 {
            t[u][x] = c * (((2 * x + 1) as f32) * u as f32 * std::f32::consts::PI / 16.0).cos();
        }
    }
    t
}

pub fn jpeg_glitch(img: &Image, quality: u32, corrupt: f32, seed: u32) -> Image {
    let (w, h) = (img.w, img.h);
    if w == 0 || h == 0 { return img.clone(); }
    let q = quality.clamp(1, 100) as f32;
    let scale = if q < 50.0 { 5000.0 / q } else { 200.0 - 2.0 * q };
    let tq = |t: &[f32; 64]| -> Vec<f32> { t.iter().map(|v| ((v * scale + 50.0) / 100.0).floor().max(1.0)).collect() };
    let (qy, qc) = (tq(&QY), tq(&QC));
    let t = dct_matrix();
    let mut planes = vec![vec![0f32; w * h]; 3];
    for (i, p) in img.px.iter().enumerate() {
        let (r, g, b) = (p[0] * 255.0, p[1] * 255.0, p[2] * 255.0);
        planes[0][i] = 0.299 * r + 0.587 * g + 0.114 * b;
        planes[1][i] = 128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b;
        planes[2][i] = 128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b;
    }
    for (ch, plane) in planes.iter_mut().enumerate() {
        let table = if ch == 0 { &qy } else { &qc };
        let src = plane.clone();
        for by in 0..(h + 7) / 8 {
            for bx in 0..(w + 7) / 8 {
                let mut blk = [[0f32; 8]; 8];
                for y in 0..8 {
                    for x in 0..8 {
                        let (sx, sy) = ((bx * 8 + x).min(w - 1), (by * 8 + y).min(h - 1));
                        blk[y][x] = src[sy * w + sx] - 128.0;
                    }
                }
                let mut tmp = [[0f32; 8]; 8];
                let mut f = [[0f32; 8]; 8];
                for v in 0..8 { for x in 0..8 { tmp[v][x] = (0..8).map(|y| t[v][y] * blk[y][x]).sum(); } }
                for v in 0..8 { for u in 0..8 { f[v][u] = (0..8).map(|x| t[u][x] * tmp[v][x]).sum(); } }
                let mut qv = [0f32; 64];
                for i in 0..64 { qv[i] = (f[i / 8][i % 8] / table[i]).round(); }
                let bseed = seed.wrapping_add(ch as u32 * 977);
                if rand01(bx as u32, by as u32, bseed) < corrupt {
                    let n = 1 + (rand01(by as u32, bx as u32, bseed ^ 0x55) * 3.0) as u32;
                    for k in 0..n {
                        let idx = 1 + (rand01(bx as u32 + k * 31, by as u32, bseed ^ 0x99) * 20.0) as usize;
                        let amt = (rand01(bx as u32, by as u32 + k * 17, bseed ^ 0x77) * 2.0 - 1.0) * 12.0;
                        qv[idx.min(63)] += amt.round();
                    }
                }
                for i in 0..64 { f[i / 8][i % 8] = qv[i] * table[i]; }
                for v in 0..8 { for x in 0..8 { tmp[v][x] = (0..8).map(|u| t[u][x] * f[v][u]).sum(); } }
                for y in 0..8 {
                    for x in 0..8 {
                        let (ox, oy) = (bx * 8 + x, by * 8 + y);
                        if ox < w && oy < h {
                            let v: f32 = (0..8).map(|k| t[k][y] * tmp[k][x]).sum();
                            plane[oy * w + ox] = v + 128.0;
                        }
                    }
                }
            }
        }
    }
    let mut out = img.clone();
    for i in 0..w * h {
        let (y, cb, cr) = (planes[0][i], planes[1][i] - 128.0, planes[2][i] - 128.0);
        let r = y + 1.402 * cr;
        let g = y - 0.344_136 * cb - 0.714_136 * cr;
        let b = y + 1.772 * cb;
        out.px[i] = [clamp01(r / 255.0), clamp01(g / 255.0), clamp01(b / 255.0)];
    }
    out
}

pub fn blend(a: &Image, b: &Image, amount: f32) -> Image {
    let mut out = a.clone();
    for (o, (pa, pb)) in out.px.iter_mut().zip(a.px.iter().zip(b.px.iter())) {
        for c in 0..3 { o[c] = pa[c] + (pb[c] - pa[c]) * amount; }
    }
    out
}

pub fn resize_nearest(img: &Image, w: usize, h: usize) -> Image {
    let mut out = Image::new(w, h);
    if img.w == 0 || img.h == 0 { return out; }
    for y in 0..h {
        let sy = (y * img.h / h.max(1)).min(img.h - 1);
        for x in 0..w {
            let sx = (x * img.w / w.max(1)).min(img.w - 1);
            out.px[y * w + x] = img.px[sy * img.w + sx];
        }
    }
    out
}

pub fn posterize(img: &Image, levels: u32) -> Image {
    let n = (levels.max(2) - 1) as f32;
    let mut out = img.clone();
    for p in out.px.iter_mut() {
        for c in p.iter_mut() { *c = (clamp01(*c) * n).round() / n; }
    }
    out
}

pub fn gradient_map(img: &Image, stops: &[[f32; 3]]) -> Image {
    if stops.len() < 2 { return img.clone(); }
    let last = (stops.len() - 1) as f32;
    let mut out = img.clone();
    for p in out.px.iter_mut() {
        let pos = clamp01(luma(*p)) * last;
        let i = (pos.floor() as usize).min(stops.len() - 2);
        let t = pos - i as f32;
        let (a, b) = (stops[i], stops[i + 1]);
        *p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    }
    out
}

pub fn sharpen(img: &Image, amount: f32, radius: u32) -> Image {
    let blur = box_blur(img, radius.max(1));
    let mut out = img.clone();
    for (o, b) in out.px.iter_mut().zip(blur.px.iter()) {
        for c in 0..3 { o[c] = clamp01(o[c] + (o[c] - b[c]) * amount); }
    }
    out
}
