mod commands;
mod i18n;
mod keyboard;
mod platform;
mod profiles;
mod state;
mod tray;

use commands::*;
use keyboard::hook::HookEvent;
use platform::start_input_backend;
use state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
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
        HookEvent::EmergencyUnlock => {
            if let Some(state) = app.try_state::<AppState>() {
                state.emergency_unlock();
                let _ = app.emit("keyboard:emergency-unlock", ());
                let _ = app.emit("keyboard:state-changed", state.snapshot_disabled());
                let _ = app.emit("keyboard:cat-lock", false);
                let _ = app.emit("profile:changed", state.config.lock().clone());
                tray::refresh(app);
                tracing::warn!("emergency unlock triggered (Ctrl+Shift+F12)");
            }
        }
    }
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
            show_main_window,
            get_profiles,
            select_profile,
            create_profile,
            duplicate_profile,
            rename_profile,
            delete_profile,
            reset_profile,
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
            });

            spawn_event_pump(app.handle().clone(), backend.events);
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
