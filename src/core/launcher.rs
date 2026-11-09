use super::{applications, config::View};
use crate::os::{linux, Backend};
use std::{
    collections::{HashMap, HashSet},
    process::{Command, Stdio},
    sync::mpsc::Sender,
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
mod gnome_tests;
#[cfg(test)]
mod x11_tests;

// Serialize view runs on a worker so rapid repeated clicks cannot spawn duplicates.
static VIEW_RUN: std::sync::Mutex<()> = std::sync::Mutex::new(());
mod matching;

pub fn launch(view: View, events: Sender<String>) {
    let backend = Backend::detect();
    thread::spawn(move || {
        let _view_run = VIEW_RUN.lock().unwrap_or_else(|error| error.into_inner());
        let apps = if view
            .zones
            .iter()
            .any(|zone| zone.application.is_some() || zone.run_in_terminal)
        {
            applications::discover()
        } else {
            Vec::new()
        };
        let monitors = match backend.monitors() {
            Ok(monitors) => monitors,
            Err(error) => {
                let _ = events.send(error);
                return;
            }
        };
        let windows = match backend.windows() {
            Ok(windows) => windows,
            Err(error) => {
                let _ = events.send(error);
                return;
            }
        };
        let initial: HashSet<_> = windows.iter().map(|window| window.id.clone()).collect();
        let claimed = std::sync::Mutex::new(HashSet::new());
        let roots = std::sync::Mutex::new(HashMap::new());
        let selections: Vec<_> = view
            .zones
            .iter()
            .map(|zone| select_app(zone, &apps))
            .collect();
        let app_locks: HashMap<String, std::sync::Mutex<()>> = app_lock_keys(
            selections
                .iter()
                .map(|selection| selection.as_ref().ok()?.as_ref())
                .map(|app| app.map(|app| app.application.id.as_str())),
        )
        .into_iter()
        .map(|key| (key, std::sync::Mutex::new(())))
        .collect();
        let run = Run {
            backend,
            initial: &initial,
            claimed: &claimed,
            roots: &roots,
        };
        thread::scope(|scope| {
            for (index, (zone, selection)) in view.zones.into_iter().zip(selections).enumerate() {
                let Some(monitor) = monitors
                    .iter()
                    .find(|monitor| monitor.name == zone.monitor)
                    .cloned()
                else {
                    let _ = events.send(format!(
                        "{}: monitor {} is disconnected",
                        zone.name, zone.monitor
                    ));
                    continue;
                };
                let selected_app = match selection {
                    Ok(app) => app,
                    Err(error) => {
                        let _ = events.send(format!("{}: {error}", zone.name));
                        continue;
                    }
                };
                let script = script(&zone);
                if !script.trim().is_empty()
                    && selected_app.as_ref().is_some_and(|app| !app.is_terminal)
                {
                    let _ = events.send(format!(
                        "{}: choose a terminal application to run a terminal program",
                        zone.name
                    ));
                    continue;
                }
                let matcher = selected_app
                    .as_ref()
                    .map(|app| matching::AppMatcher::new(app, script));
                let reused = matcher.as_ref().and_then(|matcher| {
                    let mut claimed = claimed.lock().unwrap_or_else(|error| error.into_inner());
                    matching::reserve(
                        windows
                            .iter()
                            .filter(|window| matcher.reusable(window, &windows))
                            .cloned(),
                        &mut claimed,
                    )
                });
                let app_lock = selected_app
                    .as_ref()
                    .and_then(|app| app_locks.get(&app.application.id));
                let events = events.clone();
                scope.spawn(move || {
                    let bounds = zone.bounds.pixels(&monitor);
                    if let Some(window) = reused {
                        let message = match backend.position(&window, &monitor, bounds) {
                            Ok(()) => format!("{}: existing window arranged", zone.name),
                            Err(error) => format!("{}: {error}", zone.name),
                        };
                        let _ = events.send(message);
                        return;
                    }
                    let placed = {
                        let _guard = app_lock
                            .map(|lock| lock.lock().unwrap_or_else(|error| error.into_inner()));
                        launch_new(
                            index,
                            &zone,
                            selected_app.as_ref(),
                            matcher.as_ref(),
                            &monitor,
                            run,
                        )
                    };
                    match placed {
                        Ok(window) => {
                            let _ = events.send(format!("{}: positioned", zone.name));
                            settle(backend, &window, &monitor, bounds);
                        }
                        Err(error) => {
                            let _ = events.send(format!("{}: {error}", zone.name));
                        }
                    }
                });
            }
        });
    });
}

#[derive(Clone, Copy)]
struct Run<'a> {
    backend: Backend,
    initial: &'a HashSet<String>,
    claimed: &'a std::sync::Mutex<HashSet<String>>,
    roots: &'a std::sync::Mutex<HashMap<usize, u32>>,
}

