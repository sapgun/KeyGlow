use super::models::AppConfig;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn load(path: &Path) -> AppConfig {
    match fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<AppConfig>(&raw) {
            Ok(cfg) => {
                tracing::info!("loaded settings from {}", path.display());
                cfg.normalize()
            }
            Err(err) => {
                tracing::error!("settings corrupted ({err}); restoring defaults");
                backup_corrupt(path);
                AppConfig::default()
            }
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            tracing::info!("no settings file yet; using defaults");
            AppConfig::default()
        }
        Err(err) => {
            tracing::error!("failed to read settings ({err}); using defaults");
            AppConfig::default()
        }
    }
}

pub fn save(path: &Path, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
    }
    let data = serde_json::to_vec_pretty(config).map_err(|e| format!("serialize settings: {e}"))?;
    atomic_write(path, &data).map_err(|e| format!("write settings: {e}"))
}

fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = tmp_path(path);
    fs::write(&tmp, data)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut tmp = path.to_path_buf();
    tmp.set_extension("json.tmp");
    tmp
}

fn backup_corrupt(path: &Path) {
    let mut bak = path.to_path_buf();
    bak.set_extension("json.bak");
    if let Err(err) = fs::rename(path, &bak) {
        tracing::warn!("could not backup corrupt settings: {err}");
    } else {
        tracing::warn!("corrupt settings moved to {}", bak.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::models::Profile;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("keyglow-{name}-{nanos}.json"))
    }

    #[test]
    fn roundtrip_and_corrupt_fallback() {
        let path = temp_file("ok");
        let mut cfg = AppConfig::default();
        cfg.selected_layout = "60-ansi".into();
        cfg.profiles.push(Profile {
            id: "custom".into(),
            name: "Custom".into(),
            layout: "60-ansi".into(),
            disabled_keys: vec!["CapsLock".into()],
            builtin: false,
        });
        save(&path, &cfg).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.selected_layout, "60-ansi");
        assert!(loaded.profiles.iter().any(|p| p.id == "custom"));

        fs::write(&path, "{not json").unwrap();
        let recovered = load(&path);
        assert_eq!(recovered.selected_profile, "default");
        assert!(!recovered.profiles.is_empty());
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn missing_file_returns_defaults() {
        let path = temp_file("missing");
        let _ = fs::remove_file(&path);
        let cfg = load(&path);
        assert_eq!(cfg.profiles.len(), 3);
        assert_eq!(cfg.selected_profile, "default");
    }
}
