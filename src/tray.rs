//! Background tray process: owns the global hotkeys and launches views without
//! an editor window, so no renderer or GUI state stays resident.
use crate::{
    core::{
        config::Config,
        launcher::{self, Report},
        update::{self, State},
    },
    os::{gnome, Backend},
};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use ksni::{blocking::TrayMethods, menu::*, Icon, MenuItem};
use std::{
    collections::HashMap,
    fs::{self, File},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

const TRAY_LOCK: &str = "tray";
pub const EDITOR_LOCK: &str = "editor";

enum Message {
    Launch(usize),
    Edit,
    Autostart(bool),
    Quit,
    /// Result of the one-shot `fetch_latest` thread.
    Checked(Result<update::Version, String>),
    /// The "Check for updates daily" checkmark.
    UpdateCheck(bool),
    /// The "Update to X…" item.
    Update,
    /// The updater thread could not open a terminal.
    UpdaterFailed(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UpdateCheck {
    On,
    Off,
    /// `HERDCTRL_NO_UPDATE_CHECK` is set or `update.json` is invalid.
    Unavailable,
}

fn update_label(version: update::Version) -> String {
    format!("Update to {version}…")
}

/// The version to announce: available and not announced yet.
fn pending_notice(state: &State, available: Option<update::Version>) -> Option<update::Version> {
    available.filter(|v| state.notified.as_deref() != Some(v.to_string().as_str()))
}

/// A check may start: allowed, idle, past the back-off, and forced or due.
fn should_check(
    state: &State,
    running: bool,
    now: Instant,
    not_before: Instant,
    force: bool,
    unix_now: u64,
) -> bool {
    state.enabled && !running && now >= not_before && (force || update::due(state, unix_now))
}

/// Update bookkeeping owned by the run loop: the only writer of `update.json`.
struct Updates {
    path: Option<PathBuf>,
    /// `None` while checks are unavailable.
    state: Option<State>,
    running: bool,
    force: bool,
    not_before: Instant,
}

impl Updates {
    fn new(notifier: &Notifier) -> Self {
        // No file access at all when disabled by the environment.
        if update::disabled_by_env() {
            return Self::with_path(Err(String::new())).0;
        }
        let (updates, problem) = Self::with_path(State::path());
        if let Some(problem) = problem {
            notifier.show("Update check is off until re-enabled", &problem);
        }
        updates
    }

    /// Loads `update.json`; an invalid file leaves the checks off but clickable.
    fn with_path(path: Result<PathBuf, String>) -> (Self, Option<String>) {
        let mut updates = Self {
            path: None,
            state: None,
            running: false,
            force: false,
            not_before: Instant::now() + Duration::from_secs(60),
        };
        let path = match path {
            Ok(path) => path,
            Err(e) => return (updates, Some(e)),
        };
        updates.path = Some(path.clone());
        match State::load(&path) {
            Ok(state) => (updates.with_state(state), None),
            Err(e) => (updates, Some(e)),
        }
    }

    fn with_state(mut self, state: State) -> Self {
        self.state = Some(state);
        self
    }

    fn check(&self) -> UpdateCheck {
        match &self.state {
            // Invalid file: unchecked but active; a click writes a new file.
            None if self.path.is_some() => UpdateCheck::Off,
            None => UpdateCheck::Unavailable,
            Some(state) if state.enabled => UpdateCheck::On,
            Some(_) => UpdateCheck::Off,
        }
    }

    fn available(&self) -> Option<update::Version> {
        update::available(self.state.as_ref()?)
    }

    fn save(&self, notifier: &Notifier) {
        if let (Some(path), Some(state)) = (&self.path, &self.state) {
            if let Err(e) = state.store(path) {
                notifier.show("Update check setting not saved", &e);
            }
        }
    }

    /// Announces a version once, then remembers it.
    fn notify(&mut self, notifier: &Notifier) {
        let Some(state) = &self.state else { return };
        let Some(version) = pending_notice(state, self.available()) else {
            return;
        };
        notifier.show(
            &format!("HerdCtrl {version} is available"),
            "Update from the tray menu, or run herdctrl-update in a terminal.",
        );
        if let Some(state) = &mut self.state {
            state.notified = Some(version.to_string());
        }
        self.save(notifier);
    }

    fn start_check(&mut self, tx: &Sender<Message>) {
        self.running = true;
        self.force = false;
        let tx = tx.clone();
        thread::spawn(move || {
            let _ = tx.send(Message::Checked(update::fetch_latest()));
        });
    }

    /// Returns whether the result changed the state (worth saving and announcing).
    fn finish_check(&mut self, result: Result<update::Version, String>) -> bool {
        self.running = false;
        match result {
            Ok(version) => {
                if let Some(state) = &mut self.state {
                    state.checked_at = update::now();
                    state.latest = Some(version.to_string());
                }
                true
            }
            Err(e) => {
                eprintln!("Update check failed: {e}");
                self.not_before = Instant::now() + Duration::from_secs(3600);
                false
            }
        }
    }

    fn set_enabled(&mut self, enabled: bool, notifier: &Notifier) {
        if self.state.is_none() && self.path.is_some() {
            // Deliberately replaces the invalid file.
            self.state = Some(State {
                enabled,
                ..State::default()
            });
        }
        let Some(state) = &mut self.state else { return };
        state.enabled = enabled;
        if enabled {
            self.force = true;
            self.not_before = Instant::now();
        }
        self.save(notifier);
    }
}

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn request_stop(_signal: libc::c_int) {
    // Only an atomic store: everything else is unsafe inside a signal handler.
    STOP.store(true, Ordering::Relaxed);
}

/// SIGTERM, SIGINT and SIGHUP end the run loop through the normal shutdown.
fn handle_stop_signals() -> Result<(), String> {
    // SAFETY: the handler only stores to a lock-free atomic; the sigaction struct is
    // zeroed and fully set up before use. SA_RESTART keeps other threads' blocking
    // calls (zbus, global-hotkey, ksni) from failing with EINTR.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = request_stop as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
    }
    Ok(())
}

fn stop_requested() -> bool {
    STOP.load(Ordering::Relaxed)
}

/// FNV-1a, 64 bit: stable across Rust toolchains, unlike DefaultHasher.
fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// "herdctrl-{kind}-{:016x}.lock" over the bytes of the layouts path.
fn lock_name(kind: &str, config: &Path) -> String {
    format!(
        "herdctrl-{kind}-{:016x}.lock",
        fnv1a64(config.as_os_str().as_bytes())
    )
}

/// Takes an exclusive per-session lock, or returns `None` if another process holds it.
/// Locks are per layouts file, so a separate configuration gets its own tray.
pub fn lock(kind: &str) -> Result<Option<File>, String> {
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let name = lock_name(kind, &Config::path()?);
    let file = File::create(directory.join(name)).map_err(|e| e.to_string())?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => Err(e.to_string()),
    }
}

/// True while a tray process holds this layouts file's lock.
pub fn is_running() -> bool {
    matches!(lock(TRAY_LOCK), Ok(None))
}

/// Starts the tray process unless one is already running in this session.
pub fn ensure_running() -> Result<(), String> {
    if lock(TRAY_LOCK)?.is_none() {
        return Ok(());
    }
    spawn_self(&["--tray"])
}

/// The running binary may have been rebuilt; start the file that is there now.
fn executable() -> Result<PathBuf, String> {
    let path = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(path
        .to_str()
        .and_then(|s| s.strip_suffix(" (deleted)"))
        .map(PathBuf::from)
        .unwrap_or(path))
}

fn spawn_self(args: &[&str]) -> Result<(), String> {
    let mut child = crate::os::child_env::clean(&mut Command::new(executable()?))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    thread::spawn(move || child.wait());
    Ok(())
}

pub fn run() -> Result<(), String> {
    let Some(_lock) = lock(TRAY_LOCK)? else {
        eprintln!("HerdCtrl is already running in the tray");
        return Ok(());
    };
    if let Err(e) = handle_stop_signals() {
        eprintln!("HerdCtrl: signal handlers not installed: {e}");
    }
    let path = Config::path()?;
    let (tx, rx) = mpsc::channel();
    let (events, event_rx) = mpsc::channel();
    let (launches, launch_rx) = mpsc::channel();
    let mut hotkeys = Hotkeys::new(Backend::detect(), events.clone());
    let notifier = Notifier::default();
    let mut updates = Updates::new(&notifier);
    updates.notify(&notifier);
    // The loop keeps its own sender, so hotkeys keep running even if the icon fails.
    let handle = Tray {
        views: Vec::new(),
        autostart: autostart_enabled(),
        tx: tx.clone(),
        icon: icon(),
        update: updates.available(),
        update_check: updates.check(),
    }
    // Without a tray host (screen locked, AppIndicator extension off) wait for one to
    // appear instead of failing; ksni registers the icon when the watcher comes online.
    .assume_sni_available(true)
    .spawn()
    .map_err(|e| notifier.show("Tray icon unavailable", &e.to_string()))
    .ok();
    let mut config = Config::default();
    let mut stamp = None;
    let mut next_check = Instant::now();
    let mut autostart = autostart_enabled();
    loop {
        if stop_requested() {
            break;
        }
        if Instant::now() >= next_check {
            next_check = Instant::now() + Duration::from_secs(1);
            // The editor can toggle start at login too; keep the checkmark in step.
            let enabled = autostart_enabled();
            if enabled != autostart {
                autostart = enabled;
                if let Some(handle) = &handle {
                    handle.update(|tray| tray.autostart = enabled);
                }
            }
            if let Some(state) = &updates.state {
                if should_check(
                    state,
                    updates.running,
                    Instant::now(),
                    updates.not_before,
                    updates.force,
                    update::now(),
                ) {
                    updates.start_check(&tx);
                }
            }
            let current = modified(&path);
            if current != stamp {
                stamp = current;
                match Config::load(&path) {
                    Ok(loaded) => {
                        config = loaded;
                        hotkeys.configure(&config, &notifier);
                        let views = menu_views(&config);
                        if let Some(handle) = &handle {
                            handle.update(|tray| tray.views = views);
                        }
                    }
                    Err(e) => notifier.show("Layouts could not be loaded", &e),
                }
            }
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Message::Launch(index)) => launch(&config, index, &launches),
            Ok(Message::Edit) => match lock(EDITOR_LOCK) {
                Ok(Some(_)) => {
                    if let Err(e) = spawn_self(&[]) {
                        notifier.show("Editor could not start", &e);
                    }
                }
                Ok(None) => notifier.show("HerdCtrl", "The layout editor is already open"),
                Err(e) => notifier.show("Editor could not start", &e),
            },
            Ok(Message::Autostart(enabled)) => {
                if let Err(e) = set_autostart(enabled) {
                    notifier.show("Start at login could not be changed", &e);
                }
                autostart = autostart_enabled();
                if let Some(handle) = &handle {
                    handle.update(|tray| tray.autostart = autostart);
                }
            }
            Ok(Message::Checked(result)) => {
                if updates.finish_check(result) {
                    updates.save(&notifier);
                    updates.notify(&notifier);
                }
                refresh_update(&handle, &updates);
            }
            Ok(Message::UpdateCheck(enabled)) => {
                updates.set_enabled(enabled, &notifier);
                refresh_update(&handle, &updates);
            }
            Ok(Message::Update) => {
                let tx = tx.clone();
                // Application discovery and terminal detection block.
                thread::spawn(move || {
                    if let Err(e) = update::launch_updater() {
                        let _ = tx.send(Message::UpdaterFailed(e));
                    }
                });
            }
            Ok(Message::UpdaterFailed(e)) => notifier.show("Updater could not start", &e),
            Ok(Message::Quit) => break,
            Err(RecvTimeoutError::Disconnected) => unreachable!("the run loop holds a sender"),
            Err(RecvTimeoutError::Timeout) => {}
        }
        if stop_requested() {
            break;
        }
        for index in hotkeys.triggered() {
            launch(&config, index, &launches);
        }
        report(&event_rx, &launch_rx, &notifier);
    }
    // Dropping the GNOME worker releases its grabs before the tray disappears.
    drop(hotkeys);
    if let Some(handle) = handle {
        handle.shutdown().wait();
    }
    Ok(())
}