struct RootGuard<'a> {
    roots: &'a std::sync::Mutex<HashMap<usize, u32>>,
    index: usize,
}

impl Drop for RootGuard<'_> {
    fn drop(&mut self) {
        self.roots
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.index);
    }
}

fn script(zone: &super::config::Zone) -> &str {
    if zone.run_in_terminal {
        &zone.command
    } else {
        &zone.terminal_command
    }
}

fn select_app(
    zone: &super::config::Zone,
    apps: &[applications::InstalledApp],
) -> Result<Option<applications::InstalledApp>, String> {
    if let Some(selected) = &zone.application {
        apps.iter()
            .find(|app| app.application.id == selected.id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "{} is no longer available. Choose another application for this zone.",
                    selected.name
                )
            })
            .map(Some)
    } else if zone.run_in_terminal {
        applications::default_terminal(apps).map(Some)
    } else {
        Ok(None)
    }
}

/// Application ids whose launches must not overlap. Terminal servers can open every window of
/// all their zones inside one process, so script zones need the same lock as plain ones.
fn app_lock_keys<'a>(apps: impl IntoIterator<Item = Option<&'a str>>) -> HashSet<String> {
    apps.into_iter().flatten().map(str::to_string).collect()
}

/// Pauses before re-applying the placement of a newly opened window.
const SETTLE_MS: [u64; 3] = [250, 500, 1000];

/// Freshly started apps (e.g. Ptyxis with restore-window-size, Zen) may apply
/// their saved size or position after the window first appears, overriding
/// the zone. Re-apply it a few times; stop once the window is gone.
fn settle(
    backend: Backend,
    window: &linux::Window,
    monitor: &super::monitor::Monitor,
    bounds: (i32, i32, u32, u32),
) {
    for pause in SETTLE_MS {
        thread::sleep(Duration::from_millis(pause));
        if backend.position(window, monitor, bounds).is_err() {
            return;
        }
    }
}

fn launch_new(
    index: usize,
    zone: &super::config::Zone,
    app: Option<&applications::InstalledApp>,
    matcher: Option<&matching::AppMatcher>,
    monitor: &super::monitor::Monitor,
    run: Run,
) -> Result<linux::Window, String> {
    let script = script(zone);
    // Checked under the app lock so a same-app zone that just launched is already visible.
    let app_running = match matcher {
        Some(matcher) if script.trim().is_empty() => run
            .backend
            .windows()?
            .iter()
            .any(|window| matcher.owns_pid(window.pid)),
        _ => false,
    };
    // Set when the app is running but offers no New Window action, so no second window can open.
    let mut single_instance = None;
    let mut command = if let Some(app) = app {
        if !script.trim().is_empty() {
            app.terminal_command(script)?
        } else if let Some(command) = app_running.then(|| app.new_window_command()).flatten() {
            command
        } else {
            if app_running {
                single_instance = Some(app.application.name.as_str());
            }
            app.command()
        }
    } else if !zone.command.trim().is_empty() {
        let mut command = Command::new("sh");
        command.args(["-c", &zone.command]);
        command
    } else {
        return Err("Choose an application for this zone before launching".into());
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start application: {error}"))?;
    let pid = child.id();
    run.roots
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(index, pid);
    let _root = RootGuard {
        roots: run.roots,
        index,
    };
    let (exit_tx, exit_rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        if let Ok(status) = child.wait() {
            let _ = exit_tx.send(status);
        }
    });
    let delay_end = Instant::now() + Duration::from_millis(zone.delay_ms);
    let deadline = delay_end + Duration::from_secs(5);
    let mut pids = HashSet::new();
    loop {
        linux::descendants(pid, &mut pids);
        if Instant::now() >= delay_end {
            let windows = run.backend.windows()?;
            let mut claimed = run
                .claimed
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let other_roots: HashSet<u32> = run
                .roots
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .iter()
                .filter(|(zone, _)| **zone != index)
                .map(|(_, root)| *root)
                .collect();
            let mut windows_per_pid: HashMap<u32, usize> = HashMap::new();
            for window in &windows {
                *windows_per_pid.entry(window.pid).or_default() += 1;
            }
            let weak = |window: &linux::Window| {
                matcher.is_some_and(|matcher| matcher.owns_pid(window.pid))
                    && !linux::has_ancestor(window.pid, &other_roots)
                    && script_matches(
                        matching::ProcessIdentity::read(window.pid)
                            .terminal_command
                            .as_deref(),
                        script,
                        windows_per_pid
                            .get(&window.pid)
                            .is_some_and(|count| *count > 1),
                    )
            };
            let window = pick(windows, run.initial, &pids, weak, &mut claimed);
            drop(claimed);
            if let Some(window) = window {
                return run
                    .backend
                    .position(&window, monitor, zone.bounds.pixels(monitor))
                    .map(|()| window);
            }
        }
        if let Ok(status) = exit_rx.try_recv() {
            if !status.success() {
                return Err(format!("Program exited with {status}. For terminal programs, enable Run in terminal or choose a terminal application."));
            }
        }
        if Instant::now() >= deadline {
            if let Some(name) = single_instance {
                return Err(format!("{name} is already running and its desktop entry offers no New Window action, so no second window opened. Open another window manually, then launch the view again."));
            }
            return Err(match app {
                Some(app) => format!("No new {} window appeared within 5 s.", app.application.name),
                None => "No new window belonging to this process appeared within 5 s. Select a terminal application for terminal programs.".into(),
            });
        }
        thread::sleep(Duration::from_millis(80));
    }
}

