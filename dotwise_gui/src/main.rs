use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use dotwise_core::dither::{Diffusion, Method};
use dotwise_core::effects::downscale;
use dotwise_video::{ExportOptions, VideoInfo};
use dotwise_core::{presets, BlendMode, Image, KeyMode, Layer, PaletteSpec, Pipeline, Step};

const PREVIEW_MAX: usize = 2048;

enum Msg {
    Preview(u64, Image),
    Exported(Result<String, String>),
    Picked(bool, PathBuf),
    VideoLoaded(String, Result<(VideoInfo, Image), String>),
    VideoFrame(Result<Image, String>),
    VideoProgress(u64, u64),
}

struct Tab {
    source: Option<Arc<Image>>,
    preview_src: Option<Arc<Image>>,
    preview_factor: u32,
    pipeline: Pipeline,
    tex_result: Option<egui::TextureHandle>,
    tex_orig: Option<egui::TextureHandle>,
    in_path: String,
    out_path: String,
    preset_path: String,
    status: String,
    show_original: bool,
    zoom: f32,
    pan: egui::Vec2,
    split_view: bool,
    split: f32,
    history: Vec<Pipeline>,
    hist_pos: usize,
    snap_pending: bool,
    dirty: bool,
    busy: bool,
    gen: Arc<AtomicU64>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    exporting: bool,
    video: Option<(String, VideoInfo)>,
    video_time: f64,
    progress: Option<(u64, u64)>,
    cancel: Arc<AtomicBool>,
    crf: u32,
    hold: f32,
    loading: bool,
}

fn is_video_output(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    [".mp4", ".mov", ".mkv", ".webm"].iter().any(|e| l.ends_with(e))
}

fn default_out(input: &str, ext: &str) -> String {
    let p = std::path::Path::new(input);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let name = format!("{}_dotwise.{}", stem, ext);
    match p.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(name).to_string_lossy().to_string(),
        _ => name,
    }
}

fn to_color_image(img: &Image) -> egui::ColorImage {
    egui::ColorImage::from_rgb([img.w, img.h], &img.to_rgb8())
}

impl Tab {
    fn title(&self) -> String {
        let name = std::path::Path::new(&self.in_path).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let mut t = if name.is_empty() { "Untitled".to_string() } else { name };
        if t.chars().count() > 22 {
            t = format!("{}...", t.chars().take(20).collect::<String>());
        }
        if self.video.is_some() {
            t.push_str(" (video)");
        }
        if self.exporting {
            t.push_str(" ...");
        }
        t
    }

    fn is_empty(&self) -> bool {
        self.source.is_none() && !self.loading
    }

    fn new() -> Self {
        let (tx, rx) = channel();
        let p0 = presets::find("Classic mono").unwrap();
        Self {
            source: None,
            preview_src: None,
            preview_factor: 1,
            pipeline: p0.clone(),
            history: vec![p0],
            hist_pos: 0,
            snap_pending: false,
            zoom: 0.0,
            pan: egui::Vec2::ZERO,
            split_view: false,
            split: 0.5,
            tex_result: None,
            tex_orig: None,
            in_path: String::new(),
            out_path: "out.png".into(),
            preset_path: "preset.json".into(),
            status: "Drop an image onto the window or type a path and press Open.".into(),
            show_original: false,
            dirty: false,
            busy: false,
            gen: Arc::new(AtomicU64::new(0)),
            tx,
            rx,
            exporting: false,
            video: None,
            video_time: 0.0,
            progress: None,
            cancel: Arc::new(AtomicBool::new(false)),
            crf: 14,
            hold: 0.0,
            loading: false,
        }
    }

    fn set_source(&mut self, ctx: &egui::Context, full: Image) {
        let factor = ((full.w.max(full.h) + PREVIEW_MAX - 1) / PREVIEW_MAX).max(1) as u32;
        let prev = if factor > 1 { downscale(&full, factor) } else { full.clone() };
        self.tex_orig = Some(ctx.load_texture("orig", to_color_image(&prev), egui::TextureOptions::NEAREST));
        self.status = format!(
            "{}x{}{}",
            full.w,
            full.h,
            if factor > 1 { format!(" (preview at 1/{}, export uses full size)", factor) } else { String::new() }
        );
        self.source = Some(Arc::new(full));
        self.preview_src = Some(Arc::new(prev));
        self.preview_factor = factor;
        self.dirty = true;
    }

    fn load(&mut self, ctx: &egui::Context, path: &str) {
        if dotwise_video::is_video_path(path) {
            self.status = format!("Reading video {}...", path);
            self.loading = true;
            let (tx, ctx2, p) = (self.tx.clone(), ctx.clone(), path.to_string());
            std::thread::spawn(move || {
                let r = dotwise_video::probe(&p).and_then(|info| dotwise_video::grab_frame(&p, 0.0).map(|f| (info, f)));
                let _ = tx.send(Msg::VideoLoaded(p, r));
                ctx2.request_repaint();
            });
            return;
        }
        match image::open(path) {
            Ok(img) => {
                let rgb = img.to_rgb8();
                let full = Image::from_rgb8(rgb.width() as usize, rgb.height() as usize, rgb.as_raw());
                self.video = None;
                self.in_path = path.to_string();
                if is_video_output(&self.out_path) {
                    self.out_path = default_out(path, "png");
                }
                self.set_source(ctx, full);
            }
            Err(e) => self.status = format!("Cannot open {}: {}", path, e),
        }
    }