fn refresh_update(handle: &Option<ksni::blocking::Handle<Tray>>, updates: &Updates) {
    let (update, check) = (updates.available(), updates.check());
    if let Some(handle) = handle {
        handle.update(|tray| {
            tray.update = update;
            tray.update_check = check;
        });
    }
}

fn launch(config: &Config, index: usize, events: &Sender<Report>) {
    if let Some(view) = config.views.get(index) {
        launcher::launch(view.clone(), events.clone());
    }
}

/// Successful placements stay quiet; anything else needs the user's attention.
fn report(events: &Receiver<String>, launches: &Receiver<Report>, notifier: &Notifier) {
    while let Ok(message) = events.try_recv() {
        notifier.show("HerdCtrl", &message);
    }
    while let Ok(report) = launches.try_recv() {
        eprintln!("{report}");
        if !report.is_success() {
            notifier.show("HerdCtrl", &report.to_string());
        }
    }
}

fn modified(path: &PathBuf) -> Option<(SystemTime, u64)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

enum Hotkeys {
    Gnome(gnome::hotkeys::Hotkeys),
    X11 {
        manager: GlobalHotKeyManager,
        bindings: Vec<(HotKey, usize)>,
    },
    Unavailable,
}

impl Hotkeys {
    fn new(backend: Backend, events: Sender<String>) -> Self {
        match backend {
            Backend::Gnome => Self::Gnome(gnome::hotkeys::Hotkeys::new(events)),
            Backend::X11 => match GlobalHotKeyManager::new() {
                Ok(manager) => Self::X11 {
                    manager,
                    bindings: Vec::new(),
                },
                Err(e) => {
                    let _ = events.send(format!("Global hotkeys unavailable: {e}"));
                    Self::Unavailable
                }
            },
            Backend::UnsupportedWayland => {
                let _ = events.send(
                    "Global hotkeys need GNOME 50 with the HerdCtrl extension, or an X11 session"
                        .into(),
                );
                Self::Unavailable
            }
        }
    }

