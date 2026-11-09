use super::{
    app_picker::AppPicker,
    canvas::{self, Canvas},
    hotkey_input::{self, Recorder},
    identify::{Event as IdentifyEvent, Identification},
    zone_editor,
};
use crate::{
    core::{
        applications::{self, InstalledApp},
        config::{Config, View},
        launcher,
        monitor::{sort_for_display, Monitor},
    },
    os::Backend,
};
use eframe::egui::{self, Color32, RichText};
use global_hotkey::hotkey::HotKey;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

pub struct Dashboard {
    config: Config,
    path: PathBuf,
    monitors: Vec<Monitor>,
    monitor_rx: Receiver<Result<Vec<Monitor>, String>>,
    monitor_tx: Sender<Result<Vec<Monitor>, String>>,
    messages: Receiver<String>,
    events: Sender<String>,
    log: Vec<String>,
    selected_view: usize,
    selected_zone: Option<usize>,
    canvas: Canvas,
    backend: Backend,
    dirty: bool,
    close_dialog: bool,
    applications: Vec<InstalledApp>,
    applications_rx: Receiver<Vec<InstalledApp>>,
    applications_tx: Sender<Vec<InstalledApp>>,
    applications_loading: bool,
    app_picker: Option<AppPicker>,
    identification: Identification,
    identify_return_focus: bool,
    confirm_delete: Option<usize>,
    hotkey_recorder: Recorder,
}