    fn pick(&self, ctx: &egui::Context, input: bool) {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        let video = self.video.is_some();
        let suggested = std::path::Path::new(&self.out_path)
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_string())
            .unwrap_or_else(|| if video { "out.mp4".into() } else { "out.png".into() });
        std::thread::spawn(move || {
            let d = rfd::FileDialog::new();
            let r = if input {
                d.add_filter("Images and video", &["png", "jpg", "jpeg", "mp4", "mov", "mkv", "webm", "avi", "gif"]).pick_file()
            } else {
                if video {
                    d.add_filter("Video", &["mp4", "mov", "mkv", "webm"]).set_file_name(&suggested).save_file()
                } else {
                    d.add_filter("PNG, JPEG or SVG", &["png", "jpg", "jpeg", "svg"]).set_file_name(&suggested).save_file()
                }
            };
            if let Some(p) = r {
                let _ = tx.send(Msg::Picked(input, p));
                ctx.request_repaint();
            }
        });
    }

    fn grab(&self, ctx: &egui::Context, path: String, t: f64) {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::VideoFrame(dotwise_video::grab_frame(&path, t)));
            ctx.request_repaint();
        });
    }

    fn start_video_export(&mut self, ctx: &egui::Context, input: String) {
        let pipeline = self.pipeline.clone();
        let out = dotwise_video::normalize_output(&self.out_path);
        self.out_path = out.clone();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        let cancel = self.cancel.clone();
        let crf = self.crf;
        let hold = self.hold;
        cancel.store(false, Ordering::SeqCst);
        self.exporting = true;
        self.progress = Some((0, 1));
        self.status = "Encoding video...".into();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let ctx2 = ctx.clone();
            let res = dotwise_video::export(&input, &out, &pipeline, &ExportOptions { crf, hold, ..Default::default() }, &mut |d, t| {
                let _ = tx2.send(Msg::VideoProgress(d, t));
                ctx2.request_repaint();
                !cancel.load(Ordering::SeqCst)
            });
            let _ = tx.send(Msg::Exported(res.map(|p| format!("Saved {}", p))));
            ctx.request_repaint();
        });
    }

    fn undo(&mut self) {
        if self.hist_pos > 0 {
            self.hist_pos -= 1;
            self.pipeline = self.history[self.hist_pos].clone();
            self.dirty = true;
            self.snap_pending = false;
        }
    }

    fn redo(&mut self) {
        if self.hist_pos + 1 < self.history.len() {
            self.hist_pos += 1;
            self.pipeline = self.history[self.hist_pos].clone();
            self.dirty = true;
            self.snap_pending = false;
        }
    }

    fn start_batch(&mut self, ctx: &egui::Context) {
        let pipeline = self.pipeline.clone();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        let cancel = self.cancel.clone();
        cancel.store(false, Ordering::SeqCst);
        self.exporting = true;
        self.progress = None;
        self.status = "Choose the input folder, then the output folder...".into();
        std::thread::spawn(move || {
            let cancelled = |tx: &Sender<Msg>, ctx: &egui::Context| {
                let _ = tx.send(Msg::Exported(Err("Batch cancelled".into())));
                ctx.request_repaint();
            };
            let Some(inp) = rfd::FileDialog::new().set_title("Input folder").pick_folder() else { return cancelled(&tx, &ctx) };
            let Some(outp) = rfd::FileDialog::new().set_title("Output folder").pick_folder() else { return cancelled(&tx, &ctx) };
            let (tx2, ctx2) = (tx.clone(), ctx.clone());
            let res = dotwise_video::batch::process_folder(&inp, &outp, &pipeline, &mut |d, t| {
                let _ = tx2.send(Msg::VideoProgress(d, t));
                ctx2.request_repaint();
                !cancel.load(Ordering::SeqCst)
            });
            let msg = res.map(|(d, f)| format!("Batch finished: {} files written to {}, {} failed", d, outp.display(), f));
            let _ = tx.send(Msg::Exported(msg));
            ctx.request_repaint();
        });
    }

    fn start_preview(&mut self, ctx: &egui::Context) {
        let Some(src) = self.preview_src.clone() else { return };
        let id = self.gen.fetch_add(1, Ordering::SeqCst) + 1;
        let pipeline = self.pipeline.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.busy = true;
        self.dirty = false;
        std::thread::spawn(move || {
            let out = pipeline.run(&src);
            let _ = tx.send(Msg::Preview(id, out));
            ctx.request_repaint();
        });
    }

    fn start_export(&mut self, ctx: &egui::Context) {
        if let Some((path, _)) = self.video.clone() {
            self.start_video_export(ctx, path);
            return;
        }
        let Some(src) = self.source.clone() else {
            self.status = "Open an image first.".into();
            return;
        };
        let pipeline = self.pipeline.clone();
        let path = self.out_path.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.exporting = true;
        self.status = "Exporting at full size...".into();
        std::thread::spawn(move || {
            let out = pipeline.run(&src);
            let res = if path.to_ascii_lowercase().ends_with(".svg") {
                dotwise_core::svg::to_svg(&out, 1)
                    .and_then(|svg| std::fs::write(&path, svg).map_err(|e| e.to_string()))
                    .map(|_| format!("Saved {} ({}x{} vector)", path, out.w, out.h))
            } else {
                image::save_buffer(&path, &out.to_rgb8(), out.w as u32, out.h as u32, image::ColorType::Rgb8)
                    .map(|_| format!("Saved {} ({}x{})", path, out.w, out.h))
                    .map_err(|e| format!("Cannot write {}: {}", path, e))
            };
            let _ = tx.send(Msg::Exported(res));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Preview(id, img) => {
                    self.busy = false;
                    if id == self.gen.load(Ordering::SeqCst) {
                        self.tex_result = Some(ctx.load_texture("result", to_color_image(&img), egui::TextureOptions::NEAREST));
                    }
                }
                Msg::Picked(is_input, path) => {
                    let p = path.to_string_lossy().to_string();
                    if is_input {
                        self.load(ctx, &p);
                    } else {
                        self.out_path = if self.video.is_some() { dotwise_video::normalize_output(&p) } else { p };
                    }
                }
                Msg::VideoLoaded(p, r) => match r {
                    Ok((info, frame)) => {
                        self.loading = false;
                        if !is_video_output(&self.out_path) {
                            self.out_path = default_out(&p, "mp4");
                        }
                        self.in_path = p.clone();
                        self.video_time = 0.0;
                        self.set_source(ctx, frame);
                        self.status = format!("Video {}x{} at {:.2} fps, {:.1} s", info.w, info.h, info.fps, info.duration);
                        self.video = Some((p, info));
                    }
                    Err(e) => {
                        self.loading = false;
                        self.status = format!("Cannot read video: {}", e);
                    }
                },
                Msg::VideoFrame(r) => match r {
                    Ok(frame) => self.set_source(ctx, frame),
                    Err(e) => self.status = format!("Cannot read frame: {}", e),
                },
                Msg::VideoProgress(d, t) => self.progress = Some((d, t)),
                Msg::Exported(r) => {
                    self.exporting = false;
                    self.progress = None;
                    self.status = match r { Ok(s) | Err(s) => s };
                }
            }
        }
    }
}

