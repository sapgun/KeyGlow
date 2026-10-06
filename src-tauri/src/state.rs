use crate::keyboard::engine::KeyboardController;
use crate::keyboard::{KeyCode, SafetyState};
use crate::profiles::{save, AppConfig, DEFAULT_PROFILE_ID};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
}

impl AppState {
    pub fn persist(&self) -> Result<(), String> {
        let cfg = self.config.lock().clone();
        save(&self.config_path, &cfg)
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
            if cfg.profile(DEFAULT_PROFILE_ID).is_some() {
                cfg.selected_profile = DEFAULT_PROFILE_ID.to_string();
            }
            if let Some(profile) = cfg.current_profile_mut() {
                profile.disabled_keys.clear();
            }
        }
        if let Err(err) = self.persist() {
            // Best effort by design: physical keys stay enabled regardless of
            // disk state. The error is logged, not swallowed, so a broken
            // settings path is visible in diagnostics.
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
}
