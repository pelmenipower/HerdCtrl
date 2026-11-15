mod core;
mod os;
mod tray;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--identify-monitors") {
        let raw = std::env::args()
            .nth(2)
            .ok_or("Missing monitor identification data")?;
        return ui::identify::run_helper(&raw);
    }
    if std::env::args().nth(1).as_deref() == Some("--tray") {
        return tray::run().map_err(Into::into);
    }
    // Editor only, and before any thread exists: it sets variables the Vulkan loader reads.
    ui::renderer::prepare();
    let Some(_editor) = tray::lock(tray::EDITOR_LOCK)? else {
        eprintln!("The HerdCtrl editor is already open");
        return Ok(());
    };
    // The tray owns the global hotkeys, so they keep working after the editor closes.
    let tray = tray::ensure_running().err();
    let path = core::config::Config::path().map_err(std::io::Error::other)?;
    let config = core::config::Config::load(&path).map_err(std::io::Error::other)?;
    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!(
                "../assets/icon.png"
            ))?)
            .with_inner_size([1100.0, 740.0])
            .with_min_inner_size([860.0, 580.0]),
        ..Default::default()
    };
    ui::renderer::configure(&mut options);
    eframe::run_native(
        "HerdCtrl",
        options,
        Box::new(move |cc| {
            Ok(Box::new(ui::dashboard::Dashboard::new(
                cc, config, path, tray,
            )))
        }),
    )
    .map_err(ui::renderer::fall_back_to_glow)
}
