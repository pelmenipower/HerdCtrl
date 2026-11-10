//! Renders the editor's main states to PNG files for visual review against the mockups, and the
//! README gallery in `docs/images`. Opt-in, because both need a GPU adapter:
//! `cargo test --locked render_ui_snapshots -- --ignored` and
//! `cargo test --locked render_docs_screenshots -- --ignored`.
use super::*;
use crate::core::{config::Application, launcher::Outcome, monitor::Area};
use egui_kittest::Harness;

fn monitor(name: &str, x: i32, y: i32, height: u32, work_area: Option<Area>) -> Monitor {
    Monitor {
        name: name.into(),
        x,
        y,
        width: 1920,
        height,
        scale: 1.0,
        work_area,
    }
}

pub(super) fn monitors() -> Vec<Monitor> {
    vec![
        monitor("DP-2", 0, 0, 1200, None),
        monitor("DP-7", 1920, 0, 1200, None),
        monitor(
            "DP-5",
            0,
            1200,
            1080,
            Some(Area {
                x: 0,
                y: 1230,
                width: 1920,
                height: 991,
            }),
        ),
        monitor("DP-6", 1920, 1200, 1080, None),
    ]
}

fn app(id: &str, name: &str, description: &str, terminal: bool) -> InstalledApp {
    InstalledApp {
        application: Application {
            id: id.into(),
            name: name.into(),
        },
        description: description.into(),
        argv: vec!["/bin/true".into()],
        directory: None,
        desktop_file: format!("/usr/share/applications/{id}.desktop").into(),
        is_terminal: terminal,
        terminal_exec: terminal.then(|| vec!["--".into()]),
        new_window_argv: None,
    }
}