/// Whether a fresh window of the app may be this zone's window. A window of a shared server
/// process cannot carry its own terminal marker, so the app lock is what keeps it unambiguous.
fn script_matches(marker: Option<&str>, script: &str, shared_process: bool) -> bool {
    script.trim().is_empty() || shared_process || marker == Some(script)
}

fn pick(
    windows: Vec<linux::Window>,
    initial: &HashSet<String>,
    own: &HashSet<u32>,
    weak: impl Fn(&linux::Window) -> bool,
    claimed: &mut HashSet<String>,
) -> Option<linux::Window> {
    let fresh: Vec<_> = windows
        .into_iter()
        .filter(|window| !initial.contains(&window.id) && !claimed.contains(&window.id))
        .collect();
    let strong: Vec<_> = fresh
        .iter()
        .filter(|window| own.contains(&window.pid))
        .cloned()
        .collect();
    let tier = if strong.is_empty() {
        fresh.into_iter().filter(|window| weak(window)).collect()
    } else {
        strong
    };
    matching::reserve(tier, claimed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn window(id: &str, pid: u32) -> linux::Window {
        linux::Window { id: id.into(), pid }
    }

    #[test]
    fn every_selected_app_gets_a_lock_key() {
        let keys = app_lock_keys([Some("a"), Some("a"), Some("b"), None, Some("c")]);
        assert_eq!(
            keys,
            HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn script_matches_marker_or_shared_process() {
        // Empty scripts never need a marker.
        assert!(script_matches(None, "", false));
        assert!(script_matches(Some("htop"), " ", false));
        // A matching marker always passes.
        assert!(script_matches(Some("htop"), "htop", false));
        // Without a marker or a shared process the window is rejected.
        assert!(!script_matches(None, "htop", false));
        assert!(!script_matches(Some("top"), "htop", false));
        // A shared server process cannot identify its window, so the serialized launch decides.
        assert!(script_matches(None, "htop", true));
        assert!(script_matches(Some("top"), "htop", true));
    }

    #[test]
    fn pick_prefers_own_tree_and_ignores_existing_or_claimed_windows() {
        let existing = HashSet::from(["old".to_string()]);
        let own = HashSet::from([124]);
        let windows = || {
            vec![
                window("old", 124),
                window("unrelated", 999),
                window("weak", 200),
                window("child", 124),
            ]
        };
        let mut claimed = HashSet::new();
        let weak = |window: &linux::Window| window.pid == 200;
        let picked = pick(windows(), &existing, &own, weak, &mut claimed).unwrap();
        assert_eq!(picked.id, "child");
        assert!(pick(windows(), &existing, &own, weak, &mut claimed)
            .is_some_and(|window| window.id == "weak"));
        assert!(pick(windows(), &existing, &own, weak, &mut claimed).is_none());
    }

    #[test]
    fn pick_uses_weak_matches_only_without_own_windows() {
        let own = HashSet::from([124]);
        let windows = vec![window("weak", 200), window("rejected", 201)];
        let mut claimed = HashSet::new();
        let picked = pick(
            windows,
            &HashSet::new(),
            &own,
            |window| window.id == "weak",
            &mut claimed,
        );
        assert_eq!(picked.map(|window| window.id), Some("weak".into()));
        let mut claimed = HashSet::new();
        let picked = pick(
            vec![window("weak", 200), window("child", 124)],
            &HashSet::new(),
            &own,
            |_| false,
            &mut claimed,
        );
        assert_eq!(picked.map(|window| window.id), Some("child".into()));
    }
}