fn spread_ui(ui: &mut egui::Ui, spread: &mut Option<f32>) -> bool {
    let mut custom = spread.is_some();
    let mut changed = ui.checkbox(&mut custom, "Custom spread").changed();
    if changed {
        *spread = if custom { Some(1.0) } else { None };
    }
    if let Some(s) = spread {
        changed |= ui.add(egui::Slider::new(s, 0.05..=2.0).text("Spread")).changed();
    }
    changed
}

fn method_name(m: &Method) -> &'static str {
    match m {
        Method::Diffusion { .. } => "Error diffusion",
        Method::Bayer { .. } => "Ordered (Bayer)",
        Method::Halftone { .. } => "Halftone dots",
        Method::Lines { .. } => "Lines",
        Method::WhiteNoise { .. } => "White noise",
        Method::Ign { .. } => "Gradient noise",
        Method::BlueNoise { .. } => "Blue noise",
    }
}

fn method_defaults() -> Vec<Method> {
    vec![
        Method::Diffusion { kernel: Diffusion::FloydSteinberg, serpentine: true, strength: 1.0 },
        Method::Bayer { size: 4, spread: None },
        Method::Halftone { cell: 6.0, angle: 45.0, spread: None },
        Method::Lines { period: 5.0, angle: 20.0, spread: None },
        Method::WhiteNoise { seed: 1, spread: None },
        Method::Ign { spread: None },
        Method::BlueNoise { seed: 1, spread: None },
    ]
}

