#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::Engine;
use dotwise_core::{presets, Image, Pipeline};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::io::Cursor;
use tauri::{Emitter, State};

#[derive(Default)]
struct SourceCache {
    next_id: AtomicU64,
    images: Mutex<HashMap<u64, CachedImage>>,
}

struct CachedImage {
    original: Arc<Image>,
    preview: Arc<Image>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedImage {
    source_id: u64,
    width: usize,
    height: usize,
    preview_png: String,
}

struct PreparedImageParts {
    original: Arc<Image>,
    preview: Arc<Image>,
    preview_png: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LoadedVideo {
    width: u32,
    height: u32,
    fps: f64,
    duration: f64,
    frame: PreparedImage,
}

fn decode_bytes(bytes: &[u8]) -> Result<Image, String> {
    let decoded = image::load_from_memory(&bytes).map_err(|error| error.to_string())?.to_rgb8();
    Ok(Image::from_rgb8(decoded.width() as usize, decoded.height() as usize, decoded.as_raw()))
}

fn decode_source(image_data: &str) -> Result<Image, String> {
    let encoded = image_data.split_once(',').map(|(_, data)| data).unwrap_or(image_data);
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|error| error.to_string())?;
    decode_bytes(&bytes)
}

fn encode_png(image: &Image) -> Result<String, String> {
    let rgb = image::RgbImage::from_raw(image.w as u32, image.h as u32, image.to_rgb8())
        .ok_or_else(|| "Could not encode image dimensions".to_string())?;
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rgb)
        .write_to(&mut bytes, image::ImageOutputFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()))
}

fn prepare_image_parts(original: Image) -> Result<PreparedImageParts, String> {
    let original = Arc::new(original);
    let longest_side = original.w.max(original.h);
    let factor = ((longest_side + 1535) / 1536).max(1);
    let preview = if factor == 1 { original.clone() } else { Arc::new(dotwise_core::effects::downscale(&original, factor as u32)) };
    let preview_png = encode_png(&preview)?;
    Ok(PreparedImageParts { original, preview, preview_png })
}

fn store_image(cache: &SourceCache, parts: PreparedImageParts) -> Result<PreparedImage, String> {
    let width = parts.original.w;
    let height = parts.original.h;
    let source_id = cache.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    cache.images.lock().map_err(|error| error.to_string())?.insert(source_id, CachedImage {
        original: parts.original,
        preview: parts.preview,
    });
    Ok(PreparedImage { source_id, width, height, preview_png: parts.preview_png })
}