impl Dashboard {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: Config,
        path: PathBuf,
        tray_error: Option<String>,
    ) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.visuals.selection.bg_fill = Color32::from_rgb(37, 88, 76);
        cc.egui_ctx.set_style(style);
        let (events, messages) = mpsc::channel();
        let (monitor_tx, monitor_rx) = mpsc::channel();
        let (applications_tx, applications_rx) = mpsc::channel();
        let selected_zone = config
            .views
            .first()
            .and_then(|view| (!view.zones.is_empty()).then_some(0));
        let backend = Backend::detect();
        let mut log = vec!["Ready. Draw zones, choose applications, then save your view.".into()];
        if let Some(e) = tray_error {
            log.push(format!(
                "Tray could not start, so hotkeys are inactive: {e}"
            ));
        }
        let mut app = Self {
            config,
            path,
            monitors: Vec::new(),
            monitor_rx,
            monitor_tx,
            messages,
            backend,
            events,
            log,
            selected_view: 0,
            selected_zone,
            canvas: Canvas::default(),
            dirty: false,
            close_dialog: false,
            applications: Vec::new(),
            applications_rx,
            applications_tx,
            applications_loading: false,
            app_picker: None,
            identification: Identification::default(),
            identify_return_focus: false,
            confirm_delete: None,
            hotkey_recorder: Recorder::default(),
        };
        app.refresh();
        app.refresh_applications();
        app
    }

    fn refresh(&self) {
        let tx = self.monitor_tx.clone();
        let backend = self.backend;
        std::thread::spawn(move || {
            let _ = tx.send(backend.monitors());
        });
    }

    fn refresh_applications(&mut self) {
        self.applications_loading = true;
        let tx = self.applications_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(applications::discover());
        });
    }

    fn show_app_picker(&mut self, ctx: &egui::Context) {
        let Some(picker) = &mut self.app_picker else {
            return;
        };
        let choice = picker.show(ctx, &self.applications, self.applications_loading);
        if let Some(application) = choice.selected {
            let is_terminal = self
                .applications
                .iter()
                .any(|app| app.application.id == application.id && app.is_terminal);
            if let Some(zone) = self
                .config
                .views
                .get_mut(picker.view)
                .and_then(|view| view.zones.get_mut(picker.zone))
            {
                if is_terminal && zone.application.is_none() && !zone.command.trim().is_empty() {
                    zone.terminal_command = zone.command.clone();
                } else if !is_terminal {
                    zone.terminal_command.clear();
                }
                zone.application = Some(application);
                zone.command.clear();
                zone.run_in_terminal = false;
                self.dirty = true;
            }
        }
        if choice.close {
            self.app_picker = None;
        }
        if choice.refresh {
            self.refresh_applications();
        }
    }

    fn delete_view(&mut self, index: usize) {
        if index >= self.config.views.len() {
            return;
        }
        self.config.views.remove(index);
        self.selected_view = selection_after_removal(index, self.config.views.len());
        self.hotkey_recorder = Recorder::default();
        self.selected_zone = self
            .config
            .views
            .get(self.selected_view)
            .and_then(|view| (!view.zones.is_empty()).then_some(0));
        self.app_picker = None;
        reset_canvas(&mut self.canvas);
        self.dirty = true;
    }

    fn show_delete_view_dialog(&mut self, ctx: &egui::Context) {
        let Some(index) = self.confirm_delete else {
            return;
        };
        let Some(name) = self.config.views.get(index).map(|view| view.name.clone()) else {
            self.confirm_delete = None;
            return;
        };
        let mut confirmed = false;
        let mut cancelled = false;
        let response = egui::Modal::new(egui::Id::new("delete-view-dialog")).show(ctx, |ui| {
            ui.heading(format!("Delete view “{name}”?"));
            ui.label("Its zones and hotkey will be removed when you save.");
            ui.horizontal(|ui| {
                confirmed = ui.button("Delete view").clicked();
                cancelled = ui.button("Cancel").clicked();
            });
        });
        if confirmed {
            self.confirm_delete = None;
            self.delete_view(index);
        } else if cancelled || response.should_close() {
            self.confirm_delete = None;
        }
    }

    fn run(&self, index: usize) {
        if let Some(view) = self.config.views.get(index) {
            launcher::launch(view.clone(), self.events.clone());
        }
    }

    fn save(&mut self) {
        for view in &self.config.views {
            if !view.hotkey.trim().is_empty() && view.hotkey.parse::<HotKey>().is_err() {
                self.log.push(format!(
                    "Invalid hotkey for {}. Example: Super+Shift+KeyD",
                    view.name
                ));
                return;
            }
            if !view.hotkey.trim().is_empty() && !hotkey_input::has_modifier(&view.hotkey) {
                self.log.push(format!(
                    "Hotkey for {} needs at least one modifier (Super, Ctrl, Alt or Shift)",
                    view.name
                ));
                return;
            }
        }
        match self.config.save(&self.path) {
            Ok(()) => {
                self.dirty = false;
                // The tray reloads the file and rebinds the hotkeys; restart it if it was quit.
                match crate::tray::ensure_running() {
                    Ok(()) => self
                        .log
                        .push("Layouts saved. The tray applies the hotkeys.".into()),
                    Err(e) => self.log.push(format!(
                        "Layouts saved, but the tray could not start, so hotkeys are inactive: {e}"
                    )),
                }
            }
            Err(e) => self.log.push(format!("Save failed: {e}")),
        }
    }
}

/// Index of the view to select after removing the view at `removed`: the previous view, or 0.
fn selection_after_removal(removed: usize, remaining: usize) -> usize {
    if remaining == 0 {
        0
    } else {
        removed.saturating_sub(1).min(remaining - 1)
    }
}

/// Drops in-progress canvas state but keeps the user's snapping preference.
fn reset_canvas(canvas: &mut Canvas) {
    let snap_enabled = canvas.snap_enabled;
    *canvas = Canvas::default();
    canvas.snap_enabled = snap_enabled;
}

