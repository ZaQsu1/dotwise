//! Blue noise threshold texture made with the void and cluster method (toroidal, tileable).
use crate::effects::rand01;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub const SIZE: usize = 64;

fn cache() -> &'static Mutex<HashMap<u32, Arc<Vec<f32>>>> {
    static C: OnceLock<Mutex<HashMap<u32, Arc<Vec<f32>>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cached SIZE x SIZE texture with values in 0..1 (rank based, so the histogram is flat).
pub fn texture(seed: u32) -> Arc<Vec<f32>> {
    if let Some(t) = cache().lock().unwrap().get(&seed) {
        return t.clone();
    }
    let t = Arc::new(generate(SIZE, seed));
    cache().lock().unwrap().insert(seed, t.clone());
    t
}

pub fn generate(n: usize, seed: u32) -> Vec<f32> {
    let nn = n * n;
    let sigma = 1.5f32;
    let mut g = vec![0f32; nn];
    for dy in 0..n {
        for dx in 0..n {
            let (wx, wy) = (dx.min(n - dx) as f32, dy.min(n - dy) as f32);
            g[dy * n + dx] = (-(wx * wx + wy * wy) / (2.0 * sigma * sigma)).exp();
        }
    }
    let add = |energy: &mut Vec<f32>, idx: usize, sign: f32| {
        let (px, py) = (idx % n, idx / n);
        for y in 0..n {
            let dy = (y + n - py) % n;
            for x in 0..n {
                let dx = (x + n - px) % n;
                energy[y * n + x] += sign * g[dy * n + dx];
            }
        }
    };
    let argmax = |e: &Vec<f32>, p: &Vec<bool>, want: bool| -> usize {
        let mut best = usize::MAX;
        let mut bv = f32::MIN;
        for i in 0..nn {
            if p[i] == want && e[i] > bv {
                bv = e[i];
                best = i;
            }
        }
        best
    };
    let argmin = |e: &Vec<f32>, p: &Vec<bool>, want: bool| -> usize {
        let mut best = usize::MAX;
        let mut bv = f32::MAX;
        for i in 0..nn {
            if p[i] == want && e[i] < bv {
                bv = e[i];
                best = i;
            }
        }
        best
    };

    let k = (nn / 10).max(2);
    let mut order: Vec<usize> = (0..nn).collect();
    order.sort_by(|a, b| rand01(*a as u32, 7, seed).partial_cmp(&rand01(*b as u32, 7, seed)).unwrap());
    let mut pat = vec![false; nn];
    let mut energy = vec![0f32; nn];
    for &i in order.iter().take(k) {
        pat[i] = true;
        add(&mut energy, i, 1.0);
    }
    for _ in 0..nn * 4 {
        let cluster = argmax(&energy, &pat, true);
        pat[cluster] = false;
        add(&mut energy, cluster, -1.0);
        let void = argmin(&energy, &pat, false);
        pat[void] = true;
        add(&mut energy, void, 1.0);
        if void == cluster {
            break;
        }
    }

    let mut rank = vec![0usize; nn];
    let (mut p, mut e) = (pat.clone(), energy.clone());
    for r in (0..k).rev() {
        let c = argmax(&e, &p, true);
        rank[c] = r;
        p[c] = false;
        add(&mut e, c, -1.0);
    }
    let (mut p, mut e) = (pat, energy);
    for r in k..nn {
        let v = argmin(&e, &p, false);
        rank[v] = r;
        p[v] = true;
        add(&mut e, v, 1.0);
    }
    rank.iter().map(|r| (*r as f32 + 0.5) / nn as f32).collect()
}
