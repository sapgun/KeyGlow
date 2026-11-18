//! S9: macro execution state machine (pure logic, no Win32).
//!
//! A macro is a timed list of key down/up steps, started by one trigger
//! key and stopped by its cancel key, by the emergency chord, by a
//! profile switch, or by its own limits. The runner is a virtual-clock
//! state machine: the caller drives it with `tick(now_ms)` and executes
//! the returned [`InjectAction`]s (same `SendInput` path as S8 remaps).
//! The hook callback never sleeps and never runs a macro: it only starts
//! one (via the engine) and the dedicated tick thread in `lib.rs` polls
//! `macro_tick_execute` on the controller.
//!
//! Invariants:
//! - Steps are validated before a macro can be stored (see
//!   `profiles::models::validate_macro`): at most `MAX_MACRO_STEPS`
//!   steps, each delay at most `MAX_STEP_DELAY_MS`, and the down/up
//!   sequence is balanced per key (a down is always followed by its up
//!   before the same key goes down again; no up without a prior down).
//!   A run that completes therefore leaves no key held.
//! - A run that is aborted (cancel key, emergency, profile switch,
//!   time limit, injection failure) releases every key the macro
//!   currently holds, in reverse down order, via `cancel()`.
//! - One macro runs at a time. A second trigger while one is running is
//!   consumed and ignored (the engine decides this; the runner refuses
//!   a start while running).
//! - Macro output is injected input: it never remaps (S8 D1 — the
//!   injected lane), never counts toward the emergency chord, and still
//!   passes the hook's disable / Cat Lock check, so a step naming a
//!   disabled key is dropped by the hook, exactly like an S8 injected
//!   target.

use crate::keyboard::engine::InjectAction;
use crate::keyboard::KeyCode;
use crate::profiles::models::MacroDef;

/// Maximum steps in one macro (validated at save time).
pub const MAX_MACRO_STEPS: usize = 64;
/// Maximum delay before a single step, in milliseconds.
pub const MAX_STEP_DELAY_MS: u32 = 5_000;
/// Maximum wall time for one macro run. A run that exceeds it is
/// aborted (held keys released). With the step/delay caps a macro can
/// request up to 64 x 5s of delays; the run cap is the backstop that
/// keeps a run finite even when the author maxed every delay.
pub const MAX_MACRO_RUN_MS: u64 = 30_000;
/// Maximum macros stored on one profile.
pub const MAX_MACROS_PER_PROFILE: usize = 16;
/// Maximum layers stored on one profile.
pub const MAX_LAYERS_PER_PROFILE: usize = 8;

/// A macro run in progress. All times are milliseconds on the caller's
/// clock (the tick thread's monotonic clock in production, a virtual
/// clock in tests).
#[derive(Clone, Debug, Default)]
pub struct MacroRunner {
    running: bool,
    steps: Vec<MacroStep>,
    /// Next step to execute.
    index: usize,
    started_ms: u64,
    /// When `steps[index]` becomes due.
    next_due_ms: u64,
    /// Keys this run currently holds down, in down order. Released in
    /// reverse order by `cancel()`.
    held: Vec<KeyCode>,
    trigger: Option<KeyCode>,
    cancel_key: Option<KeyCode>,
}

/// One macro step, parsed from a stored [`MacroDef`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacroStep {
    pub key: KeyCode,
    pub down: bool,
    pub delay_ms: u32,
}

