use crate::keyboard::engine::{FilterEngine, KeyboardController};
use crate::keyboard::hook::{start_hook, HookEvent, HookHandle, HookStart};
use crate::keyboard::{KeyCode, SafetyState};
use crate::platform::ControllerStart;
use parking_lot::Mutex;
use std::sync::mpsc::{Receiver, Sender, SyncSender};
use std::sync::Arc;

pub struct WindowsController {
    engine: Arc<Mutex<FilterEngine>>,
}

impl KeyboardController for WindowsController {
    fn set_emergency_shortcut(&self, shortcut: &crate::keyboard::shortcut::EmergencyShortcut) -> Result<(), String> {
        self.engine.lock().set_emergency_shortcut(shortcut)
    }
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

pub fn start(
    engine: Arc<Mutex<FilterEngine>>,
    tx: SyncSender<HookEvent>,
    rx: Receiver<HookEvent>,
    safety: Arc<SafetyState>,
    safety_wake_tx: Sender<()>,
    safety_wake_rx: Receiver<()>,
) -> ControllerStart {
    let controller = Arc::new(WindowsController {
        engine: engine.clone(),
    });

    match start_hook(engine, tx, safety.clone(), safety_wake_tx) {
        Ok(HookStart { handle, exit }) => {
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
                safety,
                safety_wake: safety_wake_rx,
                // The hook thread reports its terminal state here exactly
                // once (HF-07); lib.rs watches it for unexpected death.
                hook_exit: Some(exit),
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
                safety,
                safety_wake: safety_wake_rx,
                hook_exit: None,
            }
        }
    }
}
