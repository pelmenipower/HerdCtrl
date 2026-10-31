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
    pub command: String,
    #[serde(default)]
    pub delay_ms: u64,
    pub bounds: Bounds,
}

impl Config {
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
    fn malformed_config_is_not_replaced_with_defaults() {
        assert!(serde_json::from_str::<Config>("{\"viwes\": []}").is_err());
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
