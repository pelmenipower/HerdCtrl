use crate::{
    core::applications::{executable_path, InstalledApp},
    os::linux::{self, Window},
};
use std::{
    collections::HashSet, ffi::OsString, fs, io::Read, os::unix::ffi::OsStringExt, path::PathBuf,
};

#[derive(Default, Debug)]
pub(super) struct ProcessIdentity {
    pub executable: Option<PathBuf>,
    pub desktop_file: Option<PathBuf>,
    pub terminal_command: Option<String>,
}

impl ProcessIdentity {
    pub fn read(pid: u32) -> Self {
        let directory = PathBuf::from(format!("/proc/{pid}"));
        let environment = fs::read(directory.join("environ")).unwrap_or_default();
        let variable = |key: &[u8]| {
            environment
                .split(|byte| *byte == 0)
                .find_map(|item| item.strip_prefix(key))
        };
        Self {
            executable: fs::read_link(directory.join("exe")).ok(),
            desktop_file: variable(b"GIO_LAUNCHED_DESKTOP_FILE=")
                .map(|value| PathBuf::from(OsString::from_vec(value.to_vec())))
                .and_then(|path| fs::canonicalize(path).ok()),
            terminal_command: variable(b"HERDCTRL_TERMINAL_COMMAND=")
                .and_then(|value| String::from_utf8(value.to_vec()).ok()),
        }
    }
}

pub(super) struct AppMatcher {
    executable: Option<PathBuf>,
    desktop_file: Option<PathBuf>,
    is_terminal: bool,
    terminal_command: String,
}