    fn configure(&mut self, config: &Config, notifier: &Notifier) {
        match self {
            Self::Gnome(hotkeys) => hotkeys.configure(&config.views),
            Self::X11 { manager, bindings } => {
                for (key, _) in bindings.drain(..) {
                    let _ = manager.unregister(key);
                }
                for (index, view) in config.views.iter().enumerate() {
                    if view.hotkey.trim().is_empty() {
                        continue;
                    }
                    match view.hotkey.parse::<HotKey>() {
                        Ok(key) if key.mods.is_empty() => notifier.show(
                            "HerdCtrl",
                            &format!(
                                "{} hotkey needs at least one modifier (Super, Ctrl, Alt or Shift)",
                                view.name
                            ),
                        ),
                        Ok(key) => match manager.register(key) {
                            Ok(()) => bindings.push((key, index)),
                            Err(e) => notifier.show(
                                "HerdCtrl",
                                &format!("{} hotkey unavailable: {e}", view.name),
                            ),
                        },
                        Err(e) => {
                            notifier.show("HerdCtrl", &format!("{} hotkey invalid: {e}", view.name))
                        }
                    }
                }
            }
            Self::Unavailable => {}
        }
    }

    fn triggered(&self) -> Vec<usize> {
        let mut result = Vec::new();
        match self {
            Self::Gnome(hotkeys) => {
                while let Some(index) = hotkeys.take_trigger() {
                    result.push(index);
                }
            }
            Self::X11 { bindings, .. } => {
                while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                    if event.state == HotKeyState::Pressed {
                        result.extend(
                            bindings
                                .iter()
                                .filter(|(key, _)| key.id() == event.id)
                                .map(|(_, index)| *index),
                        );
                    }
                }
            }
            Self::Unavailable => {}
        }
        result
    }
}

