//! Installed desktop applications. Discovery and launch preparation run on workers.
use super::config::Application;
use freedesktop_desktop_entry::{DesktopEntry, Iter};
use std::{
    collections::HashSet, fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command,
};

#[derive(Clone, Debug)]
pub struct InstalledApp {
    pub application: Application,
    pub description: String,
    pub argv: Vec<String>,
    pub directory: Option<PathBuf>,
    pub desktop_file: PathBuf,
    pub is_terminal: bool,
    pub terminal_exec: Option<Vec<String>>,
    pub new_window_argv: Option<Vec<String>>,
}

impl InstalledApp {
    pub fn command(&self) -> Command {
        self.command_for(&self.argv)
    }

    pub fn new_window_command(&self) -> Option<Command> {
        self.new_window_argv
            .as_deref()
            .map(|argv| self.command_for(argv))
    }

    fn command_for(&self, argv: &[String]) -> Command {
        let mut command = Command::new(&argv[0]);
        command.args(&argv[1..]);
        command.env("GIO_LAUNCHED_DESKTOP_FILE", &self.desktop_file);
        command.env_remove("HERDCTRL_TERMINAL_COMMAND");
        if let Some(directory) = &self.directory {
            command.current_dir(directory);
        }
        command
    }

    pub fn terminal_command(&self, script: &str) -> Result<Command, String> {
        if !self.is_terminal {
            return Err("Choose a terminal application to run a terminal program".into());
        }
        let separator = self.terminal_exec.as_ref().ok_or_else(|| {
            format!(
                "{} does not describe how to run a program. Choose another terminal application.",
                self.application.name
            )
        })?;
        let mut command = self.command();
        command.args(separator).args(["sh", "-c", script]);
        command.env("HERDCTRL_TERMINAL_COMMAND", script);
        Ok(command)
    }

    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        self.application.name.to_lowercase().contains(&query)
            || self.description.to_lowercase().contains(&query)
            || self.application.id.to_lowercase().contains(&query)
    }
}

pub fn discover() -> Vec<InstalledApp> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    {
        paths.push(home.join("applications"));
    }
    let dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    paths.extend(
        std::env::split_paths(&dirs)
            .filter(|p| p.is_absolute())
            .map(|p| p.join("applications")),
    );
    let locales = locales();
    let desktops: Vec<_> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .map(str::to_string)
        .collect();
    catalogue(
        Iter::new(paths.into_iter()).entries(Some(&locales)),
        &locales,
        &desktops,
    )
}

fn locales() -> Vec<String> {
    let mut result = Vec::new();
    for key in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            for locale in value.split(':').filter(|s| !s.is_empty()) {
                let locale = locale.split('.').next().unwrap_or(locale);
                result.push(locale.to_string());
                if let Some((language, _)) = locale.split_once('_') {
                    result.push(language.to_string());
                }
            }
        }
    }
    result
}

fn catalogue(
    entries: impl Iterator<Item = DesktopEntry>,
    locales: &[String],
    desktops: &[String],
) -> Vec<InstalledApp> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for entry in entries {
        // A hidden user entry also masks an identically named system entry.
        if !seen.insert(entry.appid.clone())
            || entry.hidden()
            || entry.no_display()
            || entry.type_() != Some("Application")
        {
            continue;
        }
        if entry
            .only_show_in()
            .is_some_and(|allowed| !allowed.iter().any(|a| desktops.iter().any(|d| d == a)))
            || entry
                .not_show_in()
                .is_some_and(|denied| denied.iter().any(|a| desktops.iter().any(|d| d == a)))
            || entry.try_exec().is_some_and(|exec| !executable(exec))
        {
            continue;
        }
        if let Ok(app) = prepare(&entry, locales) {
            apps.push(app);
        }
    }
    apps.sort_by_cached_key(|a| (a.application.name.to_lowercase(), a.application.id.clone()));
    apps
}

