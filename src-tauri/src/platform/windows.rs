use crate::keyboard::engine::{FilterEngine, KeyboardController};
use crate::keyboard::hook::{start_hook, HookEvent, HookHandle};
use crate::keyboard::KeyCode;
use crate::platform::ControllerStart;
use parking_lot::Mutex;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;

pub struct WindowsController {
    engine: Arc<Mutex<FilterEngine>>,
}

impl KeyboardController for WindowsController {
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

pub fn start(
    engine: Arc<Mutex<FilterEngine>>,
    tx: SyncSender<HookEvent>,
    rx: Receiver<HookEvent>,
) -> ControllerStart {
    let controller = Arc::new(WindowsController {
        engine: engine.clone(),
    });

    match start_hook(engine, tx) {
        Ok(handle) => {
            let shutdown_slot: Arc<Mutex<Option<HookHandle>>> = Arc::new(Mutex::new(Some(handle)));
            let shutdown_slot_clone = shutdown_slot.clone();
            ControllerStart {
                controller,
                events: rx,
                hook_active: true,
                hook_error: None,
                shutdown: Arc::new(move || {
                    if let Some(handle) = shutdown_slot_clone.lock().take() {
                        handle.shutdown();
                    }
                }),
            }
        }
        Err(err) => {
            tracing::error!("keyboard hook unavailable: {err}");
            ControllerStart {
                controller,
                events: rx,
                hook_active: false,
                hook_error: Some(err),
                shutdown: Arc::new(|| {}),
            }
        }
    }
}
