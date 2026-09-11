use crate::profiles::{Profile, DEFAULT_PROFILE_ID};
use crate::state::{parse_key_ids, AppState};
use crate::tray;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn get_profiles(state: State<AppState>) -> Vec<Profile> {
    state.config.lock().profiles.clone()
}

#[tauri::command]
pub fn select_profile(app: AppHandle, state: State<AppState>, id: String) -> Result<Profile, String> {
    {
        let mut cfg = state.config.lock();
        if cfg.profile(&id).is_none() {
            return Err(format!("unknown profile: {id}"));
        }
        cfg.selected_profile = id.clone();
        if let Some(layout) = cfg.profile(&id).map(|p| p.layout.clone()) {
            cfg.selected_layout = layout;
        }
    }
    state.apply_current_profile();
    state.persist()?;
    tracing::info!("profile selected: {id}");
    let profile = state
        .config
        .lock()
        .current_profile()
        .cloned()
        .ok_or_else(|| "profile missing after select".to_string())?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state.snapshot_disabled());
    tray::refresh(&app);
    Ok(profile)
}

#[tauri::command]
pub fn create_profile(
    app: AppHandle,
    state: State<AppState>,
    name: String,
) -> Result<Profile, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("profile name is required".into());
    }
    let id = unique_id(name);
    let layout = state.config.lock().selected_layout.clone();
    let profile = Profile {
        id: id.clone(),
        name: name.to_string(),
        layout,
        disabled_keys: vec![],
        builtin: false,
    };
    state.config.lock().profiles.push(profile.clone());
    state.config.lock().selected_profile = id;
    state.apply_current_profile();
    state.persist()?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    tray::refresh(&app);
    Ok(profile)
}

#[tauri::command]
pub fn duplicate_profile(
    app: AppHandle,
    state: State<AppState>,
    id: String,
) -> Result<Profile, String> {
    let source = state
        .config
        .lock()
        .profile(&id)
        .cloned()
        .ok_or_else(|| format!("unknown profile: {id}"))?;
    let copy = Profile {
        id: unique_id(&source.name),
        name: format!("{} Copy", source.name),
        layout: source.layout,
        disabled_keys: source.disabled_keys,
        builtin: false,
    };
    state.config.lock().profiles.push(copy.clone());
    state.config.lock().selected_profile = copy.id.clone();
    state.apply_current_profile();
    state.persist()?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    tray::refresh(&app);
    Ok(copy)
}

#[tauri::command]
pub fn rename_profile(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    name: String,
) -> Result<Profile, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("profile name is required".into());
    }
    {
        let mut cfg = state.config.lock();
        let profile = cfg
            .profile_mut(&id)
            .ok_or_else(|| format!("unknown profile: {id}"))?;
        profile.name = name;
    }
    state.persist()?;
    let profile = state
        .config
        .lock()
        .profile(&id)
        .cloned()
        .ok_or_else(|| "profile missing after rename".to_string())?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    tray::refresh(&app);
    Ok(profile)
}

#[tauri::command]
pub fn delete_profile(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    if id == DEFAULT_PROFILE_ID {
        return Err("the Default profile cannot be deleted".into());
    }
    {
        let mut cfg = state.config.lock();
        if cfg.profiles.len() <= 1 {
            return Err("cannot delete the last profile".into());
        }
        if !cfg.profiles.iter().any(|p| p.id == id) {
            return Err(format!("unknown profile: {id}"));
        }
        cfg.profiles.retain(|p| p.id != id);
        if cfg.selected_profile == id {
            cfg.selected_profile = DEFAULT_PROFILE_ID.to_string();
            if let Some(layout) = cfg.profile(DEFAULT_PROFILE_ID).map(|p| p.layout.clone()) {
                cfg.selected_layout = layout;
            }
        }
    }
    state.apply_current_profile();
    state.persist()?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state.snapshot_disabled());
    tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn reset_profile(app: AppHandle, state: State<AppState>, id: String) -> Result<Profile, String> {
    {
        let mut cfg = state.config.lock();
        let profile = cfg
            .profile_mut(&id)
            .ok_or_else(|| format!("unknown profile: {id}"))?;
        if profile.id == "gaming" {
            profile.disabled_keys = vec!["MetaLeft".into(), "MetaRight".into()];
        } else {
            profile.disabled_keys.clear();
        }
    }
    if state.config.lock().selected_profile == id {
        let keys = {
            let cfg = state.config.lock();
            cfg.profile(&id)
                .map(|p| parse_key_ids(&p.disabled_keys))
                .unwrap_or_default()
        };
        state.controller.set_disabled_keys(&keys);
    }
    state.persist()?;
    let profile = state
        .config
        .lock()
        .profile(&id)
        .cloned()
        .ok_or_else(|| "profile missing after reset".to_string())?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state.snapshot_disabled());
    Ok(profile)
}

fn unique_id(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(24)
        .collect();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let slug = if slug.is_empty() {
        "profile".to_string()
    } else {
        slug
    };
    format!("{slug}-{stamp}")
}
