mod core;
mod os;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--identify-monitors") {
        let raw = std::env::args()
            .nth(2)
            .ok_or("Missing monitor identification data")?;
        return ui::identify::run_helper(&raw);
    }
    let path = core::config::Config::path().map_err(std::io::Error::other)?;
    let config = core::config::Config::load(&path).map_err(std::io::Error::other)?;
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!(
                "../assets/icon.png"
            ))?)
            .with_inner_size([1100.0, 740.0])
            .with_min_inner_size([860.0, 580.0]),
        ..Default::default()
    };
    eframe::run_native(
        "HerdCtrl",
        options,
        Box::new(move |cc| Ok(Box::new(ui::dashboard::Dashboard::new(cc, config, path)))),
    )
    .map_err(Into::into)
}