fn method_ui(ui: &mut egui::Ui, m: &mut Method) -> bool {
    let mut changed = false;
    egui::ComboBox::from_label("Method").selected_text(method_name(m)).show_ui(ui, |ui| {
        for d in method_defaults() {
            let same = std::mem::discriminant(&d) == std::mem::discriminant(&*m);
            if ui.selectable_label(same, method_name(&d)).clicked() && !same {
                *m = d;
                changed = true;
            }
        }
    });
    match m {
        Method::Diffusion { kernel, serpentine, strength } => {
            egui::ComboBox::from_label("Kernel").selected_text(format!("{:?}", kernel)).show_ui(ui, |ui| {
                for k in Diffusion::ALL {
                    changed |= ui.selectable_value(kernel, k, format!("{:?}", k)).changed();
                }
            });
            changed |= ui.checkbox(serpentine, "Serpentine scan").changed();
            changed |= ui.add(egui::Slider::new(strength, 0.0..=1.2).text("Strength")).changed();
        }
        Method::Bayer { size, spread } => {
            egui::ComboBox::from_label("Matrix").selected_text(format!("{0}x{0}", size)).show_ui(ui, |ui| {
                for s in [2u32, 4, 8, 16] {
                    changed |= ui.selectable_value(size, s, format!("{0}x{0}", s)).changed();
                }
            });
            changed |= spread_ui(ui, spread);
        }
        Method::Halftone { cell, angle, spread } => {
            changed |= ui.add(egui::Slider::new(cell, 3.0..=24.0).text("Cell")).changed();
            changed |= ui.add(egui::Slider::new(angle, 0.0..=90.0).text("Angle")).changed();
            changed |= spread_ui(ui, spread);
        }
        Method::Lines { period, angle, spread } => {
            changed |= ui.add(egui::Slider::new(period, 3.0..=24.0).text("Period")).changed();
            changed |= ui.add(egui::Slider::new(angle, -90.0..=90.0).text("Angle")).changed();
            changed |= spread_ui(ui, spread);
        }
        Method::WhiteNoise { seed, spread } => {
            changed |= ui.add(egui::DragValue::new(seed).prefix("Seed ")).changed();
            changed |= spread_ui(ui, spread);
        }
        Method::BlueNoise { seed, spread } => {
            changed |= ui.add(egui::DragValue::new(seed).prefix("Seed ")).changed();
            changed |= spread_ui(ui, spread);
        }
        Method::Ign { spread } => changed |= spread_ui(ui, spread),
    }
    changed
}

fn palette_name(p: &PaletteSpec) -> &'static str {
    match p {
        PaletteSpec::Mono => "Black and white",
        PaletteSpec::Gray { .. } => "Gray levels",
        PaletteSpec::Duo { .. } => "Two colours",
        PaletteSpec::Gameboy => "Handheld green",
        PaletteSpec::Cga => "CGA",
        PaletteSpec::Custom { .. } => "Custom list",
        PaletteSpec::Extract { .. } => "Extract from image",
    }
}

fn palette_ui(ui: &mut egui::Ui, p: &mut PaletteSpec) -> bool {
    let mut changed = false;
    let options = vec![
        PaletteSpec::Mono,
        PaletteSpec::Gray { levels: 4 },
        PaletteSpec::Duo { dark: "#1b1b3a".into(), light: "#f2e9c9".into() },
        PaletteSpec::Gameboy,
        PaletteSpec::Cga,
        PaletteSpec::Custom { colors: vec!["#000000".into(), "#ff5555".into(), "#ffffff".into()] },
        PaletteSpec::Extract { count: 8 },
    ];
    egui::ComboBox::from_label("Palette").selected_text(palette_name(p)).show_ui(ui, |ui| {
        for o in options {
            let same = std::mem::discriminant(&o) == std::mem::discriminant(&*p);
            if ui.selectable_label(same, palette_name(&o)).clicked() && !same {
                *p = o;
                changed = true;
            }
        }
    });
    match p {
        PaletteSpec::Gray { levels } => changed |= ui.add(egui::Slider::new(levels, 2..=16).text("Levels")).changed(),
        PaletteSpec::Duo { dark, light } => {
            ui.horizontal(|ui| {
                changed |= ui.add(egui::TextEdit::singleline(dark).desired_width(70.0)).changed();
                changed |= ui.add(egui::TextEdit::singleline(light).desired_width(70.0)).changed();
            });
        }
        PaletteSpec::Custom { colors } => {
            let mut remove = None;
            for (i, c) in colors.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::TextEdit::singleline(c).desired_width(80.0)).changed();
                    if ui.small_button("Remove").clicked() && true {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                if colors.len() > 2 {
                    colors.remove(i);
                    changed = true;
                }
            }
            if ui.button("Add colour").clicked() {
                colors.push("#808080".into());
                changed = true;
            }
        }
        PaletteSpec::Extract { count } => changed |= ui.add(egui::Slider::new(count, 2..=64).text("Colours")).changed(),
        _ => {}
    }
    changed
}

fn step_name(s: &Step) -> &'static str {
    match s {
        Step::Adjust { .. } => "Adjust",
        Step::Blur { .. } => "Blur",
        Step::Downscale { .. } => "Downscale",
        Step::Upscale { .. } => "Upscale",
        Step::Dither { .. } => "Dither",
        Step::ChromaticAberration { .. } => "Chromatic aberration",
        Step::Glow { .. } => "Glow",
        Step::Scanlines { .. } => "Scanlines",
        Step::Invert => "Invert",
        Step::Noise { .. } => "Grain",
        Step::RowShift { .. } => "Row shift glitch",
        Step::JpegGlitch { .. } => "JPEG glitch",
        Step::Posterize { .. } => "Posterize",
        Step::GradientMap { .. } => "Gradient map",
        Step::Sharpen { .. } => "Sharpen",
    }
}

