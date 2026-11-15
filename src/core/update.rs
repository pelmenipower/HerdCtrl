//! Release check state, version parsing and the updater launch. No HTTP in-process:
//! `fetch_latest` runs curl and must only be called from a worker thread.

use crate::core::{applications, config::Config};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const LATEST_URL: &str = "https://github.com/pelmenipower/HerdCtrl/releases/latest";
pub const TAG_URL_PREFIX: &str = "https://github.com/pelmenipower/HerdCtrl/releases/tag/v";
pub const UPDATE_SCRIPT_URL: &str =
    "https://github.com/pelmenipower/HerdCtrl/releases/latest/download/update.sh";
pub const NO_CHECK_ENV: &str = "HERDCTRL_NO_UPDATE_CHECK";
pub const CHECK_INTERVAL_SECS: u64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// Exactly "X.Y.Z" with decimal digits; no "v", no whitespace, no suffix.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let mut next = || {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse::<u32>().ok()
        };
        let version = Version(next()?, next()?, next()?);
        parts.next().is_none().then_some(version)
    }

    /// This build's version (CARGO_PKG_VERSION).
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version(0, 0, 0))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Trims ASCII whitespace, requires TAG_URL_PREFIX exactly, then Version::parse on the rest.
pub fn version_from_redirect(url: &str) -> Option<Version> {
    Version::parse(
        url.trim_matches(|c: char| c.is_ascii_whitespace())
            .strip_prefix(TAG_URL_PREFIX)?,
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)] // no deny_unknown_fields: newer builds may add fields
pub struct State {
    pub enabled: bool,
    /// Unix seconds of the last successful check, 0 = never.
    pub checked_at: u64,
    /// "X.Y.Z" from the last successful check.
    pub latest: Option<String>,
    pub notified: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            enabled: true,
            checked_at: 0,
            latest: None,
            notified: None,
        }
    }
}

impl State {
    /// `<dir of Config::path()>/update.json`.
    pub fn path() -> Result<PathBuf, String> {
        let layouts = Config::path()?;
        let dir = layouts.parent().ok_or("Invalid configuration path")?;
        Ok(dir.join("update.json"))
    }

    /// NotFound → Ok(State::default()); unreadable or invalid JSON → Err.
    pub fn load(path: &Path) -> Result<State, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Temp file (write+create+truncate, 0600), sync, rename. A stale temp file is reused.
    pub fn store(&self, path: &Path) -> Result<(), String> {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let parent = path.parent().ok_or("Invalid update state path")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temp)?;
            // A reused stale temp file keeps its old mode; force 0600.
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result.map_err(|e| format!("{}: {e}", temp.display()))
    }

    pub fn latest_version(&self) -> Option<Version> {
        Version::parse(self.latest.as_deref()?)
    }
}

/// Unix seconds, 0 on clock error.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Set to a non-empty value other than "0".
pub fn disabled_by_env() -> bool {
    std::env::var_os(NO_CHECK_ENV).is_some_and(|v| !v.is_empty() && v != "0")
}

/// `now < checked_at` (clock moved back) or `now - checked_at >= CHECK_INTERVAL_SECS`.
pub fn due(state: &State, now: u64) -> bool {
    now < state.checked_at || now - state.checked_at >= CHECK_INTERVAL_SECS
}

/// `enabled` and latest > current. Pure; for tests.
pub fn newer(state: &State, current: Version) -> Option<Version> {
    if !state.enabled {
        return None;
    }
    state.latest_version().filter(|latest| *latest > current)
}

/// `None` if disabled_by_env(), else newer(state, Version::current()).
pub fn available(state: &State) -> Option<Version> {
    if disabled_by_env() {
        return None;
    }
    newer(state, Version::current())
}

