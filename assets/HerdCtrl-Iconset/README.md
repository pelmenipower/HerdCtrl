# HerdCtrl Iconset

Final selected HerdCtrl artwork (Bernese Mountain Dog with headset and glasses, in front of a tiled app-window layout), exported from the image created in this conversation.

## Files

- `source/herdctrl-original.png`: completely unchanged generated artwork, 1254×1254 (includes the dark background outside the icon).
- `source/herdctrl-master-transparent.png`: extracted 1084×1084 app badge, corners made transparent for use as an icon.
- `herdctrl.png`: ready-to-use 512×512 RGBA icon.
- `png/herdctrl-{size}.png`: sizes 16, 20, 24, 32, 40, 48, 64, 72, 96, 128, 144, 192, 256, 384, 512, 1024.
- `herdctrl.ico`: Windows ICO with embedded sizes from 16 to 256 px.
- `herdctrl.icns`: macOS ICNS file.
- `linux/hicolor/...`: themed Linux icon directories.
- `linux/herdctrl.desktop`: example Ubuntu app launcher. `Exec=herdctrl` assumes the executable is in your PATH.

## Rust / egui / eframe

Copy `herdctrl.png` to your project's `assets/icon.png`.

```rust
use eframe::egui;

let icon = eframe::icon_data::from_png_bytes(
    include_bytes!("../assets/icon.png")
).expect("HerdCtrl icon PNG could not be loaded");

let options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default().with_icon(icon),
    ..Default::default()
};
```

The relative `include_bytes!` path is appropriate for a `src/main.rs` file and an `assets/` directory at the project root. Adapt it if your project structure differs. `with_icon` is supported on applicable desktop platforms/window systems; Ubuntu Wayland desktop integration may also depend on `.desktop` and application IDs.

## Linux installation example

From this package directory:

```bash
mkdir -p ~/.local/share/icons/hicolor ~/.local/share/applications
cp -r linux/hicolor/* ~/.local/share/icons/hicolor/
cp linux/herdctrl.desktop ~/.local/share/applications/
```

Set `Exec=` in the desktop file to the actual installed HerdCtrl binary/path if necessary. Use `herdctrl.png` for app embedding or release packaging.

## Codex instruction

> Use the supplied HerdCtrl icon package. Copy `herdctrl.png` to `assets/icon.png`, embed it into the eframe app via `ViewportBuilder::with_icon`, and package the appropriate Linux icons and `.desktop` entry. Do not replace or regenerate the image. Verify relative paths and the actual Rust crate API/version.

Note: this is a raster iconset; no true editable SVG/vector asset is included. Tiny icons are resized from the original and may be visually dense at 16px.
