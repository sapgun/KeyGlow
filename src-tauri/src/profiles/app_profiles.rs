//! App profiles (S5): foreground-app rule matching and the runtime
//! effective-profile state machine.
//!
//! Pure logic only: no Windows API, no disk, no hook callback. The Windows
//! foreground query lives in `crate::platform::foreground`; it only feeds
//! normalized executable paths into this module. Everything here is
//! deterministic under a virtual clock (`now_ms`), so the acceptance
//! scenarios run as plain unit tests with no sleeps.
//!
//! Contracts (see ADR in `docs/ARCHITECTURE.md`):
//! - Matching is by executable *path* only, normalized (case-insensitive,
//!   `/` unified to `\`). Window titles are never used for matching.
//! - Priority: Cat Lock (engine-level, untouched here) > safety pause
//!   (after emergency) > manual selection > app rule > Default.
//! - The effective profile is runtime-only: foreground switches never
//!   write `settings.json` and never touch `config.selected_profile`.
//! - Duplicate rules (same normalized exe, different profiles): the
//!   highest `priority` wins; on a tie the earliest rule wins. Exact
//!   duplicates (same exe + same profile + same enabled) are dropped by
//!   `dedupe_rules` at config normalize time.

use serde::{Deserialize, Serialize};

/// Debounce for foreground switches: a new foreground app must stay
/// foreground this long before the profile switches. Rapid Alt+Tab spam
/// restarts the timer, so only the settled app commits. 200ms is inside
/// the 150-300ms window the spec proposes; the acceptance tests pin it.
pub const DEFAULT_DEBOUNCE_MS: u64 = 200;
/// How often the observer thread polls the foreground window (Windows).
pub const OBSERVER_POLL_MS: u64 = 50;

/// One persisted app rule: when the foreground executable matches,
/// switch to `profile_id` (if automation is on, nothing higher-priority
/// applies, and the profile still exists).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppRule {
    pub id: String,
    pub executable_path: String,
    pub profile_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
}

fn default_true() -> bool {
    true
}

/// Why the effective profile is what it is, for the UI ("currently applied
/// because ..."). Serialized kebab-case for the snapshot/event payloads.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MatchReason {
    Manual,
    AppRule,
    Default,
    SafetyPaused,
}

/// A change of the effective profile, for the caller to apply to the
/// controller and announce to the UI (with the P3 ordering stamp).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveChange {
    pub profile_id: String,
    pub reason: MatchReason,
    /// Normalized exe of the foreground app that caused this, if any.
    pub exe: Option<String>,
}

/// Normalize an executable path for matching: trim, unify separators,
/// lowercase. Windows paths are case-insensitive, and the foreground query
/// may return either separator style.
pub fn normalize_executable_path(path: &str) -> String {
    path.trim().replace('/', "\\").to_lowercase()
}

/// Resolve the winning rule for a normalized executable path.
/// Only enabled rules match. Highest `priority` wins; on a tie the
/// earliest rule in the list wins (stable, deterministic).
pub fn resolve_rule<'a>(exe_normalized: &str, rules: &'a [AppRule]) -> Option<&'a AppRule> {
    let mut best: Option<&AppRule> = None;
    for rule in rules {
        if !rule.enabled {
            continue;
        }
        if normalize_executable_path(&rule.executable_path) != exe_normalized {
            continue;
        }
        match best {
            // Strictly greater replaces: ties keep the earlier rule.
            Some(b) if rule.priority <= b.priority => {}
            _ => best = Some(rule),
        }
    }
    best
}

/// Drop exact duplicates (same normalized exe + same profile + same
/// enabled flag), keeping the first occurrence. Used by config normalize.
pub fn dedupe_rules(rules: &[AppRule]) -> Vec<AppRule> {
    let mut out: Vec<AppRule> = Vec::with_capacity(rules.len());
    for rule in rules {
        let key = (
            normalize_executable_path(&rule.executable_path),
            rule.profile_id.clone(),
            rule.enabled,
        );
        if out.iter().any(|r: &AppRule| {
            (
                normalize_executable_path(&r.executable_path),
                r.profile_id.clone(),
                r.enabled,
            ) == key
        }) {
            continue;
        }
        out.push(rule.clone());
    }
    out
}

