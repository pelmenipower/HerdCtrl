use super::{applications, config::View};
use crate::os::{linux, Backend};
use std::{
    collections::HashSet,
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
        thread::scope(|scope| {
            for zone in view.zones {
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
                scope.spawn(move || {
                    let result = if let Some(window) = reused {
                        backend
                            .position(&window, &monitor, zone.bounds.pixels(&monitor))
                            .map(|()| true)
                    } else {
                        launch_new(
                            &zone,
                            selected_app.as_ref(),
                            matcher.as_ref(),
                            backend,
                            &monitor,
                            initial,
                            claimed,
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

fn launch_new(
    zone: &super::config::Zone,
    app: Option<&applications::InstalledApp>,
    matcher: Option<&matching::AppMatcher>,
    backend: Backend,
    monitor: &super::monitor::Monitor,
    initial: &HashSet<String>,
    claimed: &std::sync::Mutex<HashSet<String>>,
) -> Result<(), String> {
    let script = if zone.run_in_terminal {
        &zone.command
    } else {
        &zone.terminal_command
    };
    let mut command = if let Some(app) = app {
        if !script.trim().is_empty() {
            app.terminal_command(script)?
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
            let window = matching::reserve(
                windows
                    .into_iter()
                    .filter(|window| belongs_to_new_launch(window, initial, &pids, matcher)),
                &mut claimed,
            );
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

fn belongs_to_new_launch(
    window: &linux::Window,
    existing: &HashSet<String>,
    pids: &HashSet<u32>,
    matcher: Option<&matching::AppMatcher>,
) -> bool {
    !existing.contains(&window.id)
        && (pids.contains(&window.pid) || matcher.is_some_and(|app| app.owns_pid(window.pid)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_existing_windows_and_unrelated_processes() {
        let existing = HashSet::from(["old".to_string()]);
        let pids = HashSet::from([123, 124]);
        let windows = [
            linux::Window {
                id: "old".into(),
                pid: 123,
            },
            linux::Window {
                id: "unrelated".into(),
                pid: 999,
            },
            linux::Window {
                id: "child".into(),
                pid: 124,
            },
        ];
        let matches: Vec<_> = windows
            .iter()
            .filter(|window| belongs_to_new_launch(window, &existing, &pids, None))
            .map(|window| window.id.as_str())
            .collect();
        assert_eq!(matches, ["child"]);
    }
}
