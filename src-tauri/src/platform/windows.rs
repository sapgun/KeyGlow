use crate::keyboard::engine::{FilterEngine, InjectAction, KeyboardController};
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
    fn enable_key(&self, key: KeyCode) {
        self.engine.lock().enable_key(key);
    }

    fn disable_key(&self, key: KeyCode) {
        self.engine.lock().disable_key(key);
    }

    fn enable_all(&self) -> Vec<InjectAction> {
        self.engine.lock().enable_all()
    }

    fn is_enabled(&self, key: KeyCode) -> bool {
        self.engine.lock().is_enabled(key)
    }

    fn set_disabled_keys(&self, keys: &[KeyCode]) -> Vec<InjectAction> {
        self.engine.lock().set_disabled_keys(keys)
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

    fn set_remap_rules(&self, rules: &[(KeyCode, KeyCode)]) -> Vec<InjectAction> {
        self.engine.lock().set_remap_rules(rules)
    }

    fn flush_remaps(&self) -> Vec<InjectAction> {
        self.engine.lock().flush_remaps()
    }

    fn set_macros(&self, defs: &[crate::profiles::models::MacroDef]) -> Vec<InjectAction> {
        self.engine.lock().set_macros(defs)
    }

    fn set_layers(&self, defs: &[crate::profiles::models::LayerDef]) -> Vec<InjectAction> {
        self.engine.lock().set_layers(defs)
    }

    fn macro_tick_execute(&self, now_ms: u64) -> (bool, Option<u64>) {
        crate::platform::macro_tick_execute_on(&self.engine, now_ms)
    }

    fn active_layer_id(&self) -> Option<String> {
        self.engine.lock().active_layer_id()
    }

    fn cancel_macro(&self) -> Vec<InjectAction> {
        self.engine.lock().macro_cancel()
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

    // S8 (D8): before the hook is installed, release anything a previous
    // force-killed run left down. The sweep skips keys physically held
    // right now (GetAsyncKeyState check), so it can never cancel a live
    // hold. The ups go straight to the foreground app here — the hook is
    // not installed yet — which is harmless for keys that are not down.
    crate::keyboard::inject::startup_sweep();

    match start_hook(engine, tx, safety.clone(), safety_wake_tx) {
        Ok(HookStart { handle, exit }) => {
            let shutdown_slot: Arc<Mutex<Option<HookHandle>>> = Arc::new(Mutex::new(Some(handle)));
            let shutdown_slot_clone = shutdown_slot.clone();
            let shutdown_controller = controller.clone();
            ControllerStart {
                controller,
                events: rx,
                hook_active: true,
                hook_error: None,
                shutdown: Arc::new(move || {
                    // S9: stop any running macro first — release its held
                    // keys before the hook goes away, or the keys it
                    // holds would stick. The layer needs no release
                    // here: its tables are engine state, not held keys.
                    let release = shutdown_controller.cancel_macro();
                    if !release.is_empty() {
                        if let Err(err) =
                            crate::keyboard::inject::execute_injections(&release)
                        {
                            tracing::error!(
                                "macro cancel on shutdown failed ({err}); a macro key may stick"
                            );
                        }
                    }
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
