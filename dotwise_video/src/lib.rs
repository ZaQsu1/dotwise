//! Video support for Dotwise. Needs `ffmpeg` and `ffprobe` on PATH.
//! Frames are decoded to raw RGB by ffmpeg, processed in parallel (order preserved),
//! and piped into a second ffmpeg that encodes H.264 and carries over the audio.
pub mod batch;
pub mod hold;

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use dotwise_core::{Image, Pipeline};

#[cfg(test)]
mod tests {
    use super::normalize_output;

    #[test]
    fn mp4_wins_for_anything_that_is_not_a_video_name() {
        assert_eq!(normalize_output("a.mp4"), "a.mp4");
        assert_eq!(normalize_output("a.WEBM"), "a.WEBM");
        assert_eq!(normalize_output("out.png"), "out.mp4");
        assert_eq!(normalize_output("dir/clip.svg"), "dir/clip.mp4");
        assert_eq!(normalize_output("clip"), "clip.mp4");
        assert_eq!(normalize_output(""), "out.mp4");
    }
}

#[derive(Clone, Debug)]
pub struct VideoInfo {
    pub w: usize,
    pub h: usize,
    pub fps: f64,
    pub duration: f64,
}

#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub fps: Option<f64>,
    pub max_frames: Option<u64>,
    pub crf: u32,
    pub threads: usize,
    /// Temporal hold threshold (0 disables). Typical values 0.02 to 0.1.
    pub hold: f32,
}

impl Default for ExportOptions {
    fn default() -> Self {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        Self { fps: None, max_frames: None, crf: 14, threads, hold: 0.0 }
    }
}

/// Video output always ends up as mp4, mov, mkv or webm: anything else gets the extension replaced by mp4.
pub fn normalize_output(output: &str) -> String {
    let lower = output.to_ascii_lowercase();
    if [".mp4", ".mov", ".mkv", ".webm"].iter().any(|e| lower.ends_with(e)) {
        return output.to_string();
    }
    if output.trim().is_empty() {
        return "out.mp4".into();
    }
    let p = std::path::Path::new(output);
    if p.extension().is_some() {
        p.with_extension("mp4").to_string_lossy().to_string()
    } else {
        format!("{}.mp4", output)
    }
}

pub fn is_video_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [".mp4", ".mov", ".mkv", ".webm", ".avi", ".m4v", ".gif"].iter().any(|e| p.ends_with(e))
}

fn tool_missing(name: &str, e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!("{} not found. Install ffmpeg and make sure it is on PATH.", name)
    } else {
        format!("cannot run {}: {}", name, e)
    }
}

fn parse_rate(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once('/') {
        let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
        if b > 0.0 { Some(a / b) } else { None }
    } else {
        s.parse().ok()
    }
}

/// Reads size from a decoded frame (so rotation metadata is already applied) and timing from ffprobe.
pub fn probe(path: &str) -> Result<VideoInfo, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=r_frame_rate,duration", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1", path])
        .output()
        .map_err(|e| tool_missing("ffprobe", e))?;
    if !out.status.success() {
        return Err(format!("ffprobe failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut fps = None;
    let mut duration = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("r_frame_rate=") {
            fps = fps.or_else(|| parse_rate(v));
        } else if let Some(v) = line.strip_prefix("duration=") {
            duration = duration.or_else(|| v.trim().parse::<f64>().ok());
        }
    }
    let first = grab_frame(path, 0.0)?;
    Ok(VideoInfo { w: first.w, h: first.h, fps: fps.filter(|f| *f > 0.0).unwrap_or(30.0), duration: duration.unwrap_or(0.0) })
}

/// Decodes one frame at `seconds` (rotation applied).
pub fn grab_frame(path: &str, seconds: f64) -> Result<Image, String> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{:.3}", seconds.max(0.0)), "-i", path, "-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| tool_missing("ffmpeg", e))?;
    if out.stdout.is_empty() {
        return Err(format!("could not decode a frame: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let img = image::load_from_memory(&out.stdout).map_err(|e| e.to_string())?.to_rgb8();
    Ok(Image::from_rgb8(img.width() as usize, img.height() as usize, img.as_raw()))
}

fn read_frame(r: &mut impl Read, buf: &mut [u8]) -> Result<bool, String> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..]) {
            Ok(0) => return Ok(false),
            Ok(n) => got += n,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(true)
}

fn kill(c: &mut Child) {
    let _ = c.kill();
    let _ = c.wait();
}

