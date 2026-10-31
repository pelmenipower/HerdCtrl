use super::canvas::Canvas;
use crate::{
    core::{
        config::{Config, View},
        launcher,
        monitor::Monitor,
    },
    os::linux,
};
use eframe::egui::{self, Color32, RichText};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
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
    hotkeys: Option<GlobalHotKeyManager>,
    bindings: Vec<(HotKey, usize)>,
    dirty: bool,
    close_dialog: bool,
}

impl Dashboard {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config, path: PathBuf) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.visuals.selection.bg_fill = Color32::from_rgb(37, 88, 76);
        cc.egui_ctx.set_style(style);
        let (events, messages) = mpsc::channel();
        let (monitor_tx, monitor_rx) = mpsc::channel();
        let manager = GlobalHotKeyManager::new();
        let mut log = vec!["Ready. Draw zones, assign commands, then save your view.".into()];
        if let Err(e) = &manager {
            log.push(format!("Global hotkeys unavailable: {e}"));
        }
        let mut app = Self {
            config,
            path,
            monitors: Vec::new(),
            monitor_rx,
            monitor_tx,
            messages,
            events,
            log,
            selected_view: 0,
            selected_zone: None,
            canvas: Canvas::default(),
            hotkeys: manager.ok(),
            bindings: Vec::new(),
            dirty: false,
            close_dialog: false,
        };
        app.refresh();
        app.register_hotkeys();
        app
    }

    fn refresh(&self) {
        let tx = self.monitor_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(linux::monitors());
        });
    }

    fn register_hotkeys(&mut self) {
        let Some(manager) = &self.hotkeys else {
            return;
        };
        for (key, _) in self.bindings.drain(..) {
            let _ = manager.unregister(key);
        }
        for (index, view) in self.config.views.iter().enumerate() {
            if view.hotkey.trim().is_empty() {
                continue;
            }
            match view.hotkey.parse::<HotKey>() {
                Ok(key) => match manager.register(key) {
                    Ok(()) => self.bindings.push((key, index)),
                    Err(e) => self
                        .log
                        .push(format!("{} hotkey unavailable: {e}", view.name)),
                },
                Err(e) => self.log.push(format!("{} hotkey invalid: {e}", view.name)),
            }
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
        }
        match self.config.save(&self.path) {
            Ok(()) => {
                self.dirty = false;
                self.log.push("Layouts saved".into());
                self.register_hotkeys();
            }
            Err(e) => self.log.push(format!("Save failed: {e}")),
        }
    }
}

