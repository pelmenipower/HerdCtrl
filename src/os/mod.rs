pub mod gnome;
pub mod linux;

use crate::core::monitor::Monitor;
use linux::Window;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    X11,
    Gnome,
    UnsupportedWayland,
}

impl Backend {
    pub fn detect() -> Self {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| {
            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                "wayland".into()
            } else {
                String::new()
            }
        });
        Self::for_session(
            &session,
            &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        )
    }

    fn for_session(session: &str, desktop: &str) -> Self {
        if session.eq_ignore_ascii_case("wayland") {
            if desktop
                .split(':')
                .any(|part| part.eq_ignore_ascii_case("gnome"))
            {
                Self::Gnome
            } else {
                Self::UnsupportedWayland
            }
        } else {
            Self::X11
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::X11 => "Linux / X11",
            Self::Gnome => "GNOME / Wayland",
            Self::UnsupportedWayland => "Wayland / unsupported compositor",
        }
    }

    pub fn monitors(self) -> Result<Vec<Monitor>, String> {
        match self {
            Self::X11 => linux::monitors(),
            Self::Gnome => Ok(gnome::state()?.monitors),
            Self::UnsupportedWayland => Err("This Wayland desktop is not supported. HerdCtrl supports GNOME 50 with its Shell extension, or an X11 session.".into()),
        }
    }

    pub fn windows(self) -> Result<Vec<Window>, String> {
        match self {
            Self::X11 => linux::windows(),
            Self::Gnome => Ok(gnome::state()?.windows),
            Self::UnsupportedWayland => Err("Unsupported Wayland compositor".into()),
        }
    }

    pub fn position(
        self,
        window: &Window,
        monitor: &Monitor,
        bounds: (i32, i32, u32, u32),
    ) -> Result<(), String> {
        match self {
            Self::X11 => {
                if !linux::windows()?
                    .iter()
                    .any(|current| current.id == window.id && current.pid == window.pid)
                {
                    return Err("Window disappeared or its process changed".into());
                }
                linux::position(&window.id, bounds)
            }
            Self::Gnome => gnome::position(window, monitor, bounds),
            Self::UnsupportedWayland => Err("Unsupported Wayland compositor".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_compositor_without_wayland_to_x11_fallback() {
        assert_eq!(
            Backend::for_session("wayland", "ubuntu:GNOME"),
            Backend::Gnome
        );
        assert_eq!(
            Backend::for_session("wayland", "KDE"),
            Backend::UnsupportedWayland
        );
        assert_eq!(Backend::for_session("x11", "GNOME"), Backend::X11);
    }
}