/// Runs `pipeline` over every frame. `progress(done, total)` returns false to cancel.
pub fn export(
    input: &str,
    output: &str,
    pipeline: &Pipeline,
    opts: &ExportOptions,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<String, String> {
    let output_owned = normalize_output(output);
    let output = output_owned.as_str();
    let info = probe(input)?;
    let fps = opts.fps.filter(|f| *f > 0.0).unwrap_or(info.fps);
    let (w, h) = (info.w, info.h);
    let first = grab_frame(input, 0.0)?;
    let probe_out = pipeline.run_frame(&first, 0);
    let (ow, oh) = (probe_out.w, probe_out.h);
    let mut total = (info.duration * fps).round().max(1.0) as u64;
    if let Some(m) = opts.max_frames {
        total = total.min(m);
    }

    let mut dec_cmd = Command::new("ffmpeg");
    dec_cmd.args(["-v", "error", "-i", input]);
    if opts.fps.is_some() {
        dec_cmd.args(["-vf", &format!("fps={}", fps)]);
    }
    let mut dec = dec_cmd
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| tool_missing("ffmpeg", e))?;

    let ext = output.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let crf = opts.crf.to_string();
    let (vargs, aargs): (Vec<&str>, Vec<&str>) = if ext == "webm" {
        (
            vec!["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", crf.as_str(), "-pix_fmt", "yuv420p"],
            vec!["-c:a", "libopus", "-b:a", "160k"],
        )
    } else {
        (
            vec!["-c:v", "libx264", "-preset", "medium", "-crf", crf.as_str(), "-pix_fmt", "yuv420p"],
            vec!["-c:a", "aac", "-b:a", "192k"],
        )
    };
    let mut enc = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{}x{}", ow, oh), "-r", &format!("{}", fps), "-i", "-"])
        .args(["-i", input, "-map", "0:v:0", "-map", "1:a:0?"])
        .args(["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"])
        .args(&vargs)
        .args(&aargs)
        .args(["-shortest", output])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            kill(&mut dec);
            tool_missing("ffmpeg", e)
        })?;

    let mut dec_out = dec.stdout.take().unwrap();
    let mut enc_in = enc.stdin.take().unwrap();
    let enc_err = enc.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let mut r = enc_err;
        let _ = r.read_to_string(&mut s);
        s
    });

    let fsize = w * h * 3;
    let threads = opts.threads.max(1);
    let mut buf = vec![0u8; fsize * threads];
    let mut done: u64 = 0;
    let mut result: Result<(), String> = Ok(());
    let mut ref_src: Option<Vec<u8>> = None;
    let mut prev_out: Vec<u8> = Vec::new();

    'outer: while done < total {
        let want = threads.min((total - done) as usize);
        let mut n = 0;
        while n < want {
            match read_frame(&mut dec_out, &mut buf[n * fsize..(n + 1) * fsize]) {
                Ok(true) => n += 1,
                Ok(false) => break,
                Err(e) => {
                    result = Err(e);
                    break 'outer;
                }
            }
        }
        if n == 0 {
            break;
        }
        let outs: Vec<Vec<u8>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..n)
                .map(|i| {
                    let chunk = &buf[i * fsize..(i + 1) * fsize];
                    let idx = (done + i as u64) as u32;
                    s.spawn(move || pipeline.run_frame(&Image::from_rgb8(w, h, chunk), idx).to_rgb8())
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for (i, mut o) in outs.into_iter().enumerate() {
            if opts.hold > 0.0 && o.len() == ow * oh * 3 {
                let src = &buf[i * fsize..(i + 1) * fsize];
                match &mut ref_src {
                    None => ref_src = Some(src.to_vec()),
                    Some(r) => hold::apply(src, w, h, &mut o, ow, oh, r, &prev_out, opts.hold),
                }
                prev_out = o.clone();
            }
            if o.len() != ow * oh * 3 {
                result = Err("pipeline output size changed between frames".into());
                break 'outer;
            }
            if let Err(e) = enc_in.write_all(&o) {
                result = Err(format!("encoder closed early: {}", e));
                break 'outer;
            }
        }
        done += n as u64;
        if !progress(done, total) {
            result = Err("cancelled".into());
            break;
        }
        if n < want {
            break;
        }
    }

    drop(enc_in);
    kill(&mut dec);
    let status = enc.wait().map_err(|e| e.to_string())?;
    let enc_msg = err_thread.join().unwrap_or_default();
    result?;
    if !status.success() {
        return Err(format!("encoder failed: {}", enc_msg.trim()));
    }
    Ok(output_owned)
}
