use super::monitor::Bounds;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub views: Vec<View>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub name: String,
    #[serde(default)]
    pub hotkey: String,
    pub zones: Vec<Zone>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub name: String,
    pub monitor: String,
    #[serde(default)]
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application: Option<Application>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub terminal_command: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub run_in_terminal: bool,
    #[serde(default)]
    pub delay_ms: u64,
    pub bounds: Bounds,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Application {
    pub id: String,
    pub name: String,
}

impl Config {
    /// Append so existing hotkey-to-view indices keep their meaning.
    pub fn duplicate_view(&mut self, index: usize) -> Option<usize> {
        let mut copy = self.views.get(index)?.clone();
        let base = format!("{} (copy)", copy.name);
        copy.name.clone_from(&base);
        let mut number = 2;
        while self.views.iter().any(|view| view.name == copy.name) {
            copy.name = format!("{base} {number}");
            number += 1;
        }
        // A global shortcut must never activate both source and duplicate.
        copy.hotkey.clear();
        self.views.push(copy);
        Some(self.views.len() - 1)
    }

    pub fn path() -> Result<PathBuf, String> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .ok_or("HOME and XDG_CONFIG_HOME are unavailable")?;
        Ok(base.join("herdctrl/layouts.json"))
    }

    pub fn validate(&self) -> Result<(), String> {
        for view in &self.views {
            if view.name.trim().is_empty() {
                return Err("A view needs a name".into());
            }
            for zone in &view.zones {
                if (!zone.terminal_command.is_empty() && zone.application.is_none())
                    || (zone.run_in_terminal && zone.application.is_some())
                {
                    return Err(format!(
                        "Invalid terminal mode in {} / {}",
                        view.name, zone.name
                    ));
                }
                if zone
                    .application
                    .as_ref()
                    .is_some_and(|app| app.id.trim().is_empty() || app.name.trim().is_empty())
                {
                    return Err(format!(
                        "Invalid application in {} / {}",
                        view.name, zone.name
                    ));
                }
                if zone.name.trim().is_empty()
                    || zone.monitor.trim().is_empty()
                    || !zone.bounds.valid()
                {
                    return Err(format!(
                        "Invalid name, monitor or bounds in {} / {}",
                        view.name, zone.name
                    ));
                }
                if zone.delay_ms > 60_000 {
                    return Err("Launch delay must be at most 60000 ms".into());
                }
            }
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.to_string()),
        };
        let config: Self = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let parent = path.parent().ok_or("Invalid configuration path")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .map_err(|e| format!("{}: {e}", temp.display()))?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temp, path)?;
            fs::File::open(parent)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result.map_err(|e: std::io::Error| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_roundtrip_and_invalid_bounds() {
        let mut config = Config {
            views: vec![View {
                name: "Dev".into(),
                hotkey: "Super+Shift+KeyD".into(),
                zones: vec![Zone {
                    name: "Terminal".into(),
                    monitor: "DP-1".into(),
                    command: "xterm".into(),
                    application: None,
                    terminal_command: String::new(),
                    run_in_terminal: false,
                    delay_ms: 100,
                    bounds: Bounds::default(),
                }],
            }],
        };
        let decoded: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(decoded.views[0].zones[0].command, "xterm");
        assert!(decoded.validate().is_ok());
        config.views[0].zones[0].bounds.width = 1.2;
        assert!(config.validate().is_err());
    }
    #[test]
    fn duplicate_preserves_zones_but_has_independent_edits_and_no_hotkey() {
        let mut config: Config = serde_json::from_value(serde_json::json!({
            "views": [{"name": "Dev", "hotkey": "Super+KeyD", "zones": [
                {"name": "Terminal", "monitor": "DP-2", "application": {"id": "ptyxis", "name": "Terminal"},
                 "terminal_command": "herdr", "delay_ms": 250,
                 "bounds": {"x": 0.5, "y": 0.0, "width": 0.5, "height": 1.0}},
                {"name": "Script", "monitor": "DP-1", "command": "htop", "run_in_terminal": true,
                 "bounds": {"x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0}}
            ]}]
        })).unwrap();
        let original = serde_json::to_value(&config.views[0]).unwrap();
        assert_eq!(config.duplicate_view(0), Some(1));
        assert_eq!(config.views[1].name, "Dev (copy)");
        assert!(config.views[1].hotkey.is_empty());
        assert_eq!(
            serde_json::to_value(&config.views[1].zones).unwrap(),
            original["zones"]
        );
        config.views[1].zones[0].terminal_command = "another program".into();
        config.views[1].zones[0].bounds.x = 0.0;
        config.views[1].zones.remove(1);
        assert_eq!(serde_json::to_value(&config.views[0]).unwrap(), original);
        let loaded: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.views.len(), 2);
        assert_eq!(loaded.views[1].zones[0].terminal_command, "another program");
        assert_eq!(loaded.views[0].hotkey, "Super+KeyD");
        assert!(loaded.views[1].hotkey.is_empty());
    }

    #[test]
    fn duplicate_names_are_unique_and_empty_or_missing_views_are_handled() {
        let mut config = Config {
            views: vec![View {
                name: "Empty".into(),
                hotkey: String::new(),
                zones: Vec::new(),
            }],
        };
        assert_eq!(config.duplicate_view(0), Some(1));
        assert_eq!(config.duplicate_view(0), Some(2));
        assert_eq!(config.views[2].name, "Empty (copy) 2");
        assert!(config.views[2].zones.is_empty());
        assert_eq!(config.duplicate_view(99), None);
        assert_eq!(config.views.len(), 3);
        assert_eq!(Config::default().duplicate_view(0), None);
    }

    #[test]
    fn malformed_config_is_not_replaced_with_defaults() {
        assert!(serde_json::from_str::<Config>("{\"viwes\": []}").is_err());
    }

    #[test]
    fn old_commands_and_application_selections_roundtrip() {
        let old = r#"{"views":[{"name":"Old","zones":[{"name":"Terminal","monitor":"DP-1","command":"xterm","bounds":{"x":0,"y":0,"width":0.5,"height":1}}]}]}"#;
        let mut config: Config = serde_json::from_str(old).unwrap();
        assert!(config.views[0].zones[0].application.is_none());
        config.views[0].zones[0].application = Some(Application {
            id: "org.gnome.Ptyxis".into(),
            name: "Terminal".into(),
        });
        config.views[0].zones[0].command.clear();
        config.views[0].zones[0].terminal_command = "herdr".into();
        let saved = serde_json::to_string(&config).unwrap();
        let loaded: Config = serde_json::from_str(&saved).unwrap();
        loaded.validate().unwrap();
        assert_eq!(
            loaded.views[0].zones[0].application,
            config.views[0].zones[0].application
        );
        assert!(loaded.views[0].zones[0].command.is_empty());
        assert_eq!(loaded.views[0].zones[0].terminal_command, "herdr");
    }

    #[test]
    fn disk_save_is_private_and_does_not_overwrite_an_interrupted_save() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("herdctrl-test-{}-{unique}", std::process::id()));
        let path = directory.join("layouts.json");
        let config = Config::default();
        config.save(&path).unwrap();
        assert!(Config::load(&path).unwrap().views.is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::write(path.with_extension("json.tmp"), "interrupted data").unwrap();
        assert!(config.save(&path).is_err());
        assert_eq!(
            fs::read_to_string(path.with_extension("json.tmp")).unwrap(),
            "interrupted data"
        );
        assert!(Config::load(&path).is_ok());
        fs::write(&path, "invalid JSON").unwrap();
        assert!(Config::load(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
