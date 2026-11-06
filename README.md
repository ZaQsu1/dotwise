# Dotwise

Dotwise is a creative image and video processor focused on stylized visual output: dithering, ASCII art, palettes, glow, scanlines, glitching, and export for stills and motion.

## Highlights

- Stackable image pipeline with adjustable layers and blend modes
- ASCII art effect for images and video frames
- GIF input and export support in the desktop workflow
- Dithering styles: error diffusion, Bayer, halftone, lines, blue noise, white noise, and more
- Palette tools: built-in, grayscale, duotone, Game Boy, and custom palette extraction
- Video preview and export via ffmpeg
- Windows desktop app with bundled WebView2 installer

## Features

- Adjustable image effects: brightness, contrast, gamma, saturation, blur, sharpen, glow, scanlines, invert, posterize, gradient map
- Noise and glitch layers with temporal animation support
- SVG export and raster export for PNG/JPEG
- Multi-format media picker: PNG, JPG, JPEG, SVG, GIF, MP4, MOV, MKV, WEBM, AVI, M4V
- Batch processing support for folders
- Live preview with original/result comparison and layer controls

## Requirements

- Rust stable (Cargo)
- Node.js 20+ and npm
- ffmpeg and ffprobe on PATH for video import/export
- Windows: Microsoft Edge WebView2 Runtime
- Linux: WebKitGTK 4.1 and Tauri build dependencies

Ubuntu/Debian example:

```bash
sudo apt install -y build-essential curl wget file pkg-config libssl-dev libgtk-3-dev librsvg2-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev patchelf
```

## Quick start

Install frontend dependencies and run the desktop app:

```bash
npm ci
npm run tauri dev
```

Build the production app:

```bash
npm run build
cargo test --workspace
```

For a Windows release executable:

```powershell
npx tauri build --no-bundle
```

Then build the installer using Inno Setup 6:

```powershell
& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" /O'target\release' 'instalator\Dotwise.iss'
```

## CLI usage

Build the CLI:

```bash
cargo build --release -p dotwise_cli
```

Process an image with a built-in preset:

```bash
cargo run --release -p dotwise_cli -- input.png output.png --preset "Classic mono"
cargo run --release -p dotwise_cli -- input.png output.svg --preset "Newsprint"
```

Process a whole folder:

```bash
cargo run --release -p dotwise_cli -- batch ./input ./output --preset "Handheld green"
```

Process a video:

```bash
cargo run --release -p dotwise_cli -- video input.mp4 output.mp4 --preset "Blue noise"
```

See all CLI flags:

```bash
cargo run --release -p dotwise_cli -- --help
```

## Project layout

- `dotwise_core` — image effects, pipeline, palettes, SVG output
- `dotwise_video` — frame extraction, video export, batch processing
- `dotwise_cli` — command-line interface
- `dotwise_gui` — Tauri desktop app and React UI

## License

Dotwise is licensed under the MIT License. See [LICENSE](LICENSE).

## Release notes

See [RELEASE_NOTES.md](RELEASE_NOTES.md) for the current release summary and GitHub release checklist.
