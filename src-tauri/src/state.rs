use crate::keyboard::engine::KeyboardController;
use crate::keyboard::{KeyCode, SafetyState};
use crate::profiles::app_profiles::{
    AppProfileRuntime, AppRule, EffectiveChange, MatchReason,
};
use crate::profiles::{save, AppConfig, DEFAULT_PROFILE_ID};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Stable machine-readable reason for the last persist failure, for the UI.
pub const PERSIST_ERROR_NEWER_VERSION: &str = "newer_version";
pub const PERSIST_ERROR_IO: &str = "io_error";

pub struct AppState {
    pub controller: Arc<dyn KeyboardController>,
    pub config: Mutex<AppConfig>,
    pub config_path: PathBuf,
    pub hook_active: AtomicBool,
    pub hook_error: Mutex<Option<String>>,
    pub shutdown: Arc<dyn Fn() + Send + Sync>,
    /// Shared with the hook thread (bump) and the safety worker (converge).
    /// Also serializes mutating commands: a command that started before an
    /// emergency press observes a newer epoch and must discard its intent.
    pub safety: Arc<SafetyState>,
    /// Serializes all settings writes: the config clone + save is one
    /// critical section, so concurrent commands can never interleave their
    /// writes. Lock order is always persist_lock -> config lock, never the
    /// reverse (commands must not hold persist_lock across persist()).
    pub(crate) persist_lock: Mutex<()>,
    /// Incremented after every successful persist.
    pub persisted_revision: AtomicU64,
    /// Monotonic runtime-state revision (P3). Bumped once per runtime state
    /// mutation (mutating commands, emergency convergence), regardless of
    /// whether the change reached the disk. This is the ordering key the UI
    /// uses to discard stale snapshots/events; it is deliberately distinct
    /// from `persisted_revision`, which only tracks successful disk writes
    /// (a best-effort emergency persist may fail while the runtime state
    /// still changed, and some runtime state never persists at all).
    pub runtime_revision: AtomicU64,
    /// Last persist failure, if the on-disk config is stale.
    pub persist_error: Mutex<Option<String>>,
    /// Machine-readable kind of the last failure (for translated UI).
    pub persist_error_kind: Mutex<Option<String>>,
    /// Runtime-only app-profile state (S5): effective profile, manual
    /// override, safety pause, match reason, foreground exe. Never
    /// serialized, never written to disk. `config.selected_profile` stays
    /// the manual selection; this holds what actually filters keys now.
    pub app_profiles: Mutex<AppProfileRuntime>,
}

impl AppState {
    pub fn persist(&self) -> Result<(), String> {
        // Single writer: concurrent commands serialize here instead of
        // sharing a temp file or interleaving writes (HF-02).
        let _w = self.persist_lock.lock();
        if self.config.lock().future_version {
            // Non-destructive fallback (HF-06): a newer version's settings
            // are never silently downgraded to v1.
            let err =
                "settings were written by a newer KeyGlow version; refusing to overwrite"
                    .to_string();
            *self.persist_error.lock() = Some(err.clone());
            *self.persist_error_kind.lock() = Some(PERSIST_ERROR_NEWER_VERSION.to_string());
            return Err(err);
        }
        let cfg = self.config.lock().clone();
        match save(&self.config_path, &cfg) {
            Ok(()) => {
                self.persisted_revision.fetch_add(1, Ordering::SeqCst);
                *self.persist_error.lock() = None;
                *self.persist_error_kind.lock() = None;
                Ok(())
            }
            Err(err) => {
                *self.persist_error.lock() = Some(err.clone());
                *self.persist_error_kind.lock() = Some(PERSIST_ERROR_IO.to_string());
                Err(err)
            }
        }
    }

    /// Best-effort re-save after a failure (UI "Retry" action).
    pub fn retry_persist(&self) -> Result<(), String> {
        self.persist()
    }

    /// Record a runtime state mutation for UI ordering (P3). Call exactly
    /// once per mutating command / convergence, after the mutation landed.
    pub fn bump_runtime_revision(&self) {
        self.runtime_revision.fetch_add(1, Ordering::SeqCst);
    }

    /// Current ordering stamp for native event payloads (P3):
    /// (latest safety epoch, current runtime revision).
    pub fn event_stamp(&self) -> (u64, u64) {
        (
            self.safety.epoch(),
            self.runtime_revision.load(Ordering::SeqCst),
        )
    }

