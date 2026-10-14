//! Process every png, jpg or jpeg in a folder with one pipeline.
use dotwise_core::{Image, Pipeline};
use std::path::Path;

fn is_image(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg"))
}

/// Writes `<name>_dotwise.png` into `output_dir`. Returns (done, failed).
/// `progress(done, total)` returns false to cancel.
pub fn process_folder(
    input_dir: &Path,
    output_dir: &Path,
    pipeline: &Pipeline,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(usize, usize), String> {
    let mut files: Vec<_> = std::fs::read_dir(input_dir)
        .map_err(|e| format!("cannot read {}: {}", input_dir.display(), e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && is_image(p))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err("no png or jpg files in that folder".into());
    }
    std::fs::create_dir_all(output_dir).map_err(|e| format!("cannot create {}: {}", output_dir.display(), e))?;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    let total = files.len() as u64;
    let (mut done, mut failed) = (0usize, 0usize);
    for (ci, chunk) in files.chunks(threads).enumerate() {
        let results: Vec<bool> = std::thread::scope(|s| {
            let handles: Vec<_> = chunk
                .iter()
                .enumerate()
                .map(|(k, path)| {
                    let idx = (ci * threads + k) as u32;
                    s.spawn(move || -> bool {
                        let Ok(img) = image::open(path) else { return false };
                        let rgb = img.to_rgb8();
                        let src = Image::from_rgb8(rgb.width() as usize, rgb.height() as usize, rgb.as_raw());
                        let out = pipeline.run_frame(&src, idx);
                        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
                        let target = output_dir.join(format!("{}_dotwise.png", stem));
                        image::save_buffer(target, &out.to_rgb8(), out.w as u32, out.h as u32, image::ColorType::Rgb8).is_ok()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or(false)).collect()
        });
        for ok in results {
            if ok { done += 1 } else { failed += 1 }
        }
        if !progress((done + failed) as u64, total) {
            return Err("cancelled".into());
        }
    }
    Ok((done, failed))
}
