use crate::profiles::{Profile, DEFAULT_PROFILE_ID};
use crate::state::{parse_key_ids, AppState};
use crate::tray;
use super::keyboard::state_changed_payload;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn get_profiles(state: State<AppState>) -> Vec<Profile> {
    state.config.lock().profiles.clone()
}

#[tauri::command]
pub fn select_profile(app: AppHandle, state: State<AppState>, id: String) -> Result<Profile, String> {
    let entered = state.safety_epoch();
    {
        let mut cfg = state.config.lock();
        // Common activation policy (HF-06): selection syncs selected_layout.
        cfg.activate_profile(&id)?;
    }
    // Order this mutation for the UI (P3).
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        // Stale selection must not re-apply a disabled set after the unlock.
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return converged_profile(&state);
    }
    state.apply_current_profile();
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return converged_profile(&state);
    }
    tracing::info!("profile selected: {id}");
    let profile = state
        .config
        .lock()
        .current_profile()
        .cloned()
        .ok_or_else(|| "profile missing after select".to_string())?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    tray::refresh(&app);
    Ok(profile)
}

/// Authoritative profile after emergency convergence (Default when present).
/// Returned by stale profile commands so callers report converged state.
fn converged_profile(state: &AppState) -> Result<Profile, String> {
    state
        .config
        .lock()
        .current_profile()
        .cloned()
        .ok_or_else(|| "profile missing after emergency converge".to_string())
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
    let layout = state.config.lock().selected_layout.clone();
    let id = {
        let cfg = state.config.lock();
        unique_id(name, &cfg.profiles)
    };
    let profile = Profile {
        id: id.clone(),
        name: name.to_string(),
        layout,
        disabled_keys: vec![],
        builtin: false,
    };
    let entered = state.safety_epoch();
    {
        let mut cfg = state.config.lock();
        cfg.profiles.push(profile.clone());
        // Common activation policy (HF-06).
        cfg.activate_profile(&id)?;
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        // The profile itself is harmless (no disabled keys); only the
        // selection is reverted to the safe state.
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(profile);
    }
    state.apply_current_profile();
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(profile);
    }
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
    let copy_id = {
        let cfg = state.config.lock();
        unique_id(&source.name, &cfg.profiles)
    };
    let copy = Profile {
        id: copy_id.clone(),
        name: format!("{} Copy", source.name),
        layout: source.layout,
        disabled_keys: source.disabled_keys,
        builtin: false,
    };
    let entered = state.safety_epoch();
    {
        let mut cfg = state.config.lock();
        cfg.profiles.push(copy.clone());
        // Common activation policy (HF-06): the duplicate's own layout
        // becomes selected_layout. This was the reported invariant break:
        // selected_profile pointed at the copy while selected_layout still
        // showed the source's old layout context.
        cfg.activate_profile(&copy_id)?;
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        // A stale duplicate may carry a disabled set; never apply it after
        // the unlock. The copy itself stays in the list, unselected.
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(copy);
    }
    state.apply_current_profile();
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(copy);
    }
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
    state.bump_runtime_revision();
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
    let entered = state.safety_epoch();
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
            // Common activation policy (HF-06), best-effort: fall back to
            // Default and sync its layout.
            let _ = cfg.activate_profile(DEFAULT_PROFILE_ID);
        }
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(());
    }
    state.apply_current_profile();
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(());
    }
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn reset_profile(app: AppHandle, state: State<AppState>, id: String) -> Result<Profile, String> {
    let entered = state.safety_epoch();
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
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        // A stale reset must not re-apply a disabled set after the unlock.
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return converged_profile(&state);
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
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return converged_profile(&state);
    }
    let profile = state
        .config
        .lock()
        .profile(&id)
        .cloned()
        .ok_or_else(|| "profile missing after reset".to_string())?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    Ok(profile)
}

/// Collision-resistant profile id (HF-06): millisecond timestamps alone
/// can collide when two profiles are created in the same millisecond, so
/// the id mixes a process-wide atomic counter and the process id, and the
/// caller verifies uniqueness against the profiles already on file.
static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_id(name: &str, existing: &[Profile]) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(24)
        .collect();
    let slug = if slug.is_empty() {
        "profile".to_string()
    } else {
        slug
    };
    let pid = std::process::id();
    loop {
        let ctr = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let id = format!("{slug}-{nanos:x}-{pid}-{ctr}");
        if !existing.iter().any(|p| p.id == id) {
            return id;
        }
        // Practically unreachable: nanos + pid + per-process counter.
    }
}
