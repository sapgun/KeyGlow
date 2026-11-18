use crate::keyboard::KeyCode;
use crate::profiles::app_profiles::{dedupe_rules, normalize_executable_path, AppRule};
use crate::profiles::layout_import::{validate_layout, ImportedLayout};
use crate::profiles::layout_registry::is_valid_layout_id;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const CONFIG_VERSION: u32 = 1;
pub const DEFAULT_LAYOUT: &str = "tkl-ansi";
pub const DEFAULT_PROFILE_ID: &str = "default";

/// S8: one key remap rule (ADR-REMAPPING.md §7). Stored as key ids
/// (`KeyCode::as_str`), same id space as `disabled_keys`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemapRule {
    pub from: String,
    pub to: String,
}

/// Toggle keys (S8 V4): excluded as remap *targets* — injecting a
/// toggle down flips LED state the engine does not track. Allowed as
/// sources.
pub const TOGGLE_KEYS: [&str; 3] = ["CapsLock", "NumLock", "ScrollLock"];

pub fn is_toggle_key(id: &str) -> bool {
    TOGGLE_KEYS.contains(&id)
}

/// S8 validation (ADR §7, V1–V4) for one new rule against a profile's
/// current rules and disabled keys. Save-time rejection: never silently
/// fixed. Load-time cleaning in `normalize` drops invalid rules instead.
pub fn validate_remap_rule(
    from: &str,
    to: &str,
    profile: &Profile,
) -> Result<RemapRule, String> {
    let from_code = KeyCode::from_id(from).ok_or_else(|| format!("unknown key: {from}"))?;
    let to_code = KeyCode::from_id(to).ok_or_else(|| format!("unknown key: {to}"))?;
    let _ = (from_code, to_code);
    // V1: well-formed.
    if from == to {
        return Err(format!("cannot remap {from} to itself"));
    }
    // V4: Pause has no distinct key-up, so it cannot pair a release.
    if from == "Pause" || to == "Pause" {
        return Err("Pause cannot be remapped (it has no key-up to pair)".into());
    }
    // V4: toggle keys are excluded as targets.
    if is_toggle_key(to) {
        return Err(format!("{to} cannot be a remap target (toggle key)"));
    }
    // V2: one rule per source per profile.
    if profile.remappings.iter().any(|r| r.from == from) {
        return Err(format!("{from} already has a remap rule; delete it first"));
    }
    // V3: neither participant may be disabled in this profile.
    if profile.disabled_keys.iter().any(|k| k == from) {
        return Err(format!("{from} is disabled in this profile; enable it first"));
    }
    if profile.disabled_keys.iter().any(|k| k == to) {
        return Err(format!("{to} is disabled in this profile; enable it first"));
    }
    Ok(RemapRule {
        from: from.to_string(),
        to: to.to_string(),
    })
}

