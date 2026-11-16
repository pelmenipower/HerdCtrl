//! Command-line modes. Parsed before any lock, renderer setup or thread exists.

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    Editor,
    Tray,
    Identify(String),
    Version,
    Help,
    /// The message for stderr, without the usage text.
    Unknown(String),
}

pub const USAGE: &str = "Usage: herdctrl [--tray | --version | --help]\n\n  (no option)  Open the layout editor\n  --tray       Run in the background with the global hotkeys and a tray icon\n  --version    Print the version\n  --help       Print this help\n";

/// "herdctrl 0.1.0" (from CARGO_PKG_VERSION).
pub fn version_line() -> String {
    format!("herdctrl {}", env!("CARGO_PKG_VERSION"))
}

/// `args` excludes argv[0].
pub fn parse(args: impl IntoIterator<Item = String>) -> Mode {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Mode::Editor;
    };
    let (mode, extra) = match first.as_str() {
        "--tray" => (Mode::Tray, args.next()),
        "--version" | "-V" => (Mode::Version, args.next()),
        "--help" | "-h" => (Mode::Help, args.next()),
        "--identify-monitors" => match args.next() {
            Some(data) => (Mode::Identify(data), args.next()),
            None => {
                return Mode::Unknown("Missing monitor identification data".into());
            }
        },
        _ => return Mode::Unknown(format!("Unknown option: {first}")),
    };
    match extra {
        Some(arg) => Mode::Unknown(format!("Unexpected argument: {arg}")),
        None => mode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Mode {
        parse(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn parses_every_mode() {
        assert_eq!(p(&[]), Mode::Editor);
        assert_eq!(p(&["--tray"]), Mode::Tray);
        assert_eq!(p(&["--identify-monitors", "x"]), Mode::Identify("x".into()));
        assert_eq!(p(&["--version"]), Mode::Version);
        assert_eq!(p(&["-V"]), Mode::Version);
        assert_eq!(p(&["--help"]), Mode::Help);
        assert_eq!(p(&["-h"]), Mode::Help);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(
            p(&["--identify-monitors"]),
            Mode::Unknown("Missing monitor identification data".into())
        );
        assert_eq!(
            p(&["--bogus"]),
            Mode::Unknown("Unknown option: --bogus".into())
        );
        assert_eq!(
            p(&["--tray", "x"]),
            Mode::Unknown("Unexpected argument: x".into())
        );
        assert_eq!(
            p(&["--version", "--help"]),
            Mode::Unknown("Unexpected argument: --help".into())
        );
        assert_eq!(
            p(&["--identify-monitors", "a", "b"]),
            Mode::Unknown("Unexpected argument: b".into())
        );
    }

    #[test]
    fn version_line_has_package_version() {
        assert_eq!(
            version_line(),
            concat!("herdctrl ", env!("CARGO_PKG_VERSION"))
        );
    }
}
