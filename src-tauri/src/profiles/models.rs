use crate::keyboard::KeyCode;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const CONFIG_VERSION: u32 = 1;
pub const DEFAULT_LAYOUT: &str = "tkl-ansi";
pub const DEFAULT_PROFILE_ID: &str = "default";

/// Canonical layout ids. The layout registry (G-stage) will replace this
/// list; until then every validation site shares this one constant.
pub const VALID_LAYOUT_IDS: &[&str] = &[
    "fullsize-ansi",
    "tkl-ansi",
    "75-ansi",
    "65-ansi",
    "60-ansi",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub layout: String,
    #[serde(default)]
    pub disabled_keys: Vec<String>,
    #[serde(default)]
    pub builtin: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_layout")]
    pub selected_layout: String,
    #[serde(default = "default_profile")]
    pub selected_profile: String,
    #[serde(default)]
    pub onboarded: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default)]
    pub locale: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Runtime-only: the file was written by a newer KeyGlow. Never
    /// serialized back and never overwritten by `save` (see
    /// `AppState::persist`): non-destructive fallback, read-only mode.
    #[serde(skip)]
    pub future_version: bool,
    /// The version the file was read as, for diagnostics.
    #[serde(skip)]
    pub loaded_version: u32,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}
fn default_layout() -> String {
    DEFAULT_LAYOUT.to_string()
}
fn default_profile() -> String {
    DEFAULT_PROFILE_ID.to_string()
}
fn default_theme() -> String {
    "dark".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            selected_layout: DEFAULT_LAYOUT.to_string(),
            selected_profile: DEFAULT_PROFILE_ID.to_string(),
            onboarded: false,
            start_with_windows: false,
            locale: String::new(),
            theme: "dark".to_string(),
            profiles: builtin_profiles(),
            future_version: false,
            loaded_version: CONFIG_VERSION,
        }
    }
}

pub fn builtin_profiles() -> Vec<Profile> {
    vec![
        Profile {
            id: "default".into(),
            name: "Default".into(),
            layout: DEFAULT_LAYOUT.into(),
            disabled_keys: vec![],
            builtin: true,
        },
        Profile {
            id: "gaming".into(),
            name: "Gaming".into(),
            layout: DEFAULT_LAYOUT.into(),
            disabled_keys: vec!["MetaLeft".into(), "MetaRight".into()],
            builtin: true,
        },
        Profile {
            id: "coding".into(),
            name: "Coding".into(),
            layout: DEFAULT_LAYOUT.into(),
            disabled_keys: vec![],
            builtin: true,
        },
    ]
}

fn default_profile_entry() -> Profile {
    Profile {
        id: DEFAULT_PROFILE_ID.into(),
        name: "Default".into(),
        layout: DEFAULT_LAYOUT.into(),
        disabled_keys: vec![],
        builtin: true,
    }
}

impl AppConfig {
    /// Load-time migration. Never destroys data:
    /// - `version < CONFIG_VERSION`: sequential migrations (today v0/v1 are
    ///   both covered by `normalize`; add a match arm per version later).
    /// - `version == CONFIG_VERSION`: validate + repair (`normalize`).
    /// - `version > CONFIG_VERSION`: non-destructive fallback. The config
    ///   is kept in memory read-only; `future_version` is set and
    ///   `AppState::persist` must refuse to overwrite the file, so a newer
    ///   version's settings are never silently downgraded to v1.
    pub fn migrate(mut self) -> Self {
        self.loaded_version = self.version;
        if self.version > CONFIG_VERSION {
            tracing::warn!(
                "settings are from a newer KeyGlow (v{}); running read-only, will not overwrite",
                self.version
            );
            self.future_version = true;
            return self;
        }
        // Sequential per-version migrations would go here (match on
        // self.version). v0 and v1 both normalize to the current schema.
        self.version = CONFIG_VERSION;
        self.normalize()
    }

    /// Validate and repair a current-version config. Idempotent: running it
    /// twice changes nothing the second time.
    pub fn normalize(mut self) -> Self {
        if self.profiles.is_empty() {
            self.profiles = builtin_profiles();
        }
        // Profile id uniqueness: keep the first occurrence.
        let mut seen = HashSet::new();
        self.profiles.retain(|p| seen.insert(p.id.clone()));
        // The Default profile always exists.
        if !self.profiles.iter().any(|p| p.id == DEFAULT_PROFILE_ID) {
            tracing::warn!("Default profile missing; restoring it");
            self.profiles.insert(0, default_profile_entry());
        }
        for profile in &mut self.profiles {
            // Per-profile layout must be a known layout id.
            if !VALID_LAYOUT_IDS.contains(&profile.layout.as_str()) {
                tracing::warn!(
                    "profile {} has invalid layout {:?}; resetting to {DEFAULT_LAYOUT}",
                    profile.id,
                    profile.layout
                );
                profile.layout = DEFAULT_LAYOUT.to_string();
            }
            // Unknown key ids can never match a real key; drop them.
            let before = profile.disabled_keys.len();
            profile.disabled_keys.retain(|id| KeyCode::from_id(id).is_some());
            if profile.disabled_keys.len() != before {
                tracing::warn!("profile {} had unknown key ids; removed", profile.id);
            }
            profile.disabled_keys.sort();
            profile.disabled_keys.dedup();
        }
        // Selected profile must exist.
        if !self.profiles.iter().any(|p| p.id == self.selected_profile) {
            tracing::warn!(
                "selected profile {:?} missing; falling back to Default",
                self.selected_profile
            );
            self.selected_profile = DEFAULT_PROFILE_ID.to_string();
        }
        // Activation invariant: selected_layout always follows the selected
        // profile's layout.
        if let Some(profile) = self.current_profile() {
            self.selected_layout = profile.layout.clone();
        }
        if !matches!(self.locale.as_str(), "" | "en" | "ko" | "ja") {
            self.locale.clear();
        }
        if !matches!(self.theme.as_str(), "dark" | "light") {
            self.theme = "dark".to_string();
        }
        self
    }

