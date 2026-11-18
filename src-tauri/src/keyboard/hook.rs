use super::engine::{FilterEngine, HookDecision, UiPulse};
use super::hook_lifecycle::{HookExit, HookLifecycle};
use super::keycodes::{identify_key, KeyCode};
use super::safety::SafetyState;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, Copy)]
pub enum HookEvent {
    /// Physical key press. `seq` is the engine's event sequence for this
    /// press (P3): the UI orders key events against pressed-snapshot
    /// responses with it.
    KeyDown { code: KeyCode, seq: u64 },
    /// Physical key release. `seq` as above.
    KeyUp { code: KeyCode, seq: u64 },
    /// Best-effort UI hint. The safety epoch on [`SafetyState`] is the
    /// authoritative signal: if this event is dropped by a saturated queue,
    /// the safety worker still converges from the epoch latch.
    EmergencyUnlock { epoch: u64 },
}

struct HookShared {
    engine: Arc<Mutex<FilterEngine>>,
    tx: SyncSender<HookEvent>,
    safety: Arc<SafetyState>,
    /// Wake channel for the safety worker. Unbounded: `send` never blocks
    /// and only fails after the worker is gone (shutdown), so the hook
    /// callback never waits on any queue.
    safety_wake: Sender<()>,
}

static SHARED: OnceLock<HookShared> = OnceLock::new();
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

/// Process-wide hook lifecycle (HF-07). The single source of truth for
/// start/ready/shutdown/exit transitions; the transition table itself is
/// unit-tested in `hook_lifecycle.rs`.
static LIFECYCLE: Mutex<HookLifecycle> = Mutex::new(HookLifecycle::new());

/// Set before WM_QUIT is posted so the hook thread can distinguish a
/// requested shutdown from an unexpected death in its exit report.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Record the hook thread's terminal state. Returns true when this newly
/// surfaced a failure (the caller should flip hook_active/hook_error and
/// notify the UI). Used by the hook watcher in `lib.rs`.
pub(crate) fn lifecycle_note_thread_exit(exit: &HookExit) -> bool {
    LIFECYCLE.lock().note_thread_exit(exit)
}

pub struct HookHandle {
    thread: Option<JoinHandle<()>>,
    thread_id: u32,
}

/// What a successful [`start_hook`] hands back: the joinable thread handle
/// plus the channel on which the hook thread reports its terminal state
/// exactly once (`HookStart::exit`).
pub struct HookStart {
    pub handle: HookHandle,
    pub exit: Receiver<HookExit>,
}

impl HookHandle {
    pub fn shutdown(mut self) {
        shutdown_hook_thread(self.thread_id);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            shutdown_hook_thread(self.thread_id);
            let _ = thread.join();
        }
    }
}

fn shutdown_hook_thread(thread_id: u32) {
    if thread_id == 0 {
        return;
    }
    // Mark the shutdown as requested BEFORE posting: the hook thread reads
    // this in its exit report to distinguish WM_QUIT from an unexpected
    // death. Idempotent; safe to call from Drop.
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    LIFECYCLE.lock().note_shutdown_requested();
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
        unsafe {
            // The thread force-created its message queue (PeekMessageW)
            // before reporting ready, so this post cannot be lost to a
            // queue that does not exist yet — the race that used to hang
            // shutdown on an immediate start->stop is closed. A failure
            // here is still logged loudly: without WM_QUIT the join below
            // would block forever.
            if PostThreadMessageW(thread_id, WM_QUIT, 0, 0) == 0 {
                tracing::error!(
                    "PostThreadMessageW(WM_QUIT) failed (Win32 {}); hook thread join may hang",
                    GetLastError()
                );
            }
        }
    }
}

/// Install the low-level keyboard hook.
///
/// The hook is started at most once per process: a second call is rejected
/// with an explicit error (there is no in-process restart — see the HF-07
/// ADR). The returned `HookStart::exit` channel delivers the thread's
/// terminal [`HookExit`] exactly once.
pub fn start_hook(
    engine: Arc<Mutex<FilterEngine>>,
    tx: SyncSender<HookEvent>,
    safety: Arc<SafetyState>,
    safety_wake: Sender<()>,
) -> Result<HookStart, String> {
    // Explicit restart rejection BEFORE touching any shared state: the old
    // code silently ignored the new handles via OnceLock::set, which would
    // have left a second caller believing its engine was hooked.
    LIFECYCLE.lock().request_start()?;

    let _ = SHARED.set(HookShared {
        engine,
        tx,
        safety,
        safety_wake,
    });

    #[cfg(not(windows))]
    {
        let err = "keyboard hook is only implemented on Windows".to_string();
        LIFECYCLE.lock().note_ready(Err(err.clone()));
        return Err(err);
    }

    #[cfg(windows)]
    {
        start_windows_hook()
    }
}

