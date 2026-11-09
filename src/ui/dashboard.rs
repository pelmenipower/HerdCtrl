use super::{
    app_picker::AppPicker,
    canvas::{self, Canvas},
    hotkey_input::{self, Recorder},
    identify::{Event as IdentifyEvent, Identification},
    launch_run::{self, LaunchRun, ZoneState},
    theme::{self, caps, Appearance, Tone},
    zone_editor,
};
use crate::{
    core::{
        applications::{self, InstalledApp},
        config::{Config, View},
        launcher::{self, Report},
        monitor::{sort_for_display, Monitor},
    },
    os::Backend,
    tray,
};
use eframe::egui::{
    self, Align, Align2, FontId, Frame, Layout, Margin, RichText, Sense, Stroke, Theme, Vec2,
};
use global_hotkey::hotkey::HotKey;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

pub struct Dashboard {
    config: Config,
    path: PathBuf,
    monitors: Vec<Monitor>,
    /// Why the last display query failed, e.g. a missing GNOME extension.
    display_error: Option<String>,
    monitor_rx: Receiver<Result<Vec<Monitor>, String>>,
    monitor_tx: Sender<Result<Vec<Monitor>, String>>,
    reports: Receiver<Report>,
    report_tx: Sender<Report>,
    run: Option<LaunchRun>,
    log: Vec<String>,
    show_log: bool,
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
    /// Expand and focus the custom command of the selected zone once.
    open_advanced: bool,
    identification: Identification,
    identify_return_focus: bool,
    confirm_delete: Option<usize>,
    hotkey_recorder: Recorder,
    appearance: Appearance,
    system_theme: Option<Theme>,
    theme_rx: Receiver<Option<Theme>>,
    tray_running: bool,
    tray_checked: Instant,
    autostart: bool,
}

