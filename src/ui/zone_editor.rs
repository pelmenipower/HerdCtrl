use super::theme::{self, caps};
use crate::core::{applications::InstalledApp, config::Zone, monitor::Monitor};
use eframe::egui::{self, Align2, FontId, Frame, Margin, RichText, Stroke, Vec2};

/// What the zone editor asks the dashboard to do besides editing the zone in place.
#[derive(Default)]
pub struct Outcome {
    pub changed: bool,
    pub delete: bool,
    pub choose_application: bool,
}

/// Shows the properties of the selected zone. `view` and `index` only scope widget ids;
/// `open_advanced` expands the advanced section once, e.g. after "Custom command…".
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    view: usize,
    index: usize,
    zone: &mut Zone,
    monitors: &[Monitor],
    applications: &[InstalledApp],
    open_advanced: bool,
) -> Outcome {
    let p = theme::palette(ui.ctx());
    let mut outcome = Outcome::default();
    let is_terminal = zone.application.as_ref().is_some_and(|selected| {
        applications
            .iter()
            .any(|app| app.application.id == selected.id && app.is_terminal)
    });
    ui.spacing_mut().item_spacing.y = 6.0;

    ui.horizontal(|ui| {
        let (badge, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
        ui.painter().rect_filled(badge, 6.0, p.accent);
        ui.painter().text(
            badge.center(),
            Align2::CENTER_CENTER,
            (index + 1).to_string(),
            FontId::monospace(13.0),
            p.accent_on,
        );
        ui.vertical(|ui| {
            caps(ui, "Zone name");
            outcome.changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut zone.name)
                        .margin(Margin::symmetric(8, 6))
                        .desired_width(f32::INFINITY),
                )
                .changed();
        });
    });
    ui.add_space(10.0);

    caps(ui, "Application");
    Frame::NONE
        .fill(p.raised)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(8)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (tile, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
                ui.painter().rect_filled(tile, 8.0, p.accent_soft);
                let glyph = if is_terminal {
                    ">_".to_string()
                } else {
                    zone.application
                        .as_ref()
                        .and_then(|app| app.name.chars().next())
                        .map_or("?".into(), |c| c.to_uppercase().to_string())
                };
                ui.painter().text(
                    tile.center(),
                    Align2::CENTER_CENTER,
                    glyph,
                    FontId::new(14.0, theme::strong_family()),
                    p.accent,
                );
                let text_width = (ui.available_width() - 76.0).max(40.0);
                ui.allocate_ui(Vec2::new(text_width, 36.0), |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let (name, meta) = match &zone.application {
                        Some(app) => (
                            app.name.as_str(),
                            if is_terminal {
                                "Terminal"
                            } else {
                                "Desktop app"
                            },
                        ),
                        None if !zone.command.trim().is_empty() => {
                            ("Custom command", "Set under Advanced")
                        }
                        None => ("No application", "Choose what opens here"),
                    };
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(theme::strong(name)).truncate());
                        ui.add(
                            egui::Label::new(RichText::new(meta).size(12.0).color(p.muted))
                                .truncate(),
                        );
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let clicked = if zone.application.is_some() {
                        theme::ghost(ui, "Change").clicked()
                    } else {
                        theme::primary(ui, "Choose…").clicked()
                    };
                    outcome.choose_application |= clicked;
                });
            });
        });
    ui.add_space(10.0);

    if is_terminal {
        caps(ui, "Program in terminal");
        outcome.changed |= ui
            .add(
                egui::TextEdit::singleline(&mut zone.terminal_command)
                    .font(egui::TextStyle::Monospace)
                    .margin(Margin::symmetric(8, 7))
                    .desired_width(f32::INFINITY)
                    .hint_text("Optional, e.g. herdr or htop"),
            )
            .changed();
        ui.label(
            RichText::new("Reuses a terminal already running this program.")
                .size(12.0)
                .color(p.muted),
        );
        ui.add_space(10.0);
    }

    caps(ui, "Monitor");
    let current = monitors
        .iter()
        .enumerate()
        .find(|(_, monitor)| monitor.name == zone.monitor);
    egui::ComboBox::from_id_salt((view, index, "monitor"))
        .width(ui.available_width())
        .selected_text(
            current
                .map(|(number, monitor)| monitor_label(number, monitor))
                .unwrap_or_else(|| format!("Disconnected · {}", zone.monitor)),
        )
        .show_ui(ui, |ui| {
            for (number, monitor) in monitors.iter().enumerate() {
                outcome.changed |= ui
                    .selectable_value(
                        &mut zone.monitor,
                        monitor.name.clone(),
                        monitor_label(number, monitor),
                    )
                    .changed();
            }
        });
    ui.add_space(10.0);

    caps(ui, "Geometry · % of work area");
    let bounds = &mut zone.bounds;
    let percent = |value: f64, _: std::ops::RangeInclusive<usize>| format!("{:.0}%", value * 100.0);
    let parse = |text: &str| {
        text.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f64>()
            .ok()
            .map(|value| value / 100.0)
    };
    let current_bounds = *bounds;
    ui.columns(4, |columns| {
        let bounds_now = current_bounds;
        let fields: [(&str, &mut f32, std::ops::RangeInclusive<f32>); 4] = [
            ("X", &mut bounds.x, 0.0..=1.0 - bounds_now.width),
            ("Y", &mut bounds.y, 0.0..=1.0 - bounds_now.height),
            ("W", &mut bounds.width, 0.01..=1.0 - bounds_now.x),
            ("H", &mut bounds.height, 0.01..=1.0 - bounds_now.y),
        ];
        for (column, (label, value, range)) in columns.iter_mut().zip(fields) {
            column.label(RichText::new(label).size(11.0).color(p.muted));
            outcome.changed |= column
                .add(
                    egui::DragValue::new(value)
                        .range(range)
                        .speed(0.005)
                        .custom_formatter(percent)
                        .custom_parser(parse),
                )
                .changed();
        }
    });
    if let Some((_, monitor)) = current {
        let (x, y, width, height) = zone.bounds.pixels(monitor);
        ui.label(
            theme::mono(format!("{x}, {y} · {width} × {height} px"))
                .size(12.0)
                .color(p.muted),
        );
    }
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        caps(ui, "Launch delay");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            outcome.changed |= ui
                .add(
                    egui::DragValue::new(&mut zone.delay_ms)
                        .range(0..=60_000)
                        .suffix(" ms"),
                )
                .changed();
        });
    });

    let mut advanced = egui::CollapsingHeader::new(RichText::new("Advanced").color(p.muted))
        .id_salt((view, index, "advanced"));
    if open_advanced {
        advanced = advanced.open(Some(true));
    }
    advanced.show(ui, |ui| {
        ui.label(RichText::new("Custom command").size(12.0).color(p.muted));
        let response = ui.add(
            egui::TextEdit::singleline(&mut zone.command)
                .font(egui::TextStyle::Monospace)
                .margin(Margin::symmetric(8, 7))
                .desired_width(f32::INFINITY)
                .hint_text("Replaces the selected application"),
        );
        if open_advanced {
            response.request_focus();
        }
        if response.changed() {
            if zone.application.is_some() {
                zone.run_in_terminal = is_terminal;
            }
            zone.application = None;
            zone.terminal_command.clear();
            outcome.changed = true;
        }
        if zone.application.is_none() {
            outcome.changed |= ui
                .checkbox(&mut zone.run_in_terminal, "Run in terminal")
                .changed();
        }
    });

    ui.add_space(12.0);
    outcome.delete = ui
        .add_sized(
            [ui.available_width(), 32.0],
            egui::Button::new(RichText::new("Delete zone").color(p.danger))
                .fill(egui::Color32::TRANSPARENT),
        )
        .clicked();
    outcome
}

fn monitor_label(number: usize, monitor: &Monitor) -> String {
    format!(
        "Monitor {} · {} · {} × {}",
        number + 1,
        monitor.name,
        monitor.width,
        monitor.height
    )
}