pub(super) fn config() -> Config {
    serde_json::from_value(serde_json::json!({"views": [
        {"name": "Workstation", "hotkey": "Ctrl+Shift+KeyD", "zones": [
            {"name": "Browser main", "monitor": "DP-6", "application": {"id": "zen", "name": "Zen Browser"},
             "bounds": {"x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0}},
            {"name": "herdr", "monitor": "DP-5", "application": {"id": "org.gnome.Ptyxis", "name": "Ptyxis"},
             "terminal_command": "herdr", "bounds": {"x": 0.0, "y": 0.0, "width": 0.5, "height": 1.0}},
            {"name": "Shellherd", "monitor": "DP-7", "application": {"id": "shellherd", "name": "Shellherd"},
             "bounds": {"x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0}},
            {"name": "Browser docs", "monitor": "DP-2", "application": {"id": "zen", "name": "Zen Browser"},
             "bounds": {"x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0}},
            {"name": "htop", "monitor": "DP-5", "application": {"id": "org.gnome.Ptyxis", "name": "Ptyxis"},
             "terminal_command": "htop", "bounds": {"x": 0.5, "y": 0.0, "width": 0.5, "height": 1.0}}
        ]},
        {"name": "Writing", "hotkey": "Super+Alt+KeyW", "zones": [
            {"name": "Editor", "monitor": "DP-5", "application": {"id": "org.gnome.TextEditor", "name": "Text Editor"},
             "bounds": {"x": 0.0, "y": 0.0, "width": 0.6, "height": 1.0}}
        ]}
    ]}))
    .unwrap()
}

fn report(zone: usize, outcome: Outcome) -> Report {
    Report {
        zone: Some(zone),
        name: format!("Zone {}", zone + 1),
        outcome,
    }
}

/// The editor with sample views, monitors and applications, and no background workers.
pub(super) fn dashboard(path: PathBuf) -> Dashboard {
    let ctx = egui::Context::default();
    let mut dashboard = Dashboard::with_parts(&ctx, config(), path, None);
    dashboard.monitors = monitors();
    dashboard.selected_zone = Some(1);
    dashboard.tray_running = true;
    // Fixed, so images don't depend on this machine's autostart entry.
    dashboard.autostart = true;
    dashboard.applications = vec![
        app("zen", "Zen Browser", "Browse the web", false),
        app(
            "org.gnome.Ptyxis",
            "Ptyxis",
            "Container-oriented terminal",
            true,
        ),
        app("shellherd", "Shellherd", "", false),
        app("org.gnome.Console", "Console", "Terminal emulator", true),
        app(
            "org.gnome.TextEditor",
            "Text Editor",
            "Edit text files",
            false,
        ),
        app(
            "org.gnome.Nautilus",
            "Files",
            "Access and organize files",
            false,
        ),
    ];
    dashboard
}

/// A 1280×800 harness. Fonts set in one frame apply from the next, so the first frame only
/// installs the theme.
pub(super) fn harness<'a>(dashboard: Dashboard, theme: Theme) -> Harness<'a, Dashboard> {
    let mut installed = false;
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_state(
            move |ctx, dashboard: &mut Dashboard| {
                if installed {
                    dashboard.ui(ctx);
                } else {
                    theme::install(ctx);
                    ctx.set_theme(theme);
                    installed = true;
                }
            },
            dashboard,
        );
    harness.run_steps(2);
    harness
}

fn render(name: &str, theme: Theme, setup: impl FnOnce(&mut Dashboard)) {
    render_to("target/ui-snapshots", name, theme, setup);
}

fn render_to(directory: &str, name: &str, theme: Theme, setup: impl FnOnce(&mut Dashboard)) {
    let mut dashboard = dashboard(PathBuf::new());
    setup(&mut dashboard);
    let mut harness = harness(dashboard, theme);
    harness.run_steps(2);
    let image = harness.render().expect("render with a wgpu adapter");
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(directory);
    std::fs::create_dir_all(&directory).unwrap();
    image.save(directory.join(format!("{name}.png"))).unwrap();
}

fn launching(d: &mut Dashboard) {
    let mut run = LaunchRun::new(0, 5, (0..5).collect());
    for zone in 0..3 {
        run.apply(&report(zone, Outcome::Arranged));
    }
    d.run = Some(run);
}

fn failed(d: &mut Dashboard) {
    let mut run = LaunchRun::new(0, 5, (0..5).collect());
    for zone in [0, 1, 2] {
        run.apply(&report(zone, Outcome::Arranged));
    }
    run.apply(&report(4, Outcome::Opened));
    run.apply(&report(
        3,
        Outcome::Failed("No new Zen Browser window appeared within 5 s.".into()),
    ));
    d.run = Some(run);
    d.selected_zone = Some(3);
}

fn picker(d: &mut Dashboard) {
    d.app_picker = Some(AppPicker::new(0, 1));
}

fn recording(d: &mut Dashboard) {
    d.hotkey_recorder = Recorder::recording();
    d.dirty = true;
}

fn first_run(d: &mut Dashboard) {
    d.config.views.clear();
    d.selected_zone = None;
    d.tray_running = false;
    d.autostart = false;
}

#[test]
#[ignore = "needs a GPU adapter; writes PNGs to target/ui-snapshots"]
fn render_ui_snapshots() {
    for (suffix, theme) in [("dark", Theme::Dark), ("light", Theme::Light)] {
        render(&format!("edit-{suffix}"), theme, |_| {});
        render(&format!("launching-{suffix}"), theme, launching);
        render(&format!("failed-{suffix}"), theme, failed);
        render(&format!("picker-{suffix}"), theme, picker);
        render(&format!("recording-{suffix}"), theme, recording);
        render(&format!("first-run-{suffix}"), theme, first_run);
    }
}

/// Regenerates the README and manual gallery. Review every image before committing it.
#[test]
#[ignore = "needs a GPU adapter; overwrites the PNGs in docs/images"]
fn render_docs_screenshots() {
    let docs = "docs/images";
    render_to(docs, "screenshot-editor", Theme::Dark, |_| {});
    render_to(docs, "screenshot-editor-light", Theme::Light, |_| {});
    render_to(docs, "screenshot-launching", Theme::Dark, launching);
    render_to(docs, "screenshot-launch-failed", Theme::Light, failed);
    render_to(docs, "screenshot-app-picker", Theme::Dark, picker);
    render_to(docs, "screenshot-hotkey", Theme::Dark, recording);
    render_to(docs, "screenshot-first-run", Theme::Light, first_run);
}
