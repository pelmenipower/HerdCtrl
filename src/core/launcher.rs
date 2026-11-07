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
        thread::scope(|scope| {
            for (index, zone) in view.zones.into_iter().enumerate() {
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
                let selected_app = if let Some(selected) = &zone.application {
                    apps.iter().find(|app| app.application.id == selected.id).cloned()
                        .ok_or_else(|| format!("{} is no longer available. Choose another application for this zone.", selected.name))
                        .map(Some)
                } else if zone.run_in_terminal {
                    applications::default_terminal(&apps).map(Some)
                } else {
                    Ok(None)
                };
                let selected_app = match selected_app {
                    Ok(app) => app,
                    Err(error) => {
                        let _ = events.send(format!("{}: {error}", zone.name));
                        continue;
                    }
                };
                let script = if zone.run_in_terminal {
                    &zone.command
                } else {
                    &zone.terminal_command
                };
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
                let app_running = matcher.as_ref().is_some_and(|matcher| {
                    windows.iter().any(|window| matcher.owns_pid(window.pid))
                });
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
                let events = events.clone();
                let initial = &initial;
                let claimed = &claimed;
                let roots = &roots;
                scope.spawn(move || {
                    let result = if let Some(window) = reused {
                        backend
                            .position(&window, &monitor, zone.bounds.pixels(&monitor))
                            .map(|()| true)
                    } else {
                        launch_new(
                            index,
                            app_running,
                            &zone,
                            selected_app.as_ref(),
                            matcher.as_ref(),
                            backend,
                            &monitor,
                            initial,
                            claimed,
                            roots,
                        )
                        .map(|()| false)
                    };
                    let message = match result {
                        Ok(true) => format!("{}: existing window arranged", zone.name),
                        Ok(false) => format!("{}: positioned", zone.name),
                        Err(error) => format!("{}: {error}", zone.name),
                    };
                    let _ = events.send(message);
                });
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn launch_new(
    index: usize,
    app_running: bool,
    zone: &super::config::Zone,
    app: Option<&applications::InstalledApp>,
    matcher: Option<&matching::AppMatcher>,
    backend: Backend,
    monitor: &super::monitor::Monitor,
    initial: &HashSet<String>,
    claimed: &std::sync::Mutex<HashSet<String>>,
    roots: &std::sync::Mutex<HashMap<usize, u32>>,
) -> Result<(), String> {
    let script = if zone.run_in_terminal {
        &zone.command
    } else {
        &zone.terminal_command
    };
    let mut command = if let Some(app) = app {
        if !script.trim().is_empty() {
            app.terminal_command(script)?
        } else if let Some(command) = app_running.then(|| app.new_window_command()).flatten() {
            command
        } else {
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
    roots
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(index, pid);
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
            let windows = backend.windows()?;
            let mut claimed = claimed.lock().unwrap_or_else(|error| error.into_inner());
            let other_roots: HashSet<u32> = roots
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .iter()
                .filter(|(zone, _)| **zone != index)
                .map(|(_, root)| *root)
                .collect();
            let weak = |window: &linux::Window| {
                matcher.is_some_and(|matcher| matcher.owns_pid(window.pid))
                    && !linux::has_ancestor(window.pid, &other_roots)
                    && (script.trim().is_empty()
                        || matching::ProcessIdentity::read(window.pid)
                            .terminal_command
                            .as_deref()
                            == Some(script.as_str()))
            };
            let window = pick(windows, initial, &pids, weak, &mut claimed);
            drop(claimed);
            if let Some(window) = window {
                return backend.position(&window, monitor, zone.bounds.pixels(monitor));
            }
        }
        if let Ok(status) = exit_rx.try_recv() {
            if !status.success() {
                return Err(format!("Program exited with {status}. For terminal programs, enable Run in terminal or choose a terminal application."));
            }
        }
        if Instant::now() >= deadline {
            return Err("No new window belonging to this process appeared within 5 s. Select a terminal application for terminal programs.".into());
        }
        thread::sleep(Duration::from_millis(80));
    }
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