impl AppMatcher {
    pub fn new(app: &InstalledApp, terminal_command: &str) -> Self {
        // Shared interpreters/launchers alone cannot identify an application.
        let program = app.argv.first().and_then(|s| executable_path(s));
        let executable = program.filter(|path| {
            let mut magic = [0; 4];
            if !fs::File::open(path).is_ok_and(|mut file| file.read_exact(&mut magic).is_ok())
                || magic != *b"\x7fELF"
            {
                return false;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            ![
                "sh",
                "dash",
                "bash",
                "zsh",
                "fish",
                "env",
                "node",
                "nodejs",
                "java",
                "ruby",
                "perl",
                "flatpak",
                "snap",
                "xdg-terminal-exec",
            ]
            .contains(&name.as_ref())
                && !name.starts_with("python")
        });
        Self {
            executable,
            desktop_file: fs::canonicalize(&app.desktop_file).ok(),
            is_terminal: app.is_terminal,
            terminal_command: terminal_command.into(),
        }
    }

    pub fn owns(&self, identity: &ProcessIdentity) -> bool {
        match (&self.executable, &identity.executable) {
            (Some(expected), Some(actual)) => expected == actual,
            _ => self
                .desktop_file
                .as_ref()
                .is_some_and(|file| identity.desktop_file.as_ref() == Some(file)),
        }
    }

    pub fn reusable(&self, window: &Window, windows: &[Window]) -> bool {
        let identity = ProcessIdentity::read(window.pid);
        if !self.owns(&identity) {
            return false;
        }
        if !self.is_terminal {
            return true;
        }
        if self.terminal_command.is_empty() {
            return identity.terminal_command.is_none();
        }
        if windows
            .iter()
            .filter(|candidate| candidate.pid == window.pid)
            .count()
            != 1
        {
            return false;
        }
        if let Some(command) = identity.terminal_command {
            return command == self.terminal_command;
        }
        // With a shared multi-window terminal PID, descendants cannot identify a specific window.
        terminal_descendant(window.pid, &self.terminal_command)
    }

    pub fn owns_pid(&self, pid: u32) -> bool {
        self.owns(&ProcessIdentity::read(pid))
    }
}

fn terminal_descendant(pid: u32, command: &str) -> bool {
    // Complex shell programs need a HerdCtrl-owned terminal marker; never guess from a title.
    if command.contains(['|', '&', ';', '$', '`', '<', '>', '\n']) {
        return false;
    }
    let Some(arguments) = shlex::split(command).filter(|args| !args.is_empty()) else {
        return false;
    };
    let Some(executable) = executable_path(&arguments[0]) else {
        return false;
    };
    let mut descendants = HashSet::new();
    linux::descendants(pid, &mut descendants);
    descendants
        .into_iter()
        .filter(|child| *child != pid)
        .any(|child| {
            if ProcessIdentity::read(child).executable.as_ref() != Some(&executable) {
                return false;
            }
            let Ok(raw) = fs::read(format!("/proc/{child}/cmdline")) else {
                return false;
            };
            let args: Vec<_> = raw
                .split(|byte| *byte == 0)
                .filter(|arg| !arg.is_empty())
                .collect();
            args.len() == arguments.len()
                && args
                    .iter()
                    .skip(1)
                    .zip(arguments.iter().skip(1))
                    .all(|(actual, expected)| *actual == expected.as_bytes())
        })
}

pub(super) fn reserve(
    windows: impl IntoIterator<Item = Window>,
    claimed: &mut HashSet<String>,
) -> Option<Window> {
    let mut windows: Vec<_> = windows.into_iter().collect();
    windows.sort_by_key(|window| {
        (
            window
                .id
                .parse::<u64>()
                .ok()
                .or_else(|| u64::from_str_radix(window.id.trim_start_matches("0x"), 16).ok()),
            window.id.clone(),
        )
    });
    windows
        .into_iter()
        .find(|window| claimed.insert(window.id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Application;
    #[test]
    fn exact_executable_identity_and_reservations_do_not_match_unrelated_apps() {
        let app = InstalledApp {
            application: Application {
                id: "example".into(),
                name: "Example".into(),
            },
            description: String::new(),
            argv: vec!["/bin/sleep".into()],
            directory: None,
            desktop_file: "/does/not/exist.desktop".into(),
            is_terminal: false,
            terminal_exec: None,
            new_window_argv: None,
        };
        let mut matcher = AppMatcher::new(&app, "");
        assert!(matcher.owns(&ProcessIdentity {
            executable: executable_path("/bin/sleep"),
            ..Default::default()
        }));
        assert!(!matcher.owns(&ProcessIdentity {
            executable: executable_path("/bin/true"),
            ..Default::default()
        }));
        matcher.desktop_file = Some("/tmp/example.desktop".into());
        assert!(
            !matcher.owns(&ProcessIdentity {
                executable: executable_path("/bin/true"),
                desktop_file: matcher.desktop_file.clone(),
                ..Default::default()
            }),
            "An inherited desktop marker must not match a different native executable"
        );
        let mut shared = app.clone();
        shared.argv = vec!["/bin/sh".into()];
        assert!(!AppMatcher::new(&shared, "").owns(&ProcessIdentity {
            executable: executable_path("/bin/sh"),
            ..Default::default()
        }));
        let windows = vec![
            Window {
                id: "2".into(),
                pid: 123,
            },
            Window {
                id: "1".into(),
                pid: 123,
            },
        ];
        let mut claimed = HashSet::new();
        assert_eq!(reserve(windows.clone(), &mut claimed).unwrap().id, "1");
        assert_eq!(reserve(windows.clone(), &mut claimed).unwrap().id, "2");
        assert!(reserve(windows, &mut claimed).is_none());
    }

    #[test]
    fn terminal_programs_do_not_reuse_other_programs_or_ambiguous_windows() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("3")
            .env("HERDCTRL_TERMINAL_COMMAND", "herdr")
            .spawn()
            .unwrap();
        let app = InstalledApp {
            application: Application {
                id: "terminal".into(),
                name: "Terminal".into(),
            },
            description: String::new(),
            argv: vec!["/bin/sleep".into()],
            directory: None,
            desktop_file: "/does/not/exist.desktop".into(),
            is_terminal: true,
            terminal_exec: Some(vec!["--".into()]),
            new_window_argv: None,
        };
        let window = Window {
            id: "1".into(),
            pid: child.id(),
        };
        // /proc can briefly expose the pre-exec environment after spawn.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while ProcessIdentity::read(child.id())
            .terminal_command
            .as_deref()
            != Some("herdr")
        {
            assert!(
                std::time::Instant::now() < deadline,
                "Child environment was not initialized"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let windows = vec![window.clone()];
        assert!(AppMatcher::new(&app, "herdr").reusable(&window, &windows));
        assert!(!AppMatcher::new(&app, "htop").reusable(&window, &windows));
        assert!(!AppMatcher::new(&app, "").reusable(&window, &windows));
        let ambiguous = vec![
            window.clone(),
            Window {
                id: "2".into(),
                pid: child.id(),
            },
        ];
        assert!(!AppMatcher::new(&app, "herdr").reusable(&window, &ambiguous));
        let _ = child.kill();
        let _ = child.wait();
    }
}