/// Validate one rule before saving: the executable must be non-empty after
/// normalization and the target profile must exist.
pub fn validate_rule(rule: &AppRule, valid_profile_ids: &[String]) -> Result<(), String> {
    if normalize_executable_path(&rule.executable_path).is_empty() {
        return Err("executable path is required".into());
    }
    if !valid_profile_ids.iter().any(|id| id == &rule.profile_id) {
        return Err(format!("unknown profile: {}", rule.profile_id));
    }
    Ok(())
}

/// Runtime-only effective-profile state. Never serialized, never written
/// to disk. `config.selected_profile` stays the manual selection; this
/// holds what is actually filtering keys right now.
#[derive(Clone, Debug)]
pub struct AppProfileRuntime {
    effective_profile_id: String,
    manual_override: Option<String>,
    safety_paused: bool,
    reason: MatchReason,
    current_exe: Option<String>,
    /// Pending switch: `None` = nothing pending; `Some(exe)` = a switch to
    /// `exe` is being debounced (`exe` itself may be `None` = the
    /// foreground is unknown/inaccessible). Two levels of Option are
    /// deliberate: a bare `Option<String>` could not tell "no pending
    /// switch" from "pending switch to unknown".
    pending_exe: Option<Option<String>>,
    pending_since_ms: u64,
    debounce_ms: u64,
    /// Counts effective-profile changes (diagnostics; distinct from the
    /// P3 `runtime_revision`, which AppState bumps on the same occasions).
    revision: u64,
}

impl AppProfileRuntime {
    pub fn new(default_profile_id: &str) -> Self {
        Self {
            effective_profile_id: default_profile_id.to_string(),
            manual_override: None,
            safety_paused: false,
            reason: MatchReason::Default,
            current_exe: None,
            pending_exe: None,
            pending_since_ms: 0,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
            revision: 0,
        }
    }

    pub fn with_debounce(default_profile_id: &str, debounce_ms: u64) -> Self {
        let mut rt = Self::new(default_profile_id);
        rt.debounce_ms = debounce_ms;
        rt
    }