impl eframe::App for Dashboard {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(100));
        while let Some(event) = self.identification.poll() {
            match event {
                IdentifyEvent::Shown(labels) => {
                    if self.identify_return_focus {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    self.identify_return_focus = false;
                    self.log.push(format!(
                        "Monitor numbers shown for 5 seconds: {}",
                        labels
                            .iter()
                            .map(|label| format!("{} · {}", label.number, label.monitor))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                IdentifyEvent::Finished(Ok(())) => {}
                IdentifyEvent::Finished(Err(error)) => self
                    .log
                    .push(format!("Could not identify monitors: {error}")),
            }
        }
        while let Ok(applications) = self.applications_rx.try_recv() {
            self.applications = applications;
            self.applications_loading = false;
        }
        while let Ok(result) = self.monitor_rx.try_recv() {
            match result {
                Ok(mut monitors) => {
                    sort_for_display(&mut monitors);
                    self.monitors = monitors;
                }
                Err(e) => {
                    self.monitors.clear();
                    self.log.push(e);
                }
            }
        }
        while let Ok(message) = self.messages.try_recv() {
            self.log.push(message);
        }
        if self.log.len() > 100 {
            self.log.drain(..self.log.len() - 100);
        }
        if self.dirty && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dialog = true;
        }
        if self.close_dialog {
            egui::Window::new("Unsaved layouts")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Save your layouts before closing?");
                    ui.horizontal(|ui| {
                        if ui.button("Save and close").clicked() {
                            self.save();
                            if !self.dirty {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        if ui.button("Discard and close").clicked() {
                            self.dirty = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Keep editing").clicked() {
                            self.close_dialog = false;
                        }
                    });
                });
        }
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("HerdCtrl").color(Color32::from_rgb(103, 221, 177)));
                ui.label("A place for every window.");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(if self.dirty {
                            "Save layouts *"
                        } else {
                            "Save layouts"
                        })
                        .clicked()
                    {
                        self.save();
                    }
                    if ui.button("Refresh displays").clicked() {
                        self.identification.stop();
                        self.refresh();
                    }
                    if ui
                        .add_enabled(
                            self.identification.active() || !self.monitors.is_empty(),
                            egui::Button::new(if self.identification.active() {
                                "Hide numbers"
                            } else {
                                "Identify monitors"
                            }),
                        )
                        .on_hover_text("Show matching numbers on your screens for 5 seconds")
                        .clicked()
                    {
                        if self.identification.active() {
                            self.identification.stop();
                        } else {
                            self.identify_return_focus =
                                ctx.input(|input| input.viewport().focused.unwrap_or(false));
                            self.identification
                                .start(self.backend, self.monitors.clone());
                        }
                    }
                });
            });
            ui.add_space(8.0);
        });
        egui::SidePanel::left("views")
            .exact_width(205.0)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("YOUR VIEWS").small().color(Color32::GRAY));
                for (index, view) in self.config.views.iter().enumerate() {
                    if ui
                        .selectable_label(index == self.selected_view, &view.name)
                        .clicked()
                    {
                        self.selected_view = index;
                        self.selected_zone = (!view.zones.is_empty()).then_some(0);
                        self.hotkey_recorder = Recorder::default();
                    }
                }
                if ui.button("+ New view").clicked() {
                    self.config.views.push(View {
                        name: format!("View {}", self.config.views.len() + 1),
                        hotkey: String::new(),
                        zones: Vec::new(),
                    });
                    self.selected_view = self.config.views.len() - 1;
                    self.selected_zone = None;
                    self.hotkey_recorder = Recorder::default();
                    self.dirty = true;
                }
                if ui.add_enabled(
                    self.config.views.get(self.selected_view).is_some(),
                    egui::Button::new("Duplicate view"),
                ).on_hover_text("Copy this view with all zones and applications. Assign a separate hotkey to the copy.")
                    .clicked()
                {
                    if let Some(index) = self.config.duplicate_view(self.selected_view) {
                        self.selected_view = index;
                        self.selected_zone = (!self.config.views[index].zones.is_empty()).then_some(0);
                        self.hotkey_recorder = Recorder::default();
                        self.app_picker = None;
                        reset_canvas(&mut self.canvas);
                        self.dirty = true;
                    }
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !self.monitors.is_empty()
                            && self
                                .config
                                .views
                                .get(self.selected_view)
                                .is_some_and(|view| {
                                    !view.zones.is_empty()
                                        && view.zones.iter().all(|zone| {
                                            zone.application.is_some()
                                                || !zone.command.trim().is_empty()
                                        })
                                }),
                        egui::Button::new("Launch selected view"),
                    )
                    .clicked()
                {
                    self.run(self.selected_view);
                }
                if self
                    .config
                    .views
                    .get(self.selected_view)
                    .is_some_and(|view| {
                        view.zones.iter().any(|zone| {
                            zone.application.is_none() && zone.command.trim().is_empty()
                        })
                    })
                {
                    ui.label(
                        RichText::new("Choose an application for each zone to launch this view.")
                            .small(),
                    );
                }
                ui.separator();
                ui.label(RichText::new(self.backend.label()).small());
                if self.backend == Backend::Gnome {
                    ui.label(RichText::new("Requires the HerdCtrl GNOME extension.").small());
                }
            });
        egui::TopBottomPanel::bottom("activity")
            .max_height(120.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("ACTIVITY").small().color(Color32::GRAY));
                egui::ScrollArea::vertical()
                    .max_height(75.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.log {
                            ui.label(line);
                        }
                    });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(view) = self.config.views.get_mut(self.selected_view) else {
                ui.add_space(80.0);
                ui.heading("Bring your workspace together.");
                ui.label(
                    "Create a view, draw zones on your displays and assign an app to each zone.",
                );
                return;
            };
            let mut request_delete = false;
            ui.horizontal(|ui| {
                self.dirty |= ui.text_edit_singleline(&mut view.name).changed();
                ui.label("Hotkey");
                self.dirty |=
                    hotkey_input::show(ui, &mut self.hotkey_recorder, &mut view.hotkey);
                request_delete = ui.button("Delete view").clicked();
            });
            ui.horizontal(|ui| {
                ui.heading("Workspace canvas");
                ui.toggle_value(&mut self.canvas.drawing, "Draw a zone");
                ui.checkbox(&mut self.canvas.snap_enabled,"Snap zones")
                    .on_hover_text("Dock to monitor edges, halves, thirds, quarters and neighbouring zones without overlap");
                if ui.add_enabled(!view.zones.is_empty(),egui::Button::new("Align edges")).clicked() {
                    self.dirty |= self.canvas.align(view);
                }
            });
            ui.label("Drag a zone within a monitor or onto another monitor. Drag its bottom-right handle to resize.");
            let previous_zones = view.zones.len();
            self.dirty |= self
                .canvas
                .show(ui, &self.monitors, view, &mut self.selected_zone);
            if canvas::has_overlaps(view) {
                ui.colored_label(Color32::from_rgb(244,160,110),"Zones overlap. Use Align edges, or move and resize the highlighted zones.");
            } else if let Some(feedback)=&self.canvas.feedback {ui.small(feedback);}
            if view.zones.len() > previous_zones {
                self.app_picker = Some(AppPicker::new(self.selected_view, view.zones.len() - 1));
            }
            ui.add_space(10.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.dirty |= super::zone_list::show(
                    ui, self.selected_view, &mut view.zones, &mut self.selected_zone,
                );
                if let Some(index) = self.selected_zone {
                    if let Some(zone) = view.zones.get_mut(index) {
                        let outcome = zone_editor::show(
                            ui,
                            self.selected_view,
                            index,
                            zone,
                            &self.monitors,
                            &self.applications,
                        );
                        self.dirty |= outcome.changed;
                        if outcome.choose_application {
                            self.app_picker = Some(AppPicker::new(self.selected_view, index));
                        }
                        if outcome.delete {
                            view.zones.remove(index);
                            self.selected_zone = None;
                            self.dirty = true;
                            if self
                                .app_picker
                                .as_ref()
                                .is_some_and(|picker| picker.view == self.selected_view)
                            {
                                self.app_picker = None;
                            }
                            reset_canvas(&mut self.canvas);
                        }
                    }
                }
            });
            if request_delete {
                self.confirm_delete = Some(self.selected_view);
            }
        });
        self.show_delete_view_dialog(ctx);
        self.show_app_picker(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::selection_after_removal;

    #[test]
    fn selects_previous_view_after_removal() {
        assert_eq!(selection_after_removal(0, 0), 0);
        assert_eq!(selection_after_removal(0, 3), 0);
        assert_eq!(selection_after_removal(1, 1), 0);
        assert_eq!(selection_after_removal(2, 3), 1);
        assert_eq!(selection_after_removal(3, 3), 2);
    }
}
