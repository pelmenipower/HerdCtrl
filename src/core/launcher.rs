use super::config::View;
use crate::os::linux;
use std::{
    collections::HashSet,
    process::{Command, Stdio},
    sync::mpsc::Sender,
    thread,
    time::{Duration, Instant},
};

pub fn launch(view: View, events: Sender<String>) {
    thread::spawn(move || {
        let monitors = match linux::monitors() {
            Ok(monitors) => monitors,
            Err(e) => {
                let _ = events.send(e);
                return;
            }
        };
        for zone in view.zones {
            let events = events.clone();
            let Some(monitor) = monitors.iter().find(|m| m.name == zone.monitor).cloned() else {
                let _ = events.send(format!(
                    "{}: monitor {} is disconnected",
                    zone.name, zone.monitor
                ));
                continue;
            };
            thread::spawn(move || {
                let result = (|| -> Result<(), String> {
                    if zone.command.trim().is_empty() {
                        return Err("No command configured".into());
                    }
                    let existing: HashSet<_> =
                        linux::windows()?.into_iter().map(|w| w.id).collect();
                    let child = Command::new("sh")
                        .args(["-c", &zone.command])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .map_err(|e| e.to_string())?;
                    let pid = child.id();
                    // Reap without blocking the UI or the polling worker.
                    thread::spawn(move || {
                        let mut child = child;
                        let _ = child.wait();
                    });
                    let delay_end = Instant::now() + Duration::from_millis(zone.delay_ms);
                    let deadline = delay_end + Duration::from_secs(5);
                    let mut pids = HashSet::new();
                    loop {
                        linux::descendants(pid, &mut pids);
                        if Instant::now() >= delay_end {
                            if let Some(window) =
                                matching_window(linux::windows()?, &existing, &pids)
                            {
                                return linux::position(&window.id, zone.bounds.pixels(&monitor));
                            }
                        }
                        if Instant::now() >= deadline {
                            return Err("No new window belonging to this process appeared within 5 s (use a new-instance option for single-instance apps)".into());
                        }
                        thread::sleep(Duration::from_millis(80));
                    }
                })();
                let message = match result {
                    Ok(()) => format!("{}: positioned", zone.name),
                    Err(e) => format!("{}: {e}", zone.name),
                };
                let _ = events.send(message);
            });
        }
    });
}

fn matching_window(
    windows: Vec<linux::Window>,
    existing: &HashSet<String>,
    pids: &HashSet<u32>,
) -> Option<linux::Window> {
    windows
        .into_iter()
        .find(|window| !existing.contains(&window.id) && pids.contains(&window.pid))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_existing_windows_and_unrelated_processes() {
        let existing = HashSet::from(["old".to_string()]);
        let pids = HashSet::from([123, 124]);
        let windows = vec![
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
        assert_eq!(
            matching_window(windows, &existing, &pids).unwrap().id,
            "child"
        );
        assert!(matching_window(
            vec![linux::Window {
                id: "other".into(),
                pid: 999
            }],
            &existing,
            &pids
        )
        .is_none());
    }
}