    /// (persisted, last error, error kind, revision) for the UI snapshot.
    pub fn persist_state(&self) -> (bool, Option<String>, Option<String>, u64) {
        let err = self.persist_error.lock().clone();
        let kind = self.persist_error_kind.lock().clone();
        let rev = self.persisted_revision.load(Ordering::SeqCst);
        (err.is_none(), err, kind, rev)
    }

    pub fn apply_current_profile(&self) {
        let cfg = self.config.lock();
        let keys = cfg
            .current_profile()
            .map(|p| parse_key_ids(&p.disabled_keys))
            .unwrap_or_default();
        let rules = cfg
            .current_profile()
            .map(|p| parse_remap_rules(&p.remappings))
            .unwrap_or_default();
        drop(cfg);
        self.controller.set_disabled_keys(&keys);
        debug_assert!(keys.iter().all(|key| !self.controller.is_enabled(*key)));
        self.apply_remap_rules(&rules);
    }

    pub fn snapshot_disabled(&self) -> Vec<String> {
        self.controller
            .disabled_keys()
            .into_iter()
            .map(|k| k.as_str().to_string())
            .collect()
    }

    pub fn hook_error(&self) -> Option<String> {
        self.hook_error.lock().clone()
    }

    pub fn is_hook_active(&self) -> bool {
        self.hook_active.load(Ordering::SeqCst)
    }

    pub fn emergency_unlock(&self) {
        // Physical recovery already happened in the hook thread; re-assert
        // here so config/UI convergence can never re-block input, even when
        // the settings file cannot be written.
        // S8: enable_all flushes held remap targets; execute the ups so
        // no injected key sticks after the unlock.
        let flushed = self.controller.enable_all();
        if !flushed.is_empty() {
            if let Err(err) = crate::keyboard::inject::execute_injections(&flushed) {
                tracing::error!("emergency remap flush failed ({err}); a remapped key may stick");
            }
        }
        {
            let mut cfg = self.config.lock();
            // Common activation policy (HF-06), best-effort: the unlock
            // itself must never fail. Default selection also syncs
            // selected_layout to Default's layout.
            if cfg.profile(DEFAULT_PROFILE_ID).is_some() {
                if let Err(err) = cfg.activate_profile(DEFAULT_PROFILE_ID) {
                    tracing::error!("emergency unlock: activation failed ({err})");
                }
            }
            if let Some(profile) = cfg.current_profile_mut() {
                profile.disabled_keys.clear();
            }
        }
        // S8: after the emergency the effective profile is Default, so
        // Default's rules apply (the pre-emergency profile's rules return
        // only on explicit resume, exactly like its disabled keys).
        self.apply_profile_rules(DEFAULT_PROFILE_ID);
        // S5: the emergency pauses automation and drops any manual
        // override. The effective profile becomes Default with the
        // safety-paused reason; no auto-switch can undo the unlock until
        // the user explicitly resumes.
        self.app_profiles.lock().on_emergency();
        // The runtime state changed (controller + config); order it for the
        // UI even though the persist below is best-effort (P3).
        self.bump_runtime_revision();
        if let Err(err) = self.persist() {
            // Best effort by design: physical keys stay enabled regardless of
            // disk state. The error is recorded (not swallowed) so the UI can
            // show it with a retry action.
            tracing::error!(
                "emergency unlock: settings persist failed ({err}); input remains enabled"
            );
        }
    }

    /// Safety epoch observed when a mutating command started. Compare with
    /// the live epoch after mutating: a newer epoch means an emergency press
    /// landed mid-command and the command must be discarded.
    pub fn safety_epoch(&self) -> u64 {
        self.safety.epoch()
    }

    // ---- S5: app profiles (runtime-only effective profile) ----

    /// The profile actually filtering keys right now (may differ from
    /// `config.selected_profile` when an app rule matched).
    pub fn effective_profile_id(&self) -> String {
        self.app_profiles.lock().effective_profile_id().to_string()
    }

    /// (effective, manual_override, reason, safety_paused, current_exe,
    /// runtime revision) for the snapshot / status payload.
    pub fn app_profile_status(
        &self,
    ) -> (String, Option<String>, MatchReason, bool, Option<String>, u64) {
        let rt = self.app_profiles.lock();
        (
            rt.effective_profile_id().to_string(),
            rt.manual_override().map(|s| s.to_string()),
            rt.reason(),
            rt.safety_paused(),
            rt.current_exe().map(|s| s.to_string()),
            rt.revision(),
        )
    }

