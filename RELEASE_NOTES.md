# Dotwise — release notes

## Highlights

- ASCII art effect for image and video frames
- GIF input/output support in the workflow and file pickers
- Full image pipeline with layers, blend modes, adjustments, blur, dither, glow, scanlines, posterize and more
- Video export for MP4/WEBM/MOV/MKV/AVI/GIF-style targets
- Windows desktop app with installer
- Rust core tests and frontend build validation

## Included changes

- Added ASCII render step to the Rust image pipeline
- Added GUI controls for ASCII density / cell size / color mode
- Kept output size stable while rendering glyph cells
- Added GIF detection to file filters and media handling
- Preserved Windows installer packaging and release artifact generation

## Validation

Verified in this workspace:

- `cargo test --workspace` — passed
- `npm run build` — passed
- Windows EXE and Setup EXE generated under `target/release/`

## Windows artifacts

- `target/release/dotwise_gui.exe`
- `target/release/Dotwise-Setup-0.1.0-x64.exe`

## GitHub release checklist

1. Push the branch to GitHub.
2. Open Releases > Draft a new release.
3. Use a title like: `Dotwise 0.1.0 — ASCII + GIF support`
4. Attach the Windows binaries:
   - `dotwise_gui.exe`
   - `Dotwise-Setup-0.1.0-x64.exe`
5. Paste the summary below into the release body.

## Suggested release body

### Dotwise 0.1.0

This release adds the ASCII art effect, GIF support, and more polish to the image/video editor.

Features:
- ASCII art filter for still images and video frames
- GIF-aware file filters and media handling
- Improved dark UI styling and focus visibility
- Windows desktop build and installer packaging
- Rust core validation and frontend production build

### Notes

- Windows release is verified in this environment.
- Linux packaging is not yet finalized in this session because the WSL/Tauri builder hit environment memory limits.

## Build commands

### Windows

```powershell
cargo test --workspace
npm run build
npx tauri build --no-bundle
& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" /O'target\release' 'instalator\Dotwise.iss'
```

### Linux

The project is ready for Linux packaging in a more resource-rich environment, but this local session did not complete a final `.deb` bundle due WSL/Tauri memory constraints.
