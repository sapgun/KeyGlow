//! Foreground observer (S5): watches the Windows foreground window on its
//! own thread and feeds the executable path into the pure rule resolver.
//!
//! The hook callback never touches this: no process queries, no rule
//! evaluation, no disk, no UI work in the callback. This thread polls
//! `GetForegroundWindow` every [`OBSERVER_POLL_MS`] and only reports a
//! change; the 200ms debounce lives in the pure
//! [`AppProfileRuntime`](crate::profiles::app_profiles::AppProfileRuntime).
//!
//! Any query failure (no foreground window, inaccessible process,
//! `QueryFullProcessImageNameW` failing) yields `None`, which the resolver
//! treats as "unknown app" and falls back to Default. The (HWND, pid)
//! cache avoids re-opening the same process 20 times a second; a wrong
//! hit only misattributes the profile until the next window change.

use crate::profiles::app_profiles::{EffectiveChange, OBSERVER_POLL_MS};
use crate::state::AppState;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Start the observer thread (Windows only). On other platforms this is a
/// no-op: the pure resolver still compiles and is tested there, but no
/// foreground input ever arrives.
pub fn spawn_foreground_observer(app: AppHandle) {
    #[cfg(windows)]
    {
        std::thread::Builder::new()
            .name("keyglow-foreground".into())
            .spawn(move || observer_loop(&app))
            .expect("failed to start foreground observer thread");
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

#[cfg(windows)]
fn observer_loop(app: &AppHandle) {
    let start = Instant::now();
    let mut last_hwnd: isize = 0;
    let mut last_pid: u32 = 0;
    let mut last_exe: Option<String> = None;
    loop {
        let exe = query_foreground_exe_cached(&mut last_hwnd, &mut last_pid, &mut last_exe);
        if let Some(state) = app.try_state::<AppState>() {
            let now_ms = start.elapsed().as_millis() as u64;
            if let Some(change) = state.note_foreground(exe, now_ms) {
                emit_auto_switch(app, &state, &change);
            }
        }
        std::thread::sleep(Duration::from_millis(OBSERVER_POLL_MS));
    }
}

/// Announce an effective-profile switch: the stamped auto-switch event,
/// the key state, and the config (the UI hydrates on `profile:changed`).
/// Mirrors the event pump: WebView2 eval goes through the UI thread, with
/// a direct-emit fallback.
#[cfg(windows)]
fn emit_auto_switch(app: &AppHandle, state: &AppState, change: &EffectiveChange) {
    use crate::commands::keyboard::{auto_switched_payload, state_changed_payload};
    let switch = auto_switched_payload(state, change);
    let keys = state_changed_payload(state);
    let config = state.config.lock().clone();
    let switch2 = switch.clone();
    let keys2 = keys.clone();
    let config2 = config.clone();
    let posted = app.clone();
    let emit_all = move || {
        let _ = posted.emit("profile:auto-switched", switch2);
        let _ = posted.emit("keyboard:state-changed", keys2);
        let _ = posted.emit("profile:changed", config2);
        crate::tray::refresh(&posted);
    };
    if app.run_on_main_thread(emit_all).is_err() {
        let _ = app.emit("profile:auto-switched", switch);
        let _ = app.emit("keyboard:state-changed", keys);
        let _ = app.emit("profile:changed", config);
        crate::tray::refresh(app);
    }
}

#[cfg(windows)]
fn query_foreground_exe_cached(
    last_hwnd: &mut isize,
    last_pid: &mut u32,
    last_exe: &mut Option<String>,
) -> Option<String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd == 0 {
            *last_hwnd = 0;
            *last_pid = 0;
            *last_exe = None;
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        if hwnd == *last_hwnd && pid == *last_pid {
            return last_exe.clone();
        }
        let exe = query_process_exe(pid);
        *last_hwnd = hwnd;
        *last_pid = pid;
        *last_exe = exe.clone();
        exe
    }
}

/// Full image path of a process, or `None` when it cannot be opened or
/// queried (permission, exited process). Never panics on failure.
#[cfg(windows)]
fn query_process_exe(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}
