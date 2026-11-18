use crate::profiles::app_profiles::{validate_rule, AppRule, MatchReason};
use crate::profiles::models::{LayerDef, MacroDef, RemapRule};
use crate::profiles::{
    validate_layer, validate_macro, validate_remap_rule, Profile, DEFAULT_PROFILE_ID,
};
use crate::state::AppState;
use crate::tray;
use super::keyboard::state_changed_payload;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

/// Runtime app-profile status for the UI (S5). Carries the P3 ordering
/// stamp so the UI can gate it like any other native state.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppProfileStatusPayload {
    pub active_profile_id: String,
    pub manual_override: Option<String>,
    pub match_reason: MatchReason,
    pub safety_paused: bool,
    pub auto_enabled: bool,
    pub current_exe: Option<String>,
    pub revision: u64,
    pub safety_epoch: u64,
    pub runtime_revision: u64,
}

pub fn app_profile_status_payload(state: &AppState) -> AppProfileStatusPayload {
    let (active, manual, reason, paused, exe, revision) = state.app_profile_status();
    let (safety_epoch, runtime_revision) = state.event_stamp();
    AppProfileStatusPayload {
        active_profile_id: active,
        manual_override: manual,
        match_reason: reason,
        safety_paused: paused,
        auto_enabled: state.config.lock().auto_profile_enabled,
        current_exe: exe,
        revision,
        safety_epoch,
        runtime_revision,
    }
}

/// Re-evaluate the current foreground app after a rule/config change and
/// announce the outcome like any other profile change. The effective
/// profile may have moved even though nothing was persisted by this step
/// (the switch itself is runtime-only).
fn announce_auto_switch(app: &AppHandle, state: &AppState) {
    if let Some(change) = state.reevaluate_foreground() {
        let _ = app.emit("profile:changed", state.config.lock().clone());
        let _ = app.emit("keyboard:state-changed", state_changed_payload(state));
        let _ = app.emit(
            "profile:auto-switched",
            crate::commands::keyboard::auto_switched_payload(state, &change),
        );
        tray::refresh(app);
    }
}

#[tauri::command]
pub fn get_app_rules(state: State<AppState>) -> Vec<AppRule> {
    state.config.lock().app_rules.clone()
}

#[tauri::command]
pub fn get_app_profile_status(state: State<AppState>) -> AppProfileStatusPayload {
    app_profile_status_payload(&state)
}

/// Create or update one app rule. The rule's target profile must exist
/// (save-time rejection); the rule id is generated when empty. Persists
/// like other settings commands, then re-evaluates the current foreground
/// app in case the new rule matches it right now.
#[tauri::command]
pub fn save_app_rule(
    app: AppHandle,
    state: State<AppState>,
    rule: AppRule,
) -> Result<AppRule, String> {
    let valid_ids: Vec<String> = state
        .config
        .lock()
        .profiles
        .iter()
        .map(|p| p.id.clone())
        .collect();
    validate_rule(&rule, &valid_ids)?;
    let rule = if rule.id.trim().is_empty() {
        AppRule {
            id: unique_rule_id(&rule),
            ..rule
        }
    } else {
        rule
    };
    let entered = state.safety_epoch();
    {
        let mut cfg = state.config.lock();
        if let Some(existing) = cfg.app_rules.iter_mut().find(|r| r.id == rule.id) {
            *existing = rule.clone();
        } else {
            cfg.app_rules.push(rule.clone());
        }
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(rule);
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(rule);
    }
    announce_auto_switch(&app, &state);
    let _ = app.emit("profile:changed", state.config.lock().clone());
    Ok(rule)
}

#[tauri::command]
pub fn delete_app_rule(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    let entered = state.safety_epoch();
    {
        let mut cfg = state.config.lock();
        if !cfg.app_rules.iter().any(|r| r.id == id) {
            return Err(format!("unknown app rule: {id}"));
        }
        cfg.app_rules.retain(|r| r.id != id);
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(());
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(());
    }
    announce_auto_switch(&app, &state);
    let _ = app.emit("profile:changed", state.config.lock().clone());
    Ok(())
}

#[tauri::command]
pub fn set_auto_profile_enabled(
    app: AppHandle,
    state: State<AppState>,
    enabled: bool,
) -> Result<bool, String> {
    {
        let mut cfg = state.config.lock();
        cfg.auto_profile_enabled = enabled;
    }
    state.bump_runtime_revision();
    state.persist()?;
    announce_auto_switch(&app, &state);
    let _ = app.emit("profile:changed", state.config.lock().clone());
    Ok(enabled)
}