impl Dashboard {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: Config,
        path: PathBuf,
        tray_error: Option<String>,
    ) -> Self {
        let mut app = Self::with_parts(&cc.egui_ctx, config, path, tray_error);
        let (theme_tx, theme_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = theme_tx.send(theme::system_theme());
        });
        app.theme_rx = theme_rx;
        app.refresh();
        app.refresh_applications();
        app
    }

    /// Builds the editor state without starting display, application or theme workers.
    fn with_parts(
        ctx: &egui::Context,
        config: Config,
        path: PathBuf,
        tray_error: Option<String>,
    ) -> Self {
        theme::install(ctx);
        let appearance = theme::load_appearance();
        ctx.set_theme(appearance.theme(None));
        let (_, theme_rx) = mpsc::channel();
        let (report_tx, reports) = mpsc::channel();
        let (monitor_tx, monitor_rx) = mpsc::channel();
        let (applications_tx, applications_rx) = mpsc::channel();
        let selected_zone = config
            .views
            .first()
            .and_then(|view| (!view.zones.is_empty()).then_some(0));
        let mut log = vec!["Ready.".to_string()];
        if let Some(e) = tray_error {
            log.push(format!(
                "Tray could not start, so hotkeys are inactive: {e}"
            ));
        }
        Self {
            config,
            path,
            monitors: Vec::new(),
            display_error: None,
            monitor_rx,
            monitor_tx,
            reports,
            report_tx,
            run: None,
            log,
            show_log: false,
            backend: Backend::detect(),
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
            open_advanced: false,
            identification: Identification::default(),
            identify_return_focus: false,
            confirm_delete: None,
            hotkey_recorder: Recorder::default(),
            appearance,
            system_theme: None,
            theme_rx,
            tray_running: tray::is_running(),
            tray_checked: Instant::now(),
            autostart: tray::autostart_enabled(),
        }
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

    fn select_view(&mut self, index: usize) {
        self.selected_view = index;
        self.selected_zone = self
            .config
            .views
            .get(index)
            .and_then(|view| (!view.zones.is_empty()).then_some(0));
        self.hotkey_recorder = Recorder::default();
        self.app_picker = None;
        reset_canvas(&mut self.canvas);
    }

    fn new_view(&mut self) {
        self.config.views.push(View {
            name: format!("View {}", self.config.views.len() + 1),
            hotkey: String::new(),
            zones: Vec::new(),
        });
        self.select_view(self.config.views.len() - 1);
        self.canvas.drawing = true;
        self.dirty = true;
    }

    fn show_app_picker(&mut self, ctx: &egui::Context) {
        let Some(picker) = &mut self.app_picker else {
            return;
        };
        let used: Vec<String> = self
            .config
            .views
            .iter()
            .flat_map(|view| &view.zones)
            .filter_map(|zone| zone.application.as_ref().map(|app| app.id.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let choice = picker.show(ctx, &self.applications, self.applications_loading, &used);
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
        if choice.custom && picker.view == self.selected_view {
            self.selected_zone = Some(picker.zone);
            self.open_advanced = true;
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
        self.run = None;
        self.select_view(selection_after_removal(index, self.config.views.len()));
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
        let p = theme::palette(ctx);
        let mut confirmed = false;
        let mut cancelled = false;
        let response = egui::Modal::new(egui::Id::new("delete-view-dialog")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(theme::strong(format!("Delete view “{name}”?")).size(16.0));
            ui.label(
                RichText::new("Its zones and hotkey are removed when you save.").color(p.muted),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                confirmed = ui
                    .add(
                        egui::Button::new(theme::strong("Delete view").color(p.accent_on))
                            .fill(p.danger)
                            .min_size(Vec2::new(0.0, 32.0)),
                    )
                    .clicked();
                cancelled = theme::button(ui, "Cancel").clicked();
            });
        });
        if confirmed {
            self.confirm_delete = None;
            self.delete_view(index);
        } else if cancelled || response.should_close() {
            self.confirm_delete = None;
        }
    }

    fn show_close_dialog(&mut self, ctx: &egui::Context) {
        if !self.close_dialog {
            return;
        }
        let p = theme::palette(ctx);
        let mut keep = false;
        let response = egui::Modal::new(egui::Id::new("close-dialog")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.label(theme::strong("Save your layouts before closing?").size(16.0));
            ui.label(
                RichText::new("Hotkeys keep working in the tray after you close this window.")
                    .color(p.muted),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::primary(ui, "Save and close").clicked() {
                    self.save();
                    if !self.dirty {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                if theme::button(ui, "Discard").clicked() {
                    self.dirty = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                keep = theme::button(ui, "Keep editing").clicked();
            });
        });
        if keep || response.should_close() {
            self.close_dialog = false;
        }
    }

    fn launch(&mut self, view: usize, zones: Option<Vec<usize>>) {
        let Some(source) = self.config.views.get(view) else {
            return;
        };
        let launched = zones.unwrap_or_else(|| (0..source.zones.len()).collect());
        let mut subset = source.clone();
        subset.zones = launched
            .iter()
            .filter_map(|index| source.zones.get(*index).cloned())
            .collect();
        // Drop reports of an earlier run so they cannot land on this one.
        while self.reports.try_recv().is_ok() {}
        self.run = Some(LaunchRun::new(view, source.zones.len(), launched));
        launcher::launch(subset, self.report_tx.clone());
    }

    fn launch_blocker(&self) -> Option<&'static str> {
        let view = self.config.views.get(self.selected_view)?;
        if self.monitors.is_empty() {
            Some("No displays available")
        } else if view.zones.is_empty() {
            Some("Draw a zone first")
        } else if view
            .zones
            .iter()
            .any(|zone| zone.application.is_none() && zone.command.trim().is_empty())
        {
            Some("Choose an application for each zone")
        } else if self.run.as_ref().is_some_and(|run| run.pending() > 0) {
            Some("Launch in progress")
        } else {
            None
        }
    }

    fn save(&mut self) {
        for view in &self.config.views {
            if !view.hotkey.trim().is_empty() && view.hotkey.parse::<HotKey>().is_err() {
                self.log.push(format!(
                    "Invalid hotkey for {}. Example: Super+Shift+KeyD",
                    view.name
                ));
                self.show_log = true;
                return;
            }
            if !view.hotkey.trim().is_empty() && !hotkey_input::has_modifier(&view.hotkey) {
                self.log.push(format!(
                    "Hotkey for {} needs at least one modifier (Super, Ctrl, Alt or Shift)",
                    view.name
                ));
                self.show_log = true;
                return;
            }
        }
        match self.config.save(&self.path) {
            Ok(()) => {
                self.dirty = false;
                // The tray reloads the file and rebinds the hotkeys; restart it if it was quit.
                match tray::ensure_running() {
                    Ok(()) => self
                        .log
                        .push("Layouts saved. The tray applies the hotkeys.".into()),
                    Err(e) => {
                        self.log.push(format!(
                            "Layouts saved, but the tray could not start, so hotkeys are inactive: {e}"
                        ));
                        self.show_log = true;
                    }
                }
                self.tray_checked = Instant::now() - Duration::from_secs(10);
            }
            Err(e) => {
                self.log.push(format!("Save failed: {e}"));
                self.show_log = true;
            }
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
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
                    self.display_error = None;
                }
                Err(e) => {
                    self.monitors.clear();
                    self.log.push(e.clone());
                    self.display_error = Some(e);
                }
            }
        }
        while let Ok(report) = self.reports.try_recv() {
            self.log.push(report.to_string());
            if let Some(run) = &mut self.run {
                run.apply(&report);
            }
        }
        while let Ok(system) = self.theme_rx.try_recv() {
            self.system_theme = system;
            ctx.set_theme(self.appearance.theme(self.system_theme));
        }
        if self.tray_checked.elapsed() > Duration::from_secs(2) {
            self.tray_checked = Instant::now();
            self.tray_running = tray::is_running();
            self.autostart = tray::autostart_enabled();
        }
        if self.log.len() > 100 {
            self.log.drain(..self.log.len() - 100);
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            theme::logo(ui, 22.0);
            ui.label(theme::strong("HerdCtrl").size(15.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::appearance_icon(ui, self.appearance)
                    .on_hover_text(format!("{} · click to change", self.appearance.label()))
                    .clicked()
                {
                    self.appearance = self.appearance.next();
                    ui.ctx().set_theme(self.appearance.theme(self.system_theme));
                    if let Err(e) = theme::store_appearance(self.appearance) {
                        self.log.push(format!("Appearance not saved: {e}"));
                    }
                }
                let (text, tone) = if !self.tray_running {
                    ("Tray not running · hotkeys off", Tone::Warn)
                } else if self.dirty {
                    ("Tray running · save to apply hotkeys", Tone::Warn)
                } else {
                    ("Tray running · hotkeys active", Tone::Ok)
                };
                let tray_pill = theme::pill(ui, text, tone);
                if !self.tray_running
                    && tray_pill
                        .interact(Sense::click())
                        .on_hover_text("Click to start the tray")
                        .clicked()
                {
                    if let Err(e) = tray::ensure_running() {
                        self.log.push(format!("Tray could not start: {e}"));
                    }
                    self.tray_checked = Instant::now() - Duration::from_secs(10);
                }
                match (self.backend, &self.display_error) {
                    (Backend::Gnome, None) => {
                        theme::pill(ui, "Extension connected", Tone::Ok);
                    }
                    (_, Some(error)) => {
                        theme::pill(ui, "Displays unavailable", Tone::Danger).on_hover_text(error);
                    }
                    _ => {}
                }
                theme::pill(
                    ui,
                    self.backend.label(),
                    if self.backend == Backend::UnsupportedWayland {
                        Tone::Danger
                    } else {
                        Tone::Ok
                    },
                );
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            caps(ui, "Views");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("+").min_size(Vec2::splat(26.0)))
                    .on_hover_text("New view")
                    .clicked()
                {
                    self.new_view();
                }
            });
        });
        if self.config.views.is_empty() {
            Frame::NONE
                .stroke(Stroke::new(1.0_f32, p.border))
                .corner_radius(6)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(
                            "No views yet. A view is a set of windows you start together.",
                        )
                        .size(12.0)
                        .color(p.muted),
                    );
                });
        }
        let mut pick = None;
        for (index, view) in self.config.views.iter().enumerate() {
            if view_card(ui, view, index == self.selected_view).clicked() {
                pick = Some(index);
            }
        }
        if let Some(index) = pick.filter(|index| *index != self.selected_view) {
            self.select_view(index);
        }
        if let Some(view) = self.config.views.get_mut(self.selected_view) {
            if !view.zones.is_empty() {
                ui.add_space(12.0);
                caps(ui, "Zones · launch order");
                self.dirty |= super::zone_list::show(
                    ui,
                    self.selected_view,
                    &mut view.zones,
                    &mut self.selected_zone,
                );
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(4.0);
            ui.checkbox(&mut self.canvas.snap_enabled, "Snap to edges")
                .on_hover_text("Dock to monitor edges, halves, thirds, quarters and neighbouring zones without overlap");
            let mut autostart = self.autostart;
            if ui
                .checkbox(&mut autostart, "Start at login")
                .on_hover_text("Start the tray, and with it the hotkeys, when you log in")
                .changed()
            {
                match tray::set_autostart(autostart) {
                    Ok(()) => self.autostart = autostart,
                    Err(e) => self.log.push(format!("Start at login not changed: {e}")),
                }
            }
            ui.separator();
        });
    }

    fn view_header(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui.ctx());
        let blocker = self.launch_blocker();
        let running = self
            .run
            .as_ref()
            .filter(|run| run.view == self.selected_view && run.pending() > 0)
            .map(|run| (run.placed() + run.failed().len(), run.total()));
        let failed = self
            .run
            .as_ref()
            .is_some_and(|run| run.view == self.selected_view && !run.failed().is_empty());
        let subtitle = self.subtitle();
        let Some(view) = self.config.views.get_mut(self.selected_view) else {
            return;
        };
        let mut launch = false;
        let mut save = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    let font = FontId::new(20.0, theme::strong_family());
                    let width = ui
                        .fonts(|fonts| {
                            fonts.layout_no_wrap(view.name.clone(), font.clone(), p.text)
                        })
                        .size()
                        .x;
                    self.dirty |= ui
                        .add(
                            egui::TextEdit::singleline(&mut view.name)
                                .font(font)
                                .frame(false)
                                .desired_width((width + 8.0).clamp(80.0, 320.0)),
                        )
                        .on_hover_text("Rename view")
                        .changed();
                    ui.add_space(12.0);
                    self.dirty |=
                        hotkey_input::show(ui, &mut self.hotkey_recorder, &mut view.hotkey);
                });
                ui.label(RichText::new(subtitle).size(12.0).color(p.muted));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = match running {
                    Some((done, total)) => format!("Launching {done}/{total}"),
                    None if failed => "▶  Launch again".into(),
                    None => "▶  Launch view".into(),
                };
                let response = theme::primary_enabled(ui, blocker.is_none(), &label);
                launch = response.clicked();
                if let Some(reason) = blocker {
                    response.on_disabled_hover_text(reason);
                }
                let save_button = if self.dirty {
                    egui::Button::new("Save · Ctrl+S").stroke(Stroke::new(1.0_f32, p.accent))
                } else {
                    egui::Button::new(RichText::new("Saved").color(p.muted))
                };
                save = ui
                    .add_enabled(self.dirty, save_button.min_size(Vec2::new(0.0, 32.0)))
                    .clicked();
            });
        });
        if launch {
            self.launch(self.selected_view, None);
        }
        if save {
            self.save();
        }
    }

    fn subtitle(&self) -> String {
        let Some(view) = self.config.views.get(self.selected_view) else {
            return String::new();
        };
        let monitors: HashSet<_> = view.zones.iter().map(|zone| &zone.monitor).collect();
        let base = format!(
            "{} zone{} on {} monitor{}",
            view.zones.len(),
            plural(view.zones.len()),
            monitors.len(),
            plural(monitors.len())
        );
        let state = match self
            .run
            .as_ref()
            .filter(|run| run.view == self.selected_view)
        {
            Some(run) if run.pending() > 0 => "launching".to_string(),
            Some(run) if !run.failed().is_empty() => format!(
                "{} zone{} need{} attention",
                run.failed().len(),
                plural(run.failed().len()),
                if run.failed().len() == 1 { "s" } else { "" }
            ),
            _ if self.dirty => "unsaved changes".into(),
            Some(run) => format!("launched {}", launch_run::ago(run.age())),
            None => "not launched yet".into(),
        };
        format!("{base} · {state}")
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui.ctx());
        let has_zones = self
            .config
            .views
            .get(self.selected_view)
            .is_some_and(|view| !view.zones.is_empty());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let draw = ui
                .add(
                    egui::Button::new("⬚  Draw zone")
                        .selected(self.canvas.drawing)
                        .min_size(Vec2::new(0.0, 30.0)),
                )
                .on_hover_text("Drag on a monitor to add a zone");
            if draw.clicked() {
                self.canvas.drawing = !self.canvas.drawing;
            }
            let identify = ui
                .add_enabled(
                    self.identification.active() || !self.monitors.is_empty(),
                    egui::Button::new(if self.identification.active() {
                        "Hide numbers"
                    } else {
                        "Identify monitors"
                    })
                    .min_size(Vec2::new(0.0, 30.0)),
                )
                .on_hover_text("Show matching numbers on your screens for 5 seconds");
            if identify.clicked() {
                if self.identification.active() {
                    self.identification.stop();
                } else {
                    self.identify_return_focus = ui
                        .ctx()
                        .input(|input| input.viewport().focused.unwrap_or(false));
                    self.identification
                        .start(self.backend, self.monitors.clone());
                }
            }
            if theme::button(ui, "⟳  Refresh displays").clicked() {
                self.identification.stop();
                self.refresh();
            }
            if ui
                .add_enabled(
                    has_zones,
                    egui::Button::new("Align edges").min_size(Vec2::new(0.0, 30.0)),
                )
                .on_hover_text("Close gaps and overlaps between neighbouring zones")
                .clicked()
            {
                if let Some(view) = self.config.views.get_mut(self.selected_view) {
                    self.dirty |= self.canvas.align(view);
                }
            }
            if theme::button(ui, "Duplicate view")
                .on_hover_text(
                    "Copy this view with all zones and applications. The copy gets no hotkey.",
                )
                .clicked()
            {
                if let Some(index) = self.config.duplicate_view(self.selected_view) {
                    self.select_view(index);
                    self.dirty = true;
                }
            }
            ui.menu_button("…", |ui| {
                if ui
                    .button(RichText::new("Delete view…").color(p.danger))
                    .clicked()
                {
                    self.confirm_delete = Some(self.selected_view);
                    ui.close_menu();
                }
            });
        });
    }

    fn launch_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui.ctx());
        let mut retry = None;
        ui.horizontal(|ui| {
            match self
                .run
                .as_ref()
                .filter(|run| run.view == self.selected_view)
            {
                Some(run) => {
                    caps(
                        ui,
                        if run.pending() > 0 {
                            "Launching"
                        } else {
                            "Last launch"
                        },
                    );
                    let meta = if run.pending() > 0 {
                        format!(
                            "{} of {} placed · {:.1} s",
                            run.placed(),
                            run.total(),
                            run.elapsed().as_secs_f32()
                        )
                    } else {
                        format!(
                            "{} of {} placed · {:.2} s · {}",
                            run.placed(),
                            run.total(),
                            run.elapsed().as_secs_f32(),
                            launch_run::ago(run.age())
                        )
                    };
                    ui.label(RichText::new(meta).size(12.0).color(p.muted));
                }
                None => {
                    caps(ui, "Last launch");
                    ui.label(
                        RichText::new("Not launched in this session")
                            .size(12.0)
                            .color(p.muted),
                    );
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::ghost(
                    ui,
                    if self.show_log {
                        "Hide log"
                    } else {
                        "Show log"
                    },
                )
                .clicked()
                {
                    self.show_log = !self.show_log;
                }
                if let Some(run) = self
                    .run
                    .as_ref()
                    .filter(|run| run.view == self.selected_view && run.pending() == 0)
                {
                    let failed = run.failed();
                    if !failed.is_empty() {
                        let label = if failed.len() == 1 {
                            format!("Retry zone {}", failed[0] + 1)
                        } else {
                            format!("Retry {} zones", failed.len())
                        };
                        if theme::button(ui, label).clicked() {
                            retry = Some(failed);
                        }
                    }
                }
            });
        });
        if let Some(run) = self
            .run
            .as_ref()
            .filter(|run| run.view == self.selected_view)
        {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for (index, state) in run.states.iter().enumerate() {
                    if let Some(state) = state {
                        status_chip(ui, index, state);
                    }
                }
            });
        }
        if self.show_log {
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(140.0)
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.label(theme::mono(line).size(12.0).color(p.muted));
                    }
                });
        }
        if let Some(zones) = retry {
            self.launch(self.selected_view, Some(zones));
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui.ctx());
        let Some(view) = self.config.views.get_mut(self.selected_view) else {
            return;
        };
        let Some(index) = self.selected_zone.filter(|index| *index < view.zones.len()) else {
            ui.add_space(24.0);
            ui.label(theme::strong("No zone selected"));
            ui.label(
                RichText::new(if view.zones.is_empty() {
                    "Use Draw zone and drag on a monitor to add the first zone."
                } else {
                    "Select a zone on the canvas or in the list to edit it."
                })
                .color(p.muted),
            );
            return;
        };
        let outcome = zone_editor::show(
            ui,
            self.selected_view,
            index,
            &mut view.zones[index],
            &self.monitors,
            &self.applications,
            std::mem::take(&mut self.open_advanced),
        );
        self.dirty |= outcome.changed;
        if outcome.choose_application {
            self.app_picker = Some(AppPicker::new(self.selected_view, index));
        }
        if outcome.delete {
            view.zones.remove(index);
            self.selected_zone = None;
            self.dirty = true;
            self.run = None;
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

    fn first_run(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui.ctx());
        let connected = self.display_error.is_none() && self.backend != Backend::UnsupportedWayland;
        let mut create = false;
        let mut autostart = false;
        let mut identify = false;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.12).max(24.0));
            Frame::NONE
                .fill(p.panel)
                .stroke(Stroke::new(1.0_f32, p.border))
                .corner_radius(10)
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(520.0_f32.min(ui.available_width()));
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 14.0;
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(theme::strong("Set up HerdCtrl").size(20.0));
                            ui.label(
                                RichText::new("Arrange windows across your monitors with one hotkey.")
                                    .color(p.muted),
                            );
                        });
                        let backend_text = match self.backend {
                            Backend::Gnome => (
                                if connected { "GNOME extension connected" } else { "GNOME extension not reachable" },
                                if connected {
                                    "Lets HerdCtrl see monitors and move windows on Wayland."
                                } else {
                                    "Run scripts/install-gnome-extension.sh, log out and back in, then Refresh displays."
                                },
                            ),
                            Backend::X11 => ("X11 session", "Windows are moved through wmctrl."),
                            Backend::UnsupportedWayland => (
                                "Unsupported Wayland desktop",
                                "HerdCtrl supports GNOME 50 with its extension, or an X11 session.",
                            ),
                        };
                        step(ui, 1, connected, backend_text.0, backend_text.1, None);
                        let monitors = format!(
                            "{} monitor{} detected",
                            self.monitors.len(),
                            plural(self.monitors.len())
                        );
                        let names = self
                            .monitors
                            .iter()
                            .map(|monitor| monitor.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        identify = step(
                            ui,
                            2,
                            !self.monitors.is_empty(),
                            &monitors,
                            &format!("{names}. Identify them on your screens anytime."),
                            (!self.monitors.is_empty()).then_some("Identify"),
                        );
                        create = step(
                            ui,
                            3,
                            false,
                            "Create your first view",
                            "Draw zones on your monitors and pick an app for each.",
                            Some("Create view"),
                        );
                        autostart = step(
                            ui,
                            4,
                            self.autostart,
                            "Keep hotkeys running",
                            "The tray listens for hotkeys after you close this window.",
                            (!self.autostart).then_some("Start at login"),
                        );
                    });
                });
        });
        if create {
            self.new_view();
        }
        if identify {
            self.identification
                .start(self.backend, self.monitors.clone());
        }
        if autostart {
            match tray::set_autostart(true) {
                Ok(()) => self.autostart = true,
                Err(e) => self.log.push(format!("Start at login not changed: {e}")),
            }
        }
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn view_card(ui: &mut egui::Ui, view: &View, selected: bool) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::click());
    if selected || response.hovered() {
        ui.painter().rect(
            rect,
            6.0,
            if selected { p.raised } else { p.hover },
            Stroke::new(1.0_f32, if selected { p.border } else { p.hover }),
            egui::StrokeKind::Inside,
        );
    }
    let painter = ui.painter().with_clip_rect(rect.shrink(2.0));
    let mut right = rect.right() - 10.0;
    if !view.hotkey.trim().is_empty() {
        let label: String = view
            .hotkey
            .split('+')
            .map(|part| {
                part.strip_prefix("Key")
                    .or_else(|| part.strip_prefix("Digit"))
                    .filter(|rest| rest.len() == 1)
                    .unwrap_or(part)
            })
            .collect::<Vec<_>>()
            .join("+");
        let galley = painter.layout_no_wrap(label, FontId::monospace(11.0), p.muted);
        let chip = egui::Rect::from_min_size(
            egui::pos2(right - galley.size().x - 10.0, rect.top() + 8.0),
            galley.size() + Vec2::new(10.0, 4.0),
        );
        painter.rect_stroke(
            chip,
            4.0,
            Stroke::new(1.0_f32, p.border),
            egui::StrokeKind::Inside,
        );
        painter.galley(chip.min + Vec2::new(5.0, 2.0), galley, p.muted);
        right = chip.left() - 6.0;
    }
    let name = painter.layout_no_wrap(
        view.name.clone(),
        FontId::new(13.0, theme::strong_family()),
        p.text,
    );
    painter
        .with_clip_rect(egui::Rect::from_x_y_ranges(
            rect.left()..=right,
            rect.y_range(),
        ))
        .galley(
            egui::pos2(rect.left() + 10.0, rect.top() + 8.0),
            name,
            p.text,
        );
    let monitors: HashSet<_> = view.zones.iter().map(|zone| &zone.monitor).collect();
    painter.text(
        egui::pos2(rect.left() + 10.0, rect.bottom() - 8.0),
        Align2::LEFT_BOTTOM,
        format!(
            "{} zone{} · {} monitor{}",
            view.zones.len(),
            plural(view.zones.len()),
            monitors.len(),
            plural(monitors.len())
        ),
        FontId::proportional(12.0),
        p.muted,
    );
    response
}

