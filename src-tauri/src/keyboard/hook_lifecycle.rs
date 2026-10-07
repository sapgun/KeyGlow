//! Platform-agnostic lifecycle state machine for the keyboard hook thread
//! (HF-07).
//!
//! The Windows-only parts (SetWindowsHookExW, the message loop,
//! PostThreadMessageW) live in [`super::hook`]; this module owns the
//! transition table so it can be unit-tested deterministically without
//! Windows, sleeps, or thread races. The real code drives it through the
//! `pub(crate)` functions in `hook.rs`, so these tests prove the exact
//! transitions the app uses.
//!
//! Contract (see ARCHITECTURE.md ADR "hook lifecycle"):
//!
//! - The hook is started at most once per process. There is no restart:
//!   `WH_KEYBOARD_LL` is process-global, the shared hook state is a
//!   `OnceLock`, and teardown/reinstall races make in-process restart
//!   unsafe. A second start is rejected with an explicit error that tells
//!   the operator to restart the app.
//! - The hook thread reports its terminal state exactly once via
//!   [`HookExit`]. A thread that dies on its own (panic, message-loop
//!   error) moves the lifecycle to `Failed`, which the app surfaces as
//!   `hook_active = false` + `hook_error`. A silent removal of the hook by
//!   Windows while our thread keeps running is NOT detectable from
//!   userspace and is documented as a limitation, not claimed otherwise.

/// Terminal state of the hook thread, reported exactly once on exit.
#[derive(Debug, Clone)]
pub struct HookExit {
    /// True when the thread is exiting because shutdown was requested
    /// (WM_QUIT posted by [`super::hook`] shutdown path).
    pub requested: bool,
    /// Human-readable reason when the thread died on its own.
    pub error: Option<String>,
}

/// Lifecycle states of the hook thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookLifecycleState {
    /// Never started.
    Idle,
    /// Start requested; waiting for the install ready-handshake.
    Starting,
    /// Hook installed; the thread is in its message loop.
    Running,
    /// Shutdown requested; waiting for the thread to exit.
    Stopping,
    /// Thread exited after a requested shutdown.
    Stopped,
    /// Install failed, or the thread died on its own.
    Failed,
}

/// Transition table for the hook thread lifecycle.
///
/// All methods are synchronous and total: unexpected transitions are ignored
/// rather than panicking, because this machine is driven from thread
/// teardown paths where a panic would be worse than a missed update.
pub struct HookLifecycle {
    state: HookLifecycleState,
    started_once: bool,
    error: Option<String>,
}

impl HookLifecycle {
    /// Const so the global lifecycle in `hook.rs` can live in a `static`.
    pub const fn new() -> Self {
        HookLifecycle {
            state: HookLifecycleState::Idle,
            started_once: false,
            error: None,
        }
    }

    /// `Idle -> Starting`. Any other state — including a previous `Failed`
    /// or `Stopped` — is rejected: the hook cannot be restarted in-process.
    pub fn request_start(&mut self) -> Result<(), String> {
        if self.started_once || self.state != HookLifecycleState::Idle {
            return Err(
                "keyboard hook cannot be restarted in-process; restart the app to reinstall the hook"
                    .to_string(),
            );
        }
        self.started_once = true;
        self.state = HookLifecycleState::Starting;
        Ok(())
    }

    /// Record the install ready-handshake result. Only meaningful while
    /// `Starting`; calls from any other state are ignored.
    pub fn note_ready(&mut self, result: Result<(), String>) {
        if self.state != HookLifecycleState::Starting {
            return;
        }
        match result {
            Ok(()) => self.state = HookLifecycleState::Running,
            Err(err) => {
                self.state = HookLifecycleState::Failed;
                self.error = Some(err);
            }
        }
    }

    /// `Running -> Stopping`. Idempotent; ignored from other states.
    pub fn note_shutdown_requested(&mut self) {
        if self.state == HookLifecycleState::Running {
            self.state = HookLifecycleState::Stopping;
        }
    }

    /// Record the hook thread's terminal state.
    ///
    /// Returns `true` exactly when this call newly moved the lifecycle into
    /// `Failed` — i.e. the caller should surface the failure (flip
    /// `hook_active`, set `hook_error`, notify the UI). A requested exit, a
    /// repeated report, or an exit after an already-recorded failure
    /// returns `false` so the failure is never announced twice.
    pub fn note_thread_exit(&mut self, exit: &HookExit) -> bool {
        match self.state {
            // Terminal states: the first report wins.
            HookLifecycleState::Failed | HookLifecycleState::Stopped => false,
            _ => {
                if exit.requested {
                    self.state = HookLifecycleState::Stopped;
                    false
                } else {
                    self.state = HookLifecycleState::Failed;
                    self.error = Some(
                        exit.error
                            .clone()
                            .unwrap_or_else(|| "keyboard hook thread exited unexpectedly".into()),
                    );
                    true
                }
            }
        }
    }

    /// True only while the hook is installed and its thread is running.
    /// This is what the UI may display as "keyboard control active".
    pub fn is_healthy(&self) -> bool {
        self.state == HookLifecycleState::Running
    }

    pub fn state(&self) -> HookLifecycleState {
        self.state
    }

    /// The recorded failure reason, if the lifecycle reached `Failed`.
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }
}