    pub fn effective_profile_id(&self) -> &str {
        &self.effective_profile_id
    }
    pub fn manual_override(&self) -> Option<&str> {
        self.manual_override.as_deref()
    }
    pub fn safety_paused(&self) -> bool {
        self.safety_paused
    }
    pub fn reason(&self) -> MatchReason {
        self.reason
    }
    pub fn current_exe(&self) -> Option<&str> {
        self.current_exe.as_deref()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn set_effective(
        &mut self,
        profile_id: &str,
        reason: MatchReason,
        exe: Option<String>,
    ) -> EffectiveChange {
        self.effective_profile_id = profile_id.to_string();
        self.reason = reason;
        self.revision += 1;
        EffectiveChange {
            profile_id: profile_id.to_string(),
            reason,
            exe,
        }
    }

    /// Debounce step. Feed the latest foreground exe (normalized already
    /// by the caller, `None` when unknown/inaccessible) at `now_ms`.
    /// Returns true when a switch is due and `commit` should run.
    /// A different exe restarts the timer; the same exe is ignored.
    pub fn note_foreground(&mut self, exe: Option<String>, now_ms: u64) -> bool {
        let exe_norm = exe.map(|e| normalize_executable_path(&e));
        match &self.pending_exe {
            // No pending switch: settle already on this exe, else start one.
            None => {
                if self.current_exe == exe_norm {
                    return false;
                }
                self.pending_exe = Some(exe_norm);
                self.pending_since_ms = now_ms;
                false
            }
            // Same pending exe: due once the debounce elapsed.
            Some(pending) if *pending == exe_norm => {
                now_ms.saturating_sub(self.pending_since_ms) >= self.debounce_ms
            }
            // A different exe arrived: restart the timer on the new one.
            Some(_) => {
                self.pending_exe = Some(exe_norm);
                self.pending_since_ms = now_ms;
                false
            }
        }
    }

    /// Commit a due foreground switch: resolve the pending exe against the
    /// rules under the priority order. Returns the change when the
    /// effective profile actually moved, else None. Clears the pending
    /// state either way (a no-op commit still settles the exe). Calling
    /// commit with nothing pending is a no-op.
    pub fn commit(
        &mut self,
        rules: &[AppRule],
        auto_enabled: bool,
        valid_profile_ids: &[String],
    ) -> Option<EffectiveChange> {
        let exe = self.pending_exe.take()?;
        self.current_exe = exe.clone();
        if self.safety_paused {
            // Automation is paused: the foreground switch changes nothing.
            // The effective profile stays whatever it was (Default after
            // the emergency, or a manual selection made while paused).
            return None;
        }
        if let Some(manual) = &self.manual_override {
            // A manual selection outranks every rule until resumed.
            let manual = manual.clone();
            if manual != self.effective_profile_id {
                return Some(self.set_effective(&manual, MatchReason::Manual, exe));
            }
            return None;
        }
        let target = self.resolve_target(exe.as_deref(), rules, auto_enabled, valid_profile_ids);
        let (profile_id, reason) = target;
        if profile_id != self.effective_profile_id || reason != self.reason {
            return Some(self.set_effective(&profile_id, reason, exe));
        }
        None
    }

    /// Resolve the target (profile, reason) for an exe under the priority
    /// order, without mutating anything. Pure helper for commit/resume.
    fn resolve_target(
        &self,
        exe: Option<&str>,
        rules: &[AppRule],
        auto_enabled: bool,
        valid_profile_ids: &[String],
    ) -> (String, MatchReason) {
        if auto_enabled {
            if let Some(exe) = exe {
                if let Some(rule) = resolve_rule(exe, rules) {
                    if valid_profile_ids.iter().any(|id| id == &rule.profile_id) {
                        return (rule.profile_id.clone(), MatchReason::AppRule);
                    }
                    // Rule points at a deleted profile: fall through to
                    // Default (rules are cleaned on delete, so this is a
                    // defensive path for hand-edited config files).
                }
            }
        }
        ("default".to_string(), MatchReason::Default)
    }

    /// Manual profile selection (user action): outranks rules, survives
    /// foreground switches, does NOT clear the safety pause (automation
    /// stays paused until an explicit resume).
    pub fn set_manual(&mut self, profile_id: &str) -> Option<EffectiveChange> {
        self.manual_override = Some(profile_id.to_string());
        if profile_id != self.effective_profile_id || self.reason != MatchReason::Manual {
            return Some(self.set_effective(
                profile_id,
                MatchReason::Manual,
                self.current_exe.clone(),
            ));
        }
        None
    }

    /// Emergency convergence: pause automation, drop any manual override
    /// (the emergency already forced Default in config), effective profile
    /// becomes Default with the safety-paused reason.
    pub fn on_emergency(&mut self) -> EffectiveChange {
        self.safety_paused = true;
        self.manual_override = None;
        self.set_effective(
            "default",
            MatchReason::SafetyPaused,
            self.current_exe.clone(),
        )
    }

    /// User resumes automation: clears the manual override and the safety
    /// pause, then re-evaluates the current foreground app immediately.
    pub fn resume_auto(
        &mut self,
        rules: &[AppRule],
        auto_enabled: bool,
        valid_profile_ids: &[String],
    ) -> Option<EffectiveChange> {
        self.safety_paused = false;
        self.manual_override = None;
        let (profile_id, reason) = self.resolve_target(
            self.current_exe.as_deref(),
            rules,
            auto_enabled,
            valid_profile_ids,
        );
        if profile_id != self.effective_profile_id || reason != self.reason {
            return Some(self.set_effective(&profile_id, reason, self.current_exe.clone()));
        }
        None
    }

    /// Re-evaluate the current foreground app immediately (after a rule
    /// was saved/deleted or automation was toggled). Bypasses the
    /// debounce: the exe already settled long ago, only the rules changed.
    /// Manual override and safety pause still outrank rules.
    pub fn force_reevaluate(
        &mut self,
        rules: &[AppRule],
        auto_enabled: bool,
        valid_profile_ids: &[String],
    ) -> Option<EffectiveChange> {
        if self.safety_paused || self.manual_override.is_some() {
            return None;
        }
        let (profile_id, reason) = self.resolve_target(
            self.current_exe.as_deref(),
            rules,
            auto_enabled,
            valid_profile_ids,
        );
        if profile_id != self.effective_profile_id || reason != self.reason {
            return Some(self.set_effective(&profile_id, reason, self.current_exe.clone()));
        }
        None
    }
    /// If the deleted profile was the manual override or the effective
    /// profile, re-evaluate against the current foreground app.
    pub fn on_profile_deleted(
        &mut self,
        deleted_id: &str,
        rules: &[AppRule],
        auto_enabled: bool,
        valid_profile_ids: &[String],
    ) -> Option<EffectiveChange> {
        if self.manual_override.as_deref() == Some(deleted_id) {
            self.manual_override = None;
        }
        if self.effective_profile_id == deleted_id {
            let (profile_id, reason) = self.resolve_target(
                self.current_exe.as_deref(),
                rules,
                auto_enabled,
                valid_profile_ids,
            );
            return Some(self.set_effective(&profile_id, reason, self.current_exe.clone()));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, exe: &str, profile: &str, enabled: bool, priority: i32) -> AppRule {
        AppRule {
            id: id.to_string(),
            executable_path: exe.to_string(),
            profile_id: profile.to_string(),
            enabled,
            priority,
        }
    }

    fn ids() -> Vec<String> {
        vec!["default".into(), "gaming".into(), "coding".into()]
    }

    #[test]
    fn normalize_is_case_insensitive_and_unifies_separators() {
        assert_eq!(
            normalize_executable_path("C:/Games/Notepad++.EXE "),
            "c:\\games\\notepad++.exe"
        );
        assert_eq!(normalize_executable_path(""), "");
    }

    #[test]
    fn resolver_matches_normalized_path_only() {
        let rules = vec![
            rule("1", "C:\\Windows\\notepad.exe", "coding", true, 0),
            rule("2", "C:\\Windows\\NOTEPAD.EXE", "gaming", true, 0),
        ];
        // Same normalized exe, tie priority: earliest wins.
        let hit = resolve_rule("c:\\windows\\notepad.exe", &rules).unwrap();
        assert_eq!(hit.id, "1");
        // Title strings never match.
        assert!(resolve_rule("notepad", &rules).is_none());
        assert!(resolve_rule("c:\\windows\\notepad++.exe", &rules).is_none());
    }

    #[test]
    fn resolver_ignores_disabled_rules_and_uses_priority() {
        let rules = vec![
            rule("1", "C:\\a\\game.exe", "gaming", false, 99),
            rule("2", "C:\\a\\game.exe", "coding", true, 1),
            rule("3", "C:\\a\\game.exe", "gaming", true, 5),
        ];
        let hit = resolve_rule("c:\\a\\game.exe", &rules).unwrap();
        assert_eq!(hit.id, "3");
    }

    #[test]
    fn dedupe_drops_exact_duplicates_keeping_first() {
        let rules = vec![
            rule("1", "C:\\a.exe", "gaming", true, 0),
            rule("2", "C:\\A.EXE", "gaming", true, 7),
            rule("3", "C:\\a.exe", "coding", true, 0),
        ];
        let out = dedupe_rules(&rules);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "1");
        assert_eq!(out[1].id, "3");
    }

    #[test]
    fn validate_rule_rejects_unknown_profile_and_empty_exe() {
        assert!(validate_rule(&rule("1", "C:\\a.exe", "gaming", true, 0), &ids()).is_ok());
        assert!(validate_rule(&rule("1", "C:\\a.exe", "nope", true, 0), &ids()).is_err());
        assert!(validate_rule(&rule("1", "   ", "gaming", true, 0), &ids()).is_err());
    }

    // ---- Debounce (virtual clock, no sleeps) ----

    #[test]
    fn debounce_collapses_rapid_switches_to_the_last_app() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules = vec![
            rule("n", "C:\\Windows\\notepad.exe", "coding", true, 0),
            rule("b", "C:\\Program Files\\browser.exe", "gaming", true, 0),
            rule("g", "C:\\Games\\game.exe", "coding", true, 0),
        ];
        // Notepad at t=0, browser at t=50, game at t=100: each restarts it.
        assert!(!rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 0));
        assert!(!rt.note_foreground(Some("C:\\Program Files\\browser.exe".into()), 50));
        assert!(!rt.note_foreground(Some("C:\\Games\\game.exe".into()), 100));
        // At t=250 the game has only been foreground 150ms: not due.
        assert!(!rt.note_foreground(Some("C:\\Games\\game.exe".into()), 250));
        // At t=300 the game settled: commit, and only the game applies.
        assert!(rt.note_foreground(Some("C:\\Games\\game.exe".into()), 300));
        let change = rt.commit(&rules, true, &ids()).unwrap();
        assert_eq!(change.profile_id, "coding");
        assert_eq!(change.reason, MatchReason::AppRule);
        assert_eq!(rt.current_exe(), Some("c:\\games\\game.exe"));
    }

    #[test]
    fn debounce_ignores_repeated_same_exe_and_commits_once() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules = vec![rule("n", "C:\\Windows\\notepad.exe", "coding", true, 0)];
        assert!(!rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 0));
        assert!(!rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 100));
        assert!(rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 200));
        let change = rt.commit(&rules, true, &ids()).unwrap();
        assert_eq!(change.profile_id, "coding");
        // Settled: further polls of the same exe do nothing.
        assert!(!rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 500));
        assert!(rt.commit(&rules, true, &ids()).is_none());
    }

    #[test]
    fn unknown_or_inaccessible_foreground_falls_back_to_default() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules = vec![rule("n", "C:\\Windows\\notepad.exe", "coding", true, 0)];
        // First settle on Notepad (rule applies), then the app exits and
        // the foreground becomes unknown (None).
        assert!(!rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 0));
        assert!(rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 200));
        assert!(rt.commit(&rules, true, &ids()).is_some());
        assert_eq!(rt.effective_profile_id(), "coding");
        assert!(!rt.note_foreground(None, 300));
        assert!(rt.note_foreground(None, 500));
        let change = rt.commit(&rules, true, &ids()).unwrap();
        assert_eq!(change.profile_id, "default");
        assert_eq!(change.reason, MatchReason::Default);
    }

    #[test]
    fn no_rule_means_default_and_no_churn() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules: Vec<AppRule> = vec![];
        assert!(!rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 0));
        assert!(rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 200));
        // Effective is already Default: commit settles the exe, no change.
        assert!(rt.commit(&rules, true, &ids()).is_none());
        assert_eq!(rt.current_exe(), Some("c:\\windows\\notepad.exe"));
    }

    // ---- Priority ----

    #[test]
    fn manual_override_survives_foreground_switches() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules = vec![rule("g", "C:\\Games\\game.exe", "gaming", true, 0)];
        rt.set_manual("coding");
        assert_eq!(rt.effective_profile_id(), "coding");
        assert_eq!(rt.reason(), MatchReason::Manual);
        // Switch to the game: the manual selection stays.
        assert!(!rt.note_foreground(Some("C:\\Games\\game.exe".into()), 0));
        assert!(rt.note_foreground(Some("C:\\Games\\game.exe".into()), 200));
        assert!(rt.commit(&rules, true, &ids()).is_none());
        assert_eq!(rt.effective_profile_id(), "coding");
        assert_eq!(rt.reason(), MatchReason::Manual);
    }

    #[test]
    fn emergency_pauses_automation_until_explicit_resume() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules = vec![rule("g", "C:\\Games\\game.exe", "gaming", true, 0)];
        // Settle on the game first.
        assert!(!rt.note_foreground(Some("C:\\Games\\game.exe".into()), 0));
        assert!(rt.note_foreground(Some("C:\\Games\\game.exe".into()), 200));
        assert!(rt.commit(&rules, true, &ids()).is_some());
        assert_eq!(rt.effective_profile_id(), "gaming");
        // Emergency: automation pauses, effective goes to Default.
        let change = rt.on_emergency();
        assert_eq!(change.profile_id, "default");
        assert_eq!(change.reason, MatchReason::SafetyPaused);
        assert!(rt.safety_paused());
        // Foreground switches while paused change nothing.
        assert!(!rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 300));
        assert!(rt.note_foreground(Some("C:\\Windows\\notepad.exe".into()), 500));
        assert!(rt.commit(&rules, true, &ids()).is_none());
        assert_eq!(rt.effective_profile_id(), "default");
        // Manual selection still works while paused, but the pause stays.
        rt.set_manual("coding");
        assert_eq!(rt.effective_profile_id(), "coding");
        assert!(rt.safety_paused());
        // Explicit resume re-evaluates the current foreground (Notepad has
        // no rule): back to Default with the Default reason.
        let change = rt.resume_auto(&rules, true, &ids()).unwrap();
        assert_eq!(change.profile_id, "default");
        assert_eq!(change.reason, MatchReason::Default);
        assert!(!rt.safety_paused());
        assert!(rt.manual_override().is_none());
    }

    #[test]
    fn resume_reapplies_matching_rule_for_current_app() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules = vec![rule("g", "C:\\Games\\game.exe", "gaming", true, 0)];
        assert!(!rt.note_foreground(Some("C:\\Games\\game.exe".into()), 0));
        assert!(rt.note_foreground(Some("C:\\Games\\game.exe".into()), 200));
        assert!(rt.commit(&rules, true, &ids()).is_some());
        rt.set_manual("coding");
        let change = rt.resume_auto(&rules, true, &ids()).unwrap();
        assert_eq!(change.profile_id, "gaming");
        assert_eq!(change.reason, MatchReason::AppRule);
    }

    #[test]
    fn auto_disabled_means_default_even_with_matching_rule() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules = vec![rule("g", "C:\\Games\\game.exe", "gaming", true, 0)];
        assert!(!rt.note_foreground(Some("C:\\Games\\game.exe".into()), 0));
        assert!(rt.note_foreground(Some("C:\\Games\\game.exe".into()), 200));
        assert!(rt.commit(&rules, false, &ids()).is_none());
        assert_eq!(rt.effective_profile_id(), "default");
        assert_eq!(rt.reason(), MatchReason::Default);
    }

    #[test]
    fn profile_delete_falls_back_from_effective_and_manual() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules = vec![rule("g", "C:\\Games\\game.exe", "gaming", true, 0)];
        assert!(!rt.note_foreground(Some("C:\\Games\\game.exe".into()), 0));
        assert!(rt.note_foreground(Some("C:\\Games\\game.exe".into()), 200));
        assert!(rt.commit(&rules, true, &ids()).is_some());
        assert_eq!(rt.effective_profile_id(), "gaming");
        // Deleting the effective profile falls back to Default (the game
        // rule now points at a missing profile).
        let change = rt
            .on_profile_deleted("gaming", &rules, true, &["default".into(), "coding".into()])
            .unwrap();
        assert_eq!(change.profile_id, "default");
        assert_eq!(change.reason, MatchReason::Default);
        // Deleting an unrelated profile changes nothing.
        assert!(rt
            .on_profile_deleted("coding", &rules, true, &["default".into()])
            .is_none());
    }

    #[test]
    fn rule_pointing_at_deleted_profile_is_defensive_default() {
        let mut rt = AppProfileRuntime::with_debounce("default", 200);
        let rules = vec![rule("g", "C:\\Games\\game.exe", "gone", true, 0)];
        assert!(!rt.note_foreground(Some("C:\\Games\\game.exe".into()), 0));
        assert!(rt.note_foreground(Some("C:\\Games\\game.exe".into()), 200));
        // The rule matches but its profile is gone: Default, no crash.
        assert!(rt.commit(&rules, true, &ids()).is_none());
        assert_eq!(rt.effective_profile_id(), "default");
    }
}