/// Desktop notifications over the session bus; failures only go to stderr.
#[derive(Default)]
struct Notifier(std::sync::OnceLock<Option<zbus::blocking::Connection>>);

impl Notifier {
    fn show(&self, summary: &str, body: &str) {
        eprintln!("{summary}: {body}");
        let Some(connection) = self
            .0
            .get_or_init(|| zbus::blocking::Connection::session().ok())
        else {
            return;
        };
        let hints: HashMap<&str, zbus::zvariant::Value> = HashMap::new();
        let _ = connection.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "HerdCtrl",
                0u32,
                "",
                summary,
                body,
                Vec::<&str>::new(),
                hints,
                6000i32,
            ),
        );
    }
}

struct Tray {
    /// Menu labels, indexed like `Config::views`.
    views: Vec<String>,
    autostart: bool,
    tx: Sender<Message>,
    icon: Vec<Icon>,
    update: Option<update::Version>,
    update_check: UpdateCheck,
}

impl Tray {
    fn send(&self, message: Message) {
        let _ = self.tx.send(message);
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "herdctrl".into()
    }

    fn title(&self) -> String {
        "HerdCtrl".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icon.clone()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Message::Edit);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = self
            .views
            .iter()
            .enumerate()
            .map(|(index, label)| {
                StandardItem {
                    label: label.clone(),
                    activate: Box::new(move |tray: &mut Self| tray.send(Message::Launch(index))),
                    ..Default::default()
                }
                .into()
            })
            .collect();
        if items.is_empty() {
            items.push(
                StandardItem {
                    label: "No views yet".into(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(MenuItem::Separator);
        if let Some(version) = self.update {
            items.push(
                StandardItem {
                    label: update_label(version),
                    activate: Box::new(|tray: &mut Self| tray.send(Message::Update)),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.extend([
            StandardItem {
                label: "Edit layouts…".into(),
                activate: Box::new(|tray: &mut Self| tray.send(Message::Edit)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Start at login".into(),
                checked: self.autostart,
                activate: Box::new(|tray: &mut Self| {
                    tray.send(Message::Autostart(!tray.autostart))
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Check for updates daily".into(),
                checked: self.update_check == UpdateCheck::On,
                enabled: self.update_check != UpdateCheck::Unavailable,
                activate: Box::new(|tray: &mut Self| {
                    tray.send(Message::UpdateCheck(tray.update_check != UpdateCheck::On))
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit HerdCtrl".into(),
                activate: Box::new(|tray: &mut Self| tray.send(Message::Quit)),
                ..Default::default()
            }
            .into(),
        ]);
        items
    }
}

fn menu_views(config: &Config) -> Vec<String> {
    config
        .views
        .iter()
        .map(|view| {
            // Underscores are mnemonic markers in D-Bus menus.
            let name = view.name.replace('_', "__");
            match shortcut_label(&view.hotkey) {
                Some(hotkey) => format!("{name}   {hotkey}"),
                None => name,
            }
        })
        .collect()
}

/// `Super+Shift+KeyD` → `Super+Shift+D`.
fn shortcut_label(hotkey: &str) -> Option<String> {
    let hotkey = hotkey.trim();
    if hotkey.is_empty() {
        return None;
    }
    Some(
        hotkey
            .split('+')
            .map(|part| {
                part.strip_prefix("Key")
                    .or_else(|| part.strip_prefix("Digit"))
                    .filter(|rest| rest.len() == 1)
                    .unwrap_or(part)
            })
            .collect::<Vec<_>>()
            .join("+"),
    )
}

fn icon() -> Vec<Icon> {
    let Ok(image) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) else {
        return Vec::new();
    };
    // StatusNotifierItem pixmaps are ARGB32 in network byte order.
    let data = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|[r, g, b, a]| [*a, *r, *g, *b])
        .collect();
    vec![Icon {
        width: image.width as i32,
        height: image.height as i32,
        data,
    }]
}

pub fn autostart_enabled() -> bool {
    autostart_path().is_ok_and(|path| path.exists())
}

fn autostart_path() -> Result<PathBuf, String> {
    let config = Config::path()?;
    let base = config
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Invalid configuration path")?;
    Ok(base.join("autostart/herdctrl.desktop"))
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let path = autostart_path()?;
    if !enabled {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let executable = executable()?;
    let executable = executable.to_str().ok_or("Executable path is not UTF-8")?;
    fs::create_dir_all(path.parent().ok_or("Invalid autostart path")?)
        .map_err(|e| e.to_string())?;
    fs::write(&path, autostart_entry(executable)).map_err(|e| e.to_string())
}

fn autostart_entry(executable: &str) -> String {
    // Desktop Entry Exec quoting: escape ", `, $ and \ inside double quotes.
    let mut quoted = String::from('"');
    for ch in executable.chars() {
        if matches!(ch, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    format!(
        "[Desktop Entry]\nType=Application\nName=HerdCtrl\nComment=Global hotkeys for HerdCtrl views\nExec={quoted} --tray\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_hotkeys_readably() {
        assert_eq!(
            shortcut_label("Super+Shift+KeyD").as_deref(),
            Some("Super+Shift+D")
        );
        assert_eq!(
            shortcut_label("Control+Digit1").as_deref(),
            Some("Control+1")
        );
        assert_eq!(shortcut_label("Alt+F10").as_deref(), Some("Alt+F10"));
        assert_eq!(shortcut_label(" "), None);
    }

    #[test]
    fn escapes_menu_mnemonics() {
        let mut config = Config::default();
        config.views.push(crate::core::config::View {
            name: "dev_setup".into(),
            hotkey: "Super+KeyD".into(),
            zones: Vec::new(),
        });
        assert_eq!(menu_views(&config), ["dev__setup   Super+D"]);
    }

    #[test]
    fn quotes_autostart_exec() {
        let entry = autostart_entry("/home/a b/$x/herd\"ctrl");
        assert!(entry.contains("Exec=\"/home/a b/\\$x/herd\\\"ctrl\" --tray\n"));
    }

    fn menu_labels(update: Option<update::Version>, check: UpdateCheck) -> Vec<(String, bool)> {
        use ksni::Tray as _;
        let (tx, _rx) = mpsc::channel();
        let tray = Tray {
            views: Vec::new(),
            autostart: false,
            tx,
            icon: Vec::new(),
            update,
            update_check: check,
        };
        tray.menu()
            .into_iter()
            .filter_map(|item| match item {
                MenuItem::Standard(i) => Some((i.label, i.enabled)),
                MenuItem::Checkmark(i) => Some((i.label, i.enabled)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn menu_with_update_and_check() {
        let labels = menu_labels(Some(update::Version(0, 2, 0)), UpdateCheck::On);
        let names: Vec<_> = labels.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            names,
            [
                "No views yet",
                "Update to 0.2.0…",
                "Edit layouts…",
                "Start at login",
                "Check for updates daily",
                "Quit HerdCtrl"
            ]
        );
        assert!(labels[4].1);
    }

    #[test]
    fn menu_unavailable_greys_checkmark() {
        let labels = menu_labels(None, UpdateCheck::Unavailable);
        assert_eq!(labels.len(), 5);
        assert_eq!(labels[3], ("Check for updates daily".to_string(), false));
    }

    #[test]
    fn notice_once_per_version() {
        let v = update::Version(0, 2, 0);
        let mut state = State::default();
        assert_eq!(pending_notice(&state, Some(v)), Some(v));
        state.notified = Some("0.2.0".into());
        assert_eq!(pending_notice(&state, Some(v)), None);
        assert_eq!(pending_notice(&state, None), None);
    }

    #[test]
    fn check_planning() {
        let now = Instant::now();
        let mut state = State::default();
        assert!(should_check(&state, false, now, now, false, 1_000_000));
        assert!(!should_check(&state, true, now, now, false, 1_000_000));
        let later = now + Duration::from_secs(5);
        assert!(!should_check(&state, false, now, later, true, 1_000_000));
        state.checked_at = 1_000_000;
        assert!(!should_check(&state, false, now, now, false, 1_000_100));
        assert!(should_check(&state, false, now, now, true, 1_000_100));
        state.enabled = false;
        assert!(!should_check(&state, false, now, now, true, 9_000_000));
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("herdctrl-tray-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("update.json")
    }

    #[test]
    fn invalid_update_file_is_off_but_active_and_replaced_on_click() {
        let path = temp_path("invalid");
        std::fs::write(&path, "{not json").unwrap();
        let (mut updates, problem) = Updates::with_path(Ok(path.clone()));
        assert!(problem.is_some());
        assert_eq!(updates.check(), UpdateCheck::Off);
        // No save without a state: the invalid file stays.
        updates.save(&Notifier::default());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{not json");
        updates.set_enabled(true, &Notifier::default());
        assert_eq!(updates.check(), UpdateCheck::On);
        assert_eq!(State::load(&path).unwrap(), State::default());
    }

    #[test]
    fn missing_path_is_unavailable() {
        let (updates, problem) = Updates::with_path(Err("x".into()));
        assert!(problem.is_some());
        assert_eq!(updates.check(), UpdateCheck::Unavailable);
    }

    #[test]
    fn failed_check_changes_nothing() {
        let (updates, _) = Updates::with_path(Ok(temp_path("fail")));
        let mut updates = updates;
        assert!(!updates.finish_check(Err("offline".into())));
        assert_eq!(updates.state.as_ref().unwrap().checked_at, 0);
        assert!(updates.finish_check(Ok(update::Version::parse("1.2.3").unwrap())));
        assert_eq!(
            updates.state.as_ref().unwrap().latest.as_deref(),
            Some("1.2.3")
        );
    }

    #[test]
    fn fnv1a_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(
            lock_name("tray", Path::new("/home/u/.config/herdctrl/layouts.json")),
            "herdctrl-tray-b7fc927f20e81cf5.lock"
        );
    }

    #[test]
    fn stop_signal_sets_flag() {
        handle_stop_signals().unwrap();
        // SAFETY: raising a signal whose handler was just installed.
        assert_eq!(unsafe { libc::raise(libc::SIGHUP) }, 0);
        assert!(stop_requested());
        STOP.store(false, Ordering::Relaxed);
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            // SAFETY: restoring the default disposition.
            unsafe { libc::signal(signal, libc::SIG_DFL) };
        }
    }
}