impl eframe::App for Dashboard {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(100));
        while let Ok(result) = self.monitor_rx.try_recv() {
            match result {
                Ok(monitors) => self.monitors = monitors,
                Err(e) => {
                    self.monitors.clear();
                    self.log.push(e);
                }
            }
        }
        while let Ok(message) = self.messages.try_recv() {
            self.log.push(message);
        }
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                if let Some((_, index)) = self.bindings.iter().find(|(key, _)| key.id() == event.id)
                {
                    self.run(*index);
                }
            }
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
                        self.refresh();
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
                        self.selected_zone = None;
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
                    self.dirty = true;
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !self.monitors.is_empty() && !self.config.views.is_empty(),
                        egui::Button::new("Launch selected view"),
                    )
                    .clicked()
                {
                    self.run(self.selected_view);
                }
                ui.label(
                    RichText::new("Commands run as your user. Use only commands you trust.")
                        .small()
                        .color(Color32::GRAY),
                );
                ui.separator();
                ui.label(RichText::new("Linux / X11").small());
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
            let mut delete_view = false;
            ui.horizontal(|ui| {
                self.dirty |= ui.text_edit_singleline(&mut view.name).changed();
                ui.label("Hotkey");
                self.dirty |= ui
                    .add(
                        egui::TextEdit::singleline(&mut view.hotkey)
                            .hint_text("Super+Shift+KeyD")
                            .desired_width(175.0),
                    )
                    .changed();
                delete_view = ui.button("Delete view").clicked();
            });
            ui.horizontal(|ui| {
                ui.heading("Workspace canvas");
                ui.toggle_value(&mut self.canvas.drawing, "Draw a zone");
            });
            ui.label("Drag a zone to move it. Drag its bottom-right handle to resize.");
            self.dirty |= self
                .canvas
                .show(ui, &self.monitors, view, &mut self.selected_zone);
            ui.add_space(10.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, zone) in view.zones.iter().enumerate() {
                        if ui
                            .selectable_label(self.selected_zone == Some(index), &zone.name)
                            .clicked()
                        {
                            self.selected_zone = Some(index);
                        }
                    }
                });
                if let Some(index) = self.selected_zone {
                    if let Some(zone) = view.zones.get_mut(index) {
                        let mut delete = false;
                        egui::Grid::new("zone-properties")
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("Zone name");
                                self.dirty |= ui.text_edit_singleline(&mut zone.name).changed();
                                ui.end_row();
                                ui.label("Command");
                                self.dirty |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut zone.command)
                                            .desired_width(420.0)
                                            .hint_text("xterm -e htop"),
                                    )
                                    .changed();
                                ui.end_row();
                                ui.label("Monitor");
                                egui::ComboBox::from_id_salt("monitor")
                                    .selected_text(&zone.monitor)
                                    .show_ui(ui, |ui| {
                                        for monitor in &self.monitors {
                                            self.dirty |= ui
                                                .selectable_value(
                                                    &mut zone.monitor,
                                                    monitor.name.clone(),
                                                    &monitor.name,
                                                )
                                                .changed();
                                        }
                                    });
                                ui.end_row();
                                ui.label("Launch delay");
                                self.dirty |= ui
                                    .add(
                                        egui::DragValue::new(&mut zone.delay_ms)
                                            .range(0..=60_000)
                                            .suffix(" ms"),
                                    )
                                    .changed();
                                ui.end_row();
                                ui.label("Zone");
                                ui.horizontal(|ui| {
                                    self.dirty |= ui
                                        .add(
                                            egui::DragValue::new(&mut zone.bounds.x)
                                                .prefix("X ")
                                                .speed(0.005)
                                                .range(0.0..=1.0 - zone.bounds.width),
                                        )
                                        .changed();
                                    self.dirty |= ui
                                        .add(
                                            egui::DragValue::new(&mut zone.bounds.y)
                                                .prefix("Y ")
                                                .speed(0.005)
                                                .range(0.0..=1.0 - zone.bounds.height),
                                        )
                                        .changed();
                                    self.dirty |= ui
                                        .add(
                                            egui::DragValue::new(&mut zone.bounds.width)
                                                .prefix("W ")
                                                .speed(0.005)
                                                .range(0.001..=1.0 - zone.bounds.x),
                                        )
                                        .changed();
                                    self.dirty |= ui
                                        .add(
                                            egui::DragValue::new(&mut zone.bounds.height)
                                                .prefix("H ")
                                                .speed(0.005)
                                                .range(0.001..=1.0 - zone.bounds.y),
                                        )
                                        .changed();
                                });
                                ui.end_row();
                                ui.label("");
                                delete = ui.button("Delete zone").clicked();
                                ui.end_row();
                            });
                        if delete {
                            view.zones.remove(index);
                            self.selected_zone = None;
                            self.dirty = true;
                        }
                    }
                }
            });
            if delete_view {
                self.config.views.remove(self.selected_view);
                self.selected_view = 0;
                self.selected_zone = None;
                self.dirty = true;
                // Indices changed; never allow old hotkeys to trigger a different view.
                if let Some(manager) = &self.hotkeys {
                    for (key, _) in self.bindings.drain(..) {
                        let _ = manager.unregister(key);
                    }
                }
            }
        });
    }
}