pub fn executable_path(program: &str) -> Option<PathBuf> {
    let runnable = |path: PathBuf| {
        fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        let path = PathBuf::from(program);
        return runnable(path.clone())
            .then(|| fs::canonicalize(path).ok())
            .flatten();
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(program))
            .find(|p| runnable(p.clone()))
            .and_then(|p| fs::canonicalize(p).ok())
    })
}

fn executable(program: &str) -> bool {
    executable_path(program).is_some()
}

pub fn default_terminal(apps: &[InstalledApp]) -> Result<InstalledApp, String> {
    if let Ok(id) = crate::os::linux::output("xdg-terminal-exec", &["--print-id"]) {
        let id = id
            .trim()
            .split(':')
            .next()
            .unwrap_or_default()
            .trim_end_matches(".desktop");
        if let Some(app) = apps
            .iter()
            .find(|app| app.application.id == id && app.is_terminal)
        {
            return Ok(app.clone());
        }
    }
    apps.iter()
        .find(|app| app.application.id == "org.gnome.Ptyxis" && app.is_terminal)
        .cloned()
        .ok_or_else(|| {
            "No default terminal is available. Choose a terminal application for this zone.".into()
        })
}

fn expand_exec(exec: &str, entry: &DesktopEntry, name: &str) -> Result<Vec<String>, String> {
    let tokens = shlex::split(exec).ok_or("Invalid executable quoting")?;
    let mut argv = Vec::new();
    for token in tokens {
        if token == "%i" {
            if let Some(icon) = entry.icon() {
                argv.extend(["--icon".into(), icon.into()]);
            }
            continue;
        }
        if ["%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%v", "%m"].contains(&token.as_str()) {
            continue;
        }
        let mut expanded = String::new();
        let mut chars = token.chars();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                expanded.push(ch);
                continue;
            }
            match chars.next() {
                Some('%') => expanded.push('%'),
                Some('c') => expanded.push_str(name),
                Some('k') => expanded.push_str(&entry.path.to_string_lossy()),
                _ => return Err("Unsupported desktop field code".into()),
            }
        }
        argv.push(expanded);
    }
    Ok(argv)
}

fn new_window_argv(entry: &DesktopEntry, name: &str) -> Option<Vec<String>> {
    let action = entry.actions()?.into_iter().find(|id| {
        let id: String = id
            .to_lowercase()
            .chars()
            .filter(|c| !matches!(c, '-' | '_'))
            .collect();
        ["newwindow", "newemptywindow", "windownew"].contains(&id.as_str())
    })?;
    let argv = expand_exec(entry.action_exec(action)?, entry, name).ok()?;
    argv.first()
        .is_some_and(|program| executable(program))
        .then_some(argv)
}

