# Screenshot gallery

The images in this folder are rendered from the real editor code (`Dashboard::ui`) with `egui_kittest` and a wgpu adapter. The test sets fixed sample data: four monitors (one with a reserved top bar and dock), two views, six applications and a fixed autostart state. No live desktop is captured, so the images contain no personal windows, paths or layouts. All images are 1280 × 800 pixels, the editor's size on a typical laptop screen.

Last generated on 2026-11-10 from branch `feature/initial-app` (HerdCtrl 0.1.0, unreleased).

| File | State | Theme |
|---|---|---|
| `screenshot-editor.png` | Editing a view, Ptyxis zone with `herdr` selected | Dark |
| `screenshot-editor-light.png` | Same | Light |
| `screenshot-launching.png` | Launch in progress: 3 of 5 zones placed | Dark |
| `screenshot-launch-failed.png` | Launch finished with one failed zone and **Retry zone** | Light |
| `screenshot-app-picker.png` | Application picker over the canvas | Dark |
| `screenshot-hotkey.png` | Recording a hotkey with unsaved changes | Dark |
| `screenshot-first-run.png` | First start without views | Light |

## Reproduce

Requires a working Vulkan or OpenGL driver (Mesa is enough). No display server is needed.

```sh
cargo test --locked render_docs_screenshots -- --ignored
```

The test overwrites the files listed above. Open every image before committing it: the test only renders and does not compare against stored images. The states are defined in `src/ui/dashboard/snapshots.rs`. To add an image, add a `render_to("docs/images", …)` line there and reference the file from the README and both manuals.

`cargo test --locked render_ui_snapshots -- --ignored` renders every state in both themes to `target/ui-snapshots/` for reviews. Those files are not committed.

## Not covered

The tray menu, desktop notifications and monitor identification labels are drawn by GNOME or by separate windows, not by the editor, so this test cannot render them. Screenshots of those would have to be taken on a real desktop with synthetic layouts and without personal windows.