    /// Snapshot the persisted inputs the resolver needs, so the runtime
    /// lock is never held across the config lock.
    fn rules_snapshot(&self) -> (Vec<AppRule>, bool, Vec<String>) {
        let cfg = self.config.lock();
        (
            cfg.app_rules.clone(),
            cfg.auto_profile_enabled,
            cfg.profiles.iter().map(|p| p.id.clone()).collect(),
        )
    }

    /// S8: install a rule set on the controller (flush first, then
    /// install) and execute the flush injections. Called whenever the
    /// effective profile's rules change: profile switches, rule CRUD,
    /// emergency fallback. `SendInput` is callable from any thread, so
    /// this runs on command/worker threads as well as the hook thread.
    fn apply_remap_rules(&self, rules: &[(KeyCode, KeyCode)]) {
        let flushed = self.controller.set_remap_rules(rules);
        if !flushed.is_empty() {
            if let Err(err) = crate::keyboard::inject::execute_injections(&flushed) {
                tracing::error!("remap flush failed ({err}); a remapped key may stick");
            }
        }
    }

    /// S8: install one profile's rules by id (flush + apply).
    fn apply_profile_rules(&self, profile_id: &str) {
        let rules = {
            let cfg = self.config.lock();
            cfg.profile(profile_id)
                .map(|p| parse_remap_rules(&p.remappings))
                .unwrap_or_default()
        };
        self.apply_remap_rules(&rules);
    }

    /// Apply one profile's disabled keys to the controller. Runtime-only:
    /// no config write, no disk write.
    fn apply_profile_keys(&self, profile_id: &str) {
        let keys = {
            let cfg = self.config.lock();
            cfg.profile(profile_id)
                .map(|p| parse_key_ids(&p.disabled_keys))
                .unwrap_or_default()
        };
        self.controller.set_disabled_keys(&keys);
        // S8: the same switch applies the profile's remap rules (flush
        // old held targets first, then install the new set).
        self.apply_profile_rules(profile_id);
    }

    /// Apply the current effective profile's keys (after a manual
    /// selection, delete, or resume changed it).
    pub fn apply_effective_profile_keys(&self) {
        let id = self.effective_profile_id();
        self.apply_profile_keys(&id);
    }

    /// S8: re-apply the effective profile's remap rules (flush + install).
    pub fn apply_effective_profile_rules(&self) {
        let id = self.effective_profile_id();
        self.apply_profile_rules(&id);
    }

    /// Record a manual profile selection in the runtime: it outranks app
    /// rules and survives foreground switches until an explicit resume.
    /// Keys/revision are the caller's job (the profile commands already
    /// bump and apply via `apply_current_profile`, which targets the same
    /// profile). Returns the change, if the effective profile moved.
    pub fn set_manual_override(&self, id: &str) -> Option<EffectiveChange> {
        self.app_profiles.lock().set_manual(id)
    }

    /// Foreground observation (observer thread): debounce, then resolve
    /// and apply. Returns the change, if the effective profile moved.
    /// Never writes settings.json.
    pub fn note_foreground(&self, exe: Option<String>, now_ms: u64) -> Option<EffectiveChange> {
        let due = self.app_profiles.lock().note_foreground(exe, now_ms);
        if !due {
            return None;
        }
        let (rules, auto_enabled, valid_ids) = self.rules_snapshot();
        let change = self
            .app_profiles
            .lock()
            .commit(&rules, auto_enabled, &valid_ids);
        if let Some(change) = &change {
            self.apply_profile_keys(&change.profile_id);
            self.bump_runtime_revision();
        }
        change
    }

    /// Re-evaluate the current foreground app now (rule saved/deleted,
    /// automation toggled). Returns the change, if it moved.
    pub fn reevaluate_foreground(&self) -> Option<EffectiveChange> {
        let (rules, auto_enabled, valid_ids) = self.rules_snapshot();
        let change = self
            .app_profiles
            .lock()
            .force_reevaluate(&rules, auto_enabled, &valid_ids);
        if let Some(change) = &change {
            self.apply_profile_keys(&change.profile_id);
            self.bump_runtime_revision();
        }
        change
    }

    /// User resumes automation: clears the manual override and the safety
    /// pause, re-evaluates the current foreground app. Runtime-only.
    pub fn resume_auto(&self) -> Option<EffectiveChange> {
        let (rules, auto_enabled, valid_ids) = self.rules_snapshot();
        let change = self
            .app_profiles
            .lock()
            .resume_auto(&rules, auto_enabled, &valid_ids);
        if let Some(change) = &change {
            self.apply_profile_keys(&change.profile_id);
            self.bump_runtime_revision();
        }
        change
    }

