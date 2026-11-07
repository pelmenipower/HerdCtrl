use crate::core::{applications::InstalledApp, config::Application};
use eframe::egui::{self, RichText};

pub struct AppPicker {
    pub view: usize,
    pub zone: usize,
    query: String,
    focus: bool,
}

pub struct Choice {
    pub selected: Option<Application>,
    pub close: bool,
    pub refresh: bool,
}

impl AppPicker {
    pub fn new(view: usize, zone: usize) -> Self {
        Self {
            view,
            zone,
            query: String::new(),
            focus: true,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, apps: &[InstalledApp], loading: bool) -> Choice {
        let mut choice = Choice {
            selected: None,
            close: false,
            refresh: false,
        };
        let modal = egui::Modal::new(egui::Id::new("choose-application")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading("Choose an application");
            ui.label("This application will open in the selected zone.");
            ui.add_space(8.0);
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .desired_width(f32::INFINITY)
                    .hint_text("Search applications…"),
            );
            if self.focus {
                search.request_focus();
                self.focus = false;
            }
            ui.add_space(8.0);
            if loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Finding your applications…");
                });
            } else {
                let filtered: Vec<_> = apps.iter().filter(|app| app.matches(&self.query)).collect();
                if filtered.is_empty() {
                    ui.label("No applications found. Try another search or refresh the list.");
                }
                let enter = (search.has_focus() || search.lost_focus())
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if enter && !self.query.trim().is_empty() {
                    choice.selected = filtered.first().map(|app| app.application.clone());
                }
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for app in filtered {
                            let label = if app.description.is_empty() {
                                app.application.name.clone()
                            } else {
                                format!("{}\n{}", app.application.name, app.description)
                            };
                            if ui
                                .add_sized(
                                    [ui.available_width(), 46.0],
                                    egui::Button::new(RichText::new(label)).wrap(),
                                )
                                .clicked()
                            {
                                choice.selected = Some(app.application.clone());
                            }
                        }
                    });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                choice.refresh = ui
                    .add_enabled(!loading, egui::Button::new("Refresh applications"))
                    .clicked();
                choice.close = ui.button("Cancel").clicked();
            });
        });
        choice.close |= modal.should_close() || choice.selected.is_some();
        choice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_and_enter_select_the_application() {
        let context = egui::Context::default();
        let apps = vec![InstalledApp {
            application: Application {
                id: "terminal".into(),
                name: "Terminal".into(),
            },
            description: "Open a terminal".into(),
            argv: vec!["/bin/true".into()],
            directory: None,
            desktop_file: "/tmp/terminal.desktop".into(),
            is_terminal: true,
            terminal_exec: Some(vec!["--".into()]),
        }];
        let mut picker = AppPicker::new(0, 0);
        for events in [vec![], vec![egui::Event::Text("term".into())]] {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 740.0),
                )),
                events,
                ..Default::default()
            };
            let _ = context.run(input, |ctx| {
                picker.show(ctx, &apps, false);
            });
        }
        let mut selected = None;
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let _ = context.run(input, |ctx| {
            selected = picker.show(ctx, &apps, false).selected;
        });
        assert_eq!(picker.query, "term");
        assert_eq!(selected, Some(apps[0].application.clone()));
    }
}
