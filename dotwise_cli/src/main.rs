use std::process::exit;
use dotwise_core::{presets, BlendMode, Composite, Image, KeyMode, Pipeline};

const HELP: &str = "dotwise: image dithering with a stackable effects pipeline

usage:
  dotwise INPUT OUTPUT --preset NAME     (OUTPUT may be .png, .jpg or .svg)
  dotwise INPUT OUTPUT --pipeline FILE.json
  dotwise video INPUT OUTPUT (--preset NAME | --pipeline FILE)   (OUTPUT .mp4, .mov, .mkv or .webm) [--fps N] [--max-frames N] [--crf N] [--animate] [--hold N] [--overlay MODE] [--overlay-opacity N] [--key light|dark]
  dotwise batch IN_DIR OUT_DIR (--preset NAME | --pipeline FILE)   (png and jpg files, writes NAME_dotwise.png)
  dotwise --list                list built in presets
  dotwise --dump NAME           print a preset as JSON (edit it, then use --pipeline)";

fn die(msg: &str) -> ! {
    eprintln!("error: {}", msg);
    exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        println!("{}", HELP);
        return;
    }
    if args[0] == "video" {
        run_video(&args[1..]);
        return;
    }
    if args[0] == "batch" {
        run_batch(&args[1..]);
        return;
    }
    if args[0] == "--list" {
        for (name, _) in presets::builtin() { println!("{}", name); }
        return;
    }
    if args[0] == "--dump" {
        let name = args.get(1).map(String::as_str).unwrap_or("");
        match presets::find(name) {
            Some(p) => println!("{}", p.to_json()),
            None => die("unknown preset, see --list"),
        }
        return;
    }
    if args.len() < 4 { die("expected INPUT OUTPUT and --preset or --pipeline"); }
    let (input, output, mode, value) = (&args[0], &args[1], args[2].as_str(), &args[3]);
    let pipeline = match mode {
        "--preset" => presets::find(value).unwrap_or_else(|| die("unknown preset, see --list")),
        "--pipeline" => {
            let text = std::fs::read_to_string(value).unwrap_or_else(|e| die(&format!("cannot read {}: {}", value, e)));
            Pipeline::from_json(&text).unwrap_or_else(|e| die(&format!("invalid pipeline: {}", e)))
        }
        _ => die("expected --preset or --pipeline"),
    };
    let rgb = image::open(input).unwrap_or_else(|e| die(&format!("cannot open {}: {}", input, e))).to_rgb8();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    let out = pipeline.run(&Image::from_rgb8(w, h, rgb.as_raw()));
    if output.to_ascii_lowercase().ends_with(".svg") {
        let svg = dotwise_core::svg::to_svg(&out, 1).unwrap_or_else(|e| die(&e));
        std::fs::write(output, svg).unwrap_or_else(|e| die(&format!("cannot write {}: {}", output, e)));
        return;
    }
    image::save_buffer(output, &out.to_rgb8(), out.w as u32, out.h as u32, image::ColorType::Rgb8)
        .unwrap_or_else(|e| die(&format!("cannot write {}: {}", output, e)));
}

fn run_video(a: &[String]) {
    if a.len() < 4 { die("usage: dotwise video INPUT OUTPUT --preset NAME [--fps N] [--max-frames N] [--crf N] [--animate] [--hold N] [--overlay MODE] [--overlay-opacity N] [--key light|dark]"); }
    let (input, output) = (&a[0], &a[1]);
    let mut pipeline: Option<Pipeline> = None;
    let mut opts = dotwise_video::ExportOptions::default();
    let mut animate = false;
    let mut overlay = Composite { enabled: false, ..Composite::default() };
    let mut overlay_set = false;
    let mut i = 2;
    while i < a.len() {
        let val = |i: usize| a.get(i + 1).unwrap_or_else(|| die("missing value after option"));
        match a[i].as_str() {
            "--preset" => { pipeline = Some(presets::find(val(i)).unwrap_or_else(|| die("unknown preset, see --list"))); i += 2; }
            "--pipeline" => {
                let text = std::fs::read_to_string(val(i)).unwrap_or_else(|e| die(&format!("cannot read pipeline: {}", e)));
                pipeline = Some(Pipeline::from_json(&text).unwrap_or_else(|e| die(&format!("invalid pipeline: {}", e))));
                i += 2;
            }
            "--fps" => { opts.fps = Some(val(i).parse().unwrap_or_else(|_| die("bad --fps"))); i += 2; }
            "--max-frames" => { opts.max_frames = Some(val(i).parse().unwrap_or_else(|_| die("bad --max-frames"))); i += 2; }
            "--crf" => { opts.crf = val(i).parse().unwrap_or_else(|_| die("bad --crf")); i += 2; }
            "--animate" => { animate = true; i += 1; }
            "--hold" => { opts.hold = val(i).parse().unwrap_or_else(|_| die("bad --hold")); i += 2; }
            "--overlay" => {
                overlay.enabled = true;
                overlay.mode = match val(i).to_ascii_lowercase().as_str() {
                    "normal" => BlendMode::Normal,
                    "multiply" => BlendMode::Multiply,
                    "screen" => BlendMode::Screen,
                    "overlay" => BlendMode::Overlay,
                    "add" => BlendMode::Add,
                    _ => die("--overlay takes normal, multiply, screen, overlay or add"),
                };
                overlay_set = true;
                i += 2;
            }
            "--overlay-opacity" => { overlay.opacity = val(i).parse().unwrap_or_else(|_| die("bad --overlay-opacity")); i += 2; }
            "--key" => {
                overlay.key = match val(i).to_ascii_lowercase().as_str() {
                    "light" => KeyMode::DropLight,
                    "dark" => KeyMode::DropDark,
                    "none" => KeyMode::None,
                    _ => die("--key takes light, dark or none"),
                };
                i += 2;
            }
            other => die(&format!("unknown option {}", other)),
        }
    }
    let mut pipeline = pipeline.unwrap_or_else(|| die("expected --preset or --pipeline"));
    if animate { pipeline.animate_noise = true; }
    if overlay_set || overlay.key != KeyMode::None { overlay.enabled = true; pipeline.composite = overlay; }
    let normalized = dotwise_video::normalize_output(output);
    if normalized != *output { eprintln!("note: video output must be mp4, mov, mkv or webm, writing {}", normalized); }
    let res = dotwise_video::export(input, output, &pipeline, &opts, &mut |done, total| {
        eprint!("\rframe {}/{}", done, total);
        true
    });
    eprintln!();
    match res {
        Ok(path) => println!("wrote {}", path),
        Err(e) => die(&e),
    }
}

fn run_batch(a: &[String]) {
    if a.len() < 4 { die("usage: dotwise batch IN_DIR OUT_DIR --preset NAME"); }
    let pipeline = match a[2].as_str() {
        "--preset" => presets::find(&a[3]).unwrap_or_else(|| die("unknown preset, see --list")),
        "--pipeline" => {
            let text = std::fs::read_to_string(&a[3]).unwrap_or_else(|e| die(&format!("cannot read pipeline: {}", e)));
            Pipeline::from_json(&text).unwrap_or_else(|e| die(&format!("invalid pipeline: {}", e)))
        }
        _ => die("expected --preset or --pipeline"),
    };
    let res = dotwise_video::batch::process_folder(std::path::Path::new(&a[0]), std::path::Path::new(&a[1]), &pipeline, &mut |d, t| {
        eprint!("\r{}/{}", d, t);
        true
    });
    eprintln!();
    match res {
        Ok((done, failed)) => println!("done {} files, {} failed", done, failed),
        Err(e) => die(&e),
    }
}
