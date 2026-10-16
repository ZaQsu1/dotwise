#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::Engine;
use dotwise_core::{presets, Image, Pipeline};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::io::Cursor;
use tauri::{DragDropEvent, Emitter, State, WebviewEvent};

#[derive(Default)]
struct PreviewCache {
    next_id: AtomicU64,
    images: Mutex<HashMap<u64, Arc<Image>>>,
}

fn decode_source(image_data: &str) -> Result<Image, String> {
    let encoded = image_data
        .split_once(',')
        .map(|(_, data)| data)
        .unwrap_or(image_data);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| error.to_string())?;
    let decoded = image::load_from_memory(&bytes).map_err(|error| error.to_string())?.to_rgb8();
    Ok(Image::from_rgb8(decoded.width() as usize, decoded.height() as usize, decoded.as_raw()))
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
async fn prepare_preview(cache: State<'_, PreviewCache>, image_data: String) -> Result<u64, String> {
    let preview = tauri::async_runtime::spawn_blocking(move || decode_source(&image_data))
        .await
        .map_err(|error| error.to_string())??;
    let id = cache.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    cache.images.lock().map_err(|error| error.to_string())?.insert(id, Arc::new(preview));
    Ok(id)
}

#[tauri::command]
async fn render_preview(cache: State<'_, PreviewCache>, preview_id: u64, pipeline_json: String) -> Result<String, String> {
    let source = cache.images.lock()
        .map_err(|error| error.to_string())?
        .get(&preview_id)
        .cloned()
        .ok_or_else(|| "Preview source expired; reopen the image".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let pipeline = Pipeline::from_json(&pipeline_json)?;
        encode_png(&pipeline.run(&source))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn release_preview(cache: State<'_, PreviewCache>, preview_id: u64) -> Result<(), String> {
    cache.images.lock().map_err(|error| error.to_string())?.remove(&preview_id);
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
fn read_image(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    let ext = std::path::Path::new(&path).extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        _ => "image/png",
    };
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
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
async fn export_image(image_data: String, pipeline_json: String, output_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = decode_source(&image_data)?;
        let pipeline = Pipeline::from_json(&pipeline_json)?;
        let output = pipeline.run(&source);
        if output_path.to_ascii_lowercase().ends_with(".svg") {
            let svg = dotwise_core::svg::to_svg(&output, 1)?;
            std::fs::write(&output_path, svg).map_err(|error| error.to_string())?;
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
async fn load_video(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let info = dotwise_video::probe(&path)?;
        let frame_png = encode_png(&dotwise_video::grab_frame(&path, 0.0)?)?;
        serde_json::to_string(&serde_json::json!({
            "width": info.w,
            "height": info.h,
            "fps": info.fps,
            "duration": info.duration,
            "frame": frame_png,
        })).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn video_frame(path: String, seconds: f64) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || encode_png(&dotwise_video::grab_frame(&path, seconds)?))
        .await
        .map_err(|error| error.to_string())?
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
        .manage(PreviewCache::default())
        .manage(Arc::new(AtomicBool::new(false)))
        .invoke_handler(tauri::generate_handler![
            list_presets, get_preset, render_image, prepare_preview, render_preview, release_preview, pick_media, read_image, pick_output_path,
            export_image, load_video, video_frame, pick_video_path, export_video, process_batch,
            cancel_export, save_preset_file, load_preset_file
        ])
        .on_webview_event(|webview, event| {
            if let WebviewEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
                if let Some(path) = paths.iter().find(|path| {
                    matches!(
                        path.extension().and_then(|extension| extension.to_str()).map(str::to_ascii_lowercase).as_deref(),
                        Some("png" | "jpg" | "jpeg" | "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" | "gif")
                    )
                }) {
                    let _ = webview.emit("media-dropped", path.to_string_lossy().to_string());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Dotwise");
}