/// S8: does `code` participate in any rule of this profile (source or
/// target)? Used by the disable command (V3, second direction).
pub fn remap_participant(profile: &Profile, code: &str) -> Option<RemapRule> {
    profile
        .remappings
        .iter()
        .find(|r| r.from == code || r.to == code)
        .cloned()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub layout: String,
    #[serde(default)]
    pub disabled_keys: Vec<String>,
    /// S8: 1:1 key remap rules for this profile. Additive, config
    /// version stays 1; old files load with an empty list.
    #[serde(default)]
    pub remappings: Vec<RemapRule>,
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
    /// Imported custom layouts (S6). Persisted; validated on import
    /// and revalidated in `normalize` (invalid entries are dropped).
    #[serde(default)]
    pub custom_layouts: Vec<ImportedLayout>,
    /// App rules (S5): foreground executable -> profile. Persisted.
    #[serde(default)]
    pub app_rules: Vec<AppRule>,
    /// Master switch for automatic profile switching (S5). Persisted.
    #[serde(default = "default_true")]
    pub auto_profile_enabled: bool,
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
fn default_true() -> bool {
    true
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
            custom_layouts: vec![],
            app_rules: vec![],
            auto_profile_enabled: true,
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
            remappings: vec![],
            builtin: true,
        },
        Profile {
            id: "gaming".into(),
            name: "Gaming".into(),
            layout: DEFAULT_LAYOUT.into(),
            disabled_keys: vec!["MetaLeft".into(), "MetaRight".into()],
            remappings: vec![],
            builtin: true,
        },
        Profile {
            id: "coding".into(),
            name: "Coding".into(),
            layout: DEFAULT_LAYOUT.into(),
            disabled_keys: vec![],
            remappings: vec![],
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
        remappings: vec![],
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
        // S6: custom layouts are cleaned before the profile loop below,
        // so a profile can only reference a layout that survived
        // validation. Keep the first occurrence per id; drop builtin
        // collisions and anything that fails revalidation.
        let mut seen_layouts = HashSet::new();
        self.custom_layouts.retain(|l| {
            if !seen_layouts.insert(l.id.clone()) {
                tracing::warn!("duplicate custom layout {:?}; removed", l.id);
                return false;
            }
            if let Err(e) = validate_layout(l) {
                tracing::warn!("custom layout {:?} failed validation ({e}); removed", l.id);
                return false;
            }
            true
        });
        for profile in &mut self.profiles {
            // Per-profile layout must be a known layout id (builtin or
            // an imported custom layout).
            if !is_valid_layout_id(&profile.layout, &self.custom_layouts) {
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
            // S8: a stored rule that no longer validates is dropped at
            // load and the rest of the profile is kept (ADR §7). Same
            // checks as save-time validation, minus the duplicate-source
            // test (handled by first-occurrence retention below).
            let before_rules = profile.remappings.len();
            let disabled = profile.disabled_keys.clone();
            let profile_id = profile.id.clone();
            let mut seen_sources = HashSet::new();
            let cleaned: Vec<RemapRule> = profile
                .remappings
                .drain(..)
                .filter(|r| {
                    let ok = KeyCode::from_id(&r.from).is_some()
                        && KeyCode::from_id(&r.to).is_some()
                        && r.from != r.to
                        && r.from != "Pause"
                        && r.to != "Pause"
                        && !is_toggle_key(&r.to)
                        && !disabled.iter().any(|k| k == &r.from)
                        && !disabled.iter().any(|k| k == &r.to)
                        && seen_sources.insert(r.from.clone());
                    if !ok {
                        tracing::warn!(
                            "profile {profile_id} had an invalid remap rule {} -> {}; removed",
                            r.from,
                            r.to
                        );
                    }
                    ok
                })
                .collect();
            profile.remappings = cleaned;
            if profile.remappings.len() != before_rules {
                tracing::warn!("profile {profile_id} remap rules cleaned during normalize");
            }
        }
        // Selected profile must exist.
        if !self.profiles.iter().any(|p| p.id == self.selected_profile) {
            tracing::warn!(
                "selected profile {:?} missing; falling back to Default",
                self.selected_profile
            );
            self.selected_profile = DEFAULT_PROFILE_ID.to_string();
        }
        // S5: app rules must point at existing profiles and have a
        // non-empty executable path; drop the invalid ones (same
        // philosophy as unknown key ids above) and drop exact duplicates.
        let before_rules = self.app_rules.len();
        self.app_rules.retain(|r| {
            let ok = !normalize_executable_path(&r.executable_path).is_empty()
                && self.profiles.iter().any(|p| p.id == r.profile_id);
            if !ok {
                tracing::warn!(
                    "app rule {} has an unknown profile or empty path; removed",
                    r.id
                );
            }
            ok
        });
        self.app_rules = dedupe_rules(&self.app_rules);
        if self.app_rules.len() != before_rules {
            tracing::warn!("app rules cleaned during normalize");
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
            remappings: vec![],
            builtin: false,
        });
        cfg.activate_profile("wide").unwrap();
        assert_eq!(cfg.selected_profile, "wide");
        assert_eq!(cfg.selected_layout, "fullsize-ansi");
        assert!(cfg.activate_profile("nope").is_err());
    }

    #[test]
    fn custom_layout_roundtrip_and_profile_reference() {
        // S6: an imported custom layout survives save/load, and a
        // profile may reference it.
        let raw = include_str!("../../../src/layouts/fullsize-iso.json");
        let mut value: serde_json::Value = serde_json::from_str(raw).unwrap();
        value["id"] = serde_json::Value::String("my-iso".to_string());
        let layout: crate::profiles::layout_import::ImportedLayout =
            serde_json::from_value(value).unwrap();
        let mut cfg = AppConfig::default();
        cfg.custom_layouts.push(layout);
        cfg.profiles.push(Profile {
            id: "iso-user".into(),
            name: "ISO user".into(),
            layout: "my-iso".into(),
            disabled_keys: vec![],
            remappings: vec![],
            builtin: false,
        });
        let cfg = cfg.normalize();
        assert_eq!(cfg.custom_layouts.len(), 1);
        assert_eq!(cfg.profile("iso-user").unwrap().layout, "my-iso");
        // Persisted form keeps the custom layout.
        let serialized = serde_json::to_string(&cfg).unwrap();
        let reloaded: AppConfig = serde_json::from_str(&serialized).unwrap();
        assert_eq!(reloaded.custom_layouts.len(), 1);
        assert_eq!(reloaded.custom_layouts[0].keys.len(), 105);
    }

    #[test]
    fn normalize_drops_invalid_custom_layout_and_resets_profile() {
        // S6: a stored custom layout that no longer validates is
        // dropped, and profiles pointing at it fall back to Default.
        let raw = include_str!("../../../src/layouts/fullsize-jis.json");
        let mut value: serde_json::Value = serde_json::from_str(raw).unwrap();
        value["id"] = serde_json::Value::String("my-jis".to_string());
        // Break it after parsing: an out-of-range key.
        value["keys"][0]["x"] = serde_json::Value::from(99.0);
        let layout: crate::profiles::layout_import::ImportedLayout =
            serde_json::from_value(value).unwrap();
        let mut cfg = AppConfig::default();
        cfg.custom_layouts.push(layout);
        cfg.profiles.push(Profile {
            id: "jis-user".into(),
            name: "JIS user".into(),
            layout: "my-jis".into(),
            disabled_keys: vec![],
            remappings: vec![],
            builtin: false,
        });
        let cfg = cfg.normalize();
        assert!(cfg.custom_layouts.is_empty());
        assert_eq!(cfg.profile("jis-user").unwrap().layout, DEFAULT_LAYOUT);
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

    // ---- S8: remap rule validation (ADR §7, V1–V5) ----

    fn rule_profile() -> Profile {
        Profile {
            id: "test".into(),
            name: "Test".into(),
            layout: DEFAULT_LAYOUT.into(),
            disabled_keys: vec![],
            remappings: vec![],
            builtin: false,
        }
    }

    #[test]
    fn remap_validation_accepts_plain_rule_and_rejects_v1_v2_v4() {
        let mut profile = rule_profile();
        // Plain rule: accepted.
        let rule = validate_remap_rule("KeyA", "KeyB", &profile).unwrap();
        assert_eq!(rule.from, "KeyA");
        assert_eq!(rule.to, "KeyB");
        profile.remappings.push(rule);
        // V1: self-map.
        assert!(validate_remap_rule("KeyA", "KeyA", &profile).is_err());
        // V1: unknown ids.
        assert!(validate_remap_rule("KeyA", "NotAKey", &profile).is_err());
        assert!(validate_remap_rule("NotAKey", "KeyB", &profile).is_err());
        // V2: duplicate source.
        assert!(validate_remap_rule("KeyA", "KeyC", &profile).is_err());
        // V4: Pause either way.
        assert!(validate_remap_rule("Pause", "KeyB", &profile).is_err());
        assert!(validate_remap_rule("KeyA", "Pause", &profile).is_err());
        // V4: toggle keys as targets, but allowed as sources.
        for toggle in ["CapsLock", "NumLock", "ScrollLock"] {
            assert!(
                validate_remap_rule("KeyA", toggle, &profile).is_err(),
                "{toggle} must be rejected as a target"
            );
            assert!(
                validate_remap_rule(toggle, "KeyB", &profile).is_ok(),
                "{toggle} must be allowed as a source"
            );
        }
        // Shared targets are allowed (refcount, §3).
        assert!(validate_remap_rule("KeyC", "KeyB", &profile).is_ok());
    }

    #[test]
    fn remap_validation_rejects_disabled_participants_v3() {
        let mut profile = rule_profile();
        profile.disabled_keys = vec!["KeyA".into()];
        assert!(validate_remap_rule("KeyA", "KeyB", &profile).is_err());
        assert!(validate_remap_rule("KeyC", "KeyA", &profile).is_err());
        assert!(validate_remap_rule("KeyC", "KeyB", &profile).is_ok());
        // The reverse direction: a rule participant is found by lookup.
        profile.disabled_keys.clear();
        profile.remappings.push(RemapRule {
            from: "KeyA".into(),
            to: "KeyB".into(),
        });
        assert_eq!(
            remap_participant(&profile, "KeyA").unwrap().to,
            "KeyB"
        );
        assert_eq!(
            remap_participant(&profile, "KeyB").unwrap().from,
            "KeyA"
        );
        assert!(remap_participant(&profile, "KeyC").is_none());
    }

    #[test]
    fn normalize_drops_invalid_remap_rules_and_keeps_rest() {
        // V5: old files load with an empty list; a stored file with an
        // invalid rule drops that rule at load and keeps the profile.
        let raw = r#"{
            "version": 1,
            "selectedProfile": "default",
            "profiles": [{
                "id": "default", "name": "Default", "layout": "tkl-ansi",
                "disabledKeys": ["KeyZ"],
                "remappings": [
                    {"from": "KeyA", "to": "KeyB"},
                    {"from": "KeyA", "to": "KeyC"},
                    {"from": "KeyC", "to": "KeyC"},
                    {"from": "Pause", "to": "KeyB"},
                    {"from": "KeyD", "to": "CapsLock"},
                    {"from": "KeyE", "to": "NotAKey"},
                    {"from": "KeyZ", "to": "KeyB"},
                    {"from": "KeyF", "to": "KeyZ"},
                    {"from": "CapsLock", "to": "ControlLeft"}
                ]
            }]
        }"#;
        let cfg: AppConfig = serde_json::from_str(raw).unwrap();
        let cfg = cfg.migrate();
        let rules = &cfg.profile("default").unwrap().remappings;
        // Kept: A->B (first source wins), CapsLock->ControlLeft (toggle
        // source allowed). Dropped: duplicate source, self-map, Pause,
        // toggle target, unknown id, disabled participants (KeyZ both ways).
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].from, "KeyA");
        assert_eq!(rules[0].to, "KeyB");
        assert_eq!(rules[1].from, "CapsLock");
        assert_eq!(rules[1].to, "ControlLeft");
    }

    #[test]
    fn remap_rules_roundtrip_through_json() {
        let mut cfg = AppConfig::default();
        cfg.profile_mut("default").unwrap().remappings.push(RemapRule {
            from: "KeyA".into(),
            to: "KeyB".into(),
        });
        let serialized = serde_json::to_string(&cfg).unwrap();
        let reloaded: AppConfig = serde_json::from_str(&serialized).unwrap();
        assert_eq!(reloaded.profile("default").unwrap().remappings.len(), 1);
        // Old files without the field load with an empty list (V5).
        let old: AppConfig = serde_json::from_str(
            r#"{"version": 1, "selectedProfile": "default", "profiles": [{"id": "default", "name": "Default", "layout": "tkl-ansi"}]}"#,
        )
        .unwrap();
        assert!(old.migrate().profile("default").unwrap().remappings.is_empty());
    }
}
