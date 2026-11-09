//! Session D-Bus calls are only made by launch/discovery/shortcut/tray workers.
use super::linux::Window;
use crate::core::monitor::Monitor;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{sync::Mutex, time::Duration};
use zbus::blocking::{connection, Connection};

pub mod hotkeys;

#[derive(Deserialize)]
pub struct State {
    pub monitors: Vec<Monitor>,
    pub windows: Vec<Window>,
}

pub fn call<T: DeserializeOwned>(mut request: Value) -> Result<T, String> {
    request["version"] = json!(1);
    let raw = bridge(&request.to_string()).map_err(|e| format!("GNOME bridge unavailable. Install it with `bash scripts/install-gnome-extension.sh`, enable HerdCtrl in Extensions, then Refresh displays. {e}"))?;
    decode_reply(&raw)
}

// One shared session-bus connection: hotkey polling must not spawn a helper
// process several times per second.
fn bridge(request: &str) -> Result<String, String> {
    static CONNECTION: Mutex<Option<Connection>> = Mutex::new(None);
    let mut slot = CONNECTION.lock().unwrap_or_else(|e| e.into_inner());
    let connection = match slot.as_ref() {
        Some(connection) => connection.clone(),
        None => {
            let connection = connection::Builder::session()
                .map_err(|e| e.to_string())?
                .method_timeout(Duration::from_secs(1))
                .build()
                .map_err(|e| e.to_string())?;
            slot.insert(connection).clone()
        }
    };
    drop(slot);
    let reply = connection
        .call_method(
            Some("org.gnome.Shell"),
            "/org/gnome/Shell/Extensions/HerdCtrl",
            Some("org.gnome.Shell.Extensions.HerdCtrl"),
            "Call",
            &(request,),
        )
        .and_then(|reply| reply.body().deserialize::<String>());
    if let Err(zbus::Error::InputOutput(_)) = &reply {
        // Reconnect on the next call, e.g. after the session bus restarted.
        *CONNECTION.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    reply.map_err(|e| e.to_string())
}

fn decode_reply<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    let response: Value =
        serde_json::from_str(text).map_err(|e| format!("Invalid GNOME bridge response: {e}"))?;
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
    fn rejects_bridge_errors_and_wrong_schemas() {
        assert_eq!(
            decode_reply::<Value>(r#"{"ok":false,"error":"Disconnected"}"#).unwrap_err(),
            "Disconnected"
        );
        assert!(decode_reply::<State>(r#"{"ok":true,"result":[]}"#).is_err());
        let state: State = decode_reply(r#"{"ok":true,"result":{"monitors":[{"name":"DP-1","x":-1280,"y":0,"width":1280,"height":720,"scale":1.5}],"windows":[{"id":"42","pid":123}]}}"#).unwrap();
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