#[cfg(windows)]
fn start_windows_hook() -> Result<HookStart, String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_UP, MSG,
        PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYUP, WM_SYSKEYUP,
    };

    unsafe extern "system" fn low_level_keyboard_proc(
        n_code: i32,
        w_param: usize,
        l_param: isize,
    ) -> isize {
        if n_code >= 0 && l_param != 0 {
            let kb = unsafe { &*(l_param as *const KBDLLHOOKSTRUCT) };
            let flags = kb.flags;
            let is_up =
                (flags & LLKHF_UP) != 0 || w_param as u32 == WM_KEYUP || w_param as u32 == WM_SYSKEYUP;
            let extended = (flags & LLKHF_EXTENDED) != 0;

            if let Some(key) = identify_key(kb.vkCode, kb.scanCode, extended) {
                if let Some(shared) = SHARED.get() {
                    // Capture the engine's event sequence for this press
                    // (P3) in the same lock scope as event processing: the
                    // sequence read here IS this event's sequence. No new
                    // blocking in the callback; the lock was already taken.
                    let mut engine = shared.engine.lock();
                    let result = engine.process_event(key, is_up);
                    let seq = engine.event_sequence();
                    drop(engine);
                    match result.ui {
                        Some((_, UiPulse::Down)) => {
                            let _ = shared.tx.try_send(HookEvent::KeyDown { code: key, seq });
                        }
                        Some((_, UiPulse::Up)) => {
                            let _ = shared.tx.try_send(HookEvent::KeyUp { code: key, seq });
                        }
                        None => {}
                    }
                    if result.emergency {
                        // Physical recovery (enable_all) already happened
                        // inside the engine. Here the hook only records the
                        // safety epoch (one atomic increment) and wakes the
                        // worker: no blocking sends, no file or UI work.
                        let epoch = shared.safety.trigger();
                        let _ = shared.safety_wake.send(());
                        // UI hint on the lossy glow queue; safe to drop.
                        let _ = shared
                            .tx
                            .try_send(HookEvent::EmergencyUnlock { epoch });
                    }
                    if result.decision == HookDecision::Consume {
                        return 1;
                    }
                }
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param) }
    }

    /// The hook thread's whole life: install, message loop, uninstall.
    /// Returns its terminal [`HookExit`]. Runs inside `catch_unwind` (see
    /// the spawn site): a panic becomes an exit report, never a silent
    /// death with the app still showing "keyboard control active".
    unsafe fn hook_thread_main(
        ready_tx: std::sync::mpsc::Sender<Result<u32, String>>,
    ) -> HookExit {
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            std::ptr::null_mut(),
            0,
        );
        if hook.is_null() {
            let err = GetLastError();
            let msg = format!("SetWindowsHookExW(WH_KEYBOARD_LL) failed (Win32 {err})");
            let _ = ready_tx.send(Err(msg.clone()));
            return HookExit {
                requested: false,
                error: Some(msg),
            };
        }

        let thread_id = GetCurrentThreadId();
        HOOK_THREAD_ID.store(thread_id, Ordering::SeqCst);

        // Force-create this thread's message queue BEFORE reporting ready.
        // PostThreadMessageW to a thread without a queue fails and the
        // message is lost (it is NOT delivered later); without this call a
        // shutdown issued right after start would lose WM_QUIT and the
        // HookHandle join would hang forever.
        let mut msg = std::mem::zeroed::<MSG>();
        PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);

        let _ = ready_tx.send(Ok(thread_id));

        let mut loop_error: Option<String> = None;
        loop {
            let status = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if status == 0 {
                break; // WM_QUIT: requested shutdown.
            }
            if status == -1 {
                loop_error = Some("hook message loop failed (GetMessageW error)".to_string());
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(hook);
        HOOK_THREAD_ID.store(0, Ordering::SeqCst);
        tracing::info!("keyboard hook uninstalled");

        let requested = SHUTDOWN_REQUESTED.load(Ordering::SeqCst);
        HookExit {
            requested,
            error: if requested {
                None
            } else {
                Some(
                    loop_error
                        .unwrap_or_else(|| "hook message loop ended unexpectedly".to_string()),
                )
            },
        }
    }

    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
    let (exit_tx, exit_rx) = std::sync::mpsc::channel::<HookExit>();

    let thread = thread::Builder::new()
        .name("keyglow-hook".into())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                hook_thread_main(ready_tx)
            }));
            let exit = outcome.unwrap_or(HookExit {
                requested: false,
                error: Some("hook thread panicked".to_string()),
            });
            // Best-effort: during process teardown the watcher may be gone.
            let _ = exit_tx.send(exit);
        })
        .map_err(|e| format!("failed to start hook thread: {e}"))?;

    // NOTE: no timeout on the handshake. SetWindowsHookExW is synchronous
    // and non-blocking, so the thread always reports promptly; a timeout
    // here would orphan the thread (it would keep the hook installed with
    // no owner), which is worse than waiting.
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => {
            LIFECYCLE.lock().note_ready(Ok(()));
            tracing::info!("keyboard hook installed (WH_KEYBOARD_LL)");
            Ok(HookStart {
                handle: HookHandle {
                    thread: Some(thread),
                    thread_id,
                },
                exit: exit_rx,
            })
        }
        Ok(Err(err)) => {
            LIFECYCLE.lock().note_ready(Err(err.clone()));
            let _ = thread.join();
            Err(err)
        }
        Err(_) => {
            let msg = "hook thread exited before reporting status".to_string();
            LIFECYCLE.lock().note_ready(Err(msg.clone()));
            let _ = thread.join();
            Err(msg)
        }
    }
}