impl Default for HookLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> HookLifecycle {
        let mut lc = HookLifecycle::new();
        lc.request_start().unwrap();
        lc.note_ready(Ok(()));
        assert_eq!(lc.state(), HookLifecycleState::Running);
        lc
    }

    #[test]
    fn happy_path_start_run_stop() {
        let mut lc = HookLifecycle::new();
        assert_eq!(lc.state(), HookLifecycleState::Idle);
        assert!(!lc.is_healthy());

        lc.request_start().unwrap();
        assert_eq!(lc.state(), HookLifecycleState::Starting);

        lc.note_ready(Ok(()));
        assert_eq!(lc.state(), HookLifecycleState::Running);
        assert!(lc.is_healthy());

        lc.note_shutdown_requested();
        assert_eq!(lc.state(), HookLifecycleState::Stopping);
        assert!(!lc.is_healthy());

        let announced =
            lc.note_thread_exit(&HookExit { requested: true, error: None });
        assert!(!announced, "requested shutdown must not raise an alarm");
        assert_eq!(lc.state(), HookLifecycleState::Stopped);
    }

    #[test]
    fn second_start_is_rejected_after_running() {
        let mut lc = running();
        let err = lc.request_start().unwrap_err();
        assert!(err.contains("restart"), "unexpected message: {err}");
        // The rejection must not disturb the running hook.
        assert_eq!(lc.state(), HookLifecycleState::Running);
        assert!(lc.is_healthy());
    }

    #[test]
    fn no_restart_after_stop_or_failure() {
        let mut lc = running();
        lc.note_shutdown_requested();
        lc.note_thread_exit(&HookExit { requested: true, error: None });
        assert!(lc.request_start().is_err());

        let mut lc2 = HookLifecycle::new();
        lc2.request_start().unwrap();
        lc2.note_ready(Err("SetWindowsHookExW failed".into()));
        assert!(lc2.request_start().is_err());
    }

    #[test]
    fn install_failure_records_error() {
        let mut lc = HookLifecycle::new();
        lc.request_start().unwrap();
        lc.note_ready(Err("SetWindowsHookExW(WH_KEYBOARD_LL) failed (Win32 5)".into()));
        assert_eq!(lc.state(), HookLifecycleState::Failed);
        assert!(!lc.is_healthy());
        assert_eq!(
            lc.error().as_deref(),
            Some("SetWindowsHookExW(WH_KEYBOARD_LL) failed (Win32 5)")
        );
    }

    #[test]
    fn thread_death_before_ready_is_single_failure() {
        // The thread panics before the handshake: start_hook reports it via
        // the ready channel first...
        let mut lc = HookLifecycle::new();
        lc.request_start().unwrap();
        lc.note_ready(Err("hook thread exited before reporting status".into()));
        assert_eq!(lc.state(), HookLifecycleState::Failed);
        // ...then the thread-exit report arrives. It must not overwrite the
        // first error or announce twice.
        let announced = lc.note_thread_exit(&HookExit {
            requested: false,
            error: Some("hook thread panicked".into()),
        });
        assert!(!announced);
        assert_eq!(
            lc.error().as_deref(),
            Some("hook thread exited before reporting status")
        );
    }

    #[test]
    fn unexpected_thread_death_announces_once() {
        let mut lc = running();
        let announced = lc.note_thread_exit(&HookExit {
            requested: false,
            error: Some("GetMessageW failed".into()),
        });
        assert!(announced);
        assert_eq!(lc.state(), HookLifecycleState::Failed);
        assert!(!lc.is_healthy());
        assert_eq!(lc.error().as_deref(), Some("GetMessageW failed"));

        // A duplicate report (e.g. Drop racing the watcher) is silent.
        let again = lc.note_thread_exit(&HookExit {
            requested: false,
            error: Some("GetMessageW failed".into()),
        });
        assert!(!again);
    }

    #[test]
    fn panic_exit_gets_default_message() {
        let mut lc = running();
        let announced = lc.note_thread_exit(&HookExit {
            requested: false,
            error: None,
        });
        assert!(announced);
        assert_eq!(
            lc.error().as_deref(),
            Some("keyboard hook thread exited unexpectedly")
        );
    }

    #[test]
    fn death_during_stopping_without_request_is_failure() {
        // Shutdown was requested but the thread died without seeing WM_QUIT
        // (e.g. the post was lost): that is a real failure, surface it.
        let mut lc = running();
        lc.note_shutdown_requested();
        let announced = lc.note_thread_exit(&HookExit {
            requested: false,
            error: Some("message loop broke".into()),
        });
        assert!(announced);
        assert_eq!(lc.state(), HookLifecycleState::Failed);
    }

    #[test]
    fn shutdown_request_is_idempotent() {
        let mut lc = running();
        lc.note_shutdown_requested();
        lc.note_shutdown_requested();
        assert_eq!(lc.state(), HookLifecycleState::Stopping);
    }

    #[test]
    fn ready_ignored_outside_starting() {
        let mut lc = HookLifecycle::new();
        // Defensive: a stray ready signal before start changes nothing.
        lc.note_ready(Ok(()));
        assert_eq!(lc.state(), HookLifecycleState::Idle);

        let mut lc2 = running();
        lc2.note_ready(Err("late".into()));
        assert_eq!(lc2.state(), HookLifecycleState::Running);
        assert!(lc2.error().is_none());
    }

    #[test]
    fn shutdown_request_before_ready_is_ignored() {
        // start_hook only hands out the handle after the handshake, so this
        // ordering cannot happen in the real code; the machine stays total.
        let mut lc = HookLifecycle::new();
        lc.request_start().unwrap();
        lc.note_shutdown_requested();
        assert_eq!(lc.state(), HookLifecycleState::Starting);
        lc.note_ready(Ok(()));
        assert_eq!(lc.state(), HookLifecycleState::Running);
    }
}
