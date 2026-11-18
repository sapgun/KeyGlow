use super::keycodes::KeyCode;

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
        }
    }

    pub fn enable_key(&mut self, key: KeyCode) {
        self.disabled[key.index()] = false;
    }

    pub fn disable_key(&mut self, key: KeyCode) {
        self.disabled[key.index()] = true;
    }

    /// Emergency recovery: clear Cat Lock and the disabled set, and
    /// release every held remap target (S8 §5/§6). Returns the flush
    /// actions for the caller to execute — the engine never injects
    /// itself. Idempotent via `flush_remaps`.
    pub fn enable_all(&mut self) -> Vec<InjectAction> {
        let flushed = self.flush_remaps();
        self.cat_lock = false;
        self.disabled = [false; KeyCode::COUNT];
        flushed
    }

    /// S8: install a new rule set. Flushes held remaps first (their ups
    /// are returned for the caller to execute), then installs the rules.
    /// The new rules apply to the *next* press only, never retroactively
    /// to a held key (D6). A self-map is ignored defensively; real
    /// validation lives in the command layer (V1–V5).
    pub fn set_remap_rules(&mut self, rules: &[(KeyCode, KeyCode)]) -> Vec<InjectAction> {
        let flushed = self.flush_remaps();
        self.remap_rules = [None; KeyCode::COUNT];
        for &(from, to) in rules {
            if from != to {
                self.remap_rules[from.index()] = Some(to);
            }
        }
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

    pub fn set_disabled_keys(&mut self, keys: &[KeyCode]) {
        self.disabled = [false; KeyCode::COUNT];
        for key in keys {
            self.disabled[key.index()] = true;
        }
    }

    pub fn disabled_keys(&self) -> Vec<KeyCode> {
        KeyCode::ALL
            .iter()
            .copied()
            .filter(|k| self.disabled[k.index()])
            .collect()
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

    pub fn process_event(&mut self, key: KeyCode, is_up: bool) -> EngineResult {
        self.process_event_injected(key, is_up, false)
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
        self.process(key, phase)
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

    pub fn process(&mut self, key: KeyCode, phase: KeyPhase) -> EngineResult {
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
    fn enable_all(&self) -> Vec<InjectAction>;
    fn is_enabled(&self, key: KeyCode) -> bool;
    fn set_disabled_keys(&self, keys: &[KeyCode]);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(engine: &mut FilterEngine, key: KeyCode) -> EngineResult {
        engine.process(key, KeyPhase::Down)
    }

    fn up(engine: &mut FilterEngine, key: KeyCode) -> EngineResult {
        engine.process(key, KeyPhase::Up)
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
        let repeat = engine.process(KeyCode::ShiftLeft, KeyPhase::Repeat);
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
        let repeat = engine.process(KeyCode::KeyW, KeyPhase::Repeat);
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
        engine.process(KeyCode::KeyA, KeyPhase::Repeat);
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
        engine.process(KeyCode::KeyA, KeyPhase::Repeat);
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
        let repeat = engine.process(KeyCode::KeyA, KeyPhase::Repeat);
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
        let injected_down = engine.process_event_injected(KeyCode::KeyA, false, true);
        assert_eq!(injected_down.decision, HookDecision::Forward);
        assert!(injected_down.inject.is_empty());
        assert_eq!(injected_down.ui, None);
        assert!(!injected_down.emergency);
        assert!(engine.pressed_keys().is_empty());
        let injected_up = engine.process_event_injected(KeyCode::KeyA, true, true);
        assert_eq!(injected_up.decision, HookDecision::Forward);
        assert!(injected_up.inject.is_empty());
        // Injected chord keys never count toward the emergency chord:
        // the physical F12 below must not fire it.
        engine.process_event_injected(KeyCode::ControlLeft, false, true);
        engine.process_event_injected(KeyCode::ShiftLeft, false, true);
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
        let blocked = engine.process_event_injected(KeyCode::KeyA, false, true);
        assert_eq!(blocked.decision, HookDecision::Consume);
        assert!(blocked.inject.is_empty());
        engine.enable_key(KeyCode::KeyA);
        engine.set_cat_lock(true);
        let locked = engine.process_event_injected(KeyCode::KeyA, false, true);
        assert_eq!(locked.decision, HookDecision::Consume);
        // An injected up for a disabled key is consumed too (same
        // enabled-check as a physical down, §1.4).
        let locked_up = engine.process_event_injected(KeyCode::KeyA, true, true);
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
}
