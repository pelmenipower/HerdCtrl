use eframe::egui::{self, Color32, Key, RichText};
use global_hotkey::hotkey::{HotKey, Modifiers};

/// Recording state for the hotkey widget. Reset whenever the selected view changes.
#[derive(Default)]
pub struct Recorder {
    recording: bool,
    /// Modifier toggles kept while no key is chosen, because such a hotkey stores no text.
    modifiers: Modifiers,
}

/// Modifier toggles in the order they are written to the stored hotkey string.
const TOGGLES: [(Modifiers, &str); 4] = [
    (Modifiers::SUPER, "Super"),
    (Modifiers::CONTROL, "Ctrl"),
    (Modifiers::ALT, "Alt"),
    (Modifiers::SHIFT, "Shift"),
];

/// Shows the hotkey as modifier toggles plus one key. Returns true when `hotkey` changed.
pub fn show(ui: &mut egui::Ui, recorder: &mut Recorder, hotkey: &mut String) -> bool {
    if !is_representable(hotkey) {
        // Keep odd or legacy text visible so it can be cleared deliberately.
        let mut changed = false;
        ui.label(RichText::new(hotkey.as_str()).monospace());
        if ui.button("Clear").clicked() {
            hotkey.clear();
            changed = true;
        }
        return changed;
    }

    let (stored_modifiers, mut key) = parse(hotkey);
    let mut modifiers = if key.is_some() {
        stored_modifiers
    } else {
        recorder.modifiers
    };

    // Handle key presses before drawing buttons so the press that starts recording is not reused.
    if recorder.recording {
        let presses: Vec<Key> = ui.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => Some(*key),
                    _ => None,
                })
                .collect()
        });
        for pressed in presses {
            if pressed == Key::Escape {
                recorder.recording = false;
                break;
            }
            if let Some(code) = key_code(pressed) {
                key = Some(code.to_string());
                recorder.recording = false;
                break;
            }
        }
    }

    let mut clear = false;
    ui.horizontal(|ui| {
        for (modifier, label) in TOGGLES {
            let mut on = modifiers.contains(modifier);
            if ui.toggle_value(&mut on, label).clicked() {
                modifiers.set(modifier, on);
            }
        }
        let key_text = if recorder.recording {
            "Press a key…".to_string()
        } else {
            key.as_deref().map_or("Set key…".to_string(), key_label)
        };
        if ui.button(key_text).clicked() && !recorder.recording {
            recorder.recording = true;
        }
        if ui.button("Clear").clicked() {
            clear = true;
        }
        if key.is_some() && modifiers.is_empty() {
            ui.colored_label(
                Color32::from_rgb(244, 160, 110),
                "Add at least one modifier",
            );
        }
    });

    if clear {
        recorder.recording = false;
        modifiers = Modifiers::empty();
        key = None;
    }
    recorder.modifiers = modifiers;
    let next = compose(modifiers, key.as_deref());
    if next != *hotkey {
        *hotkey = next;
        true
    } else {
        false
    }
}

/// Splits a stored hotkey such as `Super+Shift+KeyD` into modifiers and key code.
fn parse(hotkey: &str) -> (Modifiers, Option<String>) {
    let mut modifiers = Modifiers::empty();
    let mut code = None;
    for token in hotkey.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        match modifier_token(token) {
            Some(modifier) => modifiers |= modifier,
            None => code = Some(token.to_string()),
        }
    }
    (modifiers, code)
}

/// Builds the stored hotkey string. Without a key there is no hotkey, so the result is empty.
fn compose(modifiers: Modifiers, code: Option<&str>) -> String {
    let Some(code) = code else {
        return String::new();
    };
    let mut parts: Vec<&str> = TOGGLES
        .iter()
        .filter(|(modifier, _)| modifiers.contains(*modifier))
        .map(|(_, label)| *label)
        .collect();
    parts.push(code);
    parts.join("+")
}

/// Modifier tokens accepted by `global_hotkey::HotKey::from_str`, without the platform-dependent aliases.
fn modifier_token(token: &str) -> Option<Modifiers> {
    match token.to_ascii_uppercase().as_str() {
        "SUPER" | "CMD" | "COMMAND" => Some(Modifiers::SUPER),
        "CTRL" | "CONTROL" => Some(Modifiers::CONTROL),
        "ALT" | "OPTION" => Some(Modifiers::ALT),
        "SHIFT" => Some(Modifiers::SHIFT),
        _ => None,
    }
}

/// True when the toggles can show the stored hotkey exactly: at most one key, no repeated modifier,
/// and a string that `HotKey` parses.
fn is_representable(hotkey: &str) -> bool {
    let trimmed = hotkey.trim();
    if trimmed.is_empty() {
        return true;
    }
    let mut keys = 0;
    let mut seen = Modifiers::empty();
    for token in trimmed.split('+').map(str::trim) {
        match modifier_token(token) {
            Some(modifier) => {
                if seen.contains(modifier) {
                    return false;
                }
                seen |= modifier;
            }
            None => keys += 1,
        }
    }
    keys == 1 && trimmed.parse::<HotKey>().is_ok()
}

/// Text shown on the key button, e.g. `KeyD` as `D`.
fn key_label(code: &str) -> String {
    code.strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))
        .unwrap_or(code)
        .to_string()
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
    fn compose_and_parse_roundtrip() {
        let (modifiers, code) = parse("Super+Shift+KeyD");
        assert_eq!(modifiers, Modifiers::SUPER | Modifiers::SHIFT);
        assert_eq!(code.as_deref(), Some("KeyD"));
        assert_eq!(compose(modifiers, code.as_deref()), "Super+Shift+KeyD");

        let (modifiers, code) = parse("Ctrl+Alt+F5");
        assert_eq!(modifiers, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(compose(modifiers, code.as_deref()), "Ctrl+Alt+F5");
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
    fn representable_only_for_single_key_hotkeys() {
        assert!(is_representable(""));
        assert!(is_representable("Super+Shift+KeyD"));
        assert!(is_representable("KeyD"));
        assert!(!is_representable("Super+Super+KeyD"));
        assert!(!is_representable("Super+KeyD+KeyE"));
        assert!(!is_representable("Super"));
        assert!(!is_representable("Super+NotAKey"));
    }
}
