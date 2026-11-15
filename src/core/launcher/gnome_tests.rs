//! Explicitly opted-in checks against the user's real GNOME compositor.
use super::*;
use crate::core::{config::Zone, monitor::Bounds};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::mpsc};

struct Sample(PathBuf);

impl Drop for Sample {
    fn drop(&mut self) {
        let _ = fs::write(self.0.with_extension("stop"), "stop");
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn view(monitor: &str, command: String) -> View {
    View {
        name: "Live GNOME acceptance".into(),
        hotkey: String::new(),
        zones: vec![Zone {
            name: "Acceptance zone".into(),
            monitor: monitor.into(),
            command,
            application: None,
            terminal_command: String::new(),
            run_in_terminal: false,
            delay_ms: 200,
            bounds: Bounds {
                x: 0.1,
                y: 0.15,
                width: 0.4,
                height: 0.5,
            },
        }],
    }
}

#[test]
#[ignore = "opens temporary windows on real GNOME; set HERDCTRL_LIVE_GNOME=1"]
fn gnome_live_launch_and_resize_on_each_monitor() {
    assert_eq!(std::env::var("HERDCTRL_LIVE_GNOME").as_deref(), Ok("1"));
    assert_eq!(Backend::detect(), Backend::Gnome);
    let monitors = Backend::Gnome.monitors().unwrap();
    let directory = std::env::temp_dir().join(format!(
        "herdctrl-gnome-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/gnome-test-window.py");
    let mut samples = Vec::new();
    for backend in ["wayland", "x11"] {
        for (index, monitor) in monitors.iter().enumerate() {
            let report = Sample(directory.join(format!("{backend}-{index}.json")));
            let layout = view(
                &monitor.name,
                format!(
                    "exec python3 {} --backend {backend} --report {}",
                    quote(helper.to_str().unwrap()),
                    quote(report.0.to_str().unwrap())
                ),
            );
            let expected = layout.zones[0].bounds.pixels(monitor);
            let (tx, rx) = mpsc::channel();
            launch(layout, tx);
            let message = rx
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .to_string();
            assert!(message.ends_with(": positioned"), "{backend}: {message}");
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let sample = fs::read_to_string(&report.0)
                    .ok()
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok());
                if let Some(sample) = sample {
                    let size_matches = sample["width"] == expected.2
                        && sample["height"] == expected.3
                        && sample["monitor"] == monitor.name;
                    let position_matches = backend == "wayland"
                        || sample["xwininfo"].as_str().is_some_and(|info| {
                            [
                                ("Absolute upper-left X:", expected.0),
                                ("Absolute upper-left Y:", expected.1),
                            ]
                            .iter()
                            .all(|(label, value)| {
                                info.lines().any(|line| {
                                    line.trim()
                                        .strip_prefix(label)
                                        .and_then(|s| s.trim().parse::<i32>().ok())
                                        == Some(*value)
                                })
                            })
                        });
                    if size_matches && position_matches {
                        eprintln!(
                            "{backend} {}: target {expected:?}, observed {sample}",
                            monitor.name
                        );
                        samples.push(
                            json!({"monitor":monitor.name,"expected":expected,"observed":sample}),
                        );
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "{backend} {}: target {expected:?}, observed {sample}",
                        monitor.name
                    );
                }
                assert!(Instant::now() < deadline, "No GTK report");
                thread::sleep(Duration::from_millis(100));
            }
            drop(report);
        }
    }
    let (tx, rx) = mpsc::channel();
    launch(view(&monitors[0].name, "true".into()), tx);
    let message = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .to_string();
    assert!(
        message.contains("No new window belonging to this process"),
        "{message}"
    );
    let (tx, rx) = mpsc::channel();
    let marker = directory.join("must-not-exist");
    launch(
        view(
            "herdctrl-disconnected",
            format!("touch {}", quote(marker.to_str().unwrap())),
        ),
        tx,
    );
    let message = rx.recv_timeout(Duration::from_secs(4)).unwrap().to_string();
    assert!(message.contains("is disconnected"), "{message}");
    assert!(!marker.exists());
    let output = json!({
        "samples":samples,
        "no_window_timeout":"passed",
        "disconnected_monitor_does_not_spawn":"passed",
        "limitations":"Native Wayland checks client size and monitor, not absolute window origin. Undecorated GTK test windows; no fractional scaling or hotplug acceptance."
    });
    let output_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/gnome-live-acceptance.json");
    fs::write(&output_path, serde_json::to_vec_pretty(&output).unwrap()).unwrap();
    eprintln!("Report: {}", output_path.display());
}

#[test]
#[ignore = "opens a dedicated temporary terminal; set HERDCTRL_LIVE_GNOME=1"]
fn gnome_live_terminal_program_has_tty_and_reuses_its_window() {
    assert_eq!(std::env::var("HERDCTRL_LIVE_GNOME").as_deref(), Ok("1"));
    assert_eq!(Backend::detect(), Backend::Gnome);
    let monitors = Backend::Gnome.monitors().unwrap();
    let apps = applications::discover();
    let terminal = apps
        .iter()
        .find(|app| app.application.id == "org.gnome.Ptyxis")
        .expect("This acceptance test requires Ptyxis");
    let marker = std::env::temp_dir().join(format!(
        "herdctrl-tty-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let command = format!(
        "if test -t 0 && test -t 1; then printf 'tty-ok\\n' >> {}; fi; sleep 25",
        quote(marker.to_str().unwrap())
    );
    let mut layout = view(&monitors[0].name, String::new());
    layout.zones[0].application = Some(terminal.application.clone());
    layout.zones[0].terminal_command = command.clone();
    let before: HashSet<_> = Backend::Gnome
        .windows()
        .unwrap()
        .into_iter()
        .map(|w| w.id)
        .collect();
    let (tx, rx) = mpsc::channel();
    launch(layout.clone(), tx.clone());
    let first = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .to_string();
    assert!(first.ends_with(": positioned"), "{first}");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    let windows = Backend::Gnome.windows().unwrap();
    let owned: Vec<_> = windows
        .iter()
        .filter(|w| {
            !before.contains(&w.id)
                && matching::ProcessIdentity::read(w.pid)
                    .terminal_command
                    .as_deref()
                    == Some(&command)
        })
        .cloned()
        .collect();
    assert_eq!(
        owned.len(),
        1,
        "Expected exactly one dedicated test terminal"
    );
    struct Cleanup(u32);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = Command::new("kill")
                .args(["-TERM", &self.0.to_string()])
                .status();
        }
    }
    let _cleanup = Cleanup(owned[0].pid);
    assert_eq!(
        fs::read_to_string(&marker).unwrap(),
        "tty-ok\n",
        "Program did not receive a terminal"
    );
    launch(layout.clone(), tx.clone());
    launch(layout, tx);
    for _ in 0..2 {
        let reused = rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .to_string();
        assert!(reused.ends_with(": existing window arranged"), "{reused}");
    }
    let after = Backend::Gnome.windows().unwrap();
    let owned_after: Vec<_> = after
        .iter()
        .filter(|w| {
            matching::ProcessIdentity::read(w.pid)
                .terminal_command
                .as_deref()
                == Some(&command)
        })
        .collect();
    assert_eq!(
        owned_after.len(),
        1,
        "Repeated launches created duplicate terminals"
    );
    assert_eq!(owned_after[0].id, owned[0].id);
    assert_eq!(
        fs::read_to_string(&marker).unwrap(),
        "tty-ok\n",
        "Terminal program ran more than once"
    );
    eprintln!(
        "TTY and reuse acceptance passed: window {} PID {}",
        owned[0].id, owned[0].pid
    );
}