    /// Common activation policy (HF-06): selecting a profile also syncs
    /// `selected_layout` to that profile's layout, so the invariant
    /// `selected_layout == current_profile.layout` holds everywhere.
    /// Used by select / duplicate / create / delete-fallback / emergency.
    pub fn activate_profile(&mut self, id: &str) -> Result<(), String> {
        let layout = self
            .profile(id)
            .map(|p| p.layout.clone())
            .ok_or_else(|| format!("unknown profile: {id}"))?;
        self.selected_profile = id.to_string();
        self.selected_layout = layout;
        Ok(())
    }

    pub fn current_profile(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == self.selected_profile)
    }

    pub fn current_profile_mut(&mut self) -> Option<&mut Profile> {
        let id = self.selected_profile.clone();
        self.profiles.iter_mut().find(|p| p.id == id)
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn profile_mut(&mut self, id: &str) -> Option<&mut Profile> {
        self.profiles.iter_mut().find(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(dir: &std::path::Path, name: &str, raw: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, raw).unwrap();
        path
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("keyglow-models-test-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn migrate_current_version_normalizes() {
        let raw = r#"{
            "version": 1,
            "selectedLayout": "bogus-layout",
            "selectedProfile": "nope",
            "profiles": [
                {"id": "a", "name": "A", "layout": "60-ansi", "disabledKeys": ["KeyA", "NotAKey", "KeyA"]},
                {"id": "a", "name": "A-duplicate", "layout": "60-ansi"},
                {"id": "b", "name": "B", "layout": "bogus", "disabledKeys": []}
            ]
        }"#;
        let cfg: AppConfig = serde_json::from_str(raw).unwrap();
        let cfg = cfg.migrate();
        assert_eq!(cfg.version, CONFIG_VERSION);
        // Duplicate id removed, Default restored.
        let ids: Vec<_> = cfg.profiles.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&DEFAULT_PROFILE_ID));
        assert_eq!(ids.iter().filter(|id| ***id == *"a").count(), 1);
        // Unknown key removed, deduped.
        let a = cfg.profile("a").unwrap();
        assert_eq!(a.disabled_keys, vec!["KeyA".to_string()]);
        // Invalid per-profile layout reset.
        assert_eq!(cfg.profile("b").unwrap().layout, DEFAULT_LAYOUT);
        // Selected profile fell back to Default; layout follows it.
        assert_eq!(cfg.selected_profile, DEFAULT_PROFILE_ID);
        assert_eq!(cfg.selected_layout, DEFAULT_LAYOUT);
        // Idempotent.
        let again = cfg.clone().normalize();
        assert_eq!(again, cfg);
    }

    #[test]
    fn migrate_future_version_is_non_destructive() {
        let raw = r#"{
            "version": 99,
            "selectedLayout": "60-ansi",
            "selectedProfile": "default",
            "futureField": "must survive in memory",
            "profiles": [{"id": "default", "name": "Default", "layout": "60-ansi"}]
        }"#;
        let cfg: AppConfig = serde_json::from_str(raw).unwrap();
        let cfg = cfg.migrate();
        assert!(cfg.future_version);
        assert_eq!(cfg.loaded_version, 99);
        // NOT silently downgraded to v1.
        assert_eq!(cfg.version, 99);
        // Profiles untouched (no Default re-insert, no layout rewrite).
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.selected_layout, "60-ansi");
    }

    #[test]
    fn migrate_missing_version_defaults_to_current() {
        let raw = r#"{"selectedProfile": "default", "profiles": []}"#;
        let cfg: AppConfig = serde_json::from_str(raw).unwrap();
        let cfg = cfg.migrate();
        assert!(!cfg.future_version);
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert!(!cfg.profiles.is_empty());
    }

    #[test]
    fn activate_profile_keeps_layout_invariant() {
        let mut cfg = AppConfig::default();
        cfg.profiles.push(Profile {
            id: "wide".into(),
            name: "Wide".into(),
            layout: "fullsize-ansi".into(),
            disabled_keys: vec![],
            builtin: false,
        });
        cfg.activate_profile("wide").unwrap();
        assert_eq!(cfg.selected_profile, "wide");
        assert_eq!(cfg.selected_layout, "fullsize-ansi");
        assert!(cfg.activate_profile("nope").is_err());
    }

    #[test]
    fn load_missing_default_profile_gets_emergency_repair() {
        // A settings file without the Default profile: emergency_unlock's
        // activation policy must still converge to a valid selected profile.
        let dir = temp_dir("nodefault");
        let raw = r#"{
            "version": 1,
            "selectedProfile": "custom",
            "selectedLayout": "60-ansi",
            "profiles": [{"id": "custom", "name": "Custom", "layout": "60-ansi", "disabledKeys": ["KeyA"]}]
        }"#;
        let path = write_config(&dir, "settings.json", raw);
        let cfg = crate::profiles::load(&path);
        assert!(cfg.profile(DEFAULT_PROFILE_ID).is_some());
        // selected custom profile is preserved; Default is available.
        assert_eq!(cfg.selected_profile, "custom");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
