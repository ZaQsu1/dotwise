<div align="center">

  <img src="assets/dotwise-baner.png" alt="Dotwise banner" width="50%">

  # dotwise

  **A flexible image dithering app for pixel textures, print-inspired graphics, and experimental visuals.**

</div>

---

Dotwise is a dithering application built in Rust. It combines a desktop GUI with a command-line interface, an ordered effects pipeline, and support for processing images, folders, and video.

Create a look with a built-in preset, then refine it by stacking and adjusting effects. Pipelines can be saved as JSON and reused from the GUI or CLI.

## Features

* **Multiple dithering methods:** error diffusion, Bayer ordered dithering, halftone, line patterns, white noise, gradient noise, and blue noise.
* **Flexible palettes:** monochrome, grayscale, duotone, CGA, Game Boy inspired green, custom colours, and palettes extracted from an image.
* **Layer based pipeline:** combine dithering with effects such as downscaling, upscaling, posterization, gradient mapping, sharpening, scanlines, glow, row shifts, chromatic aberration, and JPEG style glitches.
* **Live desktop preview:** inspect results, zoom and pan, compare the original with the result, and undo or redo changes.
* **Presets:** start with one of the included styles, including Classic mono, Compact Mac, Handheld green, Newsprint, Duotone, Blue noise, and Glow glitch.
* **Batch processing:** apply a preset or JSON pipeline to a folder of images.
* **Video processing:** preview and export supported video formats, with optional animated noise and temporal hold.
* **Reusable pipelines:** save and load processing settings as JSON.
* **Export options:** images can be exported as PNG, JPEG, or SVG. Video export supports MP4, MOV, MKV, and WebM.

## Project components

| Component | Description |
| --- | --- |
| `dotwise_core` | Dithering algorithms, palettes, effects, presets, and pipeline processing |
| `dotwise_video` | Video frame processing and export helpers |
| `dotwise_gui` | Desktop application built with `egui` and `eframe` |
| `dotwise_cli` | Command line interface for images, video, and batch jobs |

## Getting started

### Requirements

* A recent stable Rust toolchain with Cargo.
* Rust 1.85 or newer is recommended for the workspace's dependency and edition requirements.
* `ffmpeg` and `ffprobe` on `PATH` for video processing.

### Build and run

Clone the repository, then run the desktop application:

```bash
cargo run --release -p dotwise_gui
```

Open an image directly by passing its path:

```bash
cargo run --release -p dotwise_gui -- ./image.png
```

List available presets:

```bash
cargo run --release -p dotwise_cli -- --list
```

Build the GUI and CLI binaries:

```bash
cargo build --release -p dotwise_gui -p dotwise_cli
```

The resulting binaries are placed in `target/release`.

## Command line usage

### Process an image

```bash
dotwise input.png output.png --preset "Classic mono"
```

The output extension determines the image format. SVG output is also supported:

```bash
dotwise input.png output.svg --preset "Newsprint"
```

### Use a custom pipeline

Dump a built in preset to JSON, edit it, and use it for processing:

```bash
dotwise --dump "Duotone" > duotone.json
dotwise input.png output.png --pipeline duotone.json
```

### Batch process a folder

```bash
dotwise batch ./input ./output --preset "Handheld green"
```

Batch mode supports PNG and JPEG input files.

### Process video

```bash
dotwise video input.mp4 output.mp4 --preset "Blue noise"
```

Some additional options are available for video processing:

```bash
dotwise video input.mp4 output.mp4 --preset "Classic mono" --fps 24 --max-frames 500
```

For the full list of options:

```bash
dotwise --help
dotwise video --help
dotwise batch --help
```

## How the pipeline works

A pipeline is an ordered sequence of processing steps. Each step operates on the result produced by the preceding step, and individual layers can be enabled or disabled. This makes it possible to combine resizing, colour adjustments, dithering, and finishing effects in one reusable setup.

The desktop preview is scaled down when the longest image dimension exceeds 2048 pixels. Export uses the full resolution of the source image.

## Project status

Dotwise is under active development. Some features and edge cases may not yet be fully tested.

## Contributing

Bug reports, feature suggestions, and contributions are welcome. When reporting an issue, include your operating system, the steps to reproduce it, and a sample image or pipeline JSON when possible.

## AI was used in this project

## License

Dotwise is licensed under the MIT License. See [`LICENSE`](LICENSE) for details.
