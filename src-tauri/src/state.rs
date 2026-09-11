use crate::keyboard::engine::KeyboardController;
use crate::keyboard::KeyCode;
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
        let _ = self.persist();
    }
}

pub fn parse_key_ids(ids: &[String]) -> Vec<KeyCode> {
    ids.iter().filter_map(|id| KeyCode::from_id(id)).collect()
}
