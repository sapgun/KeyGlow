use crate::keyboard::KeyCode;
use crate::profiles::app_profiles::{AppRule, EffectiveChange, MatchReason};
use crate::profiles::layout_import::{validate_imported_layout, ImportedLayout};
use crate::profiles::layout_registry::is_valid_layout_id;
use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub hook_active: bool,
    pub hook_error: Option<String>,
    pub layout_id: String,
    /// Effective profile (S5): the profile actually filtering keys now.
    /// It may differ from the persisted manual selection when an app
    /// rule matched; `match_reason` says why.
    pub profile_id: String,
    pub profiles: Vec<crate::profiles::Profile>,
    /// Imported custom layouts (S6). Builtin layouts come from the
    /// frontend registry; only customs cross the IPC boundary here.
    pub custom_layouts: Vec<crate::profiles::layout_import::ImportedLayout>,
    pub disabled_keys: Vec<String>,
    pub onboarded: bool,
    pub start_with_windows: bool,
    pub device_name: String,
    pub emergency_shortcut: String,
    pub cat_lock: bool,
    pub locale: String,
    pub theme: String,
    /// Whether the on-disk settings match the in-memory config (HF-03).
    pub persisted: bool,
    /// Last persist failure message, if the on-disk config is stale.
    pub persist_error: Option<String>,
    /// Machine-readable kind of the failure ("io_error" | "newer_version").
    pub persist_error_kind: Option<String>,
    /// Increments after every successful persist.
    pub config_revision: u64,
    /// Latest emergency epoch recorded by the hook at snapshot time (P3).
    pub safety_epoch: u64,
    /// Highest emergency epoch fully converged at snapshot time (P3).
    pub safety_reconciled: u64,
    /// Monotonic runtime-state revision (P3). Distinct from
    /// `config_revision`: this orders runtime state, that one orders disk
    /// writes. Never compare the two against each other.
    pub runtime_revision: u64,
    /// Why the effective profile is active (S5).
    pub match_reason: MatchReason,
    /// Automation paused after an emergency until an explicit resume (S5).
    pub auto_profile_paused: bool,
    /// The manual selection, if any (S5). Null when automation decides.
    pub manual_override: Option<String>,
    /// Normalized exe of the current foreground app, if known (S5).
    pub active_exe: Option<String>,
    /// Persisted app rules (S5).
    pub app_rules: Vec<AppRule>,
    /// Master switch for automatic profile switching (S5).
    pub auto_profile_enabled: bool,
}

/// Ordering envelope for state-affecting native events (P3). The UI applies
/// an event only when (safety_epoch, runtime_revision) is not older than
/// what it already shows, so a late or duplicate event can never overwrite
/// newer state (e.g. post-emergency truth).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StateChangedPayload {
    pub keys: Vec<String>,
    pub safety_epoch: u64,
    pub runtime_revision: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CatLockPayload {
    pub locked: bool,
    pub safety_epoch: u64,
    pub runtime_revision: u64,
}

/// Emitted only after an emergency epoch fully converged (P3). Carries the
/// converged epoch so the UI can floor its ordering stamp: anything older
/// (a late hydrate, a stale command response) is discarded instead of
/// overwriting the safe state.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EmergencyPayload {
    pub epoch: u64,
    pub runtime_revision: u64,
}

/// Emitted when the foreground observer switches the effective profile
/// (S5). Carries the P3 ordering stamp so the UI gates it like any other
/// native state event: a late auto-switch can never overwrite newer
/// (e.g. post-emergency or manual) state.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AutoSwitchedPayload {
    pub profile_id: String,
    pub match_reason: MatchReason,
    pub active_exe: Option<String>,
    pub safety_epoch: u64,
    pub runtime_revision: u64,
}

pub(crate) fn auto_switched_payload(
    state: &AppState,
    change: &EffectiveChange,
) -> AutoSwitchedPayload {
    let (safety_epoch, runtime_revision) = state.event_stamp();
    AutoSwitchedPayload {
        profile_id: change.profile_id.clone(),
        match_reason: change.reason,
        active_exe: change.exe.clone(),
        safety_epoch,
        runtime_revision,
    }
}

/// Health of the keyboard hook thread (HF-07). Emitted as
/// `hook:status-changed` when the hook thread dies on its own; the UI flips
/// the status bar and shows the restart guidance. Never emitted on a
/// requested shutdown.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HookStatusPayload {
    pub hook_active: bool,
    pub hook_error: Option<String>,
}