impl MacroRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// The trigger that started the current run, if any.
    pub fn running_trigger(&self) -> Option<KeyCode> {
        self.trigger
    }

    /// The cancel key of the current run, if any.
    pub fn cancel_key(&self) -> Option<KeyCode> {
        self.cancel_key
    }

    /// When the next step becomes due, if a run is in progress.
    pub fn next_due_ms(&self) -> Option<u64> {
        if self.running {
            Some(self.next_due_ms)
        } else {
            None
        }
    }

    /// Keys this run currently holds down (for tests / diagnostics).
    pub fn held_keys(&self) -> &[KeyCode] {
        &self.held
    }

    /// Start a run. Refused (empty actions, not running) when a run is
    /// already in progress or the definition has no steps. Otherwise
    /// executes every step already due at `now_ms` — a leading run of
    /// zero-delay steps fires immediately, in order.
    pub fn start(&mut self, def: &MacroDef, now_ms: u64) -> Vec<InjectAction> {
        if self.running || def.steps.is_empty() {
            return vec![];
        }
        self.steps = def
            .steps
            .iter()
            .filter_map(|s| {
                let key = KeyCode::from_id(&s.key)?;
                Some(MacroStep {
                    key,
                    down: s.action == "down",
                    delay_ms: s.delay_ms,
                })
            })
            .collect();
        if self.steps.len() != def.steps.len() {
            // Validation guarantees all ids resolve; a mismatch here is
            // defensive only — refuse to run a half-parsed macro.
            self.steps.clear();
            return vec![];
        }
        self.running = true;
        self.index = 0;
        self.started_ms = now_ms;
        self.next_due_ms = now_ms.saturating_add(self.steps[0].delay_ms as u64);
        self.held.clear();
        self.trigger = KeyCode::from_id(&def.trigger);
        self.cancel_key = KeyCode::from_id(&def.cancel_key);
        self.tick(now_ms)
    }

    /// Execute every step due at `now_ms`, in order. Returns the
    /// injections for the caller to execute. A run that passes the time
    /// limit is aborted instead: held keys are released and no further
    /// step executes.
    pub fn tick(&mut self, now_ms: u64) -> Vec<InjectAction> {
        if !self.running {
            return vec![];
        }
        if now_ms.saturating_sub(self.started_ms) > MAX_MACRO_RUN_MS {
            // Time-limit abort: the run is over, held keys come up.
            return self.cancel();
        }
        let mut actions = Vec::new();
        while self.index < self.steps.len() && now_ms >= self.next_due_ms {
            let step = self.steps[self.index];
            self.index += 1;
            if step.down {
                if !self.held.contains(&step.key) {
                    self.held.push(step.key);
                }
                actions.push(InjectAction::Down(step.key));
            } else {
                self.held.retain(|k| *k != step.key);
                actions.push(InjectAction::Up(step.key));
            }
            if self.index < self.steps.len() {
                self.next_due_ms = self
                    .next_due_ms
                    .saturating_add(self.steps[self.index].delay_ms as u64);
            }
        }
        if self.index >= self.steps.len() {
            // Run complete. Balanced steps guarantee `held` is empty;
            // a stray hold would be a validation bug, so release it
            // rather than leak it.
            if !self.held.is_empty() {
                let mut release: Vec<InjectAction> =
                    self.held.iter().rev().map(|k| InjectAction::Up(*k)).collect();
                actions.append(&mut release);
                self.held.clear();
            }
            self.running = false;
            self.trigger = None;
            self.cancel_key = None;
        }
        actions
    }

    /// Abort the run: release every held key in reverse down order and
    /// stop. Idempotent — a second cancel injects nothing.
    pub fn cancel(&mut self) -> Vec<InjectAction> {
        if !self.running && self.held.is_empty() {
            return vec![];
        }
        let actions: Vec<InjectAction> =
            self.held.iter().rev().map(|k| InjectAction::Up(*k)).collect();
        self.held.clear();
        self.running = false;
        self.index = 0;
        self.trigger = None;
        self.cancel_key = None;
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::models::{MacroDef, MacroStepDef};

    fn def(trigger: &str, steps: Vec<(&str, &str, u32)>) -> MacroDef {
        MacroDef {
            id: "m1".into(),
            name: "Test".into(),
            trigger: trigger.into(),
            cancel_key: "Escape".into(),
            steps: steps
                .into_iter()
                .map(|(key, action, delay_ms)| MacroStepDef {
                    key: key.into(),
                    action: action.into(),
                    delay_ms,
                })
                .collect(),
        }
    }

    /// "Press Ctrl+C": down Ctrl, down C, up C, up Ctrl.
    fn ctrl_c() -> MacroDef {
        def(
            "F9",
            vec![
                ("ControlLeft", "down", 0),
                ("KeyC", "down", 10),
                ("KeyC", "up", 10),
                ("ControlLeft", "up", 0),
            ],
        )
    }

    #[test]
    fn run_executes_steps_in_order_with_delays() {
        let mut runner = MacroRunner::new();
        let actions = runner.start(&ctrl_c(), 1000);
        // Only the zero-delay first step is due at start.
        assert_eq!(actions, vec![InjectAction::Down(KeyCode::ControlLeft)]);
        assert!(runner.is_running());
        assert_eq!(runner.next_due_ms(), Some(1010));
        // Before the due time, nothing.
        assert!(runner.tick(1005).is_empty());
        let actions = runner.tick(1010);
        assert_eq!(actions, vec![InjectAction::Down(KeyCode::KeyC)]);
        let actions = runner.tick(1020);
        // The last step has delay 0, so it fires on the same tick.
        assert_eq!(
            actions,
            vec![
                InjectAction::Up(KeyCode::KeyC),
                InjectAction::Up(KeyCode::ControlLeft)
            ]
        );
        assert!(!runner.is_running());
        assert!(runner.held_keys().is_empty());
        // A finished run ticks nothing.
        assert!(runner.tick(2000).is_empty());
    }

    #[test]
    fn zero_delay_run_fires_all_steps_at_once() {
        let mut runner = MacroRunner::new();
        let actions = runner.start(&def("F9", vec![("KeyA", "down", 0), ("KeyA", "up", 0)]), 0);
        assert_eq!(
            actions,
            vec![
                InjectAction::Down(KeyCode::KeyA),
                InjectAction::Up(KeyCode::KeyA)
            ]
        );
        assert!(!runner.is_running());
    }

    #[test]
    fn second_start_while_running_is_refused() {
        let mut runner = MacroRunner::new();
        let d = def("F9", vec![("KeyA", "down", 100), ("KeyA", "up", 0)]);
        assert_eq!(runner.start(&d, 0).len(), 0); // first step not due yet
        assert!(runner.is_running());
        assert!(runner.start(&d, 0).is_empty());
        assert!(runner.is_running());
    }

    #[test]
    fn cancel_releases_held_keys_in_reverse_order() {
        let mut runner = MacroRunner::new();
        runner.start(&ctrl_c(), 1000);
        runner.tick(1010); // Ctrl + C down
        assert_eq!(runner.held_keys().len(), 2);
        let actions = runner.cancel();
        assert_eq!(
            actions,
            vec![
                InjectAction::Up(KeyCode::KeyC),
                InjectAction::Up(KeyCode::ControlLeft)
            ]
        );
        assert!(!runner.is_running());
        // Idempotent.
        assert!(runner.cancel().is_empty());
        assert!(runner.tick(2000).is_empty());
    }

    #[test]
    fn run_time_limit_aborts_and_releases() {
        let mut runner = MacroRunner::new();
        // Long delays: the run cannot finish before the cap.
        let d = def("F9", vec![("KeyA", "down", 0), ("KeyA", "up", 5000)]);
        runner.start(&d, 0);
        assert!(runner.is_running());
        assert_eq!(runner.held_keys(), &[KeyCode::KeyA]);
        // Just past the cap: abort, A released, no up step executed.
        let actions = runner.tick(MAX_MACRO_RUN_MS + 1);
        assert_eq!(actions, vec![InjectAction::Up(KeyCode::KeyA)]);
        assert!(!runner.is_running());
    }

    #[test]
    fn empty_macro_never_runs() {
        let mut runner = MacroRunner::new();
        let d = def("F9", vec![]);
        assert!(runner.start(&d, 0).is_empty());
        assert!(!runner.is_running());
    }
}
