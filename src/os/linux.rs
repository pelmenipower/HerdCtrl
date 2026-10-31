use crate::core::monitor::Monitor;
use std::{collections::HashSet, process::Command};

pub fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let result = Command::new("timeout")
        .args(["2s", program])
        .args(args)
        .output()
        .map_err(|e| format!("Cannot run {program}: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&result.stdout).into_owned())
}

pub fn monitors() -> Result<Vec<Monitor>, String> {
    if std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland") {
        return Err("HerdCtrl requires an X11 session; Wayland is not supported yet".into());
    }
    parse_monitors(&output("xrandr", &["--verbose"])?)
}

pub fn parse_monitors(raw: &str) -> Result<Vec<Monitor>, String> {
    let mut monitors: Vec<Monitor> = Vec::new();
    let mut active = None;
    for line in raw.lines() {
        if !line.starts_with(char::is_whitespace) {
            active = None;
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.get(1) != Some(&"connected") {
                continue;
            }
            for field in &fields[2..] {
                if let Some((width, height, x, y)) = geometry(field) {
                    monitors.push(Monitor {
                        name: fields[0].into(),
                        width,
                        height,
                        x,
                        y,
                        scale: 1.0,
                    });
                    active = Some(monitors.len() - 1);
                    break;
                }
            }
        } else if let (Some(index), Some(values)) = (active, line.trim().strip_prefix("Transform:"))
        {
            if let Some(scale) = values
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f32>().ok())
            {
                if scale.is_finite() && scale > 0.0 {
                    monitors[index].scale = scale;
                }
            }
        }
    }
    if monitors.is_empty() {
        Err("No active X11 monitors found".into())
    } else {
        Ok(monitors)
    }
}

fn geometry(value: &str) -> Option<(u32, u32, i32, i32)> {
    let (width, rest) = value.split_once('x')?;
    let offset = rest.find(['+', '-'])?;
    let (height, coords) = rest.split_at(offset);
    let second = coords[1..].find(['+', '-'])? + 1;
    Some((
        width.parse().ok()?,
        height.parse().ok()?,
        coords[..second].parse().ok()?,
        coords[second..].parse().ok()?,
    ))
}

#[derive(Debug)]
pub struct Window {
    pub id: String,
    pub pid: u32,
}

pub fn parse_windows(raw: &str) -> Vec<Window> {
    raw.lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            Some(Window {
                id: fields.first()?.to_string(),
                pid: fields.get(2)?.parse().ok()?,
            })
        })
        .collect()
}

pub fn windows() -> Result<Vec<Window>, String> {
    Ok(parse_windows(&output("wmctrl", &["-lp"])?))
}

pub fn descendants(root: u32, known: &mut HashSet<u32>) {
    known.insert(root);
    // Retain observed descendants even after the launcher shell exits.
    if let Ok(entries) = std::fs::read_dir("/proc") {
        let processes: Vec<_> = entries
            .flatten()
            .filter_map(|entry| {
                let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
                let stat = std::fs::read_to_string(entry.path().join("stat")).ok()?;
                let after_name = stat.rsplit_once(") ")?.1;
                let parent: u32 = after_name.split_whitespace().nth(1)?.parse().ok()?;
                Some((pid, parent))
            })
            .collect();
        loop {
            let before = known.len();
            for &(pid, parent) in &processes {
                if known.contains(&parent) {
                    known.insert(pid);
                }
            }
            if known.len() == before {
                break;
            }
        }
    }
}

pub fn position(id: &str, (x, y, width, height): (i32, i32, u32, u32)) -> Result<(), String> {
    output(
        "wmctrl",
        &[
            "-ir",
            id,
            "-b",
            "remove,maximized_vert,maximized_horz,fullscreen",
        ],
    )?;
    output(
        "wmctrl",
        &["-ir", id, "-e", &format!("0,{x},{y},{width},{height}")],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_active_displays_offsets_and_transform() {
        let raw = "DP-1 connected primary 2560x1440-2560+0 (normal)\n\tTransform: 1.250000 0.000000 0.000000\nHDMI-1 connected 1920x1080+0-200 (normal)\nDP-2 disconnected (normal)\n";
        let result = parse_monitors(raw).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!((result[0].x, result[1].y), (-2560, -200));
        assert_eq!(result[0].scale, 1.25);
        assert!(parse_monitors("DP-1 disconnected").is_err());
    }
    #[test]
    fn parses_window_pid_without_matching_titles() {
        let windows = parse_windows("0x01200003 0 123 host Terminal title with spaces\ninvalid");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].pid, 123);
    }
}
