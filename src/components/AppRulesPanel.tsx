import { useState } from "react";
import type { AppRule, MatchReason, Profile } from "../types/keyboard";
import type { MessageKey } from "../i18n";
import { profileLabel } from "../lib/layouts";
import { useLocale, useT } from "../hooks/useT";

interface AppRulesPanelProps {
  rules: AppRule[];
  profiles: Profile[];
  autoEnabled: boolean;
  paused: boolean;
  matchReason: MatchReason;
  activeExe: string | null;
  activeProfileId: string;
  onSave: (rule: AppRule) => void;
  onDelete: (id: string) => void;
  onToggleAuto: (enabled: boolean) => void;
  onResume: () => void;
}

function reasonLabel(reason: MatchReason, t: (key: MessageKey) => string): string {
  switch (reason) {
    case "manual":
      return t("matchManual");
    case "app-rule":
      return t("matchAppRule");
    case "safety-paused":
      return t("matchSafetyPaused");
    default:
      return t("matchDefault");
  }
}

/** S5: app-rule management + the current auto-switch state. Collapsible
 * so the main keyboard view stays the focus; the rules are secondary. */
export function AppRulesPanel({
  rules,
  profiles,
  autoEnabled,
  paused,
  matchReason,
  activeExe,
  activeProfileId,
  onSave,
  onDelete,
  onToggleAuto,
  onResume,
}: AppRulesPanelProps) {
  const t = useT();
  const locale = useLocale();
  const [open, setOpen] = useState(false);
  const [exe, setExe] = useState("");
  const [profileId, setProfileId] = useState(profiles[0]?.id ?? "default");

  const activeProfile = profiles.find((p) => p.id === activeProfileId);
  const exeBase = activeExe ? activeExe.split("\\").pop() ?? activeExe : null;

  function submit() {
    const path = exe.trim();
    if (!path || !profileId) return;
    onSave({ id: "", executablePath: path, profileId, enabled: true, priority: 0 });
    setExe("");
  }

  return (
    <section className="mt-3 flex-shrink-0 rounded-2xl border border-border bg-panel px-4 py-3">
      <button
        className="flex w-full items-center justify-between text-left text-sm"
        onClick={() => setOpen((v) => !v)}
      >
        <span className="font-medium">{t("appRules")}</span>
        <span className="text-xs text-muted">
          {activeProfile ? profileLabel(activeProfile, locale) : ""} ·{" "}
          {reasonLabel(matchReason, t)}
          {exeBase ? ` · ${exeBase}` : ""}
          <span className="ml-2">{open ? "▾" : "▸"}</span>
        </span>
      </button>

      {open && (
        <div className="mt-3 flex flex-col gap-3 text-sm">
          <p className="text-xs text-muted">{t("appRulesHint")}</p>

          <label className="flex items-center gap-2 text-xs text-muted">
            <input
              type="checkbox"
              checked={autoEnabled}
              onChange={(event) => onToggleAuto(event.target.checked)}
            />
            {t("autoSwitch")}
          </label>

          {paused && (
            <div className="flex items-center justify-between gap-3 rounded-xl border border-orange-400/40 bg-orange-500/15 px-3 py-2 text-xs">
              <span>{t("autoPaused")}</span>
              <button
                className="h-8 flex-shrink-0 rounded-lg border border-orange-400/40 px-3 hover:bg-orange-500/10"
                onClick={onResume}
              >
                {t("resumeAuto")}
              </button>
            </div>
          )}

          <div className="text-xs text-muted">
            {t("frontApp")}: {exeBase ?? t("unknownApp")}
          </div>

          {rules.length === 0 ? (
            <p className="text-xs text-muted">{t("noRules")}</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {rules.map((rule) => {
                return (
                  <li
                    key={rule.id}
                    className="flex items-center justify-between gap-3 rounded-lg border border-border bg-panel-2 px-3 py-2"
                  >
                    <label className="flex min-w-0 items-center gap-2">
                      <input
                        type="checkbox"
                        checked={rule.enabled}
                        onChange={() => onSave({ ...rule, enabled: !rule.enabled })}
                      />
                      <span className="min-w-0 truncate text-xs" title={rule.executablePath}>
                        {rule.executablePath}
                      </span>
                    </label>
                    <span className="flex flex-shrink-0 items-center gap-2">
                      <select
                        value={rule.profileId}
                        onChange={(event) => onSave({ ...rule, profileId: event.target.value })}
                        className="h-8 rounded-lg border border-border bg-panel px-2 text-xs outline-none"
                      >
                        {profiles.map((p) => (
                          <option key={p.id} value={p.id}>
                            {profileLabel(p, locale)}
                          </option>
                        ))}
                      </select>
                      <button
                        className="h-8 rounded-lg border border-border px-2 text-xs hover:border-danger/40"
                        title={t("deleteRule")}
                        onClick={() => onDelete(rule.id)}
                      >
                        ✕
                      </button>
                    </span>
                  </li>
                );
              })}
            </ul>
          )}

          <div className="flex flex-wrap items-center gap-2">
            <input
              value={exe}
              onChange={(event) => setExe(event.target.value)}
              placeholder={t("ruleExePlaceholder")}
              className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-panel-2 px-3 text-sm outline-none focus:border-lime/40"
            />
            <select
              value={profileId}
              onChange={(event) => setProfileId(event.target.value)}
              className="h-9 rounded-lg border border-border bg-panel-2 px-3 text-sm outline-none"
            >
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>
                  {profileLabel(p, locale)}
                </option>
              ))}
            </select>
            <button
              className="h-9 rounded-lg border border-border bg-panel-2 px-4 text-sm hover:border-lime/30 disabled:opacity-40"
              disabled={!exe.trim()}
              onClick={submit}
            >
              {t("addRule")}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