fn step_templates() -> Vec<Step> {
    vec![
        Step::Adjust { brightness: 0.0, contrast: 1.0, gamma: 1.0, saturation: 1.0 },
        Step::Blur { radius: 2 },
        Step::Downscale { factor: 2 },
        Step::Upscale { factor: 2 },
        Step::Dither { method: method_defaults().remove(0), palette: PaletteSpec::Mono },
        Step::ChromaticAberration { shift: 3 },
        Step::Glow { threshold: 0.6, radius: 6, intensity: 0.5 },
        Step::Scanlines { period: 3, strength: 0.25 },
        Step::Invert,
        Step::Noise { amount: 0.1, seed: 1 },
        Step::RowShift { max_shift: 12, chance: 0.05, seed: 1 },
        Step::JpegGlitch { quality: 25, corrupt: 0.05, seed: 1 },
        Step::Posterize { levels: 4 },
        Step::GradientMap { colors: vec!["#14123a".into(), "#c2386b".into(), "#fbe9c6".into()] },
        Step::Sharpen { amount: 0.6, radius: 1 },
    ]
}

fn step_ui(ui: &mut egui::Ui, s: &mut Step) -> bool {
    let mut c = false;
    match s {
        Step::Adjust { brightness, contrast, gamma, saturation } => {
            c |= ui.add(egui::Slider::new(brightness, -0.5..=0.5).text("Brightness")).changed();
            c |= ui.add(egui::Slider::new(contrast, 0.2..=3.0).text("Contrast")).changed();
            c |= ui.add(egui::Slider::new(gamma, 0.2..=3.0).text("Gamma")).changed();
            c |= ui.add(egui::Slider::new(saturation, 0.0..=2.0).text("Saturation")).changed();
        }
        Step::Blur { radius } => c |= ui.add(egui::Slider::new(radius, 0..=30).text("Radius")).changed(),
        Step::Downscale { factor } | Step::Upscale { factor } => c |= ui.add(egui::Slider::new(factor, 1..=16).text("Factor")).changed(),
        Step::Dither { method, palette } => {
            c |= method_ui(ui, method);
            ui.separator();
            c |= palette_ui(ui, palette);
        }
        Step::ChromaticAberration { shift } => c |= ui.add(egui::Slider::new(shift, -30..=30).text("Shift")).changed(),
        Step::Glow { threshold, radius, intensity } => {
            c |= ui.add(egui::Slider::new(threshold, 0.0..=1.0).text("Threshold")).changed();
            c |= ui.add(egui::Slider::new(radius, 0..=40).text("Radius")).changed();
            c |= ui.add(egui::Slider::new(intensity, 0.0..=2.0).text("Intensity")).changed();
        }
        Step::Scanlines { period, strength } => {
            c |= ui.add(egui::Slider::new(period, 2..=12).text("Period")).changed();
            c |= ui.add(egui::Slider::new(strength, 0.0..=1.0).text("Strength")).changed();
        }
        Step::Invert => {
            ui.label("No settings.");
        }
        Step::Noise { amount, seed } => {
            c |= ui.add(egui::Slider::new(amount, 0.0..=1.0).text("Amount")).changed();
            c |= ui.add(egui::DragValue::new(seed).prefix("Seed ")).changed();
        }
        Step::Posterize { levels } => c |= ui.add(egui::Slider::new(levels, 2..=32).text("Levels")).changed(),
        Step::Sharpen { amount, radius } => {
            c |= ui.add(egui::Slider::new(amount, 0.0..=3.0).text("Amount")).changed();
            c |= ui.add(egui::Slider::new(radius, 1..=10).text("Radius")).changed();
        }
        Step::GradientMap { colors } => {
            let mut remove = None;
            for (i, col) in colors.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    c |= ui.add(egui::TextEdit::singleline(col).desired_width(80.0)).changed();
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                if colors.len() > 2 {
                    colors.remove(i);
                    c = true;
                }
            }
            if ui.button("Add stop").clicked() {
                colors.push("#ffffff".into());
                c = true;
            }
        }
        Step::JpegGlitch { quality, corrupt, seed } => {
            c |= ui.add(egui::Slider::new(quality, 1..=100).text("Quality")).changed();
            c |= ui.add(egui::Slider::new(corrupt, 0.0..=1.0).text("Corruption")).changed();
            c |= ui.add(egui::DragValue::new(seed).prefix("Seed ")).changed();
        }
        Step::RowShift { max_shift, chance, seed } => {
            c |= ui.add(egui::Slider::new(max_shift, 0..=100).text("Max shift")).changed();
            c |= ui.add(egui::Slider::new(chance, 0.0..=1.0).text("Chance")).changed();
            c |= ui.add(egui::DragValue::new(seed).prefix("Seed ")).changed();
        }
    }
    c
}