/// S8: create one remap rule on a profile. Save-time validation
/// (V1–V4) rejects with a surfaced error, never silently fixed. If the
/// profile is effective, the new rules apply now (flush held targets
/// first, then install); otherwise they apply when the profile becomes
/// effective. Follows the save_app_rule stale-command pattern (P2).
#[tauri::command]
pub fn save_remap_rule(
    app: AppHandle,
    state: State<AppState>,
    profile_id: String,
    from: String,
    to: String,
) -> Result<RemapRule, String> {
    let entered = state.safety_epoch();
    let rule = {
        let cfg = state.config.lock();
        let profile = cfg
            .profile(&profile_id)
            .ok_or_else(|| format!("unknown profile: {profile_id}"))?;
        validate_remap_rule(&from, &to, profile)?
    };
    {
        let mut cfg = state.config.lock();
        let profile = cfg
            .profile_mut(&profile_id)
            .ok_or_else(|| format!("unknown profile: {profile_id}"))?;
        profile.remappings.push(rule.clone());
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(rule);
    }
    if state.effective_profile_id() == profile_id {
        state.apply_effective_profile_rules();
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(rule);
    }
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    tracing::info!("remap rule saved: {profile_id} {} -> {}", rule.from, rule.to);
    Ok(rule)
}

/// S8: delete one remap rule (by source) from a profile. Deleting a
/// rule flushes held targets of the old set first (D6), so a key held
/// while its rule is deleted releases the down-time target.
#[tauri::command]
pub fn delete_remap_rule(
    app: AppHandle,
    state: State<AppState>,
    profile_id: String,
    from: String,
) -> Result<(), String> {
    let entered = state.safety_epoch();
    {
        let mut cfg = state.config.lock();
        let profile = cfg
            .profile_mut(&profile_id)
            .ok_or_else(|| format!("unknown profile: {profile_id}"))?;
        if !profile.remappings.iter().any(|r| r.from == from) {
            return Err(format!("unknown remap rule: {from}"));
        }
        profile.remappings.retain(|r| r.from != from);
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(());
    }
    if state.effective_profile_id() == profile_id {
        state.apply_effective_profile_rules();
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(());
    }
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    tracing::info!("remap rule deleted: {profile_id} {from}");
    Ok(())
}

/// S9: create or replace one macro on a profile (replace by id).
/// Save-time validation rejects with a surfaced error, never silently
/// fixed. If the profile is effective, the new definitions apply now (a
/// running macro is cancelled first); otherwise they apply when the
/// profile becomes effective. Follows the save_remap_rule stale-command
/// pattern (P2).
#[tauri::command]
pub fn save_macro(
    app: AppHandle,
    state: State<AppState>,
    profile_id: String,
    macro_def: MacroDef,
) -> Result<MacroDef, String> {
    let entered = state.safety_epoch();
    {
        let cfg = state.config.lock();
        let profile = cfg
            .profile(&profile_id)
            .ok_or_else(|| format!("unknown profile: {profile_id}"))?;
        if !profile.macros.iter().any(|m| m.id == macro_def.id)
            && profile.macros.len() >= crate::keyboard::macro_runner::MAX_MACROS_PER_PROFILE
        {
            return Err(format!(
                "profile already has {} macros; delete one first",
                crate::keyboard::macro_runner::MAX_MACROS_PER_PROFILE
            ));
        }
        // Validate against the profile without the old version of this
        // macro (replace semantics): editing keeps its trigger.
        let mut probe = profile.clone();
        probe.macros.retain(|m| m.id != macro_def.id);
        validate_macro(&probe, &macro_def)?;
    }
    {
        let mut cfg = state.config.lock();
        let profile = cfg
            .profile_mut(&profile_id)
            .ok_or_else(|| format!("unknown profile: {profile_id}"))?;
        profile.macros.retain(|m| m.id != macro_def.id);
        profile.macros.push(macro_def.clone());
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(macro_def);
    }
    if state.effective_profile_id() == profile_id {
        state.apply_effective_profile_macros();
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(macro_def);
    }
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    tracing::info!("macro saved: {profile_id} {}", macro_def.id);
    Ok(macro_def)
}