fn status_chip(ui: &mut egui::Ui, index: usize, state: &ZoneState) {
    let p = theme::palette(ui.ctx());
    let (glyph, tone) = match state {
        ZoneState::Pending => ("…", Tone::Muted),
        ZoneState::Failed(_) => ("!", Tone::Danger),
        _ => ("✓", Tone::Ok),
    };
    let failed = tone == Tone::Danger;
    let icon = ui.painter().layout_no_wrap(
        glyph.into(),
        FontId::new(12.0, theme::strong_family()),
        tone.color(&p),
    );
    let text = ui.painter().layout_no_wrap(
        format!("{} · {}", index + 1, launch_run::describe(state)),
        FontId::proportional(12.0),
        if failed { p.text } else { p.muted },
    );
    // One allocation per chip, so the wrapping row knows its width up front.
    let size = Vec2::new(icon.size().x + text.size().x + 22.0, 24.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(
        rect,
        6.0,
        if failed { p.danger_soft } else { p.panel },
        Stroke::new(1.0_f32, if failed { p.danger } else { p.border }),
        egui::StrokeKind::Inside,
    );
    let y = rect.center().y;
    ui.painter().galley(
        egui::pos2(rect.left() + 8.0, y - icon.size().y / 2.0),
        icon.clone(),
        tone.color(&p),
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 14.0 + icon.size().x, y - text.size().y / 2.0),
        text,
        p.text,
    );
}