impl Tab {
    fn ui(&mut self, ctx: &egui::Context) {

        if !ctx.wants_keyboard_input() {
            let redo = ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT), egui::Key::Z) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
            });
            let undo = !redo && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
            if undo {
                self.undo();
            }
            if redo {
                self.redo();
            }
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Input");
                ui.add(egui::TextEdit::singleline(&mut self.in_path).desired_width(260.0));
                if ui.button("Open").clicked() {
                    let p = self.in_path.clone();
                    self.load(ctx, &p);
                }
                if ui.button("Browse").clicked() {
                    self.pick(ctx, true);
                }
                ui.separator();
                ui.label("Output");
                ui.add(egui::TextEdit::singleline(&mut self.out_path).desired_width(200.0));
                if ui.button("Browse").clicked() {
                    self.pick(ctx, false);
                }
                let export_label = if self.video.is_some() { "Export video" } else { "Export" };
                if ui.add_enabled(!self.exporting, egui::Button::new(export_label)).clicked() {
                    self.start_export(ctx);
                }
                if ui.add_enabled(!self.exporting, egui::Button::new("Batch folder")).clicked() {
                    self.start_batch(ctx);
                }
                ui.separator();
                ui.checkbox(&mut self.show_original, "Show original");
                ui.checkbox(&mut self.split_view, "Split view");
                if ui.button("Fit").clicked() {
                    self.zoom = 0.0;
                    self.pan = egui::Vec2::ZERO;
                }
                if ui.button("100%").clicked() {
                    self.zoom = 1.0;
                    self.pan = egui::Vec2::ZERO;
                }
            });
        });

        if let Some((path, info)) = self.video.clone() {
            egui::TopBottomPanel::top("video").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Video {}x{}, {:.2} fps", info.w, info.h, info.fps));
                    let mut t = self.video_time;
                    let r = ui.add(egui::Slider::new(&mut t, 0.0..=info.duration.max(0.1)).text("Preview time (s)"));
                    self.video_time = t;
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        self.grab(ctx, path.clone(), t);
                    }
                    ui.add(egui::Slider::new(&mut self.crf, 0..=40).text("CRF (lower is larger)"));
                    ui.add(egui::Slider::new(&mut self.hold, 0.0..=0.2).text("Temporal hold"));
                    ui.checkbox(&mut self.pipeline.animate_noise, "Animate noise per frame");
                    if ui.checkbox(&mut self.pipeline.composite.enabled, "Overlay on original").changed() {
                        self.dirty = true;
                        self.snap_pending = true;
                    }
                });
            });
        }

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if self.busy {
                    ui.spinner();
                }
                if self.exporting {
                    if let Some((d, tot)) = self.progress {
                        ui.add(egui::ProgressBar::new(d as f32 / tot.max(1) as f32).desired_width(200.0).show_percentage());
                    } else {
                        ui.spinner();
                    }
                    if ui.button("Cancel").clicked() {
                        self.cancel.store(true, Ordering::SeqCst);
                    }
                }
            });
        });

        let mut changed = false;
        egui::SidePanel::left("layers").min_width(340.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_source("preset").selected_text("Built in presets").show_ui(ui, |ui| {
                    for (name, p) in presets::builtin() {
                        if ui.selectable_label(false, name).clicked() {
                            self.pipeline = p;
                            changed = true;
                        }
                    }
                });
                ui.menu_button("Add layer", |ui| {
                    for t in step_templates() {
                        if ui.button(step_name(&t)).clicked() {
                            self.pipeline.layers.push(Layer { enabled: true, opacity: 1.0, step: t });
                            changed = true;
                            ui.close_menu();
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.preset_path).desired_width(150.0));
                if ui.button("Save").clicked() {
                    self.status = match std::fs::write(&self.preset_path, self.pipeline.to_json()) {
                        Ok(_) => format!("Saved preset {}", self.preset_path),
                        Err(e) => format!("Cannot save preset: {}", e),
                    };
                }
                if ui.button("Load").clicked() {
                    match std::fs::read_to_string(&self.preset_path).map_err(|e| e.to_string()).and_then(|t| Pipeline::from_json(&t)) {
                        Ok(p) => {
                            self.pipeline = p;
                            changed = true;
                            self.status = format!("Loaded preset {}", self.preset_path);
                        }
                        Err(e) => self.status = format!("Cannot load preset: {}", e),
                    }
                }
            });
            ui.separator();

            ui.horizontal(|ui| {
                if ui.add_enabled(self.hist_pos > 0, egui::Button::new("Undo")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(self.hist_pos + 1 < self.history.len(), egui::Button::new("Redo")).clicked() {
                    self.redo();
                }
                ui.label("Ctrl+Z, Ctrl+Shift+Z");
            });
            ui.separator();
            egui::CollapsingHeader::new("Overlay on original").id_source(("overlay", self.video.is_some())).default_open(self.video.is_some()).show(ui, |ui| {
                let c = &mut self.pipeline.composite;
                changed |= ui.checkbox(&mut c.enabled, "Lay the result over the original").changed();
                egui::ComboBox::from_label("Blend").selected_text(format!("{:?}", c.mode)).show_ui(ui, |ui| {
                    for m in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen, BlendMode::Overlay, BlendMode::Add] {
                        changed |= ui.selectable_value(&mut c.mode, m, format!("{:?}", m)).changed();
                    }
                });
                changed |= ui.add(egui::Slider::new(&mut c.opacity, 0.0..=1.0).text("Opacity")).changed();
                egui::ComboBox::from_label("Key out").selected_text(match c.key {
                    KeyMode::None => "Nothing",
                    KeyMode::DropLight => "Light pixels",
                    KeyMode::DropDark => "Dark pixels",
                }).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut c.key, KeyMode::None, "Nothing").changed();
                    changed |= ui.selectable_value(&mut c.key, KeyMode::DropLight, "Light pixels").changed();
                    changed |= ui.selectable_value(&mut c.key, KeyMode::DropDark, "Dark pixels").changed();
                });
                if c.key != KeyMode::None {
                    changed |= ui.add(egui::Slider::new(&mut c.key_threshold, 0.0..=1.0).text("Key threshold")).changed();
                    changed |= ui.add(egui::Slider::new(&mut c.key_softness, 0.0..=0.5).text("Key softness")).changed();
                }
            });
            ui.separator();
            let mut action: Option<(usize, i32)> = None;
            let mut reorder: Option<(usize, usize)> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                let n = self.pipeline.layers.len();
                for (i, layer) in self.pipeline.layers.iter_mut().enumerate() {
                    let (_, dropped) = ui.dnd_drop_zone::<usize, ()>(egui::Frame::none(), |ui| {
                    ui.push_id(i, |ui| {
                        ui.horizontal(|ui| {
                            ui.dnd_drag_source(egui::Id::new(("drag", i)), i, |ui| {
                                ui.label("Drag");
                            });
                            changed |= ui.checkbox(&mut layer.enabled, "").changed();
                            ui.label(format!("{}. {}", i + 1, step_name(&layer.step)));
                            if ui.add_enabled(i > 0, egui::Button::new("Up").small()).clicked() {
                                action = Some((i, -1));
                            }
                            if ui.add_enabled(i + 1 < n, egui::Button::new("Down").small()).clicked() {
                                action = Some((i, 1));
                            }
                            if ui.small_button("Remove").clicked() {
                                action = Some((i, 0));
                            }
                        });
                        egui::CollapsingHeader::new("Settings").id_source(("settings", i)).default_open(true).show(ui, |ui| {
                            changed |= ui.add(egui::Slider::new(&mut layer.opacity, 0.0..=1.0).text("Opacity")).changed();
                            changed |= step_ui(ui, &mut layer.step);
                        });
                        ui.separator();
                    });
                    });
                    if let Some(from) = dropped {
                        reorder = Some((*from, i));
                    }
                }
            });
            if let Some((from, to)) = reorder {
                if from != to && from < self.pipeline.layers.len() && to < self.pipeline.layers.len() {
                    let l = self.pipeline.layers.remove(from);
                    self.pipeline.layers.insert(to, l);
                    changed = true;
                }
            }
            if let Some((i, a)) = action {
                match a {
                    -1 => self.pipeline.layers.swap(i, i - 1),
                    1 => self.pipeline.layers.swap(i, i + 1),
                    _ => {
                        self.pipeline.layers.remove(i);
                    }
                }
                changed = true;
            }
        });

        if changed {
            self.dirty = true;
            self.snap_pending = true;
        }
        if self.snap_pending && !ctx.input(|i| i.pointer.any_down()) {
            self.snap_pending = false;
            if self.history.get(self.hist_pos) != Some(&self.pipeline) {
                self.history.truncate(self.hist_pos + 1);
                self.history.push(self.pipeline.clone());
                if self.history.len() > 100 {
                    self.history.remove(0);
                }
                self.hist_pos = self.history.len() - 1;
            }
        }
        if self.dirty && !self.busy {
            self.start_preview(ctx);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(main) = self.tex_result.clone().or(self.tex_orig.clone()) else {
                ui.centered_and_justified(|ui| {
                    ui.label("Drop an image here, or use Browse");
                });
                return;
            };
            let orig = self.tex_orig.clone().unwrap_or(main.clone());
            let size = main.size_vec2();
            let avail = ui.available_rect_before_wrap();
            let resp = ui.allocate_rect(avail, egui::Sense::click_and_drag());
            let mut fit = (avail.width() / size.x).min(avail.height() / size.y).max(0.01);
            if fit > 1.0 {
                fit = fit.floor();
            }
            let mut scale = if self.zoom <= 0.0 { fit } else { self.zoom };
            if self.zoom <= 0.0 {
                self.pan = egui::Vec2::ZERO;
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if resp.hovered() && scroll != 0.0 {
                let new = (scale * (1.0 + scroll * 0.0025)).clamp(0.05, 64.0);
                if let Some(p) = resp.hover_pos() {
                    let center = avail.center() + self.pan;
                    let new_center = p - (p - center) * (new / scale);
                    self.pan = new_center - avail.center();
                }
                self.zoom = new;
                scale = new;
            }
            if resp.dragged_by(egui::PointerButton::Primary) {
                self.pan += resp.drag_delta();
                if self.zoom <= 0.0 {
                    self.zoom = scale;
                }
            }
            if resp.double_clicked() {
                self.zoom = 0.0;
                self.pan = egui::Vec2::ZERO;
                scale = fit;
            }
            let rect = egui::Rect::from_center_size(avail.center() + self.pan, size * scale);
            let painter = ui.painter_at(avail);
            let white = egui::Color32::WHITE;
            let uv = |x0: f32, x1: f32| egui::Rect::from_min_max(egui::pos2(x0, 0.0), egui::pos2(x1, 1.0));
            if self.split_view {
                let sx = rect.left() + self.split * rect.width();
                painter.image(orig.id(), egui::Rect::from_min_max(rect.min, egui::pos2(sx, rect.max.y)), uv(0.0, self.split), white);
                painter.image(main.id(), egui::Rect::from_min_max(egui::pos2(sx, rect.min.y), rect.max), uv(self.split, 1.0), white);
                painter.line_segment(
                    [egui::pos2(sx, rect.min.y), egui::pos2(sx, rect.max.y)],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 180, 0)),
                );
                let handle = egui::Rect::from_center_size(egui::pos2(sx, rect.center().y), egui::vec2(18.0, rect.height().max(40.0)));
                let hr = ui.interact(handle, ui.id().with("split_handle"), egui::Sense::drag());
                if hr.dragged() {
                    self.split = (self.split + hr.drag_delta().x / rect.width().max(1.0)).clamp(0.0, 1.0);
                }
                if hr.hovered() || hr.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
            } else {
                let t = if self.show_original { &orig } else { &main };
                painter.image(t.id(), rect, uv(0.0, 1.0), white);
            }
        });
    }
}

