use super::theme::{self, caps};
use crate::core::{applications::InstalledApp, config::Application};
use eframe::egui::{self, Align2, FontId, Margin, RichText, Sense, Stroke, Vec2};

pub struct AppPicker {
    pub view: usize,
    pub zone: usize,
    query: String,
    focus: bool,
    highlighted: usize,
}

pub struct Choice {
    pub selected: Option<Application>,
    pub close: bool,
    pub refresh: bool,
    /// The user wants a custom command instead of an application.
    pub custom: bool,
}

impl AppPicker {
    pub fn new(view: usize, zone: usize) -> Self {
        Self {
            view,
            zone,
            query: String::new(),
            focus: true,
            highlighted: 0,
        }
    }

    /// `used` lists application ids already chosen in any view; they are offered first.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        apps: &[InstalledApp],
        loading: bool,
        used: &[String],
    ) -> Choice {
        let p = theme::palette(ctx);
        let mut choice = Choice {
            selected: None,
            close: false,
            refresh: false,
            custom: false,
        };
        let modal = egui::Modal::new(egui::Id::new("choose-application"))
            .frame(
                egui::Frame::popup(&ctx.style())
                    .inner_margin(Margin::ZERO)
                    .corner_radius(10),
            )
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 2.0);
                let search = egui::Frame::NONE
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🔍").color(p.muted));
                            let search = ui.add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .frame(false)
                                    .font(FontId::proportional(15.0))
                                    .desired_width(ui.available_width() - 48.0)
                                    .hint_text("Search applications"),
                            );
                            theme::kbd(ui, "Esc");
                            search
                        })
                        .inner
                    })
                    .inner;
                if self.focus {
                    search.request_focus();
                    self.focus = false;
                }
                if search.changed() {
                    self.highlighted = 0;
                }
                ui.separator();
                let filtered: Vec<&InstalledApp> = if loading {
                    Vec::new()
                } else if self.query.trim().is_empty() {
                    // Apps already used in a view first, then everything else.
                    let mut first: Vec<_> = apps
                        .iter()
                        .filter(|app| used.contains(&app.application.id))
                        .collect();
                    first.extend(
                        apps.iter()
                            .filter(|app| !used.contains(&app.application.id)),
                    );
                    first
                } else {
                    apps.iter().filter(|app| app.matches(&self.query)).collect()
                };
                let used_count = if self.query.trim().is_empty() {
                    filtered
                        .iter()
                        .take_while(|app| used.contains(&app.application.id))
                        .count()
                } else {
                    0
                };
                let (down, up, enter) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::ArrowDown),
                        i.key_pressed(egui::Key::ArrowUp),
                        i.key_pressed(egui::Key::Enter),
                    )
                });
                if down {
                    self.highlighted = (self.highlighted + 1).min(filtered.len().saturating_sub(1));
                }
                if up {
                    self.highlighted = self.highlighted.saturating_sub(1);
                }
                if enter && (search.has_focus() || search.lost_focus()) {
                    choice.selected = filtered
                        .get(self.highlighted)
                        .map(|app| app.application.clone());
                }
                egui::Frame::NONE
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(340.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                if loading {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("Finding your applications…");
                                    });
                                } else if filtered.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "No applications found. Try another search or refresh the list.",
                                        )
                                        .color(p.muted),
                                    );
                                }
                                for (index, app) in filtered.iter().enumerate() {
                                    if index == 0 && used_count > 0 {
                                        section(ui, "In your views");
                                    }
                                    if index == used_count {
                                        section(
                                            ui,
                                            if self.query.trim().is_empty() {
                                                "All applications"
                                            } else {
                                                "Results"
                                            },
                                        );
                                    }
                                    let response = row(ui, app, index == self.highlighted);
                                    if index == self.highlighted && (up || down) {
                                        response.scroll_to_me(None);
                                    }
                                    if response.hovered() {
                                        self.highlighted = index;
                                    }
                                    if response.clicked() {
                                        choice.selected = Some(app.application.clone());
                                    }
                                }
                            });
                    });
                ui.separator();
                egui::Frame::NONE
                    .inner_margin(Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            theme::kbd(ui, "↑↓");
                            ui.label(RichText::new("move").size(12.0).color(p.muted));
                            theme::kbd(ui, "Enter");
                            ui.label(RichText::new("choose").size(12.0).color(p.muted));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                choice.custom = theme::ghost(ui, "Custom command…").clicked();
                                choice.refresh = ui
                                    .add_enabled(!loading, egui::Button::new("Refresh").frame(false))
                                    .clicked();
                            });
                        });
                    });
            });
        choice.close |= modal.should_close() || choice.selected.is_some() || choice.custom;
        choice
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        caps(ui, title);
    });
    ui.add_space(2.0);
}

fn row(ui: &mut egui::Ui, app: &InstalledApp, highlighted: bool) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
    if highlighted {
        ui.painter().rect_filled(rect, 6.0, p.accent_soft);
    }
    let tile = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 24.0, rect.center().y),
        Vec2::splat(30.0),
    );
    ui.painter().rect_filled(tile, 7.0, p.accent_soft);
    let glyph = if app.is_terminal {
        ">_".to_string()
    } else {
        app.application
            .name
            .chars()
            .next()
            .map_or("?".into(), |c| c.to_uppercase().to_string())
    };
    ui.painter().text(
        tile.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(13.0, theme::strong_family()),
        p.accent,
    );
    let mut right = rect.right() - 8.0;
    if app.is_terminal {
        let tag =
            ui.painter()
                .layout_no_wrap("Terminal".into(), FontId::proportional(11.0), p.muted);
        let tag_rect = egui::Rect::from_min_size(
            egui::pos2(right - tag.size().x - 12.0, rect.center().y - 10.0),
            Vec2::new(tag.size().x + 12.0, 20.0),
        );
        ui.painter().rect_stroke(
            tag_rect,
            4.0,
            Stroke::new(1.0_f32, p.border),
            egui::StrokeKind::Inside,
        );
        ui.painter().galley(
            tag_rect.min + Vec2::new(6.0, (20.0 - tag.size().y) / 2.0),
            tag,
            p.muted,
        );
        right = tag_rect.left() - 8.0;
    }
    let width = (right - tile.right() - 12.0).max(10.0);
    let clip = ui.painter().with_clip_rect(egui::Rect::from_x_y_ranges(
        tile.right() + 12.0..=right,
        rect.y_range(),
    ));
    let name_y = if app.description.is_empty() {
        rect.center().y - 8.0
    } else {
        rect.top() + 6.0
    };
    clip.text(
        egui::pos2(tile.right() + 12.0, name_y),
        Align2::LEFT_TOP,
        &app.application.name,
        FontId::new(13.0, theme::strong_family()),
        p.text,
    );
    if !app.description.is_empty() {
        let mut job = egui::text::LayoutJob::simple_singleline(
            app.description.clone(),
            FontId::proportional(12.0),
            p.muted,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(width);
        let galley = ui.fonts(|fonts| fonts.layout_job(job));
        clip.galley(
            egui::pos2(tile.right() + 12.0, rect.top() + 24.0),
            galley,
            p.muted,
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_and_enter_select_the_application() {
        let context = egui::Context::default();
        theme::install(&context);
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
            new_window_argv: None,
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
                picker.show(ctx, &apps, false, &[]);
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
            selected = picker.show(ctx, &apps, false, &[]).selected;
        });
        assert_eq!(picker.query, "term");
        assert_eq!(selected, Some(apps[0].application.clone()));
    }
}