/// One checklist row of the first-run card. Returns true when its action was clicked.
fn step(
    ui: &mut egui::Ui,
    number: usize,
    done: bool,
    title: &str,
    text: &str,
    action: Option<&str>,
) -> bool {
    let p = theme::palette(ui.ctx());
    let mut clicked = false;
    ui.horizontal(|ui| {
        let (mark, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
        if done {
            ui.painter().circle_filled(mark.center(), 12.0, p.accent);
            ui.painter().text(
                mark.center(),
                Align2::CENTER_CENTER,
                "✓",
                FontId::new(13.0, theme::strong_family()),
                p.accent_on,
            );
        } else {
            ui.painter()
                .circle_stroke(mark.center(), 11.5, Stroke::new(1.0_f32, p.border));
            ui.painter().text(
                mark.center(),
                Align2::CENTER_CENTER,
                number.to_string(),
                FontId::monospace(12.0),
                p.muted,
            );
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(theme::strong(title));
            ui.label(RichText::new(text).size(12.0).color(p.muted));
        });
        if let Some(action) = action {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                clicked = if done {
                    theme::ghost(ui, action).clicked()
                } else {
                    theme::primary(ui, action).clicked()
                };
            });
        }
    });
    clicked
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
        self.ui(ctx);
    }
}

