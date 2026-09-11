use crate::keyboard::engine::KeyboardController;
use crate::keyboard::hook::HookEvent;
use crate::keyboard::FilterEngine;
use parking_lot::Mutex;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

#[cfg(windows)]
mod windows;

pub struct ControllerStart {
    pub controller: Arc<dyn KeyboardController>,
    pub events: Receiver<HookEvent>,
    pub hook_active: bool,
    pub hook_error: Option<String>,
    pub shutdown: Arc<dyn Fn() + Send + Sync>,
}

pub fn start_input_backend() -> ControllerStart {
    let engine = Arc::new(Mutex::new(FilterEngine::new()));
    let (tx, rx) = std::sync::mpsc::sync_channel(1024);

    #[cfg(windows)]
    {
        windows::start(engine, tx, rx)
    }

    #[cfg(not(windows))]
    {
        let controller = Arc::new(UnsupportedController { engine });
        let _ = tx;
        ControllerStart {
            controller,
            events: rx,
            hook_active: false,
            hook_error: Some("KeyGlow v0.1 supports Windows only".into()),
            shutdown: Arc::new(|| {}),
        }
    }
}

#[cfg(not(windows))]
struct UnsupportedController {
    engine: Arc<Mutex<FilterEngine>>,
}

#[cfg(not(windows))]
impl KeyboardController for UnsupportedController {
    fn enable_key(&self, key: crate::keyboard::KeyCode) {
        self.engine.lock().enable_key(key);
    }
    fn disable_key(&self, key: crate::keyboard::KeyCode) {
        self.engine.lock().disable_key(key);
    }
    fn enable_all(&self) {
        self.engine.lock().enable_all();
    }
    fn is_enabled(&self, key: crate::keyboard::KeyCode) -> bool {
        self.engine.lock().is_enabled(key)
    }
    fn set_disabled_keys(&self, keys: &[crate::keyboard::KeyCode]) {
        self.engine.lock().set_disabled_keys(keys);
    }
    fn disabled_keys(&self) -> Vec<crate::keyboard::KeyCode> {
        self.engine.lock().disabled_keys()
    }
    fn set_cat_lock(&self, locked: bool) {
        self.engine.lock().set_cat_lock(locked);
    }
    fn is_cat_locked(&self) -> bool {
        self.engine.lock().is_cat_locked()
    }
}
