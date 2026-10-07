use super::engine::{FilterEngine, HookDecision, UiPulse};
use super::keycodes::{identify_key, KeyCode};
use super::safety::SafetyState;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Sender, SyncSender};
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

pub struct HookHandle {
    thread: Option<JoinHandle<()>>,
    thread_id: u32,
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
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
            thread_id,
            windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT,
            0,
            0,
        );
    }
}

pub fn start_hook(
    engine: Arc<Mutex<FilterEngine>>,
    tx: SyncSender<HookEvent>,
    safety: Arc<SafetyState>,
    safety_wake: Sender<()>,
) -> Result<HookHandle, String> {
    let _ = SHARED.set(HookShared {
        engine,
        tx,
        safety,
        safety_wake,
    });

    #[cfg(not(windows))]
    {
        return Err("keyboard hook is only implemented on Windows".to_string());
    }

    #[cfg(windows)]
    {
        start_windows_hook()
    }
}

#[cfg(windows)]
fn start_windows_hook() -> Result<HookHandle, String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_UP, MSG, WH_KEYBOARD_LL,
        WM_KEYUP, WM_SYSKEYUP,
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

    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();

    let thread = thread::Builder::new()
        .name("keyglow-hook".into())
        .spawn(move || unsafe {
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(low_level_keyboard_proc),
                std::ptr::null_mut(),
                0,
            );
            if hook.is_null() {
                let err = GetLastError();
                let _ = ready_tx.send(Err(format!(
                    "SetWindowsHookExW(WH_KEYBOARD_LL) failed (Win32 {err})"
                )));
                return;
            }

            let thread_id = GetCurrentThreadId();
            HOOK_THREAD_ID.store(thread_id, Ordering::SeqCst);
            let _ = ready_tx.send(Ok(thread_id));

            let mut msg = std::mem::zeroed::<MSG>();
            loop {
                let status = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                if status == 0 || status == -1 {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = UnhookWindowsHookEx(hook);
            HOOK_THREAD_ID.store(0, Ordering::SeqCst);
            tracing::info!("keyboard hook uninstalled");
        })
        .map_err(|e| format!("failed to start hook thread: {e}"))?;

    match ready_rx.recv() {
        Ok(Ok(thread_id)) => {
            tracing::info!("keyboard hook installed (WH_KEYBOARD_LL)");
            Ok(HookHandle {
                thread: Some(thread),
                thread_id,
            })
        }
        Ok(Err(err)) => {
            let _ = thread.join();
            Err(err)
        }
        Err(_) => {
            let _ = thread.join();
            Err("hook thread exited before reporting status".into())
        }
    }
}