/// S9: delete one macro (by id) from a profile. Deleting the macro of
/// a running run cancels it (its held keys are released).
#[tauri::command]
pub fn delete_macro(
    app: AppHandle,
    state: State<AppState>,
    profile_id: String,
    macro_id: String,
) -> Result<(), String> {
    let entered = state.safety_epoch();
    {
        let mut cfg = state.config.lock();
        let profile = cfg
            .profile_mut(&profile_id)
            .ok_or_else(|| format!("unknown profile: {profile_id}"))?;
        if !profile.macros.iter().any(|m| m.id == macro_id) {
            return Err(format!("unknown macro: {macro_id}"));
        }
        profile.macros.retain(|m| m.id != macro_id);
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(());
    }
    if state.effective_profile_id() == profile_id {
        state.apply_effective_profile_macros();
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(());
    }
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    tracing::info!("macro deleted: {profile_id} {macro_id}");
    Ok(())
}

/// S9: create or replace one layer on a profile (replace by id).
/// Same stale-command pattern as save_macro; an active layer is
/// deactivated before the new definitions install.
#[tauri::command]
pub fn save_layer(
    app: AppHandle,
    state: State<AppState>,
    profile_id: String,
    layer: LayerDef,
) -> Result<LayerDef, String> {
    let entered = state.safety_epoch();
    {
        let cfg = state.config.lock();
        let profile = cfg
            .profile(&profile_id)
            .ok_or_else(|| format!("unknown profile: {profile_id}"))?;
        if !profile.layers.iter().any(|l| l.id == layer.id)
            && profile.layers.len() >= crate::keyboard::macro_runner::MAX_LAYERS_PER_PROFILE
        {
            return Err(format!(
                "profile already has {} layers; delete one first",
                crate::keyboard::macro_runner::MAX_LAYERS_PER_PROFILE
            ));
        }
        let mut probe = profile.clone();
        probe.layers.retain(|l| l.id != layer.id);
        validate_layer(&probe, &layer)?;
    }
    {
        let mut cfg = state.config.lock();
        let profile = cfg
            .profile_mut(&profile_id)
            .ok_or_else(|| format!("unknown profile: {profile_id}"))?;
        profile.layers.retain(|l| l.id != layer.id);
        profile.layers.push(layer.clone());
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(layer);
    }
    if state.effective_profile_id() == profile_id {
        state.apply_effective_profile_layers();
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(layer);
    }
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    tracing::info!("layer saved: {profile_id} {}", layer.id);
    Ok(layer)
}

/// S9: delete one layer (by id) from a profile. Deleting the active
/// layer deactivates it (its held remap targets are flushed).
#[tauri::command]
pub fn delete_layer(
    app: AppHandle,
    state: State<AppState>,
    profile_id: String,
    layer_id: String,
) -> Result<(), String> {
    let entered = state.safety_epoch();
    {
        let mut cfg = state.config.lock();
        let profile = cfg
            .profile_mut(&profile_id)
            .ok_or_else(|| format!("unknown profile: {profile_id}"))?;
        if !profile.layers.iter().any(|l| l.id == layer_id) {
            return Err(format!("unknown layer: {layer_id}"));
        }
        profile.layers.retain(|l| l.id != layer_id);
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(());
    }
    if state.effective_profile_id() == profile_id {
        state.apply_effective_profile_layers();
    }
    state.persist()?;
    if state.safety.epoch() != entered {
        crate::discard_stale_command(&app, &state, "superseded during persist");
        return Ok(());
    }
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    tracing::info!("layer deleted: {profile_id} {layer_id}");
    Ok(())
}

/// Explicit user resume: clears the manual override and the safety pause
/// (set by emergency), then re-evaluates the current foreground app.
/// Runtime-only: nothing is persisted. An emergency landing mid-command
/// re-asserts the pause instead of letting the resume win.
#[tauri::command]
pub fn resume_auto_profile(
    app: AppHandle,
    state: State<AppState>,
) -> Result<AppProfileStatusPayload, String> {
    let entered = state.safety_epoch();
    let _change = state.resume_auto();
    if state.safety.epoch() != entered {
        // An emergency landed while resuming: the pause it set stands.
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return Ok(app_profile_status_payload(&state));
    }
    state.bump_runtime_revision();
    let _ = app.emit("profile:changed", state.config.lock().clone());
    let _ = app.emit("keyboard:state-changed", state_changed_payload(&state));
    Ok(app_profile_status_payload(&state))
}

