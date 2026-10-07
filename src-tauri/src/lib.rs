mod commands;
mod i18n;
mod keyboard;
mod platform;
mod profiles;
mod state;
mod tray;

use commands::*;
use keyboard::hook::HookEvent;
use keyboard::hook_lifecycle::HookExit;
use keyboard::SafetyState;
use platform::start_input_backend;
use state::AppState;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use tauri::{Emitter, Manager};

pub fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn emit_key(app: &tauri::AppHandle, event: &str, code: &str, seq: u64) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    // P3: the native event sequence travels with the key event so the UI
    // can order key events against pressed-snapshot responses. Without it,
    // a snapshot taken before a key-down but applied after would drop the
    // physically held key from the display.
    #[derive(serde::Serialize, Clone, Debug)]
    struct KeyPressPayload<'a> {
        code: &'a str,
        seq: u64,
    }
    if let Err(err) = app.emit(event, KeyPressPayload { code, seq }) {
        if !WARNED.swap(true, Ordering::Relaxed) {
            tracing::error!("failed to emit {event} to UI: {err}");
        }
    }
}

fn dispatch_hook_event(app: &tauri::AppHandle, event: HookEvent) {
    match event {
        HookEvent::KeyDown { code, seq } => emit_key(app, "keyboard:key-down", code.as_str(), seq),
        HookEvent::KeyUp { code, seq } => emit_key(app, "keyboard:key-up", code.as_str(), seq),
        HookEvent::EmergencyUnlock { epoch } => {
            if let Some(state) = app.try_state::<AppState>() {
                // Fallback path: converges only if the safety worker hasn't,
                // and never blocks the UI thread. If the worker is already
                // converging, it will pick up the latest epoch itself via
                // pending_epoch(), so skipping here loses nothing.
                if state.safety.reconciled_epoch() < epoch {
                    if let Some(_work) = state.safety.try_convergence_lock() {
                        // Re-check under the lock: the worker may have
                        // finished while we were deciding.
                        let target = state.safety.epoch();
                        if state.safety.reconciled_epoch() < target {
                            let _elected = state.safety.claim_reconcile(target);
                            state.emergency_unlock();
                            state.safety.complete_reconcile(target);
                            drop(_work);
                            emit_safety_converged(app);
                        }
                    }
                }
            }
        }
    }
}

/// Converge UI + tray to the post-emergency safe state. Idempotent: safe to
/// call from the worker, the event pump, or a stale-command path.
pub(crate) fn emit_safety_converged(app: &tauri::AppHandle) {
    use crate::commands::keyboard::{CatLockPayload, EmergencyPayload, StateChangedPayload};
    if let Some(state) = app.try_state::<AppState>() {
        // P3: every payload carries the ordering stamp. The emergency event
        // fires only after complete_reconcile, so its (epoch, revision) is
        // authoritative truth the UI can floor on: anything older is stale.
        let epoch = state.safety.epoch();
        let runtime_revision = state.runtime_revision.load(Ordering::SeqCst);
        let _ = app.emit(
            "keyboard:emergency-unlock",
            EmergencyPayload {
                epoch,
                runtime_revision,
            },
        );
        let _ = app.emit(
            "keyboard:state-changed",
            StateChangedPayload {
                keys: state.snapshot_disabled(),
                safety_epoch: epoch,
                runtime_revision,
            },
        );
        let _ = app.emit(
            "keyboard:cat-lock",
            CatLockPayload {
                locked: false,
                safety_epoch: epoch,
                runtime_revision,
            },
        );
        let _ = app.emit("profile:changed", state.config.lock().clone());
        tray::refresh(app);
        tracing::warn!("emergency unlock converged (Default profile, all keys enabled)");
    }
}

/// Discard a mutating command that was in flight while an emergency press
/// landed: its intent must not re-disable keys after the unlock.
///
/// Convergence here is forced, not claim-based: the stale command's own
/// mutations may have landed *after* the worker's convergence, so election
/// alone cannot be relied on to repair them. The work is serialized with the
/// worker/pump paths via the convergence lock (blocking is fine: command
/// handlers run on a thread pool, never on the hook callback or UI thread),
/// and the completion marker is advanced only after the work finished.
/// `emergency_unlock` is idempotent (controller enable-all, Default
/// selection, cleared disable set, best-effort persist), and this path only
/// runs on a genuine race, never once per press.
pub(crate) fn discard_stale_command(app: &tauri::AppHandle, state: &AppState, reason: &'static str) {
    {
        let _work = state.safety.convergence_lock();
        let target = state.safety.epoch();
        let _elected = state.safety.claim_reconcile(target);
        state.emergency_unlock();
        state.safety.complete_reconcile(target);
    }
    emit_safety_converged(app);
    tracing::warn!("command discarded after emergency unlock ({reason})");
}

