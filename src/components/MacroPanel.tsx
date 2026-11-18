import { useState } from "react";
import type { KeyboardLayout, MacroDef, MacroStepDef } from "../types/keyboard";
import { useT } from "../hooks/useT";

/** Keys that can never trigger a macro (S9): Pause has no key-up. */
const EXCLUDED_TRIGGER = new Set(["Pause"]);

interface MacroPanelProps {
  macros: MacroDef[];
  layout: KeyboardLayout;
  onSave: (macro: MacroDef) => void;
  onDelete: (id: string) => void;
}

function newMacroId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `macro-${crypto.randomUUID()}`;
  }
  return `macro-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;
}

/** S9: macros for the current profile. Collapsible so the main keyboard
 * view stays the focus. The backend validates the rest (balanced steps,
 * one key one role) and surfaces the error; the panel only guards the
 * obvious (empty name, no steps, Pause trigger). */
export function MacroPanel({ macros, layout, onSave, onDelete }: MacroPanelProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [trigger, setTrigger] = useState("");
  const [cancelKey, setCancelKey] = useState("Escape");
  const [steps, setSteps] = useState<MacroStepDef[]>([]);

  const labelOf = (code: string) =>
    layout.keys.find((k) => k.code === code)?.label ?? code;

  const usedTriggers = new Set(macros.map((m) => m.trigger));
  const triggerOptions = layout.keys.filter(
    (k) => !EXCLUDED_TRIGGER.has(k.code) && (!usedTriggers.has(k.code) || k.code === trigger),
  );
  const triggerValue =
    triggerOptions.some((k) => k.code === trigger) && trigger !== ""
      ? trigger
      : (triggerOptions[0]?.code ?? "");
  const stepKeyOptions = layout.keys.filter((k) => !EXCLUDED_TRIGGER.has(k.code));

  function startNew() {
    setEditingId(null);
    setName("");
    setTrigger(triggerOptions[0]?.code ?? "");
    setCancelKey("Escape");
    setSteps([]);
  }

  function startEdit(macro: MacroDef) {
    setEditingId(macro.id);
    setName(macro.name);
    setTrigger(macro.trigger);
    setCancelKey(macro.cancelKey);
    setSteps(macro.steps.map((s) => ({ ...s })));
  }

  function submit() {
    const trimmed = name.trim();
    if (!trimmed || !triggerValue || steps.length === 0) return;
    onSave({
      id: editingId ?? newMacroId(),
      name: trimmed,
      trigger: triggerValue,
      cancelKey,
      steps: steps.map((s) => ({
        key: s.key,
        action: s.action,
        delayMs: Math.max(0, Math.min(5000, Math.floor(s.delayMs) || 0)),
      })),
    });
    startNew();
  }

  function addStep() {
    const key = stepKeyOptions[0]?.code;
    if (!key) return;
    setSteps((prev) => [...prev, { key, action: "down", delayMs: 0 }]);
  }

  return (
    <section className="mt-3 flex-shrink-0 rounded-2xl border border-border bg-panel px-4 py-3">
      <button
        className="flex w-full items-center justify-between text-left text-sm"
        onClick={() => setOpen((v) => !v)}
      >
        <span className="font-medium">{t("macroTitle")}</span>
        <span className="text-xs text-muted">
          {macros.length === 0
            ? t("noMacros")
            : macros.map((m) => `${m.name} (${labelOf(m.trigger)})`).join(" · ")}
          <span className="ml-2">{open ? "▾" : "▸"}</span>
        </span>
      </button>

      {open && (
        <div className="mt-3 flex flex-col gap-3 text-sm">
          <p className="text-xs text-muted">{t("macroHint")}</p>

          {macros.length === 0 ? (
            <p className="text-xs text-muted">{t("noMacros")}</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {macros.map((macro) => (
                <li
                  key={macro.id}
                  className="flex items-center justify-between gap-3 rounded-lg border border-border bg-panel-2 px-3 py-2"
                >
                  <span className="min-w-0 truncate text-xs">
                    {macro.name} · {labelOf(macro.trigger)} → {macro.steps.length}{" "}
                    {t("macroStepsCount")} · {t("macroCancelKey")}: {labelOf(macro.cancelKey)}
                  </span>
                  <span className="flex flex-shrink-0 gap-1">
                    <button
                      className="h-8 rounded-lg border border-border px-2 text-xs hover:border-lime/30"
                      onClick={() => startEdit(macro)}
                    >
                      {t("macroEdit")}
                    </button>
                    <button
                      className="h-8 rounded-lg border border-border px-2 text-xs hover:border-danger/40"
                      title={t("macroDelete")}
                      onClick={() => onDelete(macro.id)}
                    >
                      ✕
                    </button>
                  </span>
                </li>
              ))}
            </ul>
          )}

          <div className="flex flex-col gap-2 rounded-lg border border-border bg-panel-2 px-3 py-2">
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("macroName")}
                aria-label={t("macroName")}
                className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
              />
              <select
                value={triggerValue}
                onChange={(event) => setTrigger(event.target.value)}
                aria-label={t("macroTrigger")}
                className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
              >
                {triggerOptions.map((k) => (
                  <option key={k.code} value={k.code}>
                    {k.label}
                  </option>
                ))}
              </select>
              <select
                value={cancelKey}
                onChange={(event) => setCancelKey(event.target.value)}
                aria-label={t("macroCancelKey")}
                className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
              >
                {layout.keys
                  .filter((k) => !EXCLUDED_TRIGGER.has(k.code))
                  .map((k) => (
                    <option key={k.code} value={k.code}>
                      {k.label}
                    </option>
                  ))}
              </select>
            </div>
            <p className="text-xs text-muted">{t("macroStepsHint")}</p>
            <ul className="flex flex-col gap-2">
              {steps.map((step, i) => (
                <li key={i} className="flex flex-wrap items-center gap-2">
                  <span className="w-5 text-xs text-muted">{i + 1}</span>
                  <select
                    value={step.key}
                    onChange={(event) =>
                      setSteps((prev) =>
                        prev.map((s, j) => (j === i ? { ...s, key: event.target.value } : s)),
                      )
                    }
                    aria-label={t("macroStepKey")}
                    className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
                  >
                    {stepKeyOptions.map((k) => (
                      <option key={k.code} value={k.code}>
                        {k.label}
                      </option>
                    ))}
                  </select>
                  <select
                    value={step.action}
                    onChange={(event) =>
                      setSteps((prev) =>
                        prev.map((s, j) =>
                          j === i
                            ? { ...s, action: event.target.value as "down" | "up" }
                            : s,
                        ),
                      )
                    }
                    aria-label={t("macroStepAction")}
                    className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
                  >
                    <option value="down">{t("macroActionDown")}</option>
                    <option value="up">{t("macroActionUp")}</option>
                  </select>
                  <label className="flex items-center gap-1 text-xs text-muted">
                    <input
                      type="number"
                      min={0}
                      max={5000}
                      value={step.delayMs}
                      onChange={(event) =>
                        setSteps((prev) =>
                          prev.map((s, j) =>
                            j === i
                              ? { ...s, delayMs: Number(event.target.value) || 0 }
                              : s,
                          ),
                        )
                      }
                      aria-label={t("macroStepDelay")}
                      className="h-9 w-20 rounded-lg border border-border bg-panel px-2 text-sm outline-none"
                    />
                    {t("macroDelayMs")}
                  </label>
                  <button
                    className="h-8 rounded-lg border border-border px-2 text-xs hover:border-danger/40"
                    onClick={() => setSteps((prev) => prev.filter((_, j) => j !== i))}
                  >
                    ✕
                  </button>
                </li>
              ))}
            </ul>
            <div className="flex flex-wrap items-center gap-2">
              <button
                className="h-9 rounded-lg border border-border bg-panel px-4 text-sm hover:border-lime/30"
                onClick={addStep}
              >
                {t("macroAddStep")}
              </button>
              <button
                className="h-9 rounded-lg border border-border bg-panel px-4 text-sm hover:border-lime/30 disabled:opacity-40"
                disabled={!name.trim() || !triggerValue || steps.length === 0}
                onClick={submit}
              >
                {editingId ? t("macroSave") : t("macroAdd")}
              </button>
              {editingId && (
                <button
                  className="h-9 rounded-lg border border-border bg-panel px-4 text-sm"
                  onClick={startNew}
                >
                  {t("macroCancelEdit")}
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
