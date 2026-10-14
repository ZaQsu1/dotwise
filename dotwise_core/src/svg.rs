//! Vector export: every colour becomes one path made of merged rectangles.
use crate::image::Image;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

fn key(p: [f32; 3]) -> u32 {
    let b = |v: f32| (v.max(0.0).min(1.0) * 255.0 + 0.5) as u32;
    (b(p[0]) << 16) | (b(p[1]) << 8) | b(p[2])
}

/// Works on images with at most 256 distinct colours (any dithered output).
/// The most common colour becomes the background rectangle, runs of the others are merged
/// horizontally first and then vertically, which keeps files small.
pub fn to_svg(img: &Image, scale: u32) -> Result<String, String> {
    let (w, h) = (img.w, img.h);
    let keys: Vec<u32> = img.px.iter().map(|p| key(*p)).collect();
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for k in &keys {
        *counts.entry(*k).or_insert(0) += 1;
    }
    if counts.len() > 256 {
        return Err(format!("image has {} colours, SVG export needs a dithered image with at most 256", counts.len()));
    }
    let bg = *counts.iter().max_by_key(|(k, c)| (**c, **k)).map(|(k, _)| k).ok_or("empty image")?;

    let mut runs: HashMap<u32, Vec<(usize, usize, usize)>> = HashMap::new(); // colour -> (y, x, width)
    for y in 0..h {
        let row = &keys[y * w..(y + 1) * w];
        let mut x = 0;
        while x < w {
            let k = row[x];
            let mut e = x + 1;
            while e < w && row[e] == k {
                e += 1;
            }
            if k != bg {
                runs.entry(k).or_default().push((y, x, e - x));
            }
            x = e;
        }
    }

    let s = scale.max(1);
    let mut out = String::new();
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" shape-rendering=\"crispEdges\">\n<rect width=\"{}\" height=\"{}\" fill=\"#{:06x}\"/>\n",
        w as u32 * s, h as u32 * s, w, h, w, h, bg
    );
    let mut colours: Vec<u32> = runs.keys().cloned().collect();
    colours.sort();
    for c in colours {
        let list = &runs[&c];
        let mut d = String::new();
        let mut active: HashMap<(usize, usize), usize> = HashMap::new(); // (x, width) -> start row
        let mut last_y = 0usize;
        let mut i = 0;
        let emit = |d: &mut String, x: usize, wd: usize, y0: usize, y1: usize| {
            let _ = write!(d, "M{} {}h{}v{}h-{}z", x, y0, wd, y1 - y0, wd);
        };
        while i < list.len() {
            let y = list[i].0;
            let mut j = i;
            let mut cur: HashSet<(usize, usize)> = HashSet::new();
            while j < list.len() && list[j].0 == y {
                cur.insert((list[j].1, list[j].2));
                j += 1;
            }
            if !active.is_empty() && y != last_y + 1 {
                let mut a: Vec<_> = active.drain().collect();
                a.sort();
                for ((x, wd), y0) in a {
                    emit(&mut d, x, wd, y0, last_y + 1);
                }
            }
            let mut closing: Vec<(usize, usize)> = active.keys().filter(|k| !cur.contains(*k)).cloned().collect();
            closing.sort();
            for k in closing {
                let y0 = active.remove(&k).unwrap();
                emit(&mut d, k.0, k.1, y0, y);
            }
            let mut fresh: Vec<_> = cur.into_iter().filter(|k| !active.contains_key(k)).collect();
            fresh.sort();
            for k in fresh {
                active.insert(k, y);
            }
            last_y = y;
            i = j;
        }
        let mut a: Vec<_> = active.drain().collect();
        a.sort();
        for ((x, wd), y0) in a {
            emit(&mut d, x, wd, y0, last_y + 1);
        }
        let _ = writeln!(out, "<path fill=\"#{:06x}\" d=\"{}\"/>", c, d);
    }
    out.push_str("</svg>\n");
    Ok(out)
}