fn grayscale_bytes(image: &Image) -> Option<Vec<u8>> {
    image.px.iter().all(|pixel| {
        (pixel[0] - pixel[1]).abs() <= f32::EPSILON && (pixel[1] - pixel[2]).abs() <= f32::EPSILON
    }).then(|| image.px.iter().map(|pixel| (pixel[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect())
}

#[tauri::command]
fn list_presets() -> Vec<String> {
    presets::builtin().into_iter().map(|(name, _)| name.to_string()).collect()
}

#[tauri::command]
fn get_preset(name: String) -> Result<String, String> {
    presets::find(&name)
        .map(|pipeline| pipeline.to_json())
        .ok_or_else(|| format!("Unknown preset: {name}"))
}

#[tauri::command]
async fn render_image(image_data: String, pipeline_json: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = decode_source(&image_data)?;
        let pipeline = Pipeline::from_json(&pipeline_json)?;
        encode_png(&pipeline.run(&source))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn prepare_source(cache: State<'_, SourceCache>, image_data: String) -> Result<PreparedImage, String> {
    let parts = tauri::async_runtime::spawn_blocking(move || decode_source(&image_data).and_then(prepare_image_parts))
        .await
        .map_err(|error| error.to_string())??;
    store_image(&cache, parts)
}

#[tauri::command]
async fn open_image(cache: State<'_, SourceCache>, path: String) -> Result<PreparedImage, String> {
    let parts = tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        decode_bytes(&bytes).and_then(prepare_image_parts)
    })
    .await
    .map_err(|error| error.to_string())??;
    store_image(&cache, parts)
}

#[tauri::command]
async fn render_preview(cache: State<'_, SourceCache>, source_id: u64, pipeline_json: String) -> Result<String, String> {
    let source = cache.images.lock()
        .map_err(|error| error.to_string())?
        .get(&source_id)
        .map(|image| image.preview.clone())
        .ok_or_else(|| "Image source expired; reopen the image".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let pipeline = Pipeline::from_json(&pipeline_json)?;
        encode_png(&pipeline.run(&source))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn release_source(cache: State<'_, SourceCache>, source_id: u64) -> Result<(), String> {
    cache.images.lock().map_err(|error| error.to_string())?.remove(&source_id);
    Ok(())
}

#[tauri::command]
fn pick_media() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("Images and video", &["png", "jpg", "jpeg", "mp4", "mov", "mkv", "webm", "avi", "m4v", "gif"])
        .pick_file()
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
fn pick_output_path(format: String) -> Option<String> {
    let (name, extensions): (&str, &[&str]) = match format.as_str() {
        "svg" => ("output.svg", &["svg"]),
        "jpg" => ("output.jpg", &["jpg", "jpeg"]),
        "mp4" => ("output.mp4", &["mp4", "mov", "mkv", "webm"]),
        _ => ("output.png", &["png"]),
    };
    rfd::FileDialog::new()
        .set_file_name(name)
        .add_filter(if format == "mp4" { "Video" } else { "Output image" }, extensions)
        .save_file()
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
async fn export_image(cache: State<'_, SourceCache>, source_id: u64, pipeline_json: String, output_path: String) -> Result<String, String> {
    let source = cache.images.lock()
        .map_err(|error| error.to_string())?
        .get(&source_id)
        .map(|image| image.original.clone())
        .ok_or_else(|| "Image source expired; reopen the image".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let pipeline = Pipeline::from_json(&pipeline_json)?;
        let output = pipeline.run(&source);
        let grayscale = if output_path.to_ascii_lowercase().ends_with(".png") { grayscale_bytes(&output) } else { None };
        if output_path.to_ascii_lowercase().ends_with(".svg") {
            let svg = dotwise_core::svg::to_svg(&output, 1)?;
            std::fs::write(&output_path, svg).map_err(|error| error.to_string())?;
        } else if let Some(grayscale) = grayscale {
            image::save_buffer(
                &output_path,
                &grayscale,
                output.w as u32,
                output.h as u32,
                image::ColorType::L8,
            ).map_err(|error| error.to_string())?;
        } else {
            image::save_buffer(
                &output_path,
                &output.to_rgb8(),
                output.w as u32,
                output.h as u32,
                image::ColorType::Rgb8,
            ).map_err(|error| error.to_string())?;
        }
        Ok(format!("Saved {} ({}x{})", output_path, output.w, output.h))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn load_video(cache: State<'_, SourceCache>, path: String) -> Result<LoadedVideo, String> {
    let (info, parts) = tauri::async_runtime::spawn_blocking(move || {
        let info = dotwise_video::probe(&path)?;
        let frame = dotwise_video::grab_frame(&path, 0.0)?;
        Ok::<_, String>((info, prepare_image_parts(frame)?))
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(LoadedVideo {
        width: info.w as u32,
        height: info.h as u32,
        fps: info.fps,
        duration: info.duration,
        frame: store_image(&cache, parts)?,
    })
}

#[tauri::command]
async fn video_frame(cache: State<'_, SourceCache>, path: String, seconds: f64) -> Result<PreparedImage, String> {
    let parts = tauri::async_runtime::spawn_blocking(move || {
        dotwise_video::grab_frame(&path, seconds).and_then(prepare_image_parts)
    })
        .await
        .map_err(|error| error.to_string())??;
    store_image(&cache, parts)
}

#[tauri::command]
fn pick_video_path() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("Video", &["mp4", "mov", "mkv", "webm", "avi", "m4v", "gif"])
        .pick_file()
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
async fn export_video(
    app: tauri::AppHandle,
    cancel: State<'_, Arc<AtomicBool>>,
    path: String,
    output_path: String,
    pipeline_json: String,
    crf: u32,
    hold: f32,
) -> Result<String, String> {
    cancel.store(false, Ordering::SeqCst);
    let cancel = cancel.inner().clone();
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let pipeline = Pipeline::from_json(&pipeline_json)?;
        dotwise_video::export(
            &path,
            &output_path,
            &pipeline,
            &dotwise_video::ExportOptions { crf, hold, ..Default::default() },
            &mut |done, total| {
                let _ = app_handle.emit("export-progress", serde_json::json!({ "done": done, "total": total }));
                !cancel.load(Ordering::SeqCst)
            },
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn process_batch(
    app: tauri::AppHandle,
    cancel: State<'_, Arc<AtomicBool>>,
    pipeline_json: String,
) -> Result<String, String> {
    let Some(input) = rfd::FileDialog::new().set_title("Input folder").pick_folder() else {
        return Err("Batch cancelled".into());
    };
    let Some(output) = rfd::FileDialog::new().set_title("Output folder").pick_folder() else {
        return Err("Batch cancelled".into());
    };
    cancel.store(false, Ordering::SeqCst);
    let cancel = cancel.inner().clone();
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let pipeline = Pipeline::from_json(&pipeline_json)?;
        let output_path: PathBuf = output.clone();
        let (done, failed) = dotwise_video::batch::process_folder(
            &input,
            &output_path,
            &pipeline,
            &mut |done, total| {
                let _ = app_handle.emit("export-progress", serde_json::json!({ "done": done, "total": total }));
                !cancel.load(Ordering::SeqCst)
            },
        )?;
        Ok(format!("Batch finished: {done} files written to {}, {failed} failed", output.display()))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn cancel_export(cancel: State<'_, Arc<AtomicBool>>) {
    cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn save_preset_file(pipeline_json: String) -> Result<String, String> {
    let path = rfd::FileDialog::new()
        .set_file_name("preset.json")
        .add_filter("Dotwise preset", &["json"])
        .save_file()
        .ok_or_else(|| "Preset save cancelled".to_string())?;
    let pipeline = Pipeline::from_json(&pipeline_json)?;
    std::fs::write(&path, pipeline.to_json()).map_err(|error| error.to_string())?;
    Ok(format!("Saved preset {}", path.display()))
}

#[tauri::command]
fn load_preset_file() -> Result<String, String> {
    let path = rfd::FileDialog::new()
        .add_filter("Dotwise preset", &["json"])
        .pick_file()
        .ok_or_else(|| "Preset load cancelled".to_string())?;
    std::fs::read_to_string(path).map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .manage(SourceCache::default())
        .manage(Arc::new(AtomicBool::new(false)))
        .invoke_handler(tauri::generate_handler![
            list_presets, get_preset, render_image, prepare_source, open_image, render_preview, release_source, pick_media, pick_output_path,
            export_image, load_video, video_frame, pick_video_path, export_video, process_batch,
            cancel_export, save_preset_file, load_preset_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dotwise");
}

#[cfg(test)]
mod tests {
    use super::{grayscale_bytes, Image};

    #[test]
    fn mono_images_encode_as_single_channel_bytes() {
        let image = Image { w: 2, h: 1, px: vec![[0.0; 3], [0.5; 3]] };
        assert_eq!(grayscale_bytes(&image), Some(vec![0, 128]));
    }

    #[test]
    fn color_images_do_not_use_single_channel_encoding() {
        let image = Image { w: 1, h: 1, px: vec![[1.0, 0.0, 0.0]] };
        assert_eq!(grayscale_bytes(&image), None);
    }
}