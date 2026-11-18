//! S8: executing the injection actions the pure engine returns.
//!
//! The engine (`engine.rs`) never calls Win32: it returns
//! [`InjectAction`] lists as data. This module is the only place that
//! turns them into `SendInput` calls (ADR §2, D3/D4). One call per
//! action list, no waiting on delivery, no disk/network/UI.
//!
//! - [`KEYGLOW_INJECTED_TAG`] (`dwExtraInfo`) marks KeyGlow's own
//!   injections for diagnostics only (ADR §1.3). It is **not** the loop
//!   guard — the `LLKHF_INJECTED` flag is.
//! - [`execute_injections`] sends one action list in a single
//!   `SendInput` call and reports how many events landed. A short
//!   count is a failure: the caller fail-opens (hook callback) or
//!   logs loudly (flush paths).
//! - [`startup_sweep`] releases anything a previous force-killed run
//!   left down (ADR §6, D8): before the hook is installed, inject a
//!   key-up for every code `GetAsyncKeyState` reports as *not*
//!   currently down, in one batched call. Physically held keys are
//!   skipped, so the sweep can never cancel a key the user is
//!   actually holding.
//!
//! Off Windows this module is a no-op stub: the engine still returns
//! action lists (and tests still assert them), but nothing is sent.

use crate::keyboard::engine::InjectAction;
use crate::keyboard::keycodes::{to_vk_scan, KeyCode};

/// Magic `dwExtraInfo` tag on every KeyGlow injection (ADR §1.3).
/// ASCII "KGLW". Diagnostics only — never the loop guard.
pub const KEYGLOW_INJECTED_TAG: usize = 0x4B47_4C57;

#[cfg(windows)]
pub fn execute_injections(actions: &[InjectAction]) -> Result<usize, String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    };

    if actions.is_empty() {
        return Ok(0);
    }
    let inputs: Vec<INPUT> = actions
        .iter()
        .map(|action| {
            let (code, is_up) = match action {
                InjectAction::Down(code) => (*code, false),
                InjectAction::Up(code) => (*code, true),
            };
            let (_vk, scan, extended) = to_vk_scan(code);
            let mut flags = KEYEVENTF_SCANCODE;
            if extended {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            if is_up {
                flags |= KEYEVENTF_KEYUP;
            }
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        // Scancode injection: wVk is ignored, wScan
                        // identifies the key (ADR §2, D4).
                        wVk: 0,
                        wScan: scan as u16,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: KEYGLOW_INJECTED_TAG,
                    },
                },
            }
        })
        .collect();
    // SAFETY: `inputs` is a live Vec of INPUT structs; the pointer and
    // count match, and the size is `size_of::<INPUT>()`.
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    let sent = sent as usize;
    if sent < inputs.len() {
        return Err(format!(
            "SendInput delivered {sent} of {} events",
            inputs.len()
        ));
    }
    Ok(sent)
}

#[cfg(not(windows))]
pub fn execute_injections(actions: &[InjectAction]) -> Result<usize, String> {
    // No hook exists off Windows, so no action ever occurs in practice;
    // the engine still returns the lists for tests to assert.
    Ok(actions.len())
}

#[cfg(windows)]
pub fn startup_sweep() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    let mut actions = Vec::new();
    for code in KeyCode::ALL {
        let (vk, _scan, _extended) = to_vk_scan(code);
        // SAFETY: GetAsyncKeyState takes a virtual-key code and returns
        // a SHORT; the high bit (0x8000) means currently down.
        let state = unsafe { GetAsyncKeyState(vk as i32) };
        if (state as u16) & 0x8000 == 0 {
            // Not physically held: an up for it can only release a key
            // a previous run left down. Keys held right now are skipped
            // (checked first) so the sweep never cancels a live hold.
            actions.push(InjectAction::Up(code));
        }
    }
    if actions.is_empty() {
        return;
    }
    match execute_injections(&actions) {
        Ok(sent) => tracing::info!("startup remap sweep: released {sent} keys"),
        Err(err) => tracing::error!("startup remap sweep failed ({err})"),
    }
}

#[cfg(not(windows))]
pub fn startup_sweep() {}
