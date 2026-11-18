use super::keycodes::KeyCode;
use crate::keyboard::macro_runner::MacroRunner;
use crate::profiles::models::{LayerDef, LayerMode, MacroDef};
use std::sync::OnceLock;

/// S9: process-relative monotonic clock (milliseconds) shared by the
/// hook callback and the macro tick thread. The engine only uses it for
/// macro step delays and the run cap, so a process-start epoch is
/// correct — and immune to wall-clock jumps.
pub fn macro_now_ms() -> u64 {
    static CLOCK_START: OnceLock<std::time::Instant> = OnceLock::new();
    CLOCK_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPhase {
    Down,
    Repeat,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookDecision {
    Forward,
    Consume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiPulse {
    Down,
    Up,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineResult {
    pub decision: HookDecision,
    pub emergency: bool,
    pub ui: Option<(KeyCode, UiPulse)>,
    /// S8: keys the caller must inject with `SendInput`, in order.
    /// Pure data — the engine never calls Win32 itself. Normally at most
    /// one action (a remap down/up); an emergency flush may carry several
    /// ups (one per held target). Empty when nothing is injected.
    pub inject: Vec<InjectAction>,
    /// S8: the source whose first down was remapped by this event, if
    /// any. The caller needs it for the fail-open path: if executing
    /// `inject` fails, it rolls the optimistic held state back with
    /// `cancel_held_remap` and forwards the physical key instead.
    pub remapped_source: Option<KeyCode>,
}

/// S8: one key event to inject (ADR §2, D3/D4). Scan-code injection;
/// the executor translates this with `to_vk_scan`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InjectAction {
    Down(KeyCode),
    Up(KeyCode),
}

/// In-memory filter used by the Windows hook callback.
///
/// The hook thread must only call `process_event` and cheap getters. Persistence
/// and IPC happen on other threads after a non-blocking channel send.
pub struct FilterEngine {
    disabled: [bool; KeyCode::COUNT],
    forwarded_down: [bool; KeyCode::COUNT],
    physical_down: [bool; KeyCode::COUNT],
    cat_lock: bool,
    /// Bumps on every processed hook event. The UI uses it to detect that
    /// the native pressed snapshot changed while it was not watching
    /// (HF-04): a resync compares this sequence, not wall-clock time.
    event_seq: u64,
    /// S8: per-profile remap rules, source -> target. Installed by
    /// `set_remap_rules` (flush first, then install); a held key always
    /// releases what its down injected, never the current rules (D6).
    remap_rules: [Option<KeyCode>; KeyCode::COUNT],
    /// S8: for each source currently physically held, the target that
    /// was injected for it — a snapshot of the mapping at down time.
    remap_held: [Option<KeyCode>; KeyCode::COUNT],
    /// S8: how many held sources currently map to each target. A target
    /// up is injected only when its refcount reaches 0 (shared targets).
    target_refcount: [u32; KeyCode::COUNT],
    /// S9: macros of the effective profile. The runner owns the run in
    /// progress; the engine only starts it (trigger) and cancels it
    /// (cancel key, emergency, profile switch).
    macros: Vec<MacroDef>,
    /// S9: layers of the effective profile.
    layers: Vec<LayerDef>,
    /// S9: the runner for the macro currently executing, if any.
    macro_runner: MacroRunner,
    /// S9: the base keymap — the profile's own disabled set and remap
    /// rules. While a layer is active, `disabled` / `remap_rules` hold
    /// the layer's tables instead; deactivating restores the base.
    base_disabled: [bool; KeyCode::COUNT],
    base_remap_rules: [Option<KeyCode>; KeyCode::COUNT],
    /// S9: id of the currently active layer, if any.
    active_layer: Option<String>,
    /// S9: for each layer switch key currently physically held (hold
    /// mode), whether its layer is the active one. Needed to release the
    /// right layer on key-up when several switch keys are held.
    layer_switch_held: [bool; KeyCode::COUNT],
}

impl Default for FilterEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterEngine {
    pub fn new() -> Self {
        Self {
            disabled: [false; KeyCode::COUNT],
            forwarded_down: [false; KeyCode::COUNT],
            physical_down: [false; KeyCode::COUNT],
            cat_lock: false,
            event_seq: 0,
            remap_rules: [None; KeyCode::COUNT],
            remap_held: [None; KeyCode::COUNT],
            target_refcount: [0; KeyCode::COUNT],
            macros: vec![],
            layers: vec![],
            macro_runner: MacroRunner::new(),
            base_disabled: [false; KeyCode::COUNT],
            base_remap_rules: [None; KeyCode::COUNT],
            active_layer: None,
            layer_switch_held: [false; KeyCode::COUNT],
        }
    }

    pub fn enable_key(&mut self, key: KeyCode) {
        // S9: a UI toggle edits the base profile. The current table is
        // the active layer's list union the base set, so enabling a key
        // the layer itself disables leaves it disabled until the layer
        // is edited or deactivated.
        self.base_disabled[key.index()] = false;
        let layer_disables = self.active_layer.as_ref().map_or(false, |id| {
            self.layers.iter().any(|l| {
                &l.id == id && l.disabled_keys.iter().any(|k| k == key.as_str())
            })
        });
        self.disabled[key.index()] = layer_disables;
    }

    pub fn disable_key(&mut self, key: KeyCode) {
        // S9: a UI toggle edits the base profile. The current table
        // keeps the key enabled while the active layer remaps it (the
        // layer's remap wins for that position); the disable takes
        // effect when the layer deactivates.
        self.base_disabled[key.index()] = true;
        self.disabled[key.index()] = !self.active_layer_remaps(key);
    }

    /// Emergency recovery: clear Cat Lock and the disabled set, cancel
    /// any running macro, deactivate the active layer, and release every
    /// held remap target (S8 §5/§6). Returns the flush actions for the
    /// caller to execute — the engine never injects itself. Idempotent
    /// via `flush_remaps` / `macro_cancel` / `deactivate_layer`.
    pub fn enable_all(&mut self) -> Vec<InjectAction> {
        let mut flushed = self.macro_cancel();
        flushed.append(&mut self.deactivate_layer());
        flushed.append(&mut self.flush_remaps());
        self.cat_lock = false;
        self.disabled = [false; KeyCode::COUNT];
        self.base_disabled = [false; KeyCode::COUNT];
        flushed
    }

    /// S8: install a new rule set. Flushes held remaps first (their ups
    /// are returned for the caller to execute), then installs the rules.
    /// The new rules apply to the *next* press only, never retroactively
    /// to a held key (D6). A self-map is ignored defensively; real
    /// validation lives in the command layer (V1–V5).
    ///
    /// S9: the rules are the profile's *base* keymap. An active layer is
    /// deactivated first (flush included) — editing the rules resets the
    /// keymap to the base, by design.
    pub fn set_remap_rules(&mut self, rules: &[(KeyCode, KeyCode)]) -> Vec<InjectAction> {
        let mut flushed = self.deactivate_layer();
        flushed.append(&mut self.flush_remaps());
        self.base_remap_rules = [None; KeyCode::COUNT];
        for &(from, to) in rules {
            if from != to {
                self.base_remap_rules[from.index()] = Some(to);
            }
        }
        self.remap_rules = self.base_remap_rules;
        flushed
    }

    /// S8: current rule set, for tests and diagnostics.
    pub fn remap_rules(&self) -> Vec<(KeyCode, KeyCode)> {
        KeyCode::ALL
            .iter()
            .copied()
            .filter_map(|from| self.remap_rules[from.index()].map(|to| (from, to)))
            .collect()
    }

    /// S8: release every held remap target. For each held source, the
    /// refcount drops and a target up is injected only when its refcount
    /// reaches 0. Idempotent: a second flush injects nothing.
    pub fn flush_remaps(&mut self) -> Vec<InjectAction> {
        let mut actions = Vec::new();
        for source in KeyCode::ALL {
            if let Some(target) = self.remap_held[source.index()].take() {
                let t = target.index();
                self.target_refcount[t] = self.target_refcount[t].saturating_sub(1);
                if self.target_refcount[t] == 0 {
                    actions.push(InjectAction::Up(target));
                }
            }
        }
        actions
    }

    /// S8 fail-open rollback (ADR §2, D3): the caller tried to execute
    /// the remap down for `source` and `SendInput` failed, so the
    /// physical key is forwarded instead. Undo the optimistic held
    /// state recorded by `process` and mark the source as forwarded so
    /// its matching up forwards too.
    pub fn cancel_held_remap(&mut self, source: KeyCode) {
        if let Some(target) = self.remap_held[source.index()].take() {
            let t = target.index();
            self.target_refcount[t] = self.target_refcount[t].saturating_sub(1);
        }
        self.forwarded_down[source.index()] = true;
    }

    pub fn set_cat_lock(&mut self, locked: bool) {
        self.cat_lock = locked;
    }

    pub fn is_cat_locked(&self) -> bool {
        self.cat_lock
    }

    pub fn is_enabled(&self, key: KeyCode) -> bool {
        if self.cat_lock {
            return false;
        }
        !self.disabled[key.index()]
    }

    /// S9: install a profile's disabled set (the base keymap). An active
    /// layer is deactivated first — its flush is returned — so editing
    /// the base keymap resets the keymap to the base, by design.
    pub fn set_disabled_keys(&mut self, keys: &[KeyCode]) -> Vec<InjectAction> {
        let mut flushed = self.deactivate_layer();
        flushed.append(&mut self.flush_remaps());
        self.base_disabled = [false; KeyCode::COUNT];
        for key in keys {
            self.base_disabled[key.index()] = true;
        }
        self.disabled = self.base_disabled;
        flushed
    }

    pub fn disabled_keys(&self) -> Vec<KeyCode> {
        KeyCode::ALL
            .iter()
            .copied()
            .filter(|k| self.disabled[k.index()])
            .collect()
    }

    // --- S9: macros and layers -------------------------------------------

    /// S9: install a profile's macros. A running macro is cancelled
    /// first (its held keys are released); the new definitions apply to
    /// the next trigger press only.
    pub fn set_macros(&mut self, defs: &[MacroDef]) -> Vec<InjectAction> {
        let flushed = self.macro_cancel();
        self.macros = defs.to_vec();
        flushed
    }

    /// S9: install a profile's layers. An active layer is deactivated
    /// first (flush included); the new definitions apply to the next
    /// switch press only.
    pub fn set_layers(&mut self, defs: &[LayerDef]) -> Vec<InjectAction> {
        let mut flushed = self.deactivate_layer();
        flushed.append(&mut self.flush_remaps());
        self.layers = defs.to_vec();
        flushed
    }

    /// S9: the id of the currently active layer, if any (for the UI).
    pub fn active_layer_id(&self) -> Option<String> {
        self.active_layer.clone()
    }

    /// S9: is a macro running right now?
    pub fn macro_running(&self) -> bool {
        self.macro_runner.is_running()
    }

    /// S9: advance the running macro to `now_ms` and return the steps
    /// due. The caller executes the returned actions with `SendInput`
    /// (same path as remaps) and aborts the run if an injection fails.
    pub fn macro_tick(&mut self, now_ms: u64) -> Vec<InjectAction> {
        self.macro_runner.tick(now_ms)
    }

    /// S9: when the next macro step becomes due, if a run is in
    /// progress. The tick thread sleeps until then (at most 25ms).
    pub fn macro_next_due_ms(&self) -> Option<u64> {
        self.macro_runner.next_due_ms()
    }

    /// S9: abort the running macro and release its held keys, in
    /// reverse down order. Idempotent.
    pub fn macro_cancel(&mut self) -> Vec<InjectAction> {
        self.macro_runner.cancel()
    }

    /// S9: deactivate the active layer and restore the base keymap.
    /// Held remap targets are flushed first (their ups are included in
    /// the returned actions) — a key held across a layer switch releases
    /// what its down injected, never the new layer's mapping. Idempotent.
    pub fn deactivate_layer(&mut self) -> Vec<InjectAction> {
        if self.active_layer.is_none() {
            return vec![];
        }
        let flushed = self.flush_remaps();
        self.active_layer = None;
        self.layer_switch_held = [false; KeyCode::COUNT];
        self.disabled = self.base_disabled;
        self.remap_rules = self.base_remap_rules;
        flushed
    }

    /// S9: activate `layer` (its tables replace the base keymap) or
    /// deactivate it. The caller passes the layer definition; the flush
    /// of the previously held remaps is returned first. The current
    /// disabled table is the layer's list union the base disabled set,
    /// *except* for keys the layer remaps: a layer remap re-enables its
    /// source (QMK semantics — the layer replaces the base keymap for
    /// that position, so a base-disabled key can type again in the
    /// layer). Single-key toggles made while a layer is active edit the
    /// base and stay visible for keys the layer does not mention.
    fn activate_layer(&mut self, layer: &LayerDef) -> Vec<InjectAction> {
        let flushed = self.flush_remaps();
        self.active_layer = Some(layer.id.clone());
        self.disabled = [false; KeyCode::COUNT];
        for id in &layer.disabled_keys {
            if let Some(key) = KeyCode::from_id(id) {
                self.disabled[key.index()] = true;
            }
        }
        self.remap_rules = [None; KeyCode::COUNT];
        for rule in &layer.remappings {
            if let (Some(from), Some(to)) =
                (KeyCode::from_id(&rule.from), KeyCode::from_id(&rule.to))
            {
                if from != to {
                    self.remap_rules[from.index()] = Some(to);
                }
            }
        }
        for key in KeyCode::ALL {
            if self.base_disabled[key.index()] && self.remap_rules[key.index()].is_none() {
                self.disabled[key.index()] = true;
            }
        }
        flushed
    }

    /// S9: does the active layer remap `key`? A remapped source stays
    /// enabled in the current table even when the base disables it.
    fn active_layer_remaps(&self, key: KeyCode) -> bool {
        let Some(id) = self.active_layer.as_ref() else {
            return false;
        };
        self.layers.iter().any(|l| {
            &l.id == id
                && l
                    .remappings
                    .iter()
                    .any(|r| r.from == key.as_str())
        })
    }

    /// Native pressed snapshot for UI resync (HF-04): the keys the hook
    /// currently considers physically held, independent of what the UI was
    /// told through the (lossy) event channel.
    pub fn pressed_keys(&self) -> Vec<KeyCode> {
        KeyCode::ALL
            .iter()
            .copied()
            .filter(|k| self.physical_down[k.index()])
            .collect()
    }

    /// Sequence number of the last processed hook event (HF-04).
    pub fn event_sequence(&self) -> u64 {
        self.event_seq
    }

    fn emergency_held(&self) -> bool {
        let ctrl = self.physical_down[KeyCode::ControlLeft.index()]
            || self.physical_down[KeyCode::ControlRight.index()];
        let shift = self.physical_down[KeyCode::ShiftLeft.index()]
            || self.physical_down[KeyCode::ShiftRight.index()];
        let f12 = self.physical_down[KeyCode::F12.index()];
        ctrl && shift && f12
    }

    pub fn process_event(&mut self, key: KeyCode, is_up: bool, now_ms: u64) -> EngineResult {
        self.process_event_injected(key, is_up, false, now_ms)
    }

    /// S8 (ADR §1): `injected` is true when the hook event carries
    /// `LLKHF_INJECTED` / `LLKHF_LOWER_IL_INJECTED`. Injected events take
    /// a separate lane: they never remap, never set `physical_down`,
    /// never glow, never count toward the emergency chord — but they
    /// still respect disable / Cat Lock, so v0.1 behaviour (an on-screen
    /// keyboard sending a disabled key is blocked) is preserved. No
    /// per-key injected state is kept: a stray injected up for a key
    /// that is not down is harmless to forward.
    pub fn process_event_injected(
        &mut self,
        key: KeyCode,
        is_up: bool,
        injected: bool,
        now_ms: u64,
    ) -> EngineResult {
        if injected {
            return self.process_injected(key, is_up);
        }
        let i = key.index();
        let phase = if is_up {
            KeyPhase::Up
        } else if self.physical_down[i] {
            KeyPhase::Repeat
        } else {
            KeyPhase::Down
        };
        self.process(key, phase, now_ms)
    }

    fn process_injected(&mut self, key: KeyCode, _is_up: bool) -> EngineResult {
        // Deliberately no event_seq bump and no physical_down mutation:
        // injected events are invisible to the UI ordering machinery.
        let decision = if self.is_enabled(key) {
            HookDecision::Forward
        } else {
            HookDecision::Consume
        };
        EngineResult {
            decision,
            emergency: false,
            ui: None,
            inject: vec![],
            remapped_source: None,
        }
    }

    pub fn process(&mut self, key: KeyCode, phase: KeyPhase, now_ms: u64) -> EngineResult {
        let i = key.index();
        self.event_seq = self.event_seq.wrapping_add(1);
        match phase {
            KeyPhase::Down | KeyPhase::Repeat => {
                let first = !self.physical_down[i];
                self.physical_down[i] = true;
                let ui = if first {
                    Some((key, UiPulse::Down))
                } else {
                    None
                };

                // S8 (D7): emergency is evaluated from physical state
                // only, before any remap decision, and it always wins.
                // The chord-completing key itself is forwarded (no remap
                // lookup on this event); the flush releases held targets.
                if self.emergency_held() {
                    let inject = self.enable_all();
                    return EngineResult {
                        decision: HookDecision::Forward,
                        emergency: true,
                        ui,
                        inject,
                        remapped_source: None,
                    };
                }

                // S9: macro cancel — the cancel key (default Escape)
                // stops a running macro on its first down. It is
                // consumed, never forwarded, and works even if the
                // profile disables it: the cancel check runs before the
                // disable branch below.
                if first && self.macro_runner.is_running() {
                    if let Some(cancel) = self.macro_runner.cancel_key() {
                        if key == cancel {
                            let inject = self.macro_cancel();
                            return EngineResult {
                                decision: HookDecision::Consume,
                                emergency: false,
                                ui,
                                inject,
                                remapped_source: None,
                            };
                        }
                    }
                }

                // S9: layer switch — a switch key's first down is
                // consumed and activates (hold) or toggles (toggle) its
                // layer; a repeat consumes and injects nothing. The
                // switch key itself never reaches the OS.
                if let Some(layer) = self
                    .layers
                    .iter()
                    .find(|l| {
                        KeyCode::from_id(&l.switch_key).map_or(false, |k| k == key)
                    })
                    .cloned()
                {
                    if first {
                        let inject = match layer.mode {
                            LayerMode::Hold => {
                                self.layer_switch_held[i] = true;
                                self.activate_layer(&layer)
                            }
                            LayerMode::Toggle => {
                                if self.active_layer.as_deref() == Some(layer.id.as_str()) {
                                    self.deactivate_layer()
                                } else {
                                    self.activate_layer(&layer)
                                }
                            }
                        };
                        return EngineResult {
                            decision: HookDecision::Consume,
                            emergency: false,
                            ui,
                            inject,
                            remapped_source: None,
                        };
                    }
                    return EngineResult {
                        decision: HookDecision::Consume,
                        emergency: false,
                        ui: None,
                        inject: vec![],
                        remapped_source: None,
                    };
                }

                // S9: macro trigger — a trigger's first down starts the
                // macro and is consumed. A trigger pressed while a run
                // is in progress is consumed and ignored (the runner
                // refuses a second start, so a run never restarts
                // mid-flight). Other physical keys keep their normal
                // path while a macro runs (interleaving is allowed). A
                // disabled or Cat Locked trigger falls through to the
                // normal consume path — validation forbids a disabled
                // trigger, so this is defensive only.
                if first {
                    let trigger_def = self
                        .macros
                        .iter()
                        .find(|m| {
                            KeyCode::from_id(&m.trigger).map_or(false, |k| k == key)
                        })
                        .cloned();
                    if let Some(def) = trigger_def {
                        if self.macro_runner.is_running() {
                            return EngineResult {
                                decision: HookDecision::Consume,
                                emergency: false,
                                ui,
                                inject: vec![],
                                remapped_source: None,
                            };
                        }
                        if self.is_enabled(key) {
                            let inject = self.macro_runner.start(&def, now_ms);
                            return EngineResult {
                                decision: HookDecision::Consume,
                                emergency: false,
                                ui,
                                inject,
                                remapped_source: None,
                            };
                        }
                    }
                }

                // S8 remap (physical first down only, D5): snapshot the
                // mapping at down time and consume the source. The target
                // down is injected only when its refcount transitions
                // 0 -> 1 — mirroring the up rule (up injected only at
                // 1 -> 0): a second holder of an already-down target
                // injects nothing, so downs and ups stay paired. A
                // disabled or Cat Locked source falls through to the
                // normal consume path below.
                if let Some(target) = self.remap_rules[i] {
                    if first {
                        if self.is_enabled(key) {
                            let t = target.index();
                            let inject = if self.target_refcount[t] == 0 {
                                vec![InjectAction::Down(target)]
                            } else {
                                vec![]
                            };
                            self.remap_held[i] = Some(target);
                            self.target_refcount[t] += 1;
                            return EngineResult {
                                decision: HookDecision::Consume,
                                emergency: false,
                                ui,
                                inject,
                                remapped_source: Some(key),
                            };
                        }
                    } else {
                        // Repeat of a remapped source: consume, inject
                        // nothing. This also covers a rule flushed
                        // mid-hold: the repeat must not inject the new
                        // target — new rules apply to the next press
                        // only (D6).
                        return EngineResult {
                            decision: HookDecision::Consume,
                            emergency: false,
                            ui: None,
                            inject: vec![],
                            remapped_source: None,
                        };
                    }
                }

                if self.is_enabled(key) {
                    self.forwarded_down[i] = true;
                    EngineResult {
                        decision: HookDecision::Forward,
                        emergency: false,
                        ui,
                        inject: vec![],
                        remapped_source: None,
                    }
                } else {
                    EngineResult {
                        decision: HookDecision::Consume,
                        emergency: false,
                        ui,
                        inject: vec![],
                        remapped_source: None,
                    }
                }
            }
            KeyPhase::Up => {
                let was_physical = self.physical_down[i];
                let was_forwarded = self.forwarded_down[i];
                self.physical_down[i] = false;
                self.forwarded_down[i] = false;

                // S9: layer switch release — a hold-mode switch key's
                // up deactivates its layer (flush included); a
                // toggle-mode up, or a hold-mode up whose layer was
                // already deactivated (profile switch, emergency), is
                // just consumed. The switch key's up is consumed either
                // way: the OS never saw the down.
                if self.layer_switch_held[i] {
                    self.layer_switch_held[i] = false;
                    let inject = self.deactivate_layer();
                    return EngineResult {
                        decision: HookDecision::Consume,
                        emergency: false,
                        ui: Some((key, UiPulse::Up)),
                        inject,
                        remapped_source: None,
                    };
                }
                if self.layers.iter().any(|l| {
                    KeyCode::from_id(&l.switch_key).map_or(false, |k| k == key)
                }) {
                    return EngineResult {
                        decision: HookDecision::Consume,
                        emergency: false,
                        ui: Some((key, UiPulse::Up)),
                        inject: vec![],
                        remapped_source: None,
                    };
                }

                // S8 (D6): a held key always releases what its down
                // injected — the down-time snapshot, never the current
                // rules. The source up is consumed (the OS never saw
                // the source down); the injected target up is what the
                // OS receives. Shared targets release only when the
                // last holder lets go.
                if let Some(target) = self.remap_held[i].take() {
                    let t = target.index();
                    self.target_refcount[t] = self.target_refcount[t].saturating_sub(1);
                    let inject = if self.target_refcount[t] == 0 {
                        vec![InjectAction::Up(target)]
                    } else {
                        vec![]
                    };
                    return EngineResult {
                        decision: HookDecision::Consume,
                        emergency: false,
                        ui: Some((key, UiPulse::Up)),
                        inject,
                        remapped_source: None,
                    };
                }

                // If we never observed the matching down (hook started mid-hold,
                // missed event, etc.) forward the up so Windows cannot stick a
                // modifier. Only swallow an up when we swallowed the down.
                let decision = if was_forwarded || !was_physical {
                    HookDecision::Forward
                } else {
                    HookDecision::Consume
                };

                EngineResult {
                    decision,
                    emergency: false,
                    ui: Some((key, UiPulse::Up)),
                    inject: vec![],
                    remapped_source: None,
                }
            }
        }
    }
}

pub trait KeyboardController: Send + Sync {
    fn enable_key(&self, key: KeyCode);
    fn disable_key(&self, key: KeyCode);
    /// Emergency recovery; returns the remap flush actions (S8) for the
    /// caller to execute via `SendInput`. Empty when nothing was held.
    /// S9: also cancels a running macro and deactivates the active
    /// layer; their releases are included in the returned actions.
    fn enable_all(&self) -> Vec<InjectAction>;
    fn is_enabled(&self, key: KeyCode) -> bool;
    /// S9: install a profile's disabled set. Returns the layer
    /// deactivation flush, if a layer was active.
    fn set_disabled_keys(&self, keys: &[KeyCode]) -> Vec<InjectAction>;
    fn disabled_keys(&self) -> Vec<KeyCode>;
    fn set_cat_lock(&self, locked: bool);
    fn is_cat_locked(&self) -> bool;
    /// Native pressed snapshot for UI resync (HF-04).
    fn pressed_keys(&self) -> Vec<KeyCode>;
    /// Sequence of the last processed hook event (HF-04).
    fn event_sequence(&self) -> u64;
    /// S8: install a profile's remap rules (flush first, then install).
    /// Returns the flush actions for the caller to execute.
    fn set_remap_rules(&self, rules: &[(KeyCode, KeyCode)]) -> Vec<InjectAction>;
    /// S8: release every held remap target; returns the ups to inject.
    /// Idempotent.
    fn flush_remaps(&self) -> Vec<InjectAction>;
    /// S9: install a profile's macros (a running macro is cancelled
    /// first; its releases are returned). The new definitions apply to
    /// the next trigger press only.
    fn set_macros(&self, defs: &[MacroDef]) -> Vec<InjectAction>;
    /// S9: install a profile's layers (an active layer is deactivated
    /// first; its flush is returned). The new definitions apply to the
    /// next switch press only.
    fn set_layers(&self, defs: &[LayerDef]) -> Vec<InjectAction>;
    /// S9: advance the running macro to `now_ms` and return the steps
    /// due for the caller to inject. Returns `(still_running,
    /// next_due_ms)`: the tick thread sleeps until the next due step
    /// (at most 25ms) and stops polling when no run is in progress.
    fn macro_tick_execute(&self, now_ms: u64) -> (bool, Option<u64>);
    /// S9: the id of the currently active layer, if any.
    fn active_layer_id(&self) -> Option<String>;
    /// S9: abort the running macro; returns the held-key releases.
    /// Idempotent.
    fn cancel_macro(&self) -> Vec<InjectAction>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(engine: &mut FilterEngine, key: KeyCode) -> EngineResult {
        engine.process(key, KeyPhase::Down, 0)
    }

    fn up(engine: &mut FilterEngine, key: KeyCode) -> EngineResult {
        engine.process(key, KeyPhase::Up, 0)
    }

    #[test]
    fn disabled_key_is_consumed() {
        let mut engine = FilterEngine::new();
        engine.disable_key(KeyCode::KeyA);
        let result = down(&mut engine, KeyCode::KeyA);
        assert_eq!(result.decision, HookDecision::Consume);
        let result = up(&mut engine, KeyCode::KeyA);
        assert_eq!(result.decision, HookDecision::Consume);
    }

    #[test]
    fn enabled_key_is_forwarded() {
        let mut engine = FilterEngine::new();
        let result = down(&mut engine, KeyCode::KeyA);
        assert_eq!(result.decision, HookDecision::Forward);
        let result = up(&mut engine, KeyCode::KeyA);
        assert_eq!(result.decision, HookDecision::Forward);
    }

    #[test]
    fn disable_while_held_still_forwards_matching_up() {
        let mut engine = FilterEngine::new();
        assert_eq!(
            down(&mut engine, KeyCode::ShiftLeft).decision,
            HookDecision::Forward
        );
        engine.disable_key(KeyCode::ShiftLeft);
        let repeat = engine.process(KeyCode::ShiftLeft, KeyPhase::Repeat, 0);
        assert_eq!(repeat.decision, HookDecision::Consume);
        let result = up(&mut engine, KeyCode::ShiftLeft);
        assert_eq!(result.decision, HookDecision::Forward);
    }

    #[test]
    fn enable_all_clears_disabled_set() {
        let mut engine = FilterEngine::new();
        engine.disable_key(KeyCode::KeyA);
        engine.disable_key(KeyCode::CapsLock);
        engine.enable_all();
        assert!(engine.is_enabled(KeyCode::KeyA));
        assert!(engine.disabled_keys().is_empty());
    }

    #[test]
    fn emergency_combo_enables_all_even_if_f12_disabled() {
        let mut engine = FilterEngine::new();
        engine.disable_key(KeyCode::F12);
        engine.disable_key(KeyCode::KeyA);
        down(&mut engine, KeyCode::ControlLeft);
        down(&mut engine, KeyCode::ShiftLeft);
        let result = down(&mut engine, KeyCode::F12);
        assert!(result.emergency);
        assert_eq!(result.decision, HookDecision::Forward);
        assert!(engine.is_enabled(KeyCode::KeyA));
        assert!(engine.is_enabled(KeyCode::F12));
    }

    #[test]
    fn unknown_up_is_forwarded_to_avoid_stuck_keys() {
        let mut engine = FilterEngine::new();
        let result = up(&mut engine, KeyCode::AltLeft);
        assert_eq!(result.decision, HookDecision::Forward);
    }

    #[test]
    fn set_disabled_keys_replaces_set() {
        let mut engine = FilterEngine::new();
        engine.disable_key(KeyCode::KeyA);
        engine.set_disabled_keys(&[KeyCode::MetaLeft, KeyCode::MetaRight]);
        assert!(engine.is_enabled(KeyCode::KeyA));
        assert!(!engine.is_enabled(KeyCode::MetaLeft));
        assert!(!engine.is_enabled(KeyCode::MetaRight));
    }

    #[test]
    fn ui_pulse_only_on_first_down_and_up() {
        let mut engine = FilterEngine::new();
        let first = down(&mut engine, KeyCode::KeyW);
        assert_eq!(first.ui, Some((KeyCode::KeyW, UiPulse::Down)));
        let repeat = engine.process(KeyCode::KeyW, KeyPhase::Repeat, 0);
        assert_eq!(repeat.ui, None);
        let release = up(&mut engine, KeyCode::KeyW);
        assert_eq!(release.ui, Some((KeyCode::KeyW, UiPulse::Up)));
    }

    #[test]
    fn cat_lock_blocks_every_key() {
        let mut engine = FilterEngine::new();
        engine.set_cat_lock(true);
        assert!(!engine.is_enabled(KeyCode::KeyA));
        assert!(!engine.is_enabled(KeyCode::Space));
        assert_eq!(down(&mut engine, KeyCode::KeyA).decision, HookDecision::Consume);
        assert_eq!(up(&mut engine, KeyCode::KeyA).decision, HookDecision::Consume);
    }

    #[test]
    fn cat_lock_does_not_stick_a_held_modifier() {
        let mut engine = FilterEngine::new();
        assert_eq!(
            down(&mut engine, KeyCode::ShiftLeft).decision,
            HookDecision::Forward
        );
        engine.set_cat_lock(true);
        assert_eq!(
            up(&mut engine, KeyCode::ShiftLeft).decision,
            HookDecision::Forward
        );
    }

    #[test]
    fn emergency_unlock_clears_cat_lock() {
        let mut engine = FilterEngine::new();
        engine.set_cat_lock(true);
        down(&mut engine, KeyCode::ControlLeft);
        down(&mut engine, KeyCode::ShiftLeft);
        let result = down(&mut engine, KeyCode::F12);
        assert!(result.emergency);
        assert!(!engine.is_cat_locked());
        assert_eq!(result.decision, HookDecision::Forward);
        assert!(engine.is_enabled(KeyCode::KeyA));
    }

    /// HF-04: the native pressed snapshot tracks physical holds so the UI
    /// can resync after a dropped key-up instead of guessing with a timer.
    #[test]
    fn pressed_snapshot_tracks_physical_hold() {
        let mut engine = FilterEngine::new();
        assert!(engine.pressed_keys().is_empty());
        down(&mut engine, KeyCode::KeyA);
        down(&mut engine, KeyCode::ShiftLeft);
        let pressed = engine.pressed_keys();
        assert!(pressed.contains(&KeyCode::KeyA));
        assert!(pressed.contains(&KeyCode::ShiftLeft));
        // Repeat does not change the snapshot.
        engine.process(KeyCode::KeyA, KeyPhase::Repeat, 0);
        assert_eq!(engine.pressed_keys().len(), 2);
        up(&mut engine, KeyCode::KeyA);
        assert_eq!(engine.pressed_keys(), vec![KeyCode::ShiftLeft]);
        up(&mut engine, KeyCode::ShiftLeft);
        assert!(engine.pressed_keys().is_empty());
    }

    /// HF-04: the sequence bumps on every processed event, giving the UI a
    /// cheap "did anything change while I was away" signal.
    #[test]
    fn event_sequence_increases_on_every_event() {
        let mut engine = FilterEngine::new();
        let s0 = engine.event_sequence();
        down(&mut engine, KeyCode::KeyA);
        let s1 = engine.event_sequence();
        assert!(s1 > s0);
        engine.process(KeyCode::KeyA, KeyPhase::Repeat, 0);
        let s2 = engine.event_sequence();
        assert!(s2 > s1);
        up(&mut engine, KeyCode::KeyA);
        assert!(engine.event_sequence() > s2);
    }

    // ---- S8: key remapping (ADR-REMAPPING.md §10) ----

    fn remap_engine(rules: &[(KeyCode, KeyCode)]) -> FilterEngine {
        let mut engine = FilterEngine::new();
        assert!(engine.set_remap_rules(rules).is_empty());
        engine
    }

    #[test]
    fn remap_down_injects_target_once_repeat_nothing_up_injects_up() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        let result = down(&mut engine, KeyCode::KeyA);
        assert_eq!(result.decision, HookDecision::Consume);
        assert_eq!(result.inject, vec![InjectAction::Down(KeyCode::KeyB)]);
        assert_eq!(result.remapped_source, Some(KeyCode::KeyA));
        assert_eq!(result.ui, Some((KeyCode::KeyA, UiPulse::Down)));
        // The physical source still glows; the injected target never does.
        let repeat = engine.process(KeyCode::KeyA, KeyPhase::Repeat, 0);
        assert_eq!(repeat.decision, HookDecision::Consume);
        assert!(repeat.inject.is_empty());
        assert_eq!(repeat.ui, None);
        let release = up(&mut engine, KeyCode::KeyA);
        assert_eq!(release.decision, HookDecision::Consume);
        assert_eq!(release.inject, vec![InjectAction::Up(KeyCode::KeyB)]);
        assert_eq!(release.ui, Some((KeyCode::KeyA, UiPulse::Up)));
        // The unmapped key itself is untouched.
        let plain = down(&mut engine, KeyCode::KeyB);
        assert_eq!(plain.decision, HookDecision::Forward);
        assert!(plain.inject.is_empty());
    }

    #[test]
    fn source_up_after_rule_change_releases_down_time_target() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        down(&mut engine, KeyCode::KeyA);
        // Change the rule mid-hold: flush releases B, never injects C.
        let flushed = engine.set_remap_rules(&[(KeyCode::KeyA, KeyCode::KeyC)]);
        assert_eq!(flushed, vec![InjectAction::Up(KeyCode::KeyB)]);
        // The held key's release injects nothing (already flushed) and
        // never the new target.
        let release = up(&mut engine, KeyCode::KeyA);
        assert!(release.inject.is_empty());
        assert_eq!(release.decision, HookDecision::Consume);
        // The next press uses the new rule.
        let again = down(&mut engine, KeyCode::KeyA);
        assert_eq!(again.inject, vec![InjectAction::Down(KeyCode::KeyC)]);
        let _ = up(&mut engine, KeyCode::KeyA);
    }

    #[test]
    fn shared_target_refcount_releases_on_last_holder() {
        let mut engine = remap_engine(&[
            (KeyCode::KeyA, KeyCode::KeyB),
            (KeyCode::KeyC, KeyCode::KeyB),
        ]);
        down(&mut engine, KeyCode::KeyA);
        let second = down(&mut engine, KeyCode::KeyC);
        // C's down injects nothing: B is already down (refcount 2).
        assert!(second.inject.is_empty());
        assert_eq!(second.decision, HookDecision::Consume);
        let first_release = up(&mut engine, KeyCode::KeyA);
        assert!(first_release.inject.is_empty(), "B stays down for C");
        let last_release = up(&mut engine, KeyCode::KeyC);
        assert_eq!(last_release.inject, vec![InjectAction::Up(KeyCode::KeyB)]);
    }

    #[test]
    fn flush_on_rule_change_is_idempotent() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        down(&mut engine, KeyCode::KeyA);
        let first = engine.set_remap_rules(&[]);
        assert_eq!(first, vec![InjectAction::Up(KeyCode::KeyB)]);
        assert!(engine.flush_remaps().is_empty());
        assert!(engine.set_remap_rules(&[]).is_empty());
        // Rules are gone: the next press forwards the source.
        let again = down(&mut engine, KeyCode::KeyA);
        assert_eq!(again.decision, HookDecision::Forward);
        assert!(again.inject.is_empty());
        let _ = up(&mut engine, KeyCode::KeyA);
    }

    #[test]
    fn emergency_flushes_held_remaps_and_keeps_rules() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        engine.disable_key(KeyCode::KeyC);
        down(&mut engine, KeyCode::KeyA);
        // Emergency chord from physical keys.
        down(&mut engine, KeyCode::ControlLeft);
        down(&mut engine, KeyCode::ShiftLeft);
        let result = down(&mut engine, KeyCode::F12);
        assert!(result.emergency);
        // The flush released B; F12 itself forwarded, not remapped.
        assert_eq!(result.inject, vec![InjectAction::Up(KeyCode::KeyB)]);
        assert_eq!(result.decision, HookDecision::Forward);
        assert!(engine.is_enabled(KeyCode::KeyC));
        // Rules survive the emergency (config, not runtime disable state).
        assert_eq!(
            engine.remap_rules(),
            vec![(KeyCode::KeyA, KeyCode::KeyB)]
        );
        // The still-held A's release injects nothing (already flushed).
        let release = up(&mut engine, KeyCode::KeyA);
        assert!(release.inject.is_empty());
        // Release the chord first: while Ctrl+Shift+F12 are still held, any
        // further down is another emergency press (pre-existing semantics).
        let _ = up(&mut engine, KeyCode::F12);
        let _ = up(&mut engine, KeyCode::ShiftLeft);
        let _ = up(&mut engine, KeyCode::ControlLeft);
        // The next press remaps normally again.
        let again = down(&mut engine, KeyCode::KeyA);
        assert_eq!(again.inject, vec![InjectAction::Down(KeyCode::KeyB)]);
        let _ = up(&mut engine, KeyCode::KeyA);
    }

    #[test]
    fn injected_events_never_remap_or_touch_physical_state() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        let injected_down = engine.process_event_injected(KeyCode::KeyA, false, true, 0);
        assert_eq!(injected_down.decision, HookDecision::Forward);
        assert!(injected_down.inject.is_empty());
        assert_eq!(injected_down.ui, None);
        assert!(!injected_down.emergency);
        assert!(engine.pressed_keys().is_empty());
        let injected_up = engine.process_event_injected(KeyCode::KeyA, true, true, 0);
        assert_eq!(injected_up.decision, HookDecision::Forward);
        assert!(injected_up.inject.is_empty());
        // Injected chord keys never count toward the emergency chord:
        // the physical F12 below must not fire it.
        engine.process_event_injected(KeyCode::ControlLeft, false, true, 0);
        engine.process_event_injected(KeyCode::ShiftLeft, false, true, 0);
        let f12 = down(&mut engine, KeyCode::F12);
        assert!(!f12.emergency);
        // ...but the physical chord still fires on its own.
        down(&mut engine, KeyCode::ControlLeft);
        down(&mut engine, KeyCode::ShiftLeft);
        // F12 is already down from the line above, so this is a repeat:
        // use a fresh engine for the clean chord.
        let mut engine2 = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        down(&mut engine2, KeyCode::ControlLeft);
        down(&mut engine2, KeyCode::ShiftLeft);
        let chord = down(&mut engine2, KeyCode::F12);
        assert!(chord.emergency);
    }

    #[test]
    fn injected_events_still_respect_disable_and_cat_lock() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        engine.disable_key(KeyCode::KeyA);
        let blocked = engine.process_event_injected(KeyCode::KeyA, false, true, 0);
        assert_eq!(blocked.decision, HookDecision::Consume);
        assert!(blocked.inject.is_empty());
        engine.enable_key(KeyCode::KeyA);
        engine.set_cat_lock(true);
        let locked = engine.process_event_injected(KeyCode::KeyA, false, true, 0);
        assert_eq!(locked.decision, HookDecision::Consume);
        // An injected up for a disabled key is consumed too (same
        // enabled-check as a physical down, §1.4).
        let locked_up = engine.process_event_injected(KeyCode::KeyA, true, true, 0);
        assert_eq!(locked_up.decision, HookDecision::Consume);
    }

    #[test]
    fn cat_lock_suppresses_remap_and_releasing_it_restores_remap() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        engine.set_cat_lock(true);
        let locked = down(&mut engine, KeyCode::KeyA);
        assert_eq!(locked.decision, HookDecision::Consume);
        assert!(locked.inject.is_empty());
        let locked_up = up(&mut engine, KeyCode::KeyA);
        assert!(locked_up.inject.is_empty());
        engine.set_cat_lock(false);
        let free = down(&mut engine, KeyCode::KeyA);
        assert_eq!(free.inject, vec![InjectAction::Down(KeyCode::KeyB)]);
        let _ = up(&mut engine, KeyCode::KeyA);
    }

    #[test]
    fn remapped_modifier_alone_injects_clean_pair() {
        let mut engine = remap_engine(&[(KeyCode::ShiftLeft, KeyCode::ControlLeft)]);
        let result = down(&mut engine, KeyCode::ShiftLeft);
        assert_eq!(
            result.inject,
            vec![InjectAction::Down(KeyCode::ControlLeft)]
        );
        assert_eq!(result.decision, HookDecision::Consume);
        let release = up(&mut engine, KeyCode::ShiftLeft);
        assert_eq!(
            release.inject,
            vec![InjectAction::Up(KeyCode::ControlLeft)]
        );
        // No stuck state: a plain key afterwards forwards normally.
        assert_eq!(down(&mut engine, KeyCode::KeyC).decision, HookDecision::Forward);
    }

    #[test]
    fn fail_open_rollback_cancels_held_remap_and_forwards_up() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        let result = down(&mut engine, KeyCode::KeyA);
        assert_eq!(result.remapped_source, Some(KeyCode::KeyA));
        // Simulate the hook callback's SendInput failure: roll back.
        engine.cancel_held_remap(KeyCode::KeyA);
        assert!(engine.flush_remaps().is_empty(), "nothing left to flush");
        // The matching up now forwards (the down was forwarded).
        let release = up(&mut engine, KeyCode::KeyA);
        assert_eq!(release.decision, HookDecision::Forward);
        assert!(release.inject.is_empty());
        // And the next press remaps normally again.
        let again = down(&mut engine, KeyCode::KeyA);
        assert_eq!(again.inject, vec![InjectAction::Down(KeyCode::KeyB)]);
        let _ = up(&mut engine, KeyCode::KeyA);
    }

    #[test]
    fn enable_all_returns_flush_and_clears_disabled() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        engine.disable_key(KeyCode::KeyC);
        down(&mut engine, KeyCode::KeyA);
        let flushed = engine.enable_all();
        assert_eq!(flushed, vec![InjectAction::Up(KeyCode::KeyB)]);
        assert!(engine.is_enabled(KeyCode::KeyC));
        assert!(engine.flush_remaps().is_empty());
        // Rules survive enable_all (they are config).
        assert_eq!(
            engine.remap_rules(),
            vec![(KeyCode::KeyA, KeyCode::KeyB)]
        );
    }

    #[test]
    fn remap_source_disabled_at_down_is_consumed_without_inject() {
        let mut engine = remap_engine(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        engine.disable_key(KeyCode::KeyA);
        let result = down(&mut engine, KeyCode::KeyA);
        assert_eq!(result.decision, HookDecision::Consume);
        assert!(result.inject.is_empty());
        let release = up(&mut engine, KeyCode::KeyA);
        assert_eq!(release.decision, HookDecision::Consume);
        assert!(release.inject.is_empty());
    }

    // --- S9: macros and layers -------------------------------------------

    use crate::profiles::models::{LayerDef, LayerMode, MacroDef, MacroStepDef};

    fn step(key: &str, action: &str, delay_ms: u32) -> MacroStepDef {
        MacroStepDef {
            key: key.into(),
            action: action.into(),
            delay_ms,
        }
    }

    fn ctrl_c_macro() -> MacroDef {
        MacroDef {
            id: "m1".into(),
            name: "Copy".into(),
            trigger: "F9".into(),
            cancel_key: "Escape".into(),
            steps: vec![
                step("ControlLeft", "down", 0),
                step("KeyC", "down", 10),
                step("KeyC", "up", 10),
                step("ControlLeft", "up", 0),
            ],
        }
    }

    fn fn_layer() -> LayerDef {
        LayerDef {
            id: "l1".into(),
            name: "Fn".into(),
            switch_key: "CapsLock".into(),
            mode: LayerMode::Hold,
            disabled_keys: vec!["KeyQ".into()],
            remappings: vec![crate::profiles::models::RemapRule {
                from: "KeyW".into(),
                to: "ArrowUp".into(),
            }],
        }
    }

    fn down_at(engine: &mut FilterEngine, key: KeyCode, now_ms: u64) -> EngineResult {
        engine.process(key, KeyPhase::Down, now_ms)
    }

    fn up_at(engine: &mut FilterEngine, key: KeyCode, now_ms: u64) -> EngineResult {
        engine.process(key, KeyPhase::Up, now_ms)
    }

    #[test]
    fn macro_trigger_starts_run_and_tick_executes_steps() {
        let mut engine = FilterEngine::new();
        engine.set_macros(&[ctrl_c_macro()]);
        let result = down_at(&mut engine, KeyCode::F9, 1000);
        assert_eq!(result.decision, HookDecision::Consume);
        // Zero-delay first step fires at start.
        assert_eq!(
            result.inject,
            vec![InjectAction::Down(KeyCode::ControlLeft)]
        );
        assert!(engine.macro_running());
        assert_eq!(engine.macro_next_due_ms(), Some(1010));
        // Other physical keys keep their normal path mid-run.
        let other = down_at(&mut engine, KeyCode::KeyZ, 1005);
        assert_eq!(other.decision, HookDecision::Forward);
        let _ = up_at(&mut engine, KeyCode::KeyZ, 1005);
        let actions = engine.macro_tick(1010);
        assert_eq!(actions, vec![InjectAction::Down(KeyCode::KeyC)]);
        let actions = engine.macro_tick(1020);
        assert_eq!(
            actions,
            vec![
                InjectAction::Up(KeyCode::KeyC),
                InjectAction::Up(KeyCode::ControlLeft)
            ]
        );
        assert!(!engine.macro_running());
    }

    #[test]
    fn macro_trigger_while_running_is_ignored() {
        let mut engine = FilterEngine::new();
        engine.set_macros(&[ctrl_c_macro()]);
        down_at(&mut engine, KeyCode::F9, 0);
        assert!(engine.macro_running());
        let _ = up_at(&mut engine, KeyCode::F9, 5);
        // Second trigger: consumed, no restart (the run keeps its clock).
        let again = down_at(&mut engine, KeyCode::F9, 5);
        assert_eq!(again.decision, HookDecision::Consume);
        assert!(again.inject.is_empty());
        assert_eq!(engine.macro_next_due_ms(), Some(10));
    }

    #[test]
    fn macro_cancel_key_releases_held_keys() {
        let mut engine = FilterEngine::new();
        engine.set_macros(&[ctrl_c_macro()]);
        down_at(&mut engine, KeyCode::F9, 0);
        engine.macro_tick(10); // Ctrl + C down
        assert!(engine.macro_running());
        let result = down_at(&mut engine, KeyCode::Escape, 20);
        assert_eq!(result.decision, HookDecision::Consume);
        assert_eq!(
            result.inject,
            vec![
                InjectAction::Up(KeyCode::KeyC),
                InjectAction::Up(KeyCode::ControlLeft)
            ]
        );
        assert!(!engine.macro_running());
        // The cancel key's own up is consumed (its down was consumed).
        let release = up_at(&mut engine, KeyCode::Escape, 21);
        assert_eq!(release.decision, HookDecision::Consume);
    }

    #[test]
    fn macro_cancel_key_works_even_when_disabled() {
        let mut engine = FilterEngine::new();
        engine.set_macros(&[ctrl_c_macro()]);
        engine.disable_key(KeyCode::Escape);
        down_at(&mut engine, KeyCode::F9, 0);
        engine.macro_tick(10);
        let result = down_at(&mut engine, KeyCode::Escape, 20);
        assert_eq!(result.decision, HookDecision::Consume);
        assert_eq!(result.inject.len(), 2);
        assert!(!engine.macro_running());
    }

    #[test]
    fn macro_run_time_limit_aborts_and_releases() {
        let mut engine = FilterEngine::new();
        let mut def = ctrl_c_macro();
        def.steps[3].delay_ms = 5_000;
        def.steps[2].delay_ms = 5_000;
        engine.set_macros(&[def]);
        down_at(&mut engine, KeyCode::F9, 0);
        engine.macro_tick(10); // Ctrl + C down
        // Past the 30s cap: the tick aborts the run and releases C, Ctrl.
        let actions = engine.macro_tick(30_001);
        assert_eq!(
            actions,
            vec![
                InjectAction::Up(KeyCode::KeyC),
                InjectAction::Up(KeyCode::ControlLeft)
            ]
        );
        assert!(!engine.macro_running());
    }

    #[test]
    fn emergency_cancels_macro_and_layer_and_flushes_remap() {
        let mut engine = FilterEngine::new();
        engine.set_macros(&[ctrl_c_macro()]);
        engine.set_layers(&[fn_layer()]);
        engine.set_remap_rules(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        down_at(&mut engine, KeyCode::F9, 0);
        engine.macro_tick(10);
        down_at(&mut engine, KeyCode::KeyA, 11); // remap held: B down
        // Hold the switch: the layer activates and flushes the held
        // remap target first.
        let switched = down_at(&mut engine, KeyCode::CapsLock, 20);
        assert_eq!(switched.inject, vec![InjectAction::Up(KeyCode::KeyB)]);
        assert_eq!(engine.active_layer_id(), Some("l1".into()));
        // Ctrl+Shift+F12: emergency wins, the macro is cancelled and
        // the layer deactivated.
        down_at(&mut engine, KeyCode::ControlLeft, 30);
        down_at(&mut engine, KeyCode::ShiftLeft, 30);
        let result = down_at(&mut engine, KeyCode::F12, 30);
        assert!(result.emergency);
        assert!(!engine.macro_running());
        assert_eq!(engine.active_layer_id(), None);
        // The flush carries the macro's held keys (the remap target was
        // already released by the layer switch).
        assert!(result.inject.contains(&InjectAction::Up(KeyCode::KeyC)));
        assert!(result
            .inject
            .contains(&InjectAction::Up(KeyCode::ControlLeft)));
        assert_eq!(engine.remap_rules().len(), 1); // rules survive
        assert_eq!(engine.disabled_keys(), vec![] as Vec<KeyCode>);
    }

    #[test]
    fn layer_hold_switches_keymap_and_restores_on_up() {
        let mut engine = FilterEngine::new();
        engine.set_layers(&[fn_layer()]);
        engine.set_remap_rules(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        // Base: A -> B, Q enabled, W forwards.
        assert_eq!(
            down(&mut engine, KeyCode::KeyA).inject,
            vec![InjectAction::Down(KeyCode::KeyB)]
        );
        let _ = up(&mut engine, KeyCode::KeyA);
        // Hold the switch: the layer's tables replace the base.
        let switch = down(&mut engine, KeyCode::CapsLock);
        assert_eq!(switch.decision, HookDecision::Consume);
        assert_eq!(engine.active_layer_id(), Some("l1".into()));
        // Q is disabled in the layer; W remaps to ArrowUp; A is not
        // remapped in the layer, so it forwards.
        assert_eq!(
            down(&mut engine, KeyCode::KeyQ).decision,
            HookDecision::Consume
        );
        let _ = up(&mut engine, KeyCode::KeyQ);
        let w = down(&mut engine, KeyCode::KeyW);
        assert_eq!(
            w.inject,
            vec![InjectAction::Down(KeyCode::ArrowUp)]
        );
        let _ = up(&mut engine, KeyCode::KeyW);
        assert_eq!(
            down(&mut engine, KeyCode::KeyA).decision,
            HookDecision::Forward
        );
        let _ = up(&mut engine, KeyCode::KeyA);
        // Release the switch: base tables return.
        let release = up(&mut engine, KeyCode::CapsLock);
        assert_eq!(release.decision, HookDecision::Consume);
        assert_eq!(engine.active_layer_id(), None);
        assert_eq!(
            down(&mut engine, KeyCode::KeyA).inject,
            vec![InjectAction::Down(KeyCode::KeyB)]
        );
        let _ = up(&mut engine, KeyCode::KeyA);
    }

    #[test]
    fn layer_switch_flushes_held_remap_first() {
        let mut engine = FilterEngine::new();
        engine.set_layers(&[fn_layer()]);
        engine.set_remap_rules(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        down(&mut engine, KeyCode::KeyA); // B down, held
        let switch = down(&mut engine, KeyCode::CapsLock);
        // The switch flushes the held target before the layer applies.
        assert_eq!(switch.inject, vec![InjectAction::Up(KeyCode::KeyB)]);
        assert_eq!(engine.active_layer_id(), Some("l1".into()));
        let _ = up(&mut engine, KeyCode::CapsLock);
        let _ = up(&mut engine, KeyCode::KeyA); // no double release
        assert!(engine.flush_remaps().is_empty());
    }

    #[test]
    fn layer_toggle_switches_on_each_press() {
        let mut engine = FilterEngine::new();
        let mut layer = fn_layer();
        layer.mode = LayerMode::Toggle;
        engine.set_layers(&[layer]);
        let first = down(&mut engine, KeyCode::CapsLock);
        assert_eq!(first.decision, HookDecision::Consume);
        assert_eq!(engine.active_layer_id(), Some("l1".into()));
        // The toggle up is consumed but does not deactivate.
        let release = up(&mut engine, KeyCode::CapsLock);
        assert_eq!(release.decision, HookDecision::Consume);
        assert_eq!(engine.active_layer_id(), Some("l1".into()));
        // Second press deactivates.
        let second = down(&mut engine, KeyCode::CapsLock);
        assert_eq!(second.decision, HookDecision::Consume);
        assert_eq!(engine.active_layer_id(), None);
        let _ = up(&mut engine, KeyCode::CapsLock);
    }

    #[test]
    fn set_remap_rules_deactivates_layer_and_resets_to_base() {
        let mut engine = FilterEngine::new();
        engine.set_layers(&[fn_layer()]);
        down(&mut engine, KeyCode::CapsLock);
        assert_eq!(engine.active_layer_id(), Some("l1".into()));
        let flushed = engine.set_remap_rules(&[(KeyCode::KeyA, KeyCode::KeyB)]);
        assert!(flushed.is_empty());
        assert_eq!(engine.active_layer_id(), None);
        // The new base rules are live; the layer's W remap is gone.
        let w = down(&mut engine, KeyCode::KeyW);
        assert_eq!(w.decision, HookDecision::Forward);
        let _ = up(&mut engine, KeyCode::KeyW);
        let _ = up(&mut engine, KeyCode::CapsLock);
    }

    #[test]
    fn single_key_toggle_edits_base_while_layer_active() {
        let mut engine = FilterEngine::new();
        engine.set_layers(&[fn_layer()]);
        // Disable Z in the base while the layer is active: it stays
        // disabled after the layer deactivates.
        down(&mut engine, KeyCode::CapsLock);
        engine.disable_key(KeyCode::KeyZ);
        assert_eq!(
            down(&mut engine, KeyCode::KeyZ).decision,
            HookDecision::Consume
        );
        let _ = up(&mut engine, KeyCode::KeyZ);
        let _ = up(&mut engine, KeyCode::CapsLock);
        assert_eq!(
            down(&mut engine, KeyCode::KeyZ).decision,
            HookDecision::Consume
        );
        let _ = up(&mut engine, KeyCode::KeyZ);
        // Enabling a key the layer itself disables leaves it disabled
        // while the layer is active.
        down(&mut engine, KeyCode::CapsLock);
        engine.enable_key(KeyCode::KeyQ);
        assert_eq!(
            down(&mut engine, KeyCode::KeyQ).decision,
            HookDecision::Consume
        );
        let _ = up(&mut engine, KeyCode::KeyQ);
        let _ = up(&mut engine, KeyCode::CapsLock);
    }

    #[test]
    fn layer_remap_reenables_base_disabled_source() {
        // QMK semantics: the layer replaces the base keymap for the
        // positions it mentions, so a base-disabled key can type again
        // in the layer when the layer remaps it.
        let mut engine = FilterEngine::new();
        engine.set_layers(&[fn_layer()]); // W -> ArrowUp in the layer
        engine.set_disabled_keys(&[KeyCode::KeyW]);
        // Base: W is dead.
        assert_eq!(
            down(&mut engine, KeyCode::KeyW).decision,
            HookDecision::Consume
        );
        let _ = up(&mut engine, KeyCode::KeyW);
        // Layer: W remaps to ArrowUp.
        down(&mut engine, KeyCode::CapsLock);
        let w = down(&mut engine, KeyCode::KeyW);
        assert_eq!(w.inject, vec![InjectAction::Down(KeyCode::ArrowUp)]);
        let _ = up(&mut engine, KeyCode::KeyW);
        let _ = up(&mut engine, KeyCode::CapsLock);
        // Base again: W is dead.
        assert_eq!(
            down(&mut engine, KeyCode::KeyW).decision,
            HookDecision::Consume
        );
        let _ = up(&mut engine, KeyCode::KeyW);
    }
}