    /// A profile was deleted (rules already cleaned in config): if it was
    /// the manual override or the effective profile, fall back.
    pub fn sync_runtime_after_profile_delete(&self, deleted_id: &str) -> Option<EffectiveChange> {
        let (rules, auto_enabled, valid_ids) = self.rules_snapshot();
        let change = self.app_profiles.lock().on_profile_deleted(
            deleted_id,
            &rules,
            auto_enabled,
            &valid_ids,
        );
        if let Some(change) = &change {
            self.apply_profile_keys(&change.profile_id);
            self.bump_runtime_revision();
        }
        change
    }
}

pub fn parse_key_ids(ids: &[String]) -> Vec<KeyCode> {
    ids.iter().filter_map(|id| KeyCode::from_id(id)).collect()
}

/// S8: parse a profile's stored remap rules into engine pairs. Invalid
/// ids were already dropped by `normalize`; this is a defensive second
/// filter for rules built in memory.
pub fn parse_remap_rules(rules: &[crate::profiles::RemapRule]) -> Vec<(KeyCode, KeyCode)> {
    rules
        .iter()
        .filter_map(|r| {
            let from = KeyCode::from_id(&r.from)?;
            let to = KeyCode::from_id(&r.to)?;
            if from == to {
                return None;
            }
            Some((from, to))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::FilterEngine;
    use std::fs::File;

    struct TestController {
        engine: Mutex<FilterEngine>,
    }

    impl KeyboardController for TestController {
        fn enable_key(&self, key: KeyCode) {
            self.engine.lock().enable_key(key);
        }
        fn disable_key(&self, key: KeyCode) {
            self.engine.lock().disable_key(key);
        }
        fn enable_all(&self) -> Vec<crate::keyboard::InjectAction> {
            self.engine.lock().enable_all()
        }
        fn is_enabled(&self, key: KeyCode) -> bool {
            self.engine.lock().is_enabled(key)
        }
        fn set_disabled_keys(&self, keys: &[KeyCode]) {
            self.engine.lock().set_disabled_keys(keys);
        }
        fn disabled_keys(&self) -> Vec<KeyCode> {
            self.engine.lock().disabled_keys()
        }
        fn set_cat_lock(&self, locked: bool) {
            self.engine.lock().set_cat_lock(locked);
        }
        fn is_cat_locked(&self) -> bool {
            self.engine.lock().is_cat_locked()
        }
        fn pressed_keys(&self) -> Vec<KeyCode> {
            self.engine.lock().pressed_keys()
        }
        fn event_sequence(&self) -> u64 {
            self.engine.lock().event_sequence()
        }
        fn set_remap_rules(
            &self,
            rules: &[(KeyCode, KeyCode)],
        ) -> Vec<crate::keyboard::InjectAction> {
            self.engine.lock().set_remap_rules(rules)
        }
        fn flush_remaps(&self) -> Vec<crate::keyboard::InjectAction> {
            self.engine.lock().flush_remaps()
        }
    }

    fn test_state_with_controller(config_path: PathBuf) -> (AppState, Arc<TestController>) {
        let controller = Arc::new(TestController {
            engine: Mutex::new(FilterEngine::new()),
        });
        let state = AppState {
            controller: controller.clone(),
            config: Mutex::new(AppConfig::default()),
            config_path,
            hook_active: AtomicBool::new(false),
            hook_error: Mutex::new(None),
            shutdown: Arc::new(|| {}),
            safety: Arc::new(SafetyState::new()),
            persist_lock: Mutex::new(()),
            persisted_revision: AtomicU64::new(0),
            runtime_revision: AtomicU64::new(0),
            persist_error: Mutex::new(None),
            persist_error_kind: Mutex::new(None),
            app_profiles: Mutex::new(AppProfileRuntime::new(DEFAULT_PROFILE_ID)),
        };
        (state, controller)
    }

    fn test_state(config_path: PathBuf) -> AppState {
        test_state_with_controller(config_path).0
    }

    fn unique_temp_path(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("keyglow-safety-test-{tag}-{stamp}"))
    }

    /// The settings path can never be written (its parent is a regular
    /// file). emergency_unlock must still leave physical input enabled:
    /// persist is best-effort and never rolls the controller back.
    #[test]
    fn emergency_unlock_keeps_keys_enabled_when_persist_fails() {
        let blocker = unique_temp_path("blocker");
        File::create(&blocker).unwrap();
        let config_path = blocker.join("settings.json");
        let state = test_state(config_path);

        state.controller.disable_key(KeyCode::KeyA);
        state.controller.set_cat_lock(true);
        assert!(!state.controller.is_enabled(KeyCode::KeyA));

        // Sanity: this persist really does fail in this fixture.
        assert!(state.persist().is_err());

        state.emergency_unlock();

        assert!(state.controller.is_enabled(KeyCode::KeyA));
        assert!(state.snapshot_disabled().is_empty());
        assert!(!state.controller.is_cat_locked());
        assert_eq!(state.config.lock().selected_profile, DEFAULT_PROFILE_ID);
        // The failure is recorded for the UI instead of swallowed.
        let (persisted, err, kind, _) = state.persist_state();
        assert!(!persisted);
        assert!(err.is_some());
        assert_eq!(kind.as_deref(), Some(PERSIST_ERROR_IO));
    }

    /// A command that was in flight while an emergency press landed must not
    /// leave its disable behind, even if its mutations landed after the
    /// worker's exactly-once convergence: the stale path re-asserts the safe
    /// state unconditionally (this is what `discard_stale_command` does once
    /// the epoch check fails; the AppHandle emits are UI-only).
    #[test]
    fn stale_command_converges_to_safe_state() {
        let dir = unique_temp_path("stale");
        let state = test_state(dir.join("settings.json"));

        let entered = state.safety_epoch();
        // In-flight stale command: disables a key, switches profile.
        state.controller.disable_key(KeyCode::KeyA);
        {
            let mut cfg = state.config.lock();
            cfg.selected_profile = "gaming".to_string();
            if let Some(profile) = cfg.current_profile_mut() {
                profile.disabled_keys.push("KeyA".into());
            }
        }

        // Emergency press lands mid-command (hook already did enable_all)
        // and the worker converged exactly once for this epoch...
        state.controller.enable_all();
        let epoch = state.safety.trigger();
        assert!(state.safety.claim_reconcile(epoch));
        state.emergency_unlock();

        // ...but the stale command's mutations landed afterwards anyway.
        state.controller.disable_key(KeyCode::KeyA);
        {
            let mut cfg = state.config.lock();
            cfg.selected_profile = "gaming".to_string();
            if let Some(profile) = cfg.current_profile_mut() {
                profile.disabled_keys.push("KeyA".into());
            }
        }

        assert_ne!(state.safety_epoch(), entered);
        // Forced re-convergence: repairs even post-claim mutations.
        state.emergency_unlock();

        assert!(state.controller.is_enabled(KeyCode::KeyA));
        assert!(state.snapshot_disabled().is_empty());
        assert_eq!(state.config.lock().selected_profile, DEFAULT_PROFILE_ID);
    }

    #[test]
    fn fresh_command_epoch_matches() {
        let dir = unique_temp_path("fresh");
        let state = test_state(dir.join("settings.json"));
        let entered = state.safety_epoch();
        assert_eq!(state.safety_epoch(), entered);
    }

    #[test]
    fn successful_persist_clears_error_and_bumps_revision() {
        let dir = unique_temp_path("rev");
        std::fs::create_dir_all(&dir).unwrap();
        let state = test_state(dir.join("settings.json"));
        assert!(state.persist().is_ok());
        assert!(state.persist().is_ok());
        let (persisted, err, kind, rev) = state.persist_state();
        assert!(persisted);
        assert!(err.is_none());
        assert!(kind.is_none());
        assert_eq!(rev, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retry_persist_recovers_after_failure() {
        // Fail first (unwritable path), then point at a writable dir and
        // retry: the error clears and the revision advances.
        let blocker = unique_temp_path("retry-blocker");
        File::create(&blocker).unwrap();
        let mut state = test_state(blocker.join("settings.json"));
        assert!(state.persist().is_err());
        assert!(!state.persist_state().0);

        let dir = unique_temp_path("retry-ok");
        std::fs::create_dir_all(&dir).unwrap();
        state.config_path = dir.join("settings.json");
        assert!(state.retry_persist().is_ok());
        let (persisted, err, _, rev) = state.persist_state();
        assert!(persisted);
        assert!(err.is_none());
        assert_eq!(rev, 1);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&blocker);
    }

    #[test]
    fn future_version_config_is_never_overwritten() {
        let dir = unique_temp_path("future");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        // Simulate a file written by a newer KeyGlow.
        std::fs::write(&path, r#"{"version": 99, "selectedProfile": "default"}"#).unwrap();
        let loaded = crate::profiles::load(&path);
        assert!(loaded.future_version);

        let mut state = test_state(path.clone());
        state.config = Mutex::new(loaded);
        let err = state.persist().expect_err("must refuse to overwrite newer version");
        assert!(err.contains("newer KeyGlow version"));
        let (persisted, _, kind, _) = state.persist_state();
        assert!(!persisted);
        assert_eq!(kind.as_deref(), Some(PERSIST_ERROR_NEWER_VERSION));
        // The file still holds the newer version's data.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"version\": 99") || raw.contains("\"version\":99"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_persist_matches_latest_revision() {
        // N threads persisting concurrently: the single writer serializes
        // them, every write is a complete valid file, and the revision
        // counter matches the number of successful writes.
        use std::sync::Barrier;
        let dir = unique_temp_path("conc");
        std::fs::create_dir_all(&dir).unwrap();
        let state = Arc::new(test_state(dir.join("settings.json")));
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = vec![];
        for _ in 0..8 {
            let state = state.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..25 {
                    state.persist().unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let rev = state.persisted_revision.load(Ordering::SeqCst);
        assert_eq!(rev, 200);
        let raw = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        let parsed: AppConfig = serde_json::from_str(&raw).expect("valid JSON");
        assert_eq!(parsed.version, crate::profiles::models::CONFIG_VERSION);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- P3: runtime revision ordering ----

    #[test]
    fn runtime_revision_starts_at_zero_and_increments() {
        let dir = unique_temp_path("rt_rev");
        std::fs::create_dir_all(&dir).unwrap();
        let state = test_state(dir.join("settings.json"));
        assert_eq!(state.runtime_revision.load(Ordering::SeqCst), 0);
        assert_eq!(state.event_stamp(), (0, 0));
        state.bump_runtime_revision();
        state.bump_runtime_revision();
        assert_eq!(state.runtime_revision.load(Ordering::SeqCst), 2);
        assert_eq!(state.event_stamp(), (0, 2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emergency_unlock_bumps_runtime_revision_even_when_persist_fails() {
        // P3 core distinction: the convergence mutates runtime state
        // (controller + config), so it must advance the UI ordering
        // revision even when the best-effort persist fails.
        // persisted_revision counts disk writes; runtime_revision counts
        // state mutations. They are different numbers on purpose.
        //
        // save() creates missing parent dirs, so a missing dir is NOT a
        // failure. Use a blocker *file* as the parent: create_dir_all on
        // a file path fails on every platform.
        let blocker = unique_temp_path("rt_emg_blocker");
        File::create(&blocker).unwrap();
        let state = test_state(blocker.join("settings.json"));
        // Sanity: this persist really does fail in this fixture.
        assert!(state.persist().is_err());
        assert_eq!(state.persisted_revision.load(Ordering::SeqCst), 0);

        state.emergency_unlock();

        assert_eq!(
            state.runtime_revision.load(Ordering::SeqCst),
            1,
            "convergence changed runtime state, so the UI ordering revision must advance"
        );
        assert_eq!(
            state.persisted_revision.load(Ordering::SeqCst),
            0,
            "failed persist must not advance the disk revision"
        );
        // Physical recovery happened regardless of the disk failure.
        assert!(state.snapshot_disabled().is_empty());
        let _ = std::fs::remove_file(&blocker);
    }

    #[test]
    fn event_stamp_tracks_safety_epoch_and_runtime_revision() {
        let dir = unique_temp_path("rt_stamp");
        std::fs::create_dir_all(&dir).unwrap();
        let state = test_state(dir.join("settings.json"));
        state.bump_runtime_revision();
        let epoch = state.safety.trigger();
        // A trigger alone does not mutate runtime state: the stamp's
        // revision part only moves on bump_runtime_revision().
        assert_eq!(state.event_stamp(), (epoch, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- S5: app profiles ----

    fn gaming_rule() -> AppRule {
        AppRule {
            id: "r-game".into(),
            executable_path: "C:\\Games\\game.exe".into(),
            profile_id: "gaming".into(),
            enabled: true,
            priority: 0,
        }
    }

    fn add_gaming_rule(state: &AppState) {
        state.config.lock().app_rules.push(gaming_rule());
    }

    #[test]
    fn foreground_switch_applies_rule_without_writing_settings() {
        let dir = unique_temp_path("s5_nodisk");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let state = test_state(path.clone());
        state.persist().unwrap();
        let before = std::fs::read(&path).unwrap();
        add_gaming_rule(&state);

        // Debouncing: the first sighting starts the timer, no switch yet.
        assert!(state
            .note_foreground(Some("C:\\Games\\game.exe".into()), 0)
            .is_none());
        let change = state
            .note_foreground(Some("C:\\Games\\game.exe".into()), 250)
            .expect("settled switch must commit");
        assert_eq!(change.profile_id, "gaming");
        assert_eq!(change.reason, MatchReason::AppRule);
        // The controller filters Gaming's keys (MetaLeft/Right disabled).
        assert!(!state.controller.is_enabled(KeyCode::MetaLeft));
        assert!(!state.controller.is_enabled(KeyCode::MetaRight));
        // But the file is byte-identical: the switch never touched disk,
        // and the persisted selection is untouched too.
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(state.config.lock().selected_profile, DEFAULT_PROFILE_ID);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn foreground_switch_keeps_cat_lock() {
        let dir = unique_temp_path("s5_catlock");
        let state = test_state(dir.join("settings.json"));
        add_gaming_rule(&state);
        state.controller.set_cat_lock(true);

        state.note_foreground(Some("C:\\Games\\game.exe".into()), 0);
        let change = state.note_foreground(Some("C:\\Games\\game.exe".into()), 250);
        assert!(change.is_some());
        // The lock survives the switch: every key stays blocked.
        assert!(state.controller.is_cat_locked());
        assert!(!state.controller.is_enabled(KeyCode::KeyA));
        assert!(!state.controller.is_enabled(KeyCode::MetaLeft));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emergency_pauses_automation_and_resume_restores_rule() {
        let dir = unique_temp_path("s5_pause");
        let state = test_state(dir.join("settings.json"));
        add_gaming_rule(&state);
        state.note_foreground(Some("C:\\Games\\game.exe".into()), 0);
        assert!(state
            .note_foreground(Some("C:\\Games\\game.exe".into()), 250)
            .is_some());
        assert_eq!(state.effective_profile_id(), "gaming");

        state.emergency_unlock();
        let (eff, _, reason, paused, _, _) = state.app_profile_status();
        assert_eq!(eff, "default");
        assert_eq!(reason, MatchReason::SafetyPaused);
        assert!(paused);
        assert!(state.controller.is_enabled(KeyCode::MetaLeft));

        // Foreground switches while paused change nothing.
        state.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 500);
        assert!(state
            .note_foreground(Some("C:\\Windows\\notepad.exe".into()), 800)
            .is_none());
        assert_eq!(state.effective_profile_id(), "default");

        // Explicit resume re-evaluates the current foreground (Notepad has
        // no rule): Default with the Default reason.
        let change = state.resume_auto().expect("resume must report the change");
        assert_eq!(change.profile_id, "default");
        assert_eq!(change.reason, MatchReason::Default);
        assert!(!state.app_profile_status().3);
    }

    #[test]
    fn manual_selection_survives_foreground_switch_via_state() {
        let dir = unique_temp_path("s5_manual");
        let state = test_state(dir.join("settings.json"));
        add_gaming_rule(&state);
        state.set_manual_override("coding");
        assert_eq!(state.effective_profile_id(), "coding");

        state.note_foreground(Some("C:\\Games\\game.exe".into()), 0);
        assert!(state
            .note_foreground(Some("C:\\Games\\game.exe".into()), 250)
            .is_none());
        assert_eq!(state.effective_profile_id(), "coding");
        let (_, manual, reason, _, _, _) = state.app_profile_status();
        assert_eq!(manual.as_deref(), Some("coding"));
        assert_eq!(reason, MatchReason::Manual);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn profile_delete_sync_falls_back_effective() {
        let dir = unique_temp_path("s5_delete");
        let state = test_state(dir.join("settings.json"));
        add_gaming_rule(&state);
        state.note_foreground(Some("C:\\Games\\game.exe".into()), 0);
        assert!(state
            .note_foreground(Some("C:\\Games\\game.exe".into()), 250)
            .is_some());
        assert_eq!(state.effective_profile_id(), "gaming");

        // Simulate delete_profile's config cleanup: profile + its rules go.
        {
            let mut cfg = state.config.lock();
            cfg.profiles.retain(|p| p.id != "gaming");
            cfg.app_rules.retain(|r| r.profile_id != "gaming");
        }
        let change = state
            .sync_runtime_after_profile_delete("gaming")
            .expect("effective profile deleted must fall back");
        assert_eq!(change.profile_id, "default");
        assert_eq!(change.reason, MatchReason::Default);
        assert_eq!(state.effective_profile_id(), "default");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- S8: remap rule wiring ----

    use crate::keyboard::engine::{HookDecision, InjectAction, KeyPhase};
    use crate::profiles::RemapRule;

    fn set_profile_rules(state: &AppState, profile_id: &str, rules: Vec<RemapRule>) {
        state
            .config
            .lock()
            .profile_mut(profile_id)
            .unwrap()
            .remappings = rules;
    }

    #[test]
    fn profile_switch_installs_rules_and_flushes_held() {
        let dir = unique_temp_path("s8_switch");
        std::fs::create_dir_all(&dir).unwrap();
        let (state, controller) = test_state_with_controller(dir.join("settings.json"));
        set_profile_rules(
            &state,
            "gaming",
            vec![RemapRule {
                from: "KeyA".into(),
                to: "KeyB".into(),
            }],
        );
        {
            let mut cfg = state.config.lock();
            cfg.activate_profile("gaming").unwrap();
        }
        state.apply_current_profile();
        assert_eq!(
            controller.engine.lock().remap_rules(),
            vec![(KeyCode::KeyA, KeyCode::KeyB)]
        );
        // Hold A: the target is injected.
        let result = controller
            .engine
            .lock()
            .process(KeyCode::KeyA, KeyPhase::Down);
        assert_eq!(result.inject, vec![InjectAction::Down(KeyCode::KeyB)]);
        // Switch to Default (no rules) mid-hold: the held target is
        // flushed (execute_injections is a stub off Windows, but the
        // engine state is released) and the new rules install.
        {
            let mut cfg = state.config.lock();
            cfg.activate_profile(DEFAULT_PROFILE_ID).unwrap();
        }
        state.apply_current_profile();
        assert!(controller.engine.lock().remap_rules().is_empty());
        assert!(controller.engine.lock().flush_remaps().is_empty());
        // The held A's release injects nothing (already flushed).
        let release = controller
            .engine
            .lock()
            .process(KeyCode::KeyA, KeyPhase::Up);
        assert!(release.inject.is_empty());
        // The next A press forwards normally.
        let again = controller
            .engine
            .lock()
            .process(KeyCode::KeyA, KeyPhase::Down);
        assert_eq!(again.decision, HookDecision::Forward);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emergency_falls_back_to_default_rules() {
        let dir = unique_temp_path("s8_emergency");
        std::fs::create_dir_all(&dir).unwrap();
        let (state, controller) = test_state_with_controller(dir.join("settings.json"));
        set_profile_rules(
            &state,
            "gaming",
            vec![RemapRule {
                from: "KeyA".into(),
                to: "KeyB".into(),
            }],
        );
        set_profile_rules(
            &state,
            DEFAULT_PROFILE_ID,
            vec![RemapRule {
                from: "KeyC".into(),
                to: "KeyD".into(),
            }],
        );
        {
            let mut cfg = state.config.lock();
            cfg.activate_profile("gaming").unwrap();
        }
        state.apply_current_profile();
        assert_eq!(
            controller.engine.lock().remap_rules(),
            vec![(KeyCode::KeyA, KeyCode::KeyB)]
        );
        // Hold A under Gaming's rule.
        let held = controller
            .engine
            .lock()
            .process(KeyCode::KeyA, KeyPhase::Down);
        assert_eq!(held.inject, vec![InjectAction::Down(KeyCode::KeyB)]);
        // Emergency: held target flushed, Default's rules installed.
        state.emergency_unlock();
        assert_eq!(state.config.lock().selected_profile, DEFAULT_PROFILE_ID);
        assert_eq!(
            controller.engine.lock().remap_rules(),
            vec![(KeyCode::KeyC, KeyCode::KeyD)]
        );
        assert!(controller.engine.lock().flush_remaps().is_empty());
        // A's release injects nothing (already flushed); C remaps now.
        let release = controller
            .engine
            .lock()
            .process(KeyCode::KeyA, KeyPhase::Up);
        assert!(release.inject.is_empty());
        let remapped = controller
            .engine
            .lock()
            .process(KeyCode::KeyC, KeyPhase::Down);
        assert_eq!(remapped.inject, vec![InjectAction::Down(KeyCode::KeyD)]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