struct App {
    tabs: Vec<Tab>,
    active: usize,
}

impl App {
    fn new_tab(&mut self) {
        self.tabs.push(Tab::new());
        self.active = self.tabs.len() - 1;
    }

    fn duplicate_tab(&mut self) {
        let cur = &self.tabs[self.active];
        let mut t = Tab::new();
        t.pipeline = cur.pipeline.clone();
        t.history = vec![cur.pipeline.clone()];
        t.source = cur.source.clone();
        t.preview_src = cur.preview_src.clone();
        t.preview_factor = cur.preview_factor;
        t.tex_orig = cur.tex_orig.clone();
        t.tex_result = cur.tex_result.clone();
        t.video = cur.video.clone();
        t.in_path = cur.in_path.clone();
        t.out_path = cur.out_path.clone();
        t.crf = cur.crf;
        t.hold = cur.hold;
        t.status = "Duplicated tab".into();
        t.dirty = t.source.is_some();
        self.tabs.insert(self.active + 1, t);
        self.active += 1;
    }

    fn close_tab(&mut self, i: usize) {
        if self.tabs.len() <= 1 {
            self.tabs[0] = Tab::new();
            self.active = 0;
            return;
        }
        self.tabs.remove(i);
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for t in self.tabs.iter_mut() {
            t.poll(ctx);
            if t.dirty && !t.busy {
                t.start_preview(ctx);
            }
        }

        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for path in dropped {
            if !self.tabs[self.active].is_empty() {
                self.new_tab();
            }
            let p = path.to_string_lossy().to_string();
            let a = self.active;
            self.tabs[a].load(ctx, &p);
        }

        if !ctx.wants_keyboard_input() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::T)) {
                self.new_tab();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)) {
                let a = self.active;
                self.close_tab(a);
            }
        }

        let mut select = None;
        let mut close = None;
        let (mut add, mut dup) = (false, false);
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, t) in self.tabs.iter().enumerate() {
                    if ui.selectable_label(i == self.active, t.title()).clicked() {
                        select = Some(i);
                    }
                    if ui.small_button("x").on_hover_text("Close tab (Ctrl+W)").clicked() {
                        close = Some(i);
                    }
                    ui.add_space(6.0);
                }
                if ui.button("+ New tab").on_hover_text("Ctrl+T").clicked() {
                    add = true;
                }
                if ui.button("Duplicate tab").clicked() {
                    dup = true;
                }
            });
        });
        if let Some(i) = select {
            self.active = i;
        }
        if add {
            self.new_tab();
        }
        if dup {
            self.duplicate_tab();
        }
        if let Some(i) = close {
            self.close_tab(i);
        }

        let a = self.active;
        self.tabs[a].ui(ctx);
    }
}

fn main() -> Result<(), eframe::Error> {
    let icon = image::load_from_memory(include_bytes!("../../assets/dotwise_256.png")).ok().map(|i| {
        let r = i.to_rgba8();
        egui::IconData { width: r.width(), height: r.height(), rgba: r.into_raw() }
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 820.0])
        .with_drag_and_drop(true)
        .with_app_id("dotwise");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let arg = std::env::args().nth(1);
    eframe::run_native(
        "Dotwise",
        options,
        Box::new(move |cc| {
            let mut app = App { tabs: vec![Tab::new()], active: 0 };
            if let Some(p) = arg {
                app.tabs[0].load(&cc.egui_ctx, &p);
            }
            Box::new(app)
        }),
    )
}
