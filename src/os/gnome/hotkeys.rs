use super::call;
use crate::core::config::View;
use global_hotkey::hotkey::{HotKey, Modifiers};
use serde_json::{json, Value};
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct Hotkeys {
    commands: Sender<(u64, Vec<(usize, String)>)>,
    triggers: Receiver<(u64, usize)>,
    generation: u64,
    worker: Option<thread::JoinHandle<()>>,
}

/// Generation that tells the worker to release its grabs and stop.
const SHUTDOWN: u64 = u64::MAX;

impl Hotkeys {
    pub fn new(events: Sender<String>) -> Self {
        let (commands, command_rx) = mpsc::channel();
        let (trigger_tx, triggers) = mpsc::channel();
        let worker = thread::spawn(move || worker(command_rx, trigger_tx, events));
        Self {
            commands,
            triggers,
            generation: 0,
            worker: Some(worker),
        }
    }

    pub fn configure(&mut self, views: &[View]) {
        self.generation += 1;
        let bindings = views
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.hotkey.trim().is_empty())
            .map(|(index, view)| (index, view.hotkey.clone()))
            .collect();
        let _ = self.commands.send((self.generation, bindings));
    }

    pub fn take_trigger(&self) -> Option<usize> {
        while let Ok((generation, index)) = self.triggers.try_recv() {
            if generation == self.generation {
                return Some(index);
            }
        }
        None
    }
}

impl Drop for Hotkeys {
    // Release the grabs now; otherwise the next instance waits for the extension's heartbeat expiry.
    fn drop(&mut self) {
        let _ = self.commands.send((SHUTDOWN, Vec::new()));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker(
    commands: Receiver<(u64, Vec<(usize, String)>)>,
    triggers: Sender<(u64, usize)>,
    events: Sender<String>,
) {
    let token = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let mut generation = 0;
    let mut desired = Vec::<Value>::new();
    let mut connected = false;
    let mut last_error = None;
    loop {
        match commands.recv_timeout(if connected {
            Duration::from_millis(150)
        } else {
            Duration::from_secs(2)
        }) {
            Ok((SHUTDOWN, _)) | Err(RecvTimeoutError::Disconnected) => {
                let _ = call::<Value>(json!({"operation":"release", "token":token}));
                return;
            }
            Ok((next_generation, bindings)) => {
                generation = next_generation;
                desired.clear();
                for (id, shortcut) in bindings {
                    match accelerator(&shortcut) {
                        Ok(accelerator) => {
                            desired.push(json!({"id":id, "accelerator":accelerator}))
                        }
                        Err(error) => {
                            let _ = events.send(format!("View {}: {error}", id + 1));
                        }
                    }
                }
                // Clear pending events and old grabs before a new configuration.
                let _ = call::<Value>(json!({"operation":"release", "token":token}));
                connected = false;
                last_error = None;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if desired.is_empty() {
            continue;
        }
        let result = (|| -> Result<(), String> {
            if !connected {
                let errors: Vec<String> =
                    call(json!({"operation":"bindings", "token":token, "bindings":desired}))?;
                for error in errors {
                    let _ = events.send(error);
                }
                connected = true;
            }
            let pending: Vec<usize> = call(json!({"operation":"poll", "token":token}))?;
            for id in pending {
                let _ = triggers.send((generation, id));
            }
            Ok(())
        })();
        match result {
            Ok(()) => last_error = None,
            Err(error) => {
                connected = false;
                if last_error.as_ref() != Some(&error) {
                    let _ = events.send(error.clone());
                    last_error = Some(error);
                }
            }
        }
    }
}

pub fn accelerator(input: &str) -> Result<String, String> {
    let key: HotKey = input.parse().map_err(|e| format!("Invalid hotkey: {e}"))?;
    if key.mods.is_empty() {
        return Err(format!(
            "Hotkey {input} needs at least one modifier (Super, Ctrl, Alt or Shift)"
        ));
    }
    let mut result = String::new();
    for (modifier, name) in [
        (Modifiers::CONTROL, "Control"),
        (Modifiers::ALT, "Alt"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::SUPER, "Super"),
    ] {
        if key.mods.contains(modifier) {
            result.push_str(&format!("<{name}>"));
        }
    }
    let code = key.key.to_string();
    let name = if let Some(letter) = code.strip_prefix("Key") {
        letter.to_lowercase()
    } else if let Some(digit) = code.strip_prefix("Digit") {
        digit.into()
    } else if code
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n))
    {
        code
    } else {
        match code.as_str() {
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            "Enter" => "Return",
            "Escape" => "Escape",
            "Space" => "space",
            "Tab" => "Tab",
            "Backspace" => "BackSpace",
            "Delete" => "Delete",
            "Insert" => "Insert",
            "Home" => "Home",
            "End" => "End",
            "PageUp" => "Page_Up",
            "PageDown" => "Page_Down",
            "Minus" => "minus",
            "Equal" => "equal",
            "Comma" => "comma",
            "Period" => "period",
            "Slash" => "slash",
            "Backslash" => "backslash",
            "Semicolon" => "semicolon",
            "Quote" => "apostrophe",
            "Backquote" => "grave",
            "BracketLeft" => "bracketleft",
            "BracketRight" => "bracketright",
            _ => {
                return Err(format!(
                    "Hotkey key {code} is not supported by the GNOME bridge"
                ))
            }
        }
        .into()
    };
    result.push_str(&name);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_saved_shortcuts_to_gnome_accelerators() {
        assert_eq!(accelerator("Super+Shift+KeyD").unwrap(), "<Shift><Super>d");
        assert_eq!(
            accelerator("Control+Alt+Shift+F10").unwrap(),
            "<Control><Alt><Shift>F10"
        );
        assert_eq!(accelerator("Super+Digit1").unwrap(), "<Super>1");
        assert_eq!(accelerator("Alt+ArrowLeft").unwrap(), "<Alt>Left");
        assert!(accelerator("NotAKey").is_err());
        assert!(accelerator("KeyD").is_err());
        assert!(accelerator("F5").is_err());
    }

    #[test]
    fn ignores_events_from_previous_view_indices() {
        let (commands, _rx) = mpsc::channel();
        let (tx, triggers) = mpsc::channel();
        let mut hotkeys = Hotkeys {
            commands,
            triggers,
            generation: 1,
            worker: None,
        };
        hotkeys.configure(&[]);
        tx.send((1, 0)).unwrap();
        tx.send((2, 3)).unwrap();
        assert_eq!(hotkeys.take_trigger(), Some(3));
        assert_eq!(hotkeys.take_trigger(), None);
    }
}
