mod commands;
mod i18n;
mod keyboard;
mod platform;
mod profiles;
mod state;
mod tray;

use commands::*;
use keyboard::hook::HookEvent;
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

fn emit_key(app: &tauri::AppHandle, event: &str, code: &str) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if let Err(err) = app.emit(event, code) {
        if !WARNED.swap(true, Ordering::Relaxed) {
            tracing::error!("failed to emit {event} to UI: {err}");
        }
    }
}

fn dispatch_hook_event(app: &tauri::AppHandle, event: HookEvent) {
    match event {
        HookEvent::KeyDown { code } => emit_key(app, "keyboard:key-down", code.as_str()),
        HookEvent::KeyUp { code } => emit_key(app, "keyboard:key-up", code.as_str()),
        HookEvent::EmergencyUnlock { epoch } => {
            if let Some(state) = app.try_state::<AppState>() {
                // Fallback path: normally the safety worker already claimed
                // this epoch. If the worker is gone, a surviving queue event
                // still converges the state exactly once.
                if state.safety.claim_reconcile(epoch) {
                    state.emergency_unlock();
                    emit_safety_converged(app);
                }
            }
        }
    }
}

/// Converge UI + tray to the post-emergency safe state. Idempotent: safe to
/// call from the worker, the event pump, or a stale-command path.
pub(crate) fn emit_safety_converged(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        let _ = app.emit("keyboard:emergency-unlock", ());
        let _ = app.emit("keyboard:state-changed", state.snapshot_disabled());
        let _ = app.emit("keyboard:cat-lock", false);
        let _ = app.emit("profile:changed", state.config.lock().clone());
        tray::refresh(app);
        tracing::warn!("emergency unlock converged (Default profile, all keys enabled)");
    }
}

/// Discard a mutating command that was in flight while an emergency press
/// landed: its intent must not re-disable keys after the unlock.
///
/// Convergence here is forced, not claim-based: the stale command's own
/// mutations may have landed *after* the worker's exactly-once convergence,
/// so the claim cannot be relied on to repair them. `emergency_unlock` is
/// idempotent (controller enable-all, Default selection, cleared disable
/// set, best-effort persist), and this path only runs on a genuine race,
/// never once per press.
pub(crate) fn discard_stale_command(app: &tauri::AppHandle, state: &AppState, reason: &'static str) {
    state.emergency_unlock();
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

/// Safety worker for emergency unlock (HF-01).
///
/// The hook thread only bumps the safety epoch and wakes this worker over an
/// unbounded channel, so a saturated UI event queue (or a delayed UI thread)
/// can never lose the unlock: the epoch latch is authoritative and the worker
/// converges config + controller + persist on its own thread. UI/tray events
/// still go through the main thread, mirroring the event pump.
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
                let epoch = safety.epoch();
                let Some(state) = app.try_state::<AppState>() else {
                    continue;
                };
                if !safety.claim_reconcile(epoch) {
                    continue;
                }
                state.emergency_unlock();
                let handle = app.clone();
                let posted = handle.clone();
                if handle
                    .run_on_main_thread(move || emit_safety_converged(&posted))
                    .is_err()
                {
                    emit_safety_converged(&app);
                }
            }
        })
        .expect("failed to start safety worker thread");
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
            });

            spawn_event_pump(app.handle().clone(), backend.events);
            spawn_safety_worker(
                app.handle().clone(),
                backend.safety,
                backend.safety_wake,
            );
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
