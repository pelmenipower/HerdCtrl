use eframe::egui::{self, Color32, Key};
use global_hotkey::hotkey::{HotKey, Modifiers};

/// Recording state for the hotkey widget. Reset whenever the selected view changes.
#[derive(Default)]
pub struct Recorder {
    recording: bool,
    /// Why the last key press was not stored; recording continues with this hint.
    hint: Option<&'static str>,
}

/// What a single key press does while recording.
#[derive(Debug, PartialEq)]
enum Press {
    Cancel,
    Capture(&'static str),
    Unsupported,
}

/// Modifiers in the order they are written to the stored hotkey string.
const MODIFIERS: [(Modifiers, &str); 4] = [
    (Modifiers::SUPER, "Super"),
    (Modifiers::CONTROL, "Ctrl"),
    (Modifiers::ALT, "Alt"),
    (Modifiers::SHIFT, "Shift"),
];

/// Shows the hotkey as one button that records the pressed combination. Returns true when `hotkey` changed.
pub fn show(ui: &mut egui::Ui, recorder: &mut Recorder, hotkey: &mut String) -> bool {
    let mut next = None;
    // A focused text field owns the keyboard, so recording stops instead of stealing its keys.
    if recorder.recording && ui.ctx().wants_keyboard_input() {
        *recorder = Recorder::default();
    }
    // Handle key presses before drawing buttons so the press that starts recording is not reused.
    if recorder.recording {
        let presses: Vec<(Key, egui::Modifiers)> = ui.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
                .collect()
        });
        for (pressed, held) in presses {
            match press_outcome(pressed) {
                Press::Cancel => {
                    *recorder = Recorder::default();
                    break;
                }
                Press::Capture(code) => {
                    let modifiers = modifiers(held, super_held());
                    if modifiers.is_empty() {
                        recorder.hint =
                            Some("Hold Super, Ctrl, Alt or Shift while pressing the key");
                        continue;
                    }
                    next = Some(compose(modifiers, Some(code)));
                    *recorder = Recorder::default();
                    break;
                }
                Press::Unsupported => recorder.hint = Some("Key not supported, press another key"),
            }
        }
    }

    let mut clear = false;
    ui.horizontal(|ui| {
        let text = if recorder.recording {
            "Press the shortcut…".to_string()
        } else if hotkey.trim().is_empty() {
            "Record hotkey…".to_string()
        } else {
            label(hotkey)
        };
        let button = ui.add(egui::Button::new(text).selected(recorder.recording));
        if button.clicked() && !recorder.recording {
            recorder.recording = true;
        }
        button.on_hover_text(
            "Click, then press the whole combination, e.g. Super+Alt+D. Escape cancels.",
        );
        if !hotkey.is_empty() && ui.button("Clear").clicked() {
            clear = true;
        }
        if let Some(hint) = recorder.hint.filter(|_| recorder.recording) {
            ui.colored_label(Color32::from_rgb(244, 160, 110), hint);
        }
    });

    if clear {
        *recorder = Recorder::default();
        next = Some(String::new());
    }
    match next {
        Some(next) if next != *hotkey => {
            *hotkey = next;
            true
        }
        _ => false,
    }
}

/// Global-hotkey modifiers from egui's state plus Super, which egui does not report on Linux.
fn modifiers(held: egui::Modifiers, super_key: bool) -> Modifiers {
    let mut result = Modifiers::empty();
    result.set(Modifiers::SUPER, super_key);
    result.set(Modifiers::CONTROL, held.ctrl);
    result.set(Modifiers::ALT, held.alt);
    result.set(Modifiers::SHIFT, held.shift);
    result
}

/// Asks the X server (the editor runs on X11 or XWayland) whether Super is held.
fn super_held() -> bool {
    use x11_dl::xlib;
    let Ok(xlib) = xlib::Xlib::open() else {
        return false;
    };
    // SAFETY: a private, short-lived display connection; every out-pointer is a valid local.
    unsafe {
        let display = (xlib.XOpenDisplay)(std::ptr::null());
        if display.is_null() {
            return false;
        }
        let root = (xlib.XDefaultRootWindow)(display);
        let (mut root_return, mut child) = (0, 0);
        let (mut root_x, mut root_y, mut x, mut y) = (0, 0, 0, 0);
        let mut mask = 0;
        (xlib.XQueryPointer)(
            display,
            root,
            &mut root_return,
            &mut child,
            &mut root_x,
            &mut root_y,
            &mut x,
            &mut y,
            &mut mask,
        );
        (xlib.XCloseDisplay)(display);
        mask & xlib::Mod4Mask != 0
    }
}

/// Readable form of a stored hotkey, e.g. `Super+Alt+KeyD` as `Super+Alt+D`.
fn label(hotkey: &str) -> String {
    hotkey
        .split('+')
        .map(|part| key_label(part.trim()))
        .collect::<Vec<_>>()
        .join("+")
}

/// True when `hotkey` parses and has at least one modifier. A bare key would be grabbed system-wide.
pub fn has_modifier(hotkey: &str) -> bool {
    hotkey
        .parse::<HotKey>()
        .is_ok_and(|key| !key.mods.is_empty())
}

/// Builds the stored hotkey string. Without a key there is no hotkey, so the result is empty.
fn compose(modifiers: Modifiers, code: Option<&str>) -> String {
    let Some(code) = code else {
        return String::new();
    };
    let mut parts: Vec<&str> = MODIFIERS
        .iter()
        .filter(|(modifier, _)| modifiers.contains(*modifier))
        .map(|(_, label)| *label)
        .collect();
    parts.push(code);
    parts.join("+")
}

/// Text shown on the key button, e.g. `KeyD` as `D`.
fn key_label(code: &str) -> String {
    code.strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))
        .unwrap_or(code)
        .to_string()
}

