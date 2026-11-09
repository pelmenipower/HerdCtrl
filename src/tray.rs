//! Background tray process: owns the global hotkeys and launches views without
//! an editor window, so no renderer or GUI state stays resident.
use crate::{
    core::{config::Config, launcher},
    os::{gnome, Backend},
};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use ksni::{blocking::TrayMethods, menu::*, Icon, MenuItem};
use std::{
    collections::HashMap,
    fs::{self, File},
    hash::{Hash, Hasher},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
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
}

/// Takes an exclusive per-session lock, or returns `None` if another process holds it.
/// Locks are per layouts file, so a separate configuration gets its own tray.
pub fn lock(kind: &str) -> Result<Option<File>, String> {
    let directory = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let mut hasher = std::hash::DefaultHasher::new();
    Config::path()?.hash(&mut hasher);
    let name = format!("herdctrl-{kind}-{:016x}.lock", hasher.finish());
    let file = File::create(directory.join(name)).map_err(|e| e.to_string())?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => Err(e.to_string()),
    }
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
    let mut child = Command::new(executable()?)
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
    let path = Config::path()?;
    let (tx, rx) = mpsc::channel();
    let (events, event_rx) = mpsc::channel();
    let mut hotkeys = Hotkeys::new(Backend::detect(), events.clone());
    let notifier = Notifier::default();
    let tray = Tray {
        views: Vec::new(),
        autostart: autostart_path().is_ok_and(|p| p.exists()),
        tx,
        icon: icon(),
    };
    let handle = tray
        .spawn()
        .map_err(|e| notifier.show("Tray icon unavailable", &e.to_string()))
        .ok();
    let mut config = Config::default();
    let mut stamp = None;
    let mut next_check = Instant::now();
    loop {
        if Instant::now() >= next_check {
            next_check = Instant::now() + Duration::from_secs(1);
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
            Ok(Message::Launch(index)) => launch(&config, index, &events),
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
                let enabled = autostart_path().is_ok_and(|p| p.exists());
                if let Some(handle) = &handle {
                    handle.update(|tray| tray.autostart = enabled);
                }
            }
            Ok(Message::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        for index in hotkeys.triggered() {
            launch(&config, index, &events);
        }
        report(&event_rx, &notifier);
    }
    // Dropping the GNOME worker releases its grabs before the tray disappears.
    drop(hotkeys);
    if let Some(handle) = handle {
        handle.shutdown().wait();
    }
    Ok(())
}

fn launch(config: &Config, index: usize, events: &Sender<String>) {
    if let Some(view) = config.views.get(index) {
        launcher::launch(view.clone(), events.clone());
    }
}

/// Successful placements stay quiet; anything else needs the user's attention.
fn report(events: &Receiver<String>, notifier: &Notifier) {
    while let Ok(message) = events.try_recv() {
        eprintln!("{message}");
        if !is_success(&message) {
            notifier.show("HerdCtrl", &message);
        }
    }
}

fn is_success(message: &str) -> bool {
    message.ends_with(": positioned") || message.ends_with(": existing window arranged")
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
        items.extend([
            MenuItem::Separator,
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

fn autostart_path() -> Result<PathBuf, String> {
    let config = Config::path()?;
    let base = config
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Invalid configuration path")?;
    Ok(base.join("autostart/herdctrl.desktop"))
}

fn set_autostart(enabled: bool) -> Result<(), String> {
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

    #[test]
    fn only_placements_are_quiet() {
        assert!(is_success("Editor: positioned"));
        assert!(is_success("Shell: existing window arranged"));
        assert!(!is_success("Shell: monitor DP-1 is disconnected"));
    }
}
