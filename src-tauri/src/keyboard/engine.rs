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
        }
    }

    pub fn enable_key(&mut self, key: KeyCode) {
        self.disabled[key.index()] = false;
    }

    pub fn disable_key(&mut self, key: KeyCode) {
        self.disabled[key.index()] = true;
    }

    pub fn enable_all(&mut self) {
        self.cat_lock = false;
        self.disabled = [false; KeyCode::COUNT];
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

    fn emergency_held(&self) -> bool {
        let ctrl = self.physical_down[KeyCode::ControlLeft.index()]
            || self.physical_down[KeyCode::ControlRight.index()];
        let shift = self.physical_down[KeyCode::ShiftLeft.index()]
            || self.physical_down[KeyCode::ShiftRight.index()];
        let f12 = self.physical_down[KeyCode::F12.index()];
        ctrl && shift && f12
    }

    pub fn process_event(&mut self, key: KeyCode, is_up: bool) -> EngineResult {
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

    pub fn process(&mut self, key: KeyCode, phase: KeyPhase) -> EngineResult {
        let i = key.index();
        match phase {
            KeyPhase::Down | KeyPhase::Repeat => {
                let first = !self.physical_down[i];
                self.physical_down[i] = true;
                let ui = if first {
                    Some((key, UiPulse::Down))
                } else {
                    None
                };

                let emergency = if self.emergency_held() {
                    self.enable_all();
                    true
                } else {
                    false
                };

                if self.is_enabled(key) {
                    self.forwarded_down[i] = true;
                    EngineResult {
                        decision: HookDecision::Forward,
                        emergency,
                        ui,
                    }
                } else {
                    EngineResult {
                        decision: HookDecision::Consume,
                        emergency,
                        ui,
                    }
                }
            }
            KeyPhase::Up => {
                let was_physical = self.physical_down[i];
                let was_forwarded = self.forwarded_down[i];
                self.physical_down[i] = false;
                self.forwarded_down[i] = false;

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
                }
            }
        }
    }
}

pub trait KeyboardController: Send + Sync {
    fn enable_key(&self, key: KeyCode);
    fn disable_key(&self, key: KeyCode);
    fn enable_all(&self);
    fn is_enabled(&self, key: KeyCode) -> bool;
    fn set_disabled_keys(&self, keys: &[KeyCode]);
    fn disabled_keys(&self) -> Vec<KeyCode>;
    fn set_cat_lock(&self, locked: bool);
    fn is_cat_locked(&self) -> bool;
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
}