/// Blocking, worker only.
pub fn fetch_latest() -> Result<Version, String> {
    let output = Command::new("timeout")
        .args([
            "20s",
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--max-time",
            "15",
            "-o",
            "/dev/null",
            "-w",
            "%{redirect_url}",
            LATEST_URL,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "curl is not installed".to_string()
            } else {
                format!("curl: {e}")
            }
        })?;
    if output.status.code() == Some(127) {
        return Err("curl is not installed".into());
    }
    if !output.status.success() {
        return Err(format!(
            "curl: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let url = String::from_utf8_lossy(&output.stdout);
    version_from_redirect(&url).ok_or_else(|| format!("Unexpected redirect: {}", url.trim()))
}

/// "https://github.com/pelmenipower/HerdCtrl/releases/tag/v0.2.0"
pub fn release_url(version: Version) -> String {
    format!("{TAG_URL_PREFIX}{version}")
}

fn reap(mut child: std::process::Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

/// `xdg-open <release_url>` detached. Non-blocking.
pub fn open_release_notes(version: Version) -> Result<(), String> {
    let child = crate::os::child_env::clean(&mut Command::new("xdg-open"))
        .arg(release_url(version))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("xdg-open: {e}"))?;
    reap(child);
    Ok(())
}

/// herdctrl-update next to the running executable, else on PATH.
fn find_updater() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe
        .to_str()
        .and_then(|s| s.strip_suffix(" (deleted)"))
        .map(PathBuf::from)
        .unwrap_or(exe);
    let sibling = exe.parent()?.join("herdctrl-update");
    if sibling.is_file() {
        return Some(sibling);
    }
    applications::executable_path("herdctrl-update")
}

/// `XDG_CONFIG_HOME` if set and absolute (the same rule as `Config::path`).
fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// The shell script the updater terminal runs. Quoting failures (NUL bytes) → `Err`.
pub fn updater_script(
    updater: Option<&Path>,
    config_home: Option<&Path>,
) -> Result<String, String> {
    let quote = |path: &Path| {
        let text = path.to_str().ok_or("Path is not valid UTF-8")?;
        shlex::try_quote(text)
            .map(|q| q.into_owned())
            .map_err(|e| e.to_string())
    };
    let mut script = String::new();
    if let Some(home) = config_home {
        script.push_str(&format!("export XDG_CONFIG_HOME={}; ", quote(home)?));
    }
    match updater {
        Some(path) => script.push_str(&format!(
            "{}; printf '\\nPress Enter to close '; read -r _",
            quote(path)?
        )),
        None => script.push_str(&format!(
            "printf 'herdctrl-update is not installed. This downloads the HerdCtrl installer and installs the latest release to ~/.local/bin.\\nPress Enter to continue, or close this window. '; read -r _; t=$(mktemp) && curl -fsSL --proto '=https' {UPDATE_SCRIPT_URL} -o \"$t\" && bash \"$t\"; rm -f \"$t\"; printf '\\nPress Enter to close '; read -r _"
        )),
    }
    Ok(script)
}

/// Blocking (application discovery, terminal detection): call from a worker thread.
/// Opens a terminal that runs `updater_script(...)`.
pub fn launch_updater() -> Result<(), String> {
    let script = updater_script(find_updater().as_deref(), config_home().as_deref())?;
    let mut command = if let Some(path) = applications::executable_path("xdg-terminal-exec") {
        let mut command = Command::new(path);
        command.args(["sh", "-c", &script]);
        command
    } else {
        let terminal = applications::default_terminal(&applications::discover())
            .map_err(|_| "No terminal found. Run herdctrl-update in a terminal.".to_string())?;
        let mut command = terminal.terminal_command(&script)?;
        // The raw-command hint must not leak into the installer and the tray it restarts.
        command.env_remove("HERDCTRL_TERMINAL_COMMAND");
        command
    };
    let child = crate::os::child_env::clean(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    reap(child);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("herdctrl-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn version_parse_strict() {
        assert_eq!(Version::parse("0.2.0"), Some(Version(0, 2, 0)));
        assert_eq!(Version::parse("10.20.30"), Some(Version(10, 20, 30)));
        for bad in [
            "v0.2.0",
            "0.2",
            "0.2.0.1",
            "0.2.0-rc1",
            " 0.2.0",
            "0.2.0 ",
            "",
            "a.b.c",
            "0..1",
            "+1.2.3",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn version_order_and_display() {
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
        assert!(Version(1, 0, 0) > Version(0, 99, 99));
        assert_eq!(Version(0, 2, 0).to_string(), "0.2.0");
        assert!(Version::current() > Version(0, 0, 0));
    }

    #[test]
    fn redirect_parsing() {
        let ok = format!("{TAG_URL_PREFIX}0.2.0\n");
        assert_eq!(version_from_redirect(&ok), Some(Version(0, 2, 0)));
        for bad in [
            "https://github.com/pelmenipower/HerdCtrl/releases",
            "https://github.com/pelmenipower/HerdCtrl/releases/tag/v",
            "https://github.com/other/HerdCtrl/releases/tag/v0.2.0",
            "http://github.com/pelmenipower/HerdCtrl/releases/tag/v0.2.0",
            "https://github.com/pelmenipower/HerdCtrl/releases/tag/0.2.0",
            "",
        ] {
            assert_eq!(version_from_redirect(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn due_rules() {
        let state = State {
            checked_at: 1_000_000,
            ..State::default()
        };
        assert!(due(&State::default(), 1_800_000_000));
        assert!(!due(&state, 1_000_000 + 23 * 3600 + 59 * 60));
        assert!(due(&state, 1_000_000 + 24 * 3600));
        assert!(due(&state, 999_999));
    }

    #[test]
    fn newer_rules() {
        let current = Version(0, 2, 0);
        let with = |latest: Option<&str>, enabled| State {
            enabled,
            latest: latest.map(String::from),
            ..State::default()
        };
        assert_eq!(
            newer(&with(Some("0.3.0"), true), current),
            Some(Version(0, 3, 0))
        );
        assert_eq!(newer(&with(Some("0.3.0"), false), current), None);
        assert_eq!(newer(&with(Some("0.2.0"), true), current), None);
        assert_eq!(newer(&with(Some("0.1.0"), true), current), None);
        assert_eq!(newer(&with(Some("junk"), true), current), None);
        assert_eq!(newer(&with(None, true), current), None);
    }

    #[test]
    fn state_roundtrip_and_load() {
        let dir = temp_dir("state");
        let path = dir.join("sub/update.json");
        assert_eq!(State::load(&path), Ok(State::default()));
        let state = State {
            enabled: false,
            checked_at: 42,
            latest: Some("0.2.0".into()),
            notified: Some("0.2.0".into()),
        };
        state.store(&path).unwrap();
        assert_eq!(State::load(&path), Ok(state.clone()));
        // A stale temp file must not block saving.
        std::fs::write(path.with_extension("json.tmp"), "stale").unwrap();
        state.store(&path).unwrap();
        assert!(!path.with_extension("json.tmp").exists());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        std::fs::write(&path, r#"{"enabled": true, "future": 1}"#).unwrap();
        assert_eq!(State::load(&path), Ok(State::default()));
        // Invalid file: an error, never "enabled" by default.
        std::fs::write(&path, r#"{"enabled": false,"#).unwrap();
        assert!(State::load(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn updater_script_variants() {
        let script =
            updater_script(Some(Path::new("/h/.local/bin/herdctrl-update")), None).unwrap();
        assert_eq!(
            script,
            "/h/.local/bin/herdctrl-update; printf '\\nPress Enter to close '; read -r _"
        );
        let script = updater_script(
            Some(Path::new("/my dir/it's/herdctrl-update")),
            Some(Path::new("/cfg dir")),
        )
        .unwrap();
        assert!(script.starts_with("export XDG_CONFIG_HOME='/cfg dir'; "));
        let quoted = shlex::try_quote("/my dir/it's/herdctrl-update").unwrap();
        assert!(script.contains(&format!("{quoted}; printf")));
        assert_eq!(
            shlex::split(&script.replace("; printf", " ;printf")).unwrap()[..3],
            [
                "export",
                "XDG_CONFIG_HOME=/cfg dir;",
                "/my dir/it's/herdctrl-update"
            ][..]
        );

        let fallback = updater_script(None, None).unwrap();
        assert!(fallback.contains("Press Enter to continue"));
        assert!(fallback.contains(&format!(
            "curl -fsSL --proto '=https' {UPDATE_SCRIPT_URL} -o \"$t\" && bash \"$t\""
        )));
        assert!(!fallback.contains("| bash"));
    }
}
