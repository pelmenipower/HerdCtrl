//! Renders the editor's main states to PNG files for visual review against the mockups.
//! Opt-in, because it needs a GPU adapter: `cargo test --locked render_ui_snapshots -- --ignored`.
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

fn monitors() -> Vec<Monitor> {
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

fn config() -> Config {
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

fn render(name: &str, theme: Theme, setup: impl FnOnce(&mut Dashboard)) {
    let ctx = egui::Context::default();
    let mut dashboard = Dashboard::with_parts(&ctx, config(), PathBuf::new(), None);
    dashboard.monitors = monitors();
    dashboard.selected_zone = Some(1);
    dashboard.tray_running = true;
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
    setup(&mut dashboard);
    // Fonts set in one frame apply from the next, so the first frame only installs the theme.
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
    harness.run_steps(4);
    let image = harness.render().expect("render with a wgpu adapter");
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-snapshots");
    std::fs::create_dir_all(&directory).unwrap();
    image.save(directory.join(format!("{name}.png"))).unwrap();
}

#[test]
#[ignore = "needs a GPU adapter; writes PNGs to target/ui-snapshots"]
fn render_ui_snapshots() {
    for (suffix, theme) in [("dark", Theme::Dark), ("light", Theme::Light)] {
        render(&format!("edit-{suffix}"), theme, |_| {});
        render(&format!("launching-{suffix}"), theme, |d| {
            let mut run = LaunchRun::new(0, 5, (0..5).collect());
            for zone in 0..3 {
                run.apply(&report(zone, Outcome::Arranged));
            }
            d.run = Some(run);
        });
        render(&format!("failed-{suffix}"), theme, |d| {
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
        });
        render(&format!("picker-{suffix}"), theme, |d| {
            d.app_picker = Some(AppPicker::new(0, 1));
        });
        render(&format!("recording-{suffix}"), theme, |d| {
            d.hotkey_recorder = Recorder::recording();
            d.dirty = true;
        });
        render(&format!("first-run-{suffix}"), theme, |d| {
            d.config.views.clear();
            d.selected_zone = None;
            d.tray_running = false;
        });
    }
}