fn spawn_event_pump(app: tauri::AppHandle, rx: std::sync::mpsc::Receiver<HookEvent>) {
    std::thread::Builder::new()
        .name("keyglow-events".into())
        .spawn(move || {
            while let Ok(event) = rx.recv() {
                let handle = app.clone();
                let posted = handle.clone();
                // WebView2 eval must run on the UI thread; emitting from the
                // hook pump thread otherwise never reaches the keyboard graphic.
                if handle
                    .run_on_main_thread(move || dispatch_hook_event(&posted, event))
                    .is_err()
                {
                    dispatch_hook_event(&app, event);
                }
            }
        })
        .expect("failed to start event pump thread");
}

/// Safety worker for emergency unlock (HF-01, P2).
///
/// The hook thread only bumps the safety epoch and wakes this worker over an
/// unbounded channel, so a saturated UI event queue (or a delayed UI thread)
/// can never lose the unlock: the epoch latch is authoritative and the worker
/// converges config + controller + persist on its own thread. UI/tray events
/// still go through the main thread, mirroring the event pump.
///
/// Convergence protocol (P2): the worker loops on `pending_epoch()` and, for
/// each pending epoch, takes the convergence lock (blocking is fine here:
/// this is a dedicated background thread, never the hook callback or the UI
/// thread), re-checks under the lock, runs the idempotent
/// `emergency_unlock()`, and only then advances the completion marker. A
/// newer trigger landing mid-work is picked up by the next loop iteration,
/// and convergence work never overlaps with the pump fallback or the
/// stale-command path.
fn spawn_safety_worker(
    app: tauri::AppHandle,
    safety: Arc<SafetyState>,
    wake: Receiver<()>,
) {
    std::thread::Builder::new()
        .name("keyglow-safety".into())
        .spawn(move || {
            while wake.recv().is_ok() {
                // Coalesce rapid repeated presses; the epoch decides the work.
                while wake.try_recv().is_ok() {}
                let Some(state) = app.try_state::<AppState>() else {
                    continue;
                };
                let mut did_work = false;
                loop {
                    if safety.pending_epoch().is_none() {
                        break;
                    }
                    {
                        let _work = safety.convergence_lock();
                        // Re-check under the lock: another path may have
                        // converged while we were waiting for it.
                        if safety.reconciled_epoch() >= safety.epoch() {
                            break;
                        }
                        let target = safety.epoch();
                        let _elected = safety.claim_reconcile(target);
                        state.emergency_unlock();
                        safety.complete_reconcile(target);
                        did_work = true;
                    }
                    // Loop again: a newer epoch may have landed during the work.
                }
                if did_work {
                    let handle = app.clone();
                    let posted = handle.clone();
                    if handle
                        .run_on_main_thread(move || emit_safety_converged(&posted))
                        .is_err()
                    {
                        emit_safety_converged(&app);
                    }
                }
            }
        })
        .expect("failed to start safety worker thread");
}

