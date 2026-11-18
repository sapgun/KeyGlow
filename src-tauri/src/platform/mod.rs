use crate::keyboard::engine::KeyboardController;
use crate::keyboard::hook::HookEvent;
use crate::keyboard::{FilterEngine, HookExit, InjectAction, SafetyState};
use parking_lot::Mutex;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

#[cfg(windows)]
mod windows;

pub mod foreground;

pub struct ControllerStart {
    pub controller: Arc<dyn KeyboardController>,
    pub events: Receiver<HookEvent>,
    pub hook_active: bool,
    pub hook_error: Option<String>,
    pub shutdown: Arc<dyn Fn() + Send + Sync>,
    /// Shared safety epoch for emergency unlock (HF-01). Bumped by the hook
    /// thread, converged by the safety worker; also serializes mutating
    /// commands against stale post-emergency writes.
    pub safety: Arc<SafetyState>,
    /// Wake receiver for the safety worker. Unbounded channel: the hook
    /// never blocks on it.
    pub safety_wake: Receiver<()>,
    /// Terminal-state reports from the hook thread (HF-07). `Some` on
    /// Windows when the hook was installed; the watcher in `lib.rs` flips
    /// `hook_active`/`hook_error` if the thread dies on its own, so the UI
    /// never shows a stale "keyboard control active".
    pub hook_exit: Option<Receiver<HookExit>>,
}

/// S9: shared macro-tick implementation for the real controllers.
/// Advances the engine's macro to `now_ms` and injects the due steps.
/// The engine lock is held across tick + injection — the same atomicity
/// as the hook callback's S8 path — so a cancel key arriving mid-tick
/// cannot interleave a down after the run's up. If an injection fails,
/// the run is aborted and its held keys released instead of leaving a
/// key stuck down. Returns `(still_running, next_due_ms)` for the tick
/// thread's sleep.
pub(crate) fn macro_tick_execute_on(
    engine: &Arc<Mutex<FilterEngine>>,
    now_ms: u64,
) -> (bool, Option<u64>) {
    let mut engine = engine.lock();
    let actions = engine.macro_tick(now_ms);
    if !actions.is_empty() {
        if let Err(err) = crate::keyboard::inject::execute_injections(&actions) {
            tracing::error!("macro step injection failed ({err}); aborting the macro run");
            let release = engine.macro_cancel();
            if !release.is_empty() {
                if let Err(err) = crate::keyboard::inject::execute_injections(&release) {
                    tracing::error!(
                        "macro abort release failed ({err}); a macro key may stick — press the macro's cancel key or Ctrl+Shift+F12"
                    );
                }
            }
            return (false, None);
        }
    }
    (engine.macro_running(), engine.macro_next_due_ms())
}

pub fn start_input_backend() -> ControllerStart {
    let engine = Arc::new(Mutex::new(FilterEngine::new()));
    let (tx, rx) = std::sync::mpsc::sync_channel(1024);
    let safety = Arc::new(SafetyState::new());
    let (safety_wake_tx, safety_wake_rx) = std::sync::mpsc::channel::<()>();

    #[cfg(windows)]
    {
        windows::start(engine, tx, rx, safety, safety_wake_tx, safety_wake_rx)
    }

    #[cfg(not(windows))]
    {
        let controller = Arc::new(UnsupportedController { engine });
        let _ = tx;
        drop(safety_wake_tx);
        ControllerStart {
            controller,
            events: rx,
            hook_active: false,
            hook_error: Some("KeyGlow v0.1 supports Windows only".into()),
            shutdown: Arc::new(|| {}),
            safety,
            safety_wake: safety_wake_rx,
            hook_exit: None,
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
    fn enable_all(&self) -> Vec<InjectAction> {
        self.engine.lock().enable_all()
    }
    fn is_enabled(&self, key: crate::keyboard::KeyCode) -> bool {
        self.engine.lock().is_enabled(key)
    }
    fn set_disabled_keys(&self, keys: &[crate::keyboard::KeyCode]) -> Vec<InjectAction> {
        self.engine.lock().set_disabled_keys(keys)
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
    fn pressed_keys(&self) -> Vec<crate::keyboard::KeyCode> {
        self.engine.lock().pressed_keys()
    }
    fn event_sequence(&self) -> u64 {
        self.engine.lock().event_sequence()
    }
    fn set_remap_rules(
        &self,
        rules: &[(crate::keyboard::KeyCode, crate::keyboard::KeyCode)],
    ) -> Vec<InjectAction> {
        self.engine.lock().set_remap_rules(rules)
    }
    fn flush_remaps(&self) -> Vec<InjectAction> {
        self.engine.lock().flush_remaps()
    }
    fn set_macros(
        &self,
        defs: &[crate::profiles::models::MacroDef],
    ) -> Vec<InjectAction> {
        self.engine.lock().set_macros(defs)
    }
    fn set_layers(
        &self,
        defs: &[crate::profiles::models::LayerDef],
    ) -> Vec<InjectAction> {
        self.engine.lock().set_layers(defs)
    }
    fn macro_tick_execute(&self, now_ms: u64) -> (bool, Option<u64>) {
        macro_tick_execute_on(&self.engine, now_ms)
    }
    fn active_layer_id(&self) -> Option<String> {
        self.engine.lock().active_layer_id()
    }
    fn cancel_macro(&self) -> Vec<InjectAction> {
        self.engine.lock().macro_cancel()
    }
}