pub(crate) fn state_changed_payload(state: &AppState) -> StateChangedPayload {
    let (safety_epoch, runtime_revision) = state.event_stamp();
    StateChangedPayload {
        keys: state.snapshot_disabled(),
        safety_epoch,
        runtime_revision,
    }
}

fn cat_lock_payload(state: &AppState, locked: bool) -> CatLockPayload {
    let (safety_epoch, runtime_revision) = state.event_stamp();
    CatLockPayload {
        locked,
        safety_epoch,
        runtime_revision,
    }
}

#[tauri::command]
pub fn get_app_state(state: State<AppState>) -> AppSnapshot {
    let cfg = state.config.lock().clone();
    let (persisted, persist_error, persist_error_kind, config_revision) = state.persist_state();
    let (effective, manual, reason, paused, active_exe, _) = state.app_profile_status();
    AppSnapshot {
        hook_active: state.is_hook_active(),
        hook_error: state.hook_error(),
        layout_id: cfg.selected_layout,
        profile_id: effective,
        profiles: cfg.profiles,
        custom_layouts: cfg.custom_layouts,
        disabled_keys: state.snapshot_disabled(),
        onboarded: cfg.onboarded,
        start_with_windows: cfg.start_with_windows,
        device_name: "Generic Keyboard".into(),
        emergency_shortcut: "Ctrl+Shift+F12".into(),
        cat_lock: state.controller.is_cat_locked(),
        locale: cfg.locale,
        theme: cfg.theme,
        persisted,
        persist_error,
        persist_error_kind,
        config_revision,
        safety_epoch: state.safety.epoch(),
        safety_reconciled: state.safety.reconciled_epoch(),
        runtime_revision: state.runtime_revision.load(Ordering::SeqCst),
        match_reason: reason,
        auto_profile_paused: paused,
        manual_override: manual,
        active_exe,
        app_rules: cfg.app_rules,
        auto_profile_enabled: cfg.auto_profile_enabled,
    }
}

/// Best-effort re-save after a persist failure (UI "Retry" action, HF-03).
#[tauri::command]
pub fn retry_persist(state: State<AppState>) -> Result<(), String> {
    state.retry_persist()
}

#[tauri::command]
pub fn set_key_enabled(
    app: AppHandle,
    state: State<AppState>,
    code: String,
    enabled: bool,
) -> Result<StateChangedPayload, String> {
    let entered = state.safety_epoch();
    let key = KeyCode::from_id(&code).ok_or_else(|| format!("unknown key: {code}"))?;
    if enabled {
        state.controller.enable_key(key);
    } else {
        state.controller.disable_key(key);
    }

    {
        // S5: key edits target the *effective* profile — the one actually
        // filtering keys — which may be an app-rule match rather than the
        // persisted manual selection.
        let effective = state.effective_profile_id();
        let mut cfg = state.config.lock();
        if let Some(profile) = cfg.profile_mut(&effective) {
            profile.disabled_keys.retain(|k| k != &code);
            if !enabled {
                profile.disabled_keys.push(code.clone());
                profile.disabled_keys.sort();
                profile.disabled_keys.dedup();
            }
        }
    }
    // Order this mutation for the UI (P3), before the staleness checks below.
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        // Emergency press landed mid-command: discard, never persist stale.
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(state_changed_payload(&state));
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        // Emergency press landed during persist: the stale write may have
        // beaten the worker's convergence to disk; force safe state back.
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(state_changed_payload(&state));
    }
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    Ok(state_changed_payload(&state))
}

#[tauri::command]
pub fn enable_all_keys(app: AppHandle, state: State<AppState>) -> Result<StateChangedPayload, String> {
    let entered = state.safety_epoch();
    state.controller.enable_all();
    {
        // S5: clear the *effective* profile's disabled set (see
        // set_key_enabled).
        let effective = state.effective_profile_id();
        let mut cfg = state.config.lock();
        if let Some(profile) = cfg.profile_mut(&effective) {
            profile.disabled_keys.clear();
        }
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(state_changed_payload(&state));
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(state_changed_payload(&state));
    }
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    let _ = app.emit("keyboard:cat-lock", cat_lock_payload(&state, false));
    tracing::info!("enable all keys");
    Ok(state_changed_payload(&state))
}

