//! Opt-in tests against a real EWMH window manager, including Xvfb + Openbox.
use super::*;
use crate::core::{config::Zone, monitor::Bounds};
use std::{fs, sync::mpsc};

struct TestWindow(String);

impl Drop for TestWindow {
    fn drop(&mut self) {
        let _ = linux::output("wmctrl", &["-ic", &self.0]);
    }
}

fn window_named(title: &str) -> Option<(String, [i32; 4])> {
    linux::output("wmctrl", &["-lG"])
        .expect("wmctrl -lG must work")
        .lines()
        .find_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 8 || fields[7..].join(" ") != title {
                return None;
            }
            Some((
                fields[0].into(),
                std::array::from_fn(|i| fields[i + 2].parse().unwrap()),
            ))
        })
}

/// Client geometry from `xwininfo`. `wmctrl -lG` adds Openbox's title bar to y a second
/// time, which made placement look 40 px too low.
fn client_geometry(id: &str) -> [i32; 4] {
    let info = linux::output("xwininfo", &["-id", id]).expect("xwininfo must work");
    let value = |label: &str| {
        info.lines()
            .find_map(|line| line.trim().strip_prefix(label)?.trim().parse().ok())
            .unwrap_or_else(|| panic!("xwininfo has no {label}"))
    };
    [
        value("Absolute upper-left X:"),
        value("Absolute upper-left Y:"),
        value("Width:"),
        value("Height:"),
    ]
}

fn await_window(title: &str) -> TestWindow {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some((id, _)) = window_named(title) {
            return TestWindow(id);
        }
        assert!(Instant::now() < deadline, "No test window: {title}");
        thread::sleep(Duration::from_millis(50));
    }
}

fn view(monitor: &str, command: String) -> View {
    View {
        name: "X11 acceptance".into(),
        hotkey: String::new(),
        zones: vec![Zone {
            name: "Acceptance zone".into(),
            monitor: monitor.into(),
            command,
            application: None,
            terminal_command: String::new(),
            run_in_terminal: false,
            delay_ms: 150,
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
#[ignore = "requires X11, an EWMH window manager, wmctrl, xrandr and xterm"]
fn x11_launch_positions_windows_on_each_monitor() {
    let monitors = linux::monitors().expect("Use an X11 session");
    let title = format!("herdctrl-unrelated-{}", std::process::id());
    let mut unrelated = Command::new("timeout")
        .args(["30s", "xterm", "-T", &title, "-e", "sleep", "25"])
        .spawn()
        .unwrap();
    let guard = await_window(&title);
    let original = window_named(&title).unwrap().1;
    for (index, monitor) in monitors.iter().enumerate() {
        let target = format!("herdctrl-target-{}-{index}", std::process::id());
        // A finite lifetime also cleans up if launch fails before a handle is found.
        let layout = view(
            &monitor.name,
            format!("timeout 20s xterm -T {target} -e sleep 15"),
        );
        let expected = layout.zones[0].bounds.pixels(monitor);
        let (tx, rx) = mpsc::channel();
        launch(layout, tx);
        let message = rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .to_string();
        assert!(message.ends_with(": positioned"), "{message}");
        let target_guard = await_window(&target);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let actual = client_geometry(&target_guard.0);
            let expected = [expected.0, expected.1, expected.2 as i32, expected.3 as i32];
            // Xterm uses character increments, and the zone holds the WM frame,
            // so the client sits up to a title bar below and inside the target.
            let fits = actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| (actual - expected).abs() <= 32);
            if fits {
                eprintln!("{}: target {expected:?}, observed {actual:?}", monitor.name);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{}: target {expected:?}, observed {actual:?}",
                monitor.name
            );
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            window_named(&title).unwrap().1,
            original,
            "Unrelated window moved"
        );
    }
    drop(guard);
    let _ = unrelated.kill();
    let _ = unrelated.wait();
}

#[test]
#[ignore = "requires X11, an EWMH window manager, wmctrl and xrandr"]
fn x11_no_window_reports_timeout_without_blocking_caller() {
    let monitors = linux::monitors().unwrap();
    let (tx, rx) = mpsc::channel();
    let start = Instant::now();
    launch(view(&monitors[0].name, "true".into()), tx);
    assert!(start.elapsed() < Duration::from_millis(100));
    let message = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .to_string();
    assert!(
        message.contains("No new window belonging to this process"),
        "{message}"
    );
    assert!(start.elapsed() >= Duration::from_secs(5));
}

#[test]
#[ignore = "requires X11 and xrandr"]
fn x11_disconnected_monitor_does_not_spawn_command() {
    linux::monitors().unwrap();
    let marker = std::env::temp_dir().join(format!("herdctrl-no-spawn-{}", std::process::id()));
    assert!(!marker.exists(), "Remove stale marker {}", marker.display());
    // Quote the temporary path, including any apostrophes in TMPDIR.
    let command = format!(
        "touch '{}'",
        marker.display().to_string().replace('\'', "'\\''")
    );
    let (tx, rx) = mpsc::channel();
    launch(view("herdctrl-nonexistent-display", command), tx);
    let message = rx.recv_timeout(Duration::from_secs(4)).unwrap().to_string();
    let spawned = marker.exists();
    if spawned {
        let _ = fs::remove_file(marker);
    }
    assert!(message.contains("is disconnected"), "{message}");
    assert!(!spawned, "Command ran despite missing monitor");
}
