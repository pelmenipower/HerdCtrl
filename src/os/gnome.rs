//! Session D-Bus calls are only made by launch/discovery/shortcut workers.
use super::linux::{self, Window};
use crate::core::monitor::Monitor;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

pub mod hotkeys;

#[derive(Deserialize)]
pub struct State {
    pub monitors: Vec<Monitor>,
    pub windows: Vec<Window>,
}

pub fn call<T: DeserializeOwned>(mut request: Value) -> Result<T, String> {
    request["version"] = json!(1);
    let raw = linux::output("gdbus", &[
        "call", "--session", "--timeout", "1", "--dest", "org.gnome.Shell",
        "--object-path", "/org/gnome/Shell/Extensions/HerdCtrl",
        "--method", "org.gnome.Shell.Extensions.HerdCtrl.Call", &request.to_string(),
    ]).map_err(|e| format!("GNOME bridge unavailable. Install it with `bash scripts/install-gnome-extension.sh`, enable HerdCtrl in Extensions, then Refresh displays. {e}"))?;
    decode_reply(&raw)
}

fn decode_reply<T: DeserializeOwned>(raw: &str) -> Result<T, String> {
    let text = tuple_string(raw)?;
    let response: Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid GNOME bridge response: {e}"))?;
    if response.get("ok") != Some(&Value::Bool(true)) {
        return Err(response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("GNOME bridge rejected the request")
            .into());
    }
    serde_json::from_value(
        response
            .get("result")
            .cloned()
            .ok_or("GNOME bridge response has no result")?,
    )
    .map_err(|e| format!("Invalid GNOME bridge result: {e}"))
}

// gdbus prints a GVariant single-string tuple, with either quote style and
// C escapes. Decode that outer layer before parsing the inner JSON.
fn tuple_string(raw: &str) -> Result<String, String> {
    let invalid = || "Invalid gdbus string tuple".to_string();
    let content = raw
        .trim()
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(invalid)?
        .trim();
    let mut chars = content.chars();
    let quote = chars
        .next()
        .filter(|c| *c == '\'' || *c == '"')
        .ok_or_else(invalid)?;
    let mut result = String::new();
    while let Some(ch) = chars.next() {
        if ch == quote {
            if chars.as_str().trim() == "," {
                return Ok(result);
            }
            return Err(invalid());
        }
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars.next().ok_or_else(invalid)? {
            '\\' => result.push('\\'),
            '\'' => result.push('\''),
            '"' => result.push('"'),
            'n' => result.push('\n'),
            'r' => result.push('\r'),
            't' => result.push('\t'),
            'b' => result.push('\u{8}'),
            'f' => result.push('\u{c}'),
            'v' => result.push('\u{b}'),
            kind @ ('u' | 'U' | 'x') => {
                let count = match kind {
                    'u' => 4,
                    'U' => 8,
                    _ => 2,
                };
                let hex: String = chars.by_ref().take(count).collect();
                if hex.len() != count {
                    return Err(invalid());
                }
                let code = u32::from_str_radix(&hex, 16).map_err(|_| invalid())?;
                result.push(char::from_u32(code).ok_or_else(invalid)?);
            }
            _ => return Err(invalid()),
        }
    }
    Err(invalid())
}

pub fn state() -> Result<State, String> {
    let state: State = call(json!({"operation":"state"}))?;
    if state.monitors.is_empty()
        || state.monitors.iter().any(|m| {
            m.name.is_empty()
                || m.width == 0
                || m.height == 0
                || !m.scale.is_finite()
                || m.scale <= 0.0
                || i64::from(m.x) + i64::from(m.width) > i64::from(i32::MAX)
                || i64::from(m.y) + i64::from(m.height) > i64::from(i32::MAX)
        })
    {
        return Err("GNOME bridge returned invalid monitor geometry".into());
    }
    Ok(state)
}

pub fn position(
    window: &Window,
    monitor: &Monitor,
    (x, y, width, height): (i32, i32, u32, u32),
) -> Result<(), String> {
    call(
        json!({"operation":"position", "id":window.id, "pid":window.pid,
        "monitor":monitor.name, "x":x, "y":y, "width":width, "height":height}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_gvariant_quotes_escapes_and_unicode() {
        assert_eq!(
            tuple_string("('a\\'b\\\\c\\n😀',)\n").unwrap(),
            "a'b\\c\n😀"
        );
        assert_eq!(tuple_string(r#"("a\"b",)"#).unwrap(), "a\"b");
        assert_eq!(tuple_string(r"('\u00e4\U0001f600',)").unwrap(), "ä😀");
        for malformed in ["()", "('unfinished,)", "('x', 'y')", "('\\q',)"] {
            assert!(tuple_string(malformed).is_err(), "{malformed}");
        }
    }

    #[test]
    fn rejects_bridge_errors_and_wrong_schemas() {
        assert_eq!(
            decode_reply::<Value>(r#"('{"ok":false,"error":"Disconnected"}',)"#).unwrap_err(),
            "Disconnected"
        );
        assert!(decode_reply::<State>(r#"('{"ok":true,"result":[]}',)"#).is_err());
        let state: State = decode_reply(r#"('{"ok":true,"result":{"monitors":[{"name":"DP-1","x":-1280,"y":0,"width":1280,"height":720,"scale":1.5}],"windows":[{"id":"42","pid":123}]}}',)"#).unwrap();
        assert_eq!(state.monitors[0].x, -1280);
        assert_eq!(state.windows[0].pid, 123);
    }

    #[test]
    #[ignore = "requires the CI mock bridge on an isolated session D-Bus"]
    fn gnome_mock_dbus_roundtrip() {
        assert_eq!(
            std::env::var("HERDCTRL_MOCK_GNOME_BRIDGE").as_deref(),
            Ok("1"),
            "Run only with scripts/mock-gnome-bridge.mjs on an isolated bus"
        );
        let state = state().unwrap();
        assert_eq!(state.monitors[0].name, "DP-1");
        assert_eq!(state.monitors[0].scale, 1.5);
        assert_eq!(state.monitors[0].width, 1280);
        let monitor = &state.monitors[0];
        let window = &state.windows[0];
        position(window, monitor, (-1280, 0, 640, 720)).unwrap();
        assert!(position(
            &Window {
                id: window.id.clone(),
                pid: 999
            },
            monitor,
            (-1280, 0, 640, 720)
        )
        .is_err());
        let errors: Vec<String> = call(json!({"operation":"bindings", "token":"mock-test", "bindings":[{"id":0,"accelerator":"<Super>d"}]})).unwrap();
        assert!(errors.is_empty());
        let pending: Vec<usize> = call(json!({"operation":"poll","token":"mock-test"})).unwrap();
        assert!(pending.is_empty());
        call::<Value>(json!({"operation":"release","token":"mock-test"})).unwrap();
    }
}