/// Escape cancels recording; other keys are captured when they have a `global_hotkey` code.
fn press_outcome(key: Key) -> Press {
    if key == Key::Escape {
        return Press::Cancel;
    }
    key_code(key).map_or(Press::Unsupported, Press::Capture)
}

/// The `global_hotkey` code name for an egui key, or None when the key cannot be a hotkey.
fn key_code(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::A => "KeyA",
        Key::B => "KeyB",
        Key::C => "KeyC",
        Key::D => "KeyD",
        Key::E => "KeyE",
        Key::F => "KeyF",
        Key::G => "KeyG",
        Key::H => "KeyH",
        Key::I => "KeyI",
        Key::J => "KeyJ",
        Key::K => "KeyK",
        Key::L => "KeyL",
        Key::M => "KeyM",
        Key::N => "KeyN",
        Key::O => "KeyO",
        Key::P => "KeyP",
        Key::Q => "KeyQ",
        Key::R => "KeyR",
        Key::S => "KeyS",
        Key::T => "KeyT",
        Key::U => "KeyU",
        Key::V => "KeyV",
        Key::W => "KeyW",
        Key::X => "KeyX",
        Key::Y => "KeyY",
        Key::Z => "KeyZ",
        Key::Num0 => "Digit0",
        Key::Num1 => "Digit1",
        Key::Num2 => "Digit2",
        Key::Num3 => "Digit3",
        Key::Num4 => "Digit4",
        Key::Num5 => "Digit5",
        Key::Num6 => "Digit6",
        Key::Num7 => "Digit7",
        Key::Num8 => "Digit8",
        Key::Num9 => "Digit9",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::F13 => "F13",
        Key::F14 => "F14",
        Key::F15 => "F15",
        Key::F16 => "F16",
        Key::F17 => "F17",
        Key::F18 => "F18",
        Key::F19 => "F19",
        Key::F20 => "F20",
        Key::F21 => "F21",
        Key::F22 => "F22",
        Key::F23 => "F23",
        Key::F24 => "F24",
        Key::ArrowUp => "ArrowUp",
        Key::ArrowDown => "ArrowDown",
        Key::ArrowLeft => "ArrowLeft",
        Key::ArrowRight => "ArrowRight",
        Key::Space => "Space",
        Key::Enter => "Enter",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Delete => "Delete",
        Key::Insert => "Insert",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Minus => "Minus",
        Key::Equals => "Equal",
        Key::Comma => "Comma",
        Key::Period => "Period",
        Key::Slash => "Slash",
        Key::Backslash => "Backslash",
        Key::Semicolon => "Semicolon",
        Key::Quote => "Quote",
        Key::Backtick => "Backquote",
        Key::OpenBracket => "BracketLeft",
        Key::CloseBracket => "BracketRight",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_held_modifiers_in_stored_order() {
        let held = egui::Modifiers {
            alt: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            compose(modifiers(held, true), Some("KeyD")),
            "Super+Alt+Shift+KeyD"
        );
        assert!(modifiers(egui::Modifiers::default(), false).is_empty());
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        assert_eq!(compose(modifiers(ctrl, false), Some("F5")), "Ctrl+F5");
        assert_eq!(label("Super+Alt+KeyD"), "Super+Alt+D");
        assert_eq!(label("Ctrl+Digit1"), "Ctrl+1");
    }

    #[test]
    fn compose_without_key_is_empty() {
        assert_eq!(compose(Modifiers::SUPER, None), "");
        assert_eq!(compose(Modifiers::empty(), Some("F5")), "F5");
    }

    #[test]
    fn key_code_maps_common_keys() {
        assert_eq!(key_code(Key::D), Some("KeyD"));
        assert_eq!(key_code(Key::Num5), Some("Digit5"));
        assert_eq!(key_code(Key::F5), Some("F5"));
        assert_eq!(key_code(Key::Equals), Some("Equal"));
        assert_eq!(key_code(Key::OpenBracket), Some("BracketLeft"));
        assert_eq!(key_code(Key::Escape), None);
        assert_eq!(key_label("KeyD"), "D");
        assert_eq!(key_label("F5"), "F5");
    }

    #[test]
    fn every_key_code_is_accepted_by_global_hotkey_and_gnome() {
        let codes: Vec<&str> = Key::ALL.iter().filter_map(|key| key_code(*key)).collect();
        assert!(codes.len() >= 60);
        for code in codes {
            let hotkey = compose(Modifiers::SUPER, Some(code));
            assert!(hotkey.parse::<HotKey>().is_ok(), "{hotkey} is not a HotKey");
            assert!(
                crate::os::gnome::hotkeys::accelerator(&hotkey).is_ok(),
                "{hotkey} has no GNOME accelerator"
            );
        }
    }

    #[test]
    fn has_modifier_requires_a_parsed_modifier() {
        assert!(has_modifier("Super+KeyD"));
        assert!(has_modifier("Ctrl+F5"));
        assert!(!has_modifier("KeyD"));
        assert!(!has_modifier("F5"));
        assert!(!has_modifier("Super"));
        assert!(!has_modifier("NotAKey"));
        assert!(!has_modifier(""));
    }

    #[test]
    fn press_outcome_cancels_captures_or_flags_unsupported_keys() {
        assert_eq!(press_outcome(Key::Escape), Press::Cancel);
        assert_eq!(press_outcome(Key::D), Press::Capture("KeyD"));
        let unmapped = Key::ALL
            .iter()
            .copied()
            .find(|key| *key != Key::Escape && key_code(*key).is_none())
            .expect("egui has keys without a hotkey code");
        assert_eq!(press_outcome(unmapped), Press::Unsupported);
    }
}