fn prepare(entry: &DesktopEntry, locales: &[String]) -> Result<InstalledApp, String> {
    let name = entry
        .name(locales)
        .filter(|s| !s.trim().is_empty())
        .ok_or("Missing application name")?
        .into_owned();
    let is_terminal = entry
        .categories()
        .is_some_and(|items| items.contains(&"TerminalEmulator"));
    let mut argv = expand_exec(entry.exec().ok_or("Missing executable")?, entry, &name)?;
    let program = argv
        .first()
        .filter(|s| !s.is_empty() && !s.contains('='))
        .ok_or("Missing executable")?;
    if !executable(program) {
        return Err("Application is not installed".into());
    }
    // These GNOME apps provide an explicit independent instance, allowing precise PID matching.
    if ["org.gnome.Ptyxis", "org.gnome.TextEditor"].contains(&entry.appid.as_str())
        && !argv.iter().any(|arg| arg == "--standalone")
    {
        argv.push("--standalone".into());
    }
    let new_window_argv = if is_terminal || entry.terminal() {
        None
    } else {
        new_window_argv(entry, &name)
    };
    if entry.terminal() {
        if !executable("xdg-terminal-exec") {
            return Err("No default terminal launcher is available".into());
        }
        argv.insert(0, "xdg-terminal-exec".into());
    }
    Ok(InstalledApp {
        application: Application {
            id: entry.appid.clone(),
            name,
        },
        description: entry
            .comment(locales)
            .map(|s| s.into_owned())
            .unwrap_or_default(),
        argv,
        directory: entry.path().filter(|p| !p.is_empty()).map(PathBuf::from),
        desktop_file: entry.path.clone(),
        is_terminal,
        terminal_exec: entry
            .desktop_entry("X-TerminalArgExec")
            .or_else(|| entry.desktop_entry("X-ExecArg"))
            .and_then(shlex::split),
        new_window_argv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(id: &str, data: &str) -> DesktopEntry {
        DesktopEntry::from_str(
            PathBuf::from(format!("/tmp/{id}.desktop")),
            data,
            Some(&["de"]),
        )
        .unwrap()
    }

    #[test]
    fn desktop_arguments_are_data_and_field_codes_expand_once() {
        let app = prepare(&entry("test", "[Desktop Entry]\nType=Application\nName=Example\nName[de]=Name %c\nIcon=icon name\nExec=/bin/echo \"a b\" \"$(touch /tmp/never)\" %U %i %c %k 50%%\nPath=/tmp\n"), &["de".into()]).unwrap();
        assert_eq!(
            app.argv,
            [
                "/bin/echo",
                "a b",
                "$(touch /tmp/never)",
                "--icon",
                "icon name",
                "Name %c",
                "/tmp/test.desktop",
                "50%"
            ]
        );
        assert_eq!(app.directory, Some(PathBuf::from("/tmp")));
        assert!(app.matches("name"));
        assert!(prepare(
            &entry("bad", "[Desktop Entry]\nName=Bad\nExec=/bin/echo %Q\n"),
            &[]
        )
        .is_err());
    }

    #[test]
    fn new_window_action_skips_private_actions() {
        let app = prepare(&entry("files", "[Desktop Entry]\nType=Application\nName=Files\nExec=/bin/echo %U\nActions=new-window;new-private-window;\n\n[Desktop Action new-window]\nName=New Window\nExec=/bin/echo --new-window %u\n\n[Desktop Action new-private-window]\nName=New Private\nExec=/bin/echo --private-window\n"), &[]).unwrap();
        assert_eq!(
            app.new_window_argv,
            Some(vec!["/bin/echo".into(), "--new-window".into()])
        );
        let private = prepare(&entry("private", "[Desktop Entry]\nType=Application\nName=Private\nExec=/bin/echo\nActions=new-private-window;\n\n[Desktop Action new-private-window]\nName=New Private\nExec=/bin/echo --private-window\n"), &[]).unwrap();
        assert_eq!(private.new_window_argv, None);
        let plain = prepare(
            &entry(
                "plain",
                "[Desktop Entry]\nType=Application\nName=Plain\nExec=/bin/echo\n",
            ),
            &[],
        )
        .unwrap();
        assert_eq!(plain.new_window_argv, None);
    }

    #[test]
    fn user_hidden_entries_mask_system_entries_and_filters_apply() {
        let entries = [
            entry("masked", "[Desktop Entry]\nHidden=true\n"),
            entry("masked", "[Desktop Entry]\nType=Application\nName=Masked\nExec=/bin/true\n"),
            entry("visible", "[Desktop Entry]\nType=Application\nName=Visible\nOnlyShowIn=GNOME;\nExec=/bin/true\n"),
            entry("hidden", "[Desktop Entry]\nType=Application\nName=Hidden\nNoDisplay=true\nExec=/bin/true\n"),
            entry("missing", "[Desktop Entry]\nType=Application\nName=Missing\nTryExec=/herdctrl/nonexistent\nExec=/bin/true\n"),
            entry("other", "[Desktop Entry]\nType=Application\nName=Other\nOnlyShowIn=KDE;\nExec=/bin/true\n"),
        ];
        let apps = catalogue(entries.into_iter(), &[], &["GNOME".into()]);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].application.id, "visible");
    }
}
