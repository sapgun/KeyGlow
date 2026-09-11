use crate::keyboard::KeyCode;
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub hook_active: bool,
    pub hook_error: Option<String>,
    pub layout_id: String,
    pub profile_id: String,
    pub profiles: Vec<crate::profiles::Profile>,
    pub disabled_keys: Vec<String>,
    pub onboarded: bool,
    pub start_with_windows: bool,
    pub device_name: String,
    pub emergency_shortcut: String,
    pub cat_lock: bool,
    pub locale: String,
    pub theme: String,
}

#[tauri::command]
pub fn get_app_state(state: State<AppState>) -> AppSnapshot {
    let cfg = state.config.lock().clone();
    AppSnapshot {
        hook_active: state.is_hook_active(),
        hook_error: state.hook_error(),
        layout_id: cfg.selected_layout,
        profile_id: cfg.selected_profile,
        profiles: cfg.profiles,
        disabled_keys: state.snapshot_disabled(),
        onboarded: cfg.onboarded,
        start_with_windows: cfg.start_with_windows,
        device_name: "Generic Keyboard".into(),
        emergency_shortcut: "Ctrl+Shift+F12".into(),
        cat_lock: state.controller.is_cat_locked(),
        locale: cfg.locale,
        theme: cfg.theme,
    }
}

#[tauri::command]
pub fn set_key_enabled(
    app: AppHandle,
    state: State<AppState>,
    code: String,
    enabled: bool,
) -> Result<Vec<String>, String> {
    let key = KeyCode::from_id(&code).ok_or_else(|| format!("unknown key: {code}"))?;
    if enabled {
        state.controller.enable_key(key);
    } else {
        state.controller.disable_key(key);
    }

    {
        let mut cfg = state.config.lock();
        if let Some(profile) = cfg.current_profile_mut() {
            profile.disabled_keys.retain(|k| k != &code);
            if !enabled {
                profile.disabled_keys.push(code.clone());
                profile.disabled_keys.sort();
                profile.disabled_keys.dedup();
            }
        }
    }
    state.persist()?;
    let disabled = state.snapshot_disabled();
    let _ = app.emit("keyboard:state-changed", &disabled);
    Ok(disabled)
}

#[tauri::command]
pub fn enable_all_keys(app: AppHandle, state: State<AppState>) -> Result<Vec<String>, String> {
    state.controller.enable_all();
    {
        let mut cfg = state.config.lock();
        if let Some(profile) = cfg.current_profile_mut() {
            profile.disabled_keys.clear();
        }
    }
    state.persist()?;
    let disabled = state.snapshot_disabled();
    let _ = app.emit("keyboard:state-changed", &disabled);
    let _ = app.emit("keyboard:cat-lock", false);
    tracing::info!("enable all keys");
    Ok(disabled)
}

#[tauri::command]
pub fn set_cat_lock(app: AppHandle, state: State<AppState>, locked: bool) -> Result<bool, String> {
    state.controller.set_cat_lock(locked);
    let _ = app.emit("keyboard:cat-lock", locked);
    crate::tray::refresh(&app);
    tracing::info!(locked, "cat lock");
    Ok(locked)
}

#[tauri::command]
pub fn select_layout(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    let valid = [
        "fullsize-ansi",
        "tkl-ansi",
        "75-ansi",
        "65-ansi",
        "60-ansi",
    ];
    if !valid.contains(&id.as_str()) {
        return Err(format!("unknown layout: {id}"));
    }
    {
        let mut cfg = state.config.lock();
        cfg.selected_layout = id.clone();
        if let Some(profile) = cfg.current_profile_mut() {
            profile.layout = id;
        }
        cfg.onboarded = true;
    }
    state.persist()?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    Ok(())
}

#[tauri::command]
pub fn set_onboarded(state: State<AppState>, onboarded: bool) -> Result<(), String> {
    state.config.lock().onboarded = onboarded;
    state.persist()
}

#[tauri::command]
pub fn set_start_with_windows(
    app: AppHandle,
    state: State<AppState>,
    enabled: bool,
) -> Result<bool, String> {
    apply_autostart(&app, enabled)?;
    state.config.lock().start_with_windows = enabled;
    state.persist()?;
    Ok(enabled)
}

#[tauri::command]
pub fn set_locale(app: AppHandle, state: State<AppState>, locale: String) -> Result<String, String> {
    let normalized = match locale.as_str() {
        "ko" => "ko",
        "ja" => "ja",
        "en" => "en",
        _ => return Err(format!("unsupported locale: {locale}")),
    };
    state.config.lock().locale = normalized.to_string();
    state.persist()?;
    crate::tray::refresh(&app);
    Ok(normalized.to_string())
}

#[tauri::command]
pub fn set_theme(state: State<AppState>, theme: String) -> Result<String, String> {
    let normalized = match theme.as_str() {
        "light" => "light",
        "dark" => "dark",
        _ => return Err(format!("unsupported theme: {theme}")),
    };
    state.config.lock().theme = normalized.to_string();
    state.persist()?;
    Ok(normalized.to_string())
}

pub fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable().map_err(|e| e.to_string())?;
    } else {
        launcher.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    crate::show_main(&app);
}