/// Watch the hook thread's terminal report (HF-07).
///
/// The hook thread reports exactly once on exit. A requested shutdown is
/// silent; an unexpected death flips `hook_active`/`hook_error` — no more
/// stale "keyboard control active" — and emits `hook:status-changed` so the
/// UI updates without waiting for the next snapshot poll. The update runs
/// on this dedicated watcher thread, never on the hook callback.
fn spawn_hook_watcher(app: tauri::AppHandle, exit_rx: Receiver<HookExit>) {
    std::thread::Builder::new()
        .name("keyglow-hook-watch".into())
        .spawn(move || {
            while let Ok(exit) = exit_rx.recv() {
                // Records the exit in the lifecycle machine; true only when
                // this newly surfaced a failure (announce-once).
                if !keyboard::hook::lifecycle_note_thread_exit(&exit) {
                    continue;
                }
                let reason = exit.error.unwrap_or_else(|| {
                    "keyboard hook stopped unexpectedly; restart the app to restore keyboard control"
                        .to_string()
                });
                tracing::error!("keyboard hook thread died: {reason}");
                if let Some(state) = app.try_state::<AppState>() {
                    state.hook_active.store(false, Ordering::SeqCst);
                    *state.hook_error.lock() = Some(reason.clone());
                }
                let _ = app.emit(
                    "hook:status-changed",
                    commands::keyboard::HookStatusPayload {
                        hook_active: false,
                        hook_error: Some(reason),
                    },
                );
            }
        })
        .expect("failed to start hook watcher thread");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "keyglow=info,keyglow_lib=info".into()),
        )
        .with_target(false)
        .init();

    tracing::info!("KeyGlow starting");

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            set_key_enabled,
            enable_all_keys,
            set_cat_lock,
            select_layout,
            set_onboarded,
            set_start_with_windows,
            set_locale,
            set_theme,
            show_main_window,
            get_profiles,
            select_profile,
            create_profile,
            duplicate_profile,
            rename_profile,
            delete_profile,
            reset_profile,
            retry_persist,
            get_pressed_snapshot,
        ])
        .setup(|app| {
            let path = match app.path().app_config_dir() {
                Ok(dir) => dir.join("settings.json"),
                Err(err) => {
                    tracing::warn!("app config dir unavailable ({err}); using temp");
                    std::env::temp_dir().join("keyglow").join("settings.json")
                }
            };

            let config = crate::profiles::load(&path);
            let backend = start_input_backend();

            let keys = config
                .current_profile()
                .map(|p| crate::state::parse_key_ids(&p.disabled_keys))
                .unwrap_or_default();
            backend.controller.set_disabled_keys(&keys);

            if backend.hook_active {
                tracing::info!("keyboard control active");
            } else {
                tracing::error!(
                    "keyboard control unavailable: {:?}",
                    backend.hook_error
                );
            }

            let start_with_windows = config.start_with_windows;
            app.manage(AppState {
                controller: backend.controller,
                config: parking_lot::Mutex::new(config),
                config_path: path,
                hook_active: AtomicBool::new(backend.hook_active),
                hook_error: parking_lot::Mutex::new(backend.hook_error),
                shutdown: backend.shutdown,
                safety: backend.safety.clone(),
                persist_lock: parking_lot::Mutex::new(()),
                persisted_revision: AtomicU64::new(0),
                persist_error: parking_lot::Mutex::new(None),
                persist_error_kind: parking_lot::Mutex::new(None),
                runtime_revision: AtomicU64::new(0),
            });

            spawn_event_pump(app.handle().clone(), backend.events);
            spawn_safety_worker(
                app.handle().clone(),
                backend.safety,
                backend.safety_wake,
            );
            if let Some(exit_rx) = backend.hook_exit {
                spawn_hook_watcher(app.handle().clone(), exit_rx);
            }
            tray::setup(app.handle())?;

            if let Some(window) = app.get_webview_window("main") {
                if let Ok(size) = window.inner_size() {
                    if size.width < 1000 || size.height < 640 {
                        let _ = window.set_size(tauri::LogicalSize::new(1320.0, 860.0));
                    }
                }
                let _ = window.show();
                let _ = window.set_focus();
            }

            if start_with_windows {
                let _ = crate::commands::keyboard::apply_autostart(app.handle(), true);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        });

    let app = builder
        .build(tauri::generate_context!())
        .expect("failed to build KeyGlow");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            tracing::info!("KeyGlow exiting; releasing keyboard hook");
            if let Some(state) = app_handle.try_state::<AppState>() {
                (state.shutdown)();
            }
        }
    });
}

#[cfg(test)]
mod layout_tests {
    use crate::keyboard::KeyCode;
    use serde::Deserialize;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    struct Layout {
        id: String,
        keys: Vec<KeyDef>,
    }

    #[derive(Deserialize)]
    struct KeyDef {
        code: String,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        #[serde(default)]
        unsupported: bool,
    }

    #[test]
    fn layouts_are_valid() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/layouts");
        let mut found = 0;
        for entry in fs::read_dir(&dir).expect("layouts dir") {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            found += 1;
            let layout: Layout =
                serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            let mut codes = HashSet::new();
            for key in &layout.keys {
                assert!(key.w > 0.0 && key.h > 0.0, "{} has non-positive size", key.code);
                assert!(key.x >= 0.0 && key.y >= 0.0, "{} has negative origin", key.code);
                assert!(
                    codes.insert(key.code.clone()),
                    "duplicate {} in {}",
                    key.code,
                    layout.id
                );
                if !key.unsupported {
                    assert!(
                        KeyCode::from_id(&key.code).is_some(),
                        "unknown code {} in {}",
                        key.code,
                        layout.id
                    );
                }
            }
        }
        assert_eq!(found, 5, "expected five layout files");
    }
}