#[tauri::command]
pub fn set_cat_lock(app: AppHandle, state: State<AppState>, locked: bool) -> Result<CatLockPayload, String> {
    let entered = state.safety_epoch();
    state.controller.set_cat_lock(locked);
    // Runtime state changed even though cat lock never touches the disk (P3).
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        // A stale lock/unlock must not override the post-emergency state.
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(cat_lock_payload(&state, false));
    }
    let _ = app.emit("keyboard:cat-lock", cat_lock_payload(&state, locked));
    crate::tray::refresh(&app);
    tracing::info!(locked, "cat lock");
    Ok(cat_lock_payload(&state, locked))
}

#[tauri::command]
pub fn select_layout(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    {
        let mut cfg = state.config.lock();
        if !is_valid_layout_id(&id, &cfg.custom_layouts) {
            return Err(format!("unknown layout: {id}"));
        }
        cfg.selected_layout = id.clone();
        if let Some(profile) = cfg.current_profile_mut() {
            profile.layout = id;
        }
        cfg.onboarded = true;
    }
    state.bump_runtime_revision();
    state.persist()?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    Ok(())
}

/// S6: import a layout from a JSON string. The JSON is validated
/// (size, key count, coordinates, id format, unknown fields, duplicate
/// codes, overlaps) before anything is stored; on failure the config
/// is untouched and the current layout stays selected.
#[tauri::command]
pub fn import_layout(
    app: AppHandle,
    state: State<AppState>,
    json: String,
) -> Result<ImportedLayout, String> {
    let layout = validate_imported_layout(&json)?;
    {
        let mut cfg = state.config.lock();
        if cfg.custom_layouts.iter().any(|l| l.id == layout.id) {
            return Err(format!("layout id already exists: {}", layout.id));
        }
        cfg.custom_layouts.push(layout.clone());
    }
    state.bump_runtime_revision();
    state.persist()?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    tracing::info!(
        id = layout.id,
        keys = layout.keys.len(),
        "imported custom layout"
    );
    Ok(layout)
}

/// S6: delete an imported custom layout. Refused while any profile
/// (or the current selection) still uses it, so no profile is left
/// pointing at a missing layout.
#[tauri::command]
pub fn delete_custom_layout(
    app: AppHandle,
    state: State<AppState>,
    id: String,
) -> Result<(), String> {
    {
        let mut cfg = state.config.lock();
        if !cfg.custom_layouts.iter().any(|l| l.id == id) {
            return Err(format!("unknown custom layout: {id}"));
        }
        if cfg.selected_layout == id || cfg.profiles.iter().any(|p| p.layout == id) {
            return Err(format!("layout {id} is in use by a profile"));
        }
        cfg.custom_layouts.retain(|l| l.id != id);
    }
    state.bump_runtime_revision();
    state.persist()?;
    let _ = app.emit("profile:changed", state.config.lock().clone());
    tracing::info!(id, "deleted custom layout");
    Ok(())
}

/// S6: list imported custom layouts (builtin layouts come from the
/// frontend registry, not from here).
#[tauri::command]
pub fn list_custom_layouts(state: State<AppState>) -> Vec<ImportedLayout> {
    state.config.lock().custom_layouts.clone()
}

#[tauri::command]
pub fn set_onboarded(state: State<AppState>, onboarded: bool) -> Result<(), String> {
    state.config.lock().onboarded = onboarded;
    state.bump_runtime_revision();
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
    state.bump_runtime_revision();
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
    state.bump_runtime_revision();
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
    state.bump_runtime_revision();
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

/// Native pressed snapshot for UI resync (HF-04). Returns the keys the hook
/// currently considers physically held plus a sequence number that bumps on
/// every processed hook event. The UI uses it to verify long holds and to
/// recover from dropped key-up events instead of guessing with a timer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PressedSnapshot {
    pub pressed: Vec<String>,
    pub sequence: u64,
}

#[tauri::command]
pub fn get_pressed_snapshot(state: State<AppState>) -> PressedSnapshot {
    PressedSnapshot {
        pressed: state
            .controller
            .pressed_keys()
            .into_iter()
            .map(|k| k.as_str().to_string())
            .collect(),
        sequence: state.controller.event_sequence(),
    }
}
