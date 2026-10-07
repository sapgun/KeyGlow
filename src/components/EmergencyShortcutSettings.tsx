import { useEffect, useRef, useState } from "react";
import { useT } from "../hooks/useT";
import { useAppStore } from "../stores/appStore";
import * as api from "../lib/tauri";
import type { EmergencyShortcutConfig } from "../types/keyboard";

const defaults: EmergencyShortcutConfig = { ctrl: true, shift: true, alt: false, key: "F12" };
const keys = [
  ...Array.from({ length: 26 }, (_, i) => `Key${String.fromCharCode(65 + i)}`),
  ...Array.from({ length: 10 }, (_, i) => `Digit${i}`),
  ...Array.from({ length: 12 }, (_, i) => `F${i + 1}`),
  "Escape", "Space", "Enter", "Pause",
];
const label = (key: string) => ({
  ControlLeft: "Ctrl (L)", ControlRight: "Ctrl (R)",
  ShiftLeft: "Shift (L)", ShiftRight: "Shift (R)",
  AltLeft: "Alt (L)", AltRight: "Alt (R)", Escape: "Esc",
} as Record<string, string>)[key] ?? key.replace(/^(Key|Digit)/, "");

export function EmergencyShortcutSettings() {
  const t = useT();
  const store = useAppStore();
  const dialog = useRef<HTMLDialogElement>(null);
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<EmergencyShortcutConfig>(defaults);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [pressed, setPressed] = useState<string[]>([]);
  const [unlocked, setUnlocked] = useState(false);
  const valid = [draft.ctrl, draft.shift, draft.alt].filter(Boolean).length >= 2;

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    let initialEpoch: number | undefined;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const [press, state] = await Promise.all([api.getPressedSnapshot(), api.getAppState()]);
        if (cancelled) return;
        initialEpoch ??= state.safetyEpoch;
        setPressed(press.pressed);
        if (state.safetyEpoch > initialEpoch && state.safetyReconciled >= state.safetyEpoch) {
          setUnlocked(true);
        }
      } catch {
        if (!cancelled) setError(t("shortcutReadFailed"));
      }
      if (!cancelled) timer = setTimeout(poll, 200);
    };
    void poll();
    return () => { cancelled = true; clearTimeout(timer); };
  }, [open, t]);

  const show = () => {
    setDraft({ ...store.emergencyShortcutConfig });
    setError(""); setPressed([]); setUnlocked(false); setOpen(true);
    dialog.current?.showModal();
  };
  const close = () => { dialog.current?.close(); setOpen(false); };
  const save = async () => {
    if (!valid || busy) return;
    setBusy(true); setError("");
    const saved = await useAppStore.getState().setEmergencyShortcut(draft);
    setBusy(false);
    if (saved) close();
    else setError(t("settingsSaveFailed"));
  };

  return <>
    <button className="h-9 rounded-lg border border-border bg-panel-2 px-3 text-sm hover:border-lime/30" onClick={show}>
      {t("shortcutSettings")}
    </button>
    <dialog ref={dialog} onCancel={event => { if (busy) event.preventDefault(); else setOpen(false); }} onClose={() => setOpen(false)}
      aria-labelledby="shortcut-title" className="m-auto w-[min(480px,90vw)] rounded-2xl border border-border bg-panel p-6 text-text backdrop:bg-black/60">
      <h2 id="shortcut-title" className="mb-3 text-lg font-medium">{t("shortcutSettings")}</h2>
      <p className="mb-4 text-sm text-muted">{t("shortcutHelp")}</p>
      <fieldset disabled={busy} className="mb-4 flex gap-4">
        <legend className="mb-2 text-sm">{t("shortcutModifiers")}</legend>
        {(["ctrl", "shift", "alt"] as const).map(key => <label key={key} className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={draft[key]} onChange={e => setDraft({ ...draft, [key]: e.target.checked })} />
          {{ ctrl: "Ctrl", shift: "Shift", alt: "Alt" }[key]}
        </label>)}
      </fieldset>
      <label className="mb-4 flex items-center justify-between gap-4 text-sm">
        {t("shortcutKey")}
        <select disabled={busy} value={draft.key} onChange={e => setDraft({ ...draft, key: e.target.value })}
          className="rounded-lg border border-border bg-panel-2 px-3 py-2">
          {keys.map(key => <option key={key} value={key}>{label(key)}</option>)}
        </select>
      </label>
      <div className="mb-4 flex gap-3 text-sm">
        <button disabled={busy} className="rounded-lg border border-border px-3 py-2" onClick={() => setDraft({ ...defaults, key: "KeyU" })}>Ctrl + Shift + U</button>
        <button disabled={busy} className="rounded-lg border border-border px-3 py-2" onClick={() => setDraft({ ...defaults })}>{t("shortcutDefault")}</button>
      </div>
      {!valid && <p className="mb-3 text-sm text-danger">{t("shortcutValidation")}</p>}
      <p className="mb-3 text-xs text-muted">{t("shortcutFallback")}</p>
      <div className="mb-4 rounded-lg border border-border p-3 text-sm" role="status">
        <p>{t("shortcutCurrent", { shortcut: store.emergencyShortcut })}</p>
        <p className="mt-2 text-muted">{t("shortcutTestHelp")}</p>
        <p className="mt-2">{t("shortcutReceived", { keys: pressed.length ? pressed.map(label).join(" + ") : "—" })}</p>
        {unlocked && <p className="mt-2 text-lime">{t("shortcutVerified")}</p>}
      </div>
      {error && <p role="alert" className="mb-3 text-sm text-danger">{error}</p>}
      <div className="flex justify-end gap-3">
        <button disabled={busy} className="rounded-lg border border-border px-4 py-2" onClick={close}>{t("cancel")}</button>
        <button disabled={!valid || busy} className="rounded-lg bg-lime px-4 py-2 text-black disabled:opacity-40" onClick={() => void save()}>{t("save")}</button>
      </div>
    </dialog>
  </>;
}
