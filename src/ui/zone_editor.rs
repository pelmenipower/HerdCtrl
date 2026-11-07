use crate::core::{applications::InstalledApp, config::Zone, monitor::Monitor};
use eframe::egui;

/// What the zone editor asks the dashboard to do besides editing the zone in place.
#[derive(Default)]
pub struct Outcome {
    pub changed: bool,
    pub delete: bool,
    pub choose_application: bool,
}

/// Shows the properties of the selected zone. `view` and `index` only scope widget ids.
pub fn show(
    ui: &mut egui::Ui,
    view: usize,
    index: usize,
    zone: &mut Zone,
    monitors: &[Monitor],
    applications: &[InstalledApp],
) -> Outcome {
    let mut outcome = Outcome::default();
    let is_terminal = zone.application.as_ref().is_some_and(|selected| {
        applications
            .iter()
            .any(|app| app.application.id == selected.id && app.is_terminal)
    });
    egui::Grid::new("zone-properties")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Zone name");
            outcome.changed |= ui.text_edit_singleline(&mut zone.name).changed();
            ui.end_row();
            ui.label("Application");
            ui.horizontal(|ui| {
                ui.label(
                    zone.application
                        .as_ref()
                        .map(|app| app.name.as_str())
                        .unwrap_or(if zone.command.trim().is_empty() {
                            "No application selected"
                        } else {
                            "Custom command"
                        }),
                );
                if ui
                    .button(if zone.application.is_some() {
                        "Change…"
                    } else {
                        "Choose application…"
                    })
                    .clicked()
                {
                    outcome.choose_application = true;
                }
            });
            ui.end_row();
            if is_terminal {
                ui.label("Program in terminal");
                outcome.changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut zone.terminal_command)
                            .desired_width(420.0)
                            .hint_text("Optional, e.g. herdr or htop"),
                    )
                    .changed();
                ui.end_row();
            }
            ui.label("Monitor");
            egui::ComboBox::from_id_salt("monitor")
                .selected_text(
                    monitors
                        .iter()
                        .enumerate()
                        .find(|(_, monitor)| monitor.name == zone.monitor)
                        .map(|(index, monitor)| monitor.display_name(index))
                        .unwrap_or_else(|| format!("Disconnected · {}", zone.monitor)),
                )
                .show_ui(ui, |ui| {
                    for (number, monitor) in monitors.iter().enumerate() {
                        outcome.changed |= ui
                            .selectable_value(
                                &mut zone.monitor,
                                monitor.name.clone(),
                                monitor.display_name(number),
                            )
                            .changed();
                    }
                });
            ui.end_row();
            ui.label("");
            outcome.delete = ui.button("Delete zone").clicked();
            ui.end_row();
        });
    egui::CollapsingHeader::new("Advanced")
        .id_salt((view, index, "advanced"))
        .show(ui, |ui| {
            egui::Grid::new("advanced-properties")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Custom command");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut zone.command)
                                .desired_width(420.0)
                                .hint_text("Optional: replace the selected application"),
                        )
                        .changed()
                    {
                        if zone.application.is_some() {
                            zone.run_in_terminal = is_terminal;
                        }
                        zone.application = None;
                        zone.terminal_command.clear();
                        outcome.changed = true;
                    }
                    ui.end_row();
                    if zone.application.is_none() {
                        ui.label("");
                        outcome.changed |= ui
                            .checkbox(&mut zone.run_in_terminal, "Run in terminal")
                            .changed();
                        ui.end_row();
                    }
                    ui.label("Launch delay");
                    outcome.changed |= ui
                        .add(
                            egui::DragValue::new(&mut zone.delay_ms)
                                .range(0..=60_000)
                                .suffix(" ms"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Zone");
                    ui.horizontal(|ui| {
                        outcome.changed |= ui
                            .add(
                                egui::DragValue::new(&mut zone.bounds.x)
                                    .prefix("X ")
                                    .speed(0.005)
                                    .range(0.0..=1.0 - zone.bounds.width),
                            )
                            .changed();
                        outcome.changed |= ui
                            .add(
                                egui::DragValue::new(&mut zone.bounds.y)
                                    .prefix("Y ")
                                    .speed(0.005)
                                    .range(0.0..=1.0 - zone.bounds.height),
                            )
                            .changed();
                        outcome.changed |= ui
                            .add(
                                egui::DragValue::new(&mut zone.bounds.width)
                                    .prefix("W ")
                                    .speed(0.005)
                                    .range(0.001..=1.0 - zone.bounds.x),
                            )
                            .changed();
                        outcome.changed |= ui
                            .add(
                                egui::DragValue::new(&mut zone.bounds.height)
                                    .prefix("H ")
                                    .speed(0.005)
                                    .range(0.001..=1.0 - zone.bounds.y),
                            )
                            .changed();
                    });
                    ui.end_row();
                });
        });
    outcome
}