impl Dashboard {
    fn ui(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(100));
        self.poll(ctx);
        let p = theme::palette(ctx);
        if self.dirty && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dialog = true;
        }
        if !self.hotkey_recorder.is_recording()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S))
            && self.dirty
        {
            self.save();
        }
        // Panels butt against each other with a small gap; paint it in the page color.
        ctx.layer_painter(egui::LayerId::background())
            .rect_filled(ctx.screen_rect(), 0.0, p.bg);
        let bar = |fill| Frame::NONE.fill(fill);
        egui::TopBottomPanel::top("header")
            .frame(bar(p.panel).inner_margin(Margin::symmetric(16, 10)))
            .show(ctx, |ui| self.header(ui));
        egui::SidePanel::left("sidebar")
            .exact_width(250.0)
            .resizable(false)
            .frame(bar(p.panel).inner_margin(Margin::symmetric(12, 16)))
            .show(ctx, |ui| self.sidebar(ui));
        let has_view = !self.config.views.is_empty();
        if has_view {
            egui::SidePanel::right("inspector")
                .exact_width(310.0)
                .resizable(false)
                .frame(bar(p.panel).inner_margin(Margin::same(16)))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.inspector(ui));
                });
            egui::TopBottomPanel::bottom("launch")
                .frame(bar(p.panel).inner_margin(Margin::symmetric(20, 10)))
                .show(ctx, |ui| self.launch_panel(ui));
        }
        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .show(ctx, |ui| {
                if !has_view {
                    self.first_run(ui);
                    return;
                }
                self.view_header(ui);
                ui.add_space(6.0);
                self.toolbar(ui);
                ui.add_space(6.0);
                if let Some(run) = self
                    .run
                    .as_ref()
                    .filter(|run| run.view == self.selected_view)
                {
                    self.canvas.status = (0..run.states.len())
                        .map(|index| run.badge(index))
                        .collect();
                } else {
                    self.canvas.status.clear();
                }
                let Some(view) = self.config.views.get_mut(self.selected_view) else {
                    return;
                };
                let previous_zones = view.zones.len();
                let warning = if canvas::has_overlaps(view) {
                    Some(
                        "Zones overlap. Use Align edges, or move and resize the highlighted zones."
                            .to_string(),
                    )
                } else {
                    self.canvas.feedback.clone()
                };
                if let Some(text) = warning {
                    ui.label(RichText::new(text).size(12.0).color(p.warn));
                }
                let reserved = self.monitors.iter().any(|monitor| {
                    monitor.work_area.is_some_and(|area| {
                        area.width < monitor.width || area.height < monitor.height
                    })
                });
                if reserved {
                    // Leave room for the legend below the canvas.
                    let height = ui.available_height() - 22.0;
                    let width = ui.available_width();
                    ui.allocate_ui(Vec2::new(width, height), |ui| {
                        self.dirty |=
                            self.canvas
                                .show(ui, &self.monitors, view, &mut self.selected_zone);
                    });
                    ui.label(
                        RichText::new("Hatched areas: top bar and dock stay free")
                            .size(12.0)
                            .color(p.muted),
                    );
                } else {
                    self.dirty |=
                        self.canvas
                            .show(ui, &self.monitors, view, &mut self.selected_zone);
                }
                if view.zones.len() > previous_zones {
                    self.app_picker =
                        Some(AppPicker::new(self.selected_view, view.zones.len() - 1));
                }
            });
        self.show_delete_view_dialog(ctx);
        self.show_close_dialog(ctx);
        self.show_app_picker(ctx);
    }
}

#[cfg(test)]
mod snapshots;

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
