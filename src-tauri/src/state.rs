use crate::keyboard::engine::KeyboardController;
use crate::keyboard::{KeyCode, SafetyState};
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
        drop(cfg);
        self.controller.set_disabled_keys(&keys);
        debug_assert!(keys.iter().all(|key| !self.controller.is_enabled(*key)));
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
        self.controller.enable_all();
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
}

pub fn parse_key_ids(ids: &[String]) -> Vec<KeyCode> {
    ids.iter().filter_map(|id| KeyCode::from_id(id)).collect()
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
        fn enable_all(&self) {
            self.engine.lock().enable_all();
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
    }

    fn test_state(config_path: PathBuf) -> AppState {
        AppState {
            controller: Arc::new(TestController {
                engine: Mutex::new(FilterEngine::new()),
            }),
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
        }
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
}