/// Rule ids use the same collision-resistant scheme as profile ids.
fn unique_rule_id(rule: &AppRule) -> String {
    let slug: String = normalize_rule_slug(&rule.executable_path);
    let pid = std::process::id();
    loop {
        let ctr = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let id = format!("rule-{slug}-{nanos:x}-{pid}-{ctr}");
        return id;
    }
}

fn normalize_rule_slug(path: &str) -> String {
    let base = path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("app")
        .to_string();
    let slug: String = base
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    if slug.is_empty() {
        "app".to_string()
    } else {
        slug
    }
}

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
    // S5: an explicit selection is a manual override — it outranks app
    // rules and survives foreground switches until an explicit resume.
    state.set_manual_override(&id);
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
        remappings: vec![],
        macros: vec![],
        layers: vec![],
        builtin: false,
    };
    let entered = state.safety_epoch();
    {
        let mut cfg = state.config.lock();
        cfg.profiles.push(profile.clone());
        // Common activation policy (HF-06).
        cfg.activate_profile(&id)?;
    }
    // S5: creating (and activating) a profile is a manual selection.
    state.set_manual_override(&id);
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
        // S8: the copy inherits the source's rules (its disabled set is
        // copied too, so V3 still holds). S9: macros and layers travel
        // with the copy for the same reason.
        remappings: source.remappings,
        macros: source.macros,
        layers: source.layers,
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
    // S5: activating the duplicate is a manual selection.
    state.set_manual_override(&copy_id);
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
        // S5: deleting a profile removes the rules that point at it, so
        // no rule can ever resolve to a missing profile.
        cfg.app_rules.retain(|r| r.profile_id != id);
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
    // S5: if the deleted profile was the manual override or the effective
    // profile, fall back now (rule match on the current app, else Default)
    // instead of applying the stale selected profile's keys.
    state.sync_runtime_after_profile_delete(&id);
    state.apply_effective_profile_keys();
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
        // S8 (V3): a reset must not leave a rule naming a now-disabled
        // key. Drop the conflicting rules (loud log); the UI refresh
        // below shows them gone.
        let before = profile.remappings.len();
        let disabled = profile.disabled_keys.clone();
        profile.remappings.retain(|r| {
            !disabled.iter().any(|k| k == &r.from) && !disabled.iter().any(|k| k == &r.to)
        });
        if profile.remappings.len() != before {
            tracing::warn!("reset_profile dropped remap rules conflicting with disabled keys");
        }
        // S9: the same for macros and layers. A macro whose trigger or
        // step is now disabled would go dead / silently drop a step; a
        // layer whose switch key or remap participant is now disabled
        // would lie the same way. (A disabled cancel key is fine — the
        // engine checks the cancel before the disable branch.)
        let before_macros = profile.macros.len();
        profile.macros.retain(|m| {
            !disabled.iter().any(|k| k == &m.trigger)
                && !m
                    .steps
                    .iter()
                    .any(|s| disabled.iter().any(|k| k == &s.key))
        });
        if profile.macros.len() != before_macros {
            tracing::warn!("reset_profile dropped macros conflicting with disabled keys");
        }
        let before_layers = profile.layers.len();
        profile.layers.retain(|l| {
            // Only the target side: a base-disabled remap *source* still
            // fires in the layer (the layer re-enables it).
            !disabled.iter().any(|k| k == &l.switch_key)
                && !l
                    .remappings
                    .iter()
                    .any(|r| disabled.iter().any(|k| k == &r.to))
        });
        if profile.layers.len() != before_layers {
            tracing::warn!("reset_profile dropped layers conflicting with disabled keys");
        }
    }
    state.bump_runtime_revision();
    if state.safety.epoch() != entered {
        // A stale reset must not re-apply a disabled set after the unlock.
        crate::discard_stale_command(&app, &state, "superseded mid-command");
        return converged_profile(&state);
    }
    if state.effective_profile_id() == id {
        // S5: key edits apply to the *effective* profile (which may be an
        // app-rule match, not the persisted manual selection). S9: the
        // reset may have dropped rules, macros, or layers; re-apply the
        // whole effective keymap (flush first).
        state.apply_effective_profile_keys();
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
