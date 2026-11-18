import { useState } from "react";
import type { KeyboardLayout, RemapRule } from "../types/keyboard";
import { useT } from "../hooks/useT";

/** Keys that can never be a remap target (S8 V4): Pause is excluded from
 * both sides, toggle keys are targets-excluded (they are sources-allowed). */
const EXCLUDED_SOURCE = new Set(["Pause"]);
const EXCLUDED_TARGET = new Set(["Pause", "CapsLock", "NumLock", "ScrollLock"]);

interface RemapPanelProps {
  rules: RemapRule[];
  layout: KeyboardLayout;
  onSave: (rule: RemapRule) => void;
  onDelete: (from: string) => void;
}

/** S8: 1:1 key remapping for the current profile. Collapsible so the main
 * keyboard view stays the focus; the rules are secondary. */
export function RemapPanel({ rules, layout, onSave, onDelete }: RemapPanelProps) {
  const t = useT();
  const [open, setOpen] = useState(false);

  const labelOf = (code: string) =>
    layout.keys.find((k) => k.code === code)?.label ?? code;

  const mappedSources = new Set(rules.map((r) => r.from));
  const sourceOptions = layout.keys.filter(
    (k) => !EXCLUDED_SOURCE.has(k.code) && !mappedSources.has(k.code),
  );
  const [from, setFrom] = useState(sourceOptions[0]?.code ?? "");
  // The displayed source is the selection when still available, else the
  // first free key (e.g. after the selected one was just mapped).
  const fromValue = sourceOptions.some((k) => k.code === from)
    ? from
    : (sourceOptions[0]?.code ?? "");
  const targetOptions = layout.keys.filter(
    (k) => !EXCLUDED_TARGET.has(k.code) && k.code !== fromValue,
  );
  const [to, setTo] = useState(targetOptions[0]?.code ?? "");
  const toValue =
    targetOptions.some((k) => k.code === to) && to !== fromValue
      ? to
      : (targetOptions[0]?.code ?? "");

  function submit() {
    if (!fromValue || !toValue || fromValue === toValue) return;
    onSave({ from: fromValue, to: toValue });
  }

  return (
    <section className="mt-3 flex-shrink-0 rounded-2xl border border-border bg-panel px-4 py-3">
      <button
        className="flex w-full items-center justify-between text-left text-sm"
        onClick={() => setOpen((v) => !v)}
      >
        <span className="font-medium">{t("remapTitle")}</span>
        <span className="text-xs text-muted">
          {rules.length === 0
            ? t("noRemaps")
            : rules.map((r) => `${labelOf(r.from)} → ${labelOf(r.to)}`).join(" · ")}
          <span className="ml-2">{open ? "▾" : "▸"}</span>
        </span>
      </button>

      {open && (
        <div className="mt-3 flex flex-col gap-3 text-sm">
          <p className="text-xs text-muted">{t("remapHint")}</p>

          {rules.length === 0 ? (
            <p className="text-xs text-muted">{t("noRemaps")}</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {rules.map((rule) => (
                <li
                  key={rule.from}
                  className="flex items-center justify-between gap-3 rounded-lg border border-border bg-panel-2 px-3 py-2"
                >
                  <span className="min-w-0 truncate text-xs">
                    {labelOf(rule.from)} → {labelOf(rule.to)}
                  </span>
                  <button
                    className="h-8 flex-shrink-0 rounded-lg border border-border px-2 text-xs hover:border-danger/40"
                    title={t("deleteRemap")}
                    onClick={() => onDelete(rule.from)}
                  >
                    ✕
                  </button>
                </li>
              ))}
            </ul>
          )}

          <div className="flex flex-wrap items-center gap-2">
            <select
              value={fromValue}
              onChange={(event) => setFrom(event.target.value)}
              aria-label={t("remapFrom")}
              className="h-9 rounded-lg border border-border bg-panel-2 px-3 text-sm outline-none"
            >
              {sourceOptions.map((k) => (
                <option key={k.code} value={k.code}>
                  {k.label}
                </option>
              ))}
            </select>
            <span className="text-xs text-muted">→</span>
            <select
              value={toValue}
              onChange={(event) => setTo(event.target.value)}
              aria-label={t("remapTo")}
              className="h-9 rounded-lg border border-border bg-panel-2 px-3 text-sm outline-none"
            >
              {targetOptions.map((k) => (
                <option key={k.code} value={k.code}>
                  {k.label}
                </option>
              ))}
            </select>
            <button
              className="h-9 rounded-lg border border-border bg-panel-2 px-4 text-sm hover:border-lime/30 disabled:opacity-40"
              disabled={!fromValue || !toValue || fromValue === toValue}
              onClick={submit}
            >
              {t("addRemap")}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
