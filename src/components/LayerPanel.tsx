import { useState } from "react";
import type { KeyboardLayout, LayerDef, LayerMode, RemapRule } from "../types/keyboard";
import { useT } from "../hooks/useT";

/** Keys that can never switch a layer (S9): Pause has no key-up. */
const EXCLUDED_SWITCH = new Set(["Pause"]);

interface LayerPanelProps {
  layers: LayerDef[];
  activeLayerId: string | null;
  layout: KeyboardLayout;
  onSave: (layer: LayerDef) => void;
  onDelete: (id: string) => void;
}

function newLayerId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `layer-${crypto.randomUUID()}`;
  }
  return `layer-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;
}

/** S9: layers for the current profile. Collapsible, same pattern as
 * RemapPanel. The backend validates the rest (one key one role, switch
 * key reachability) and surfaces the error; the panel only guards the
 * obvious (empty name, no switch key). While a layer is active its
 * name is marked. */
export function LayerPanel({
  layers,
  activeLayerId,
  layout,
  onSave,
  onDelete,
}: LayerPanelProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [switchKey, setSwitchKey] = useState("");
  const [mode, setMode] = useState<LayerMode>("hold");
  const [disabledKeys, setDisabledKeys] = useState<string[]>([]);
  const [remappings, setRemappings] = useState<RemapRule[]>([]);
  const [addDisabled, setAddDisabled] = useState("");
  const [remapFrom, setRemapFrom] = useState("");
  const [remapTo, setRemapTo] = useState("");

  const labelOf = (code: string) =>
    layout.keys.find((k) => k.code === code)?.label ?? code;

  const usedSwitches = new Set(layers.map((l) => l.switchKey));
  const switchOptions = layout.keys.filter(
    (k) => !EXCLUDED_SWITCH.has(k.code) && (!usedSwitches.has(k.code) || k.code === switchKey),
  );
  const switchValue =
    switchOptions.some((k) => k.code === switchKey) && switchKey !== ""
      ? switchKey
      : (switchOptions[0]?.code ?? "");
  const keyOptions = layout.keys.filter((k) => !EXCLUDED_SWITCH.has(k.code));
  const disabledOptions = keyOptions.filter((k) => !disabledKeys.includes(k.code));
  const remapFromOptions = keyOptions.filter(
    (k) => !remappings.some((r) => r.from === k.code),
  );

  function startNew() {
    setEditingId(null);
    setName("");
    setSwitchKey(switchOptions[0]?.code ?? "");
    setMode("hold");
    setDisabledKeys([]);
    setRemappings([]);
    setAddDisabled("");
    setRemapFrom("");
    setRemapTo("");
  }

  function startEdit(layer: LayerDef) {
    setEditingId(layer.id);
    setName(layer.name);
    setSwitchKey(layer.switchKey);
    setMode(layer.mode);
    setDisabledKeys([...layer.disabledKeys]);
    setRemappings(layer.remappings.map((r) => ({ ...r })));
    setAddDisabled("");
    setRemapFrom("");
    setRemapTo("");
  }

  function submit() {
    const trimmed = name.trim();
    if (!trimmed || !switchValue) return;
    onSave({
      id: editingId ?? newLayerId(),
      name: trimmed,
      switchKey: switchValue,
      mode,
      disabledKeys: [...disabledKeys],
      remappings: remappings.map((r) => ({ ...r })),
    });
    startNew();
  }

  return (
    <section className="mt-3 flex-shrink-0 rounded-2xl border border-border bg-panel px-4 py-3">
      <button
        className="flex w-full items-center justify-between text-left text-sm"
        onClick={() => setOpen((v) => !v)}
      >
        <span className="font-medium">{t("layerTitle")}</span>
        <span className="text-xs text-muted">
          {layers.length === 0
            ? t("noLayers")
            : layers
                .map(
                  (l) =>
                    `${l.name} (${labelOf(l.switchKey)} · ${t(l.mode === "hold" ? "layerModeHold" : "layerModeToggle")})${l.id === activeLayerId ? ` · ${t("layerActive")}` : ""}`,
                )
                .join(" · ")}
          <span className="ml-2">{open ? "▾" : "▸"}</span>
        </span>
      </button>

      {open && (
        <div className="mt-3 flex flex-col gap-3 text-sm">
          <p className="text-xs text-muted">{t("layerHint")}</p>

          {layers.length === 0 ? (
            <p className="text-xs text-muted">{t("noLayers")}</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {layers.map((layer) => (
                <li
                  key={layer.id}
                  className="flex items-center justify-between gap-3 rounded-lg border border-border bg-panel-2 px-3 py-2"
                >
                  <span className="min-w-0 truncate text-xs">
                    {layer.name} · {labelOf(layer.switchKey)} ·{" "}
                    {t(layer.mode === "hold" ? "layerModeHold" : "layerModeToggle")} ·{" "}
                    {layer.disabledKeys.length} {t("layerDisabled")} ·{" "}
                    {layer.remappings.length} {t("layerRemaps")}
                    {layer.id === activeLayerId && ` · ${t("layerActive")}`}
                  </span>
                  <span className="flex flex-shrink-0 gap-1">
                    <button
                      className="h-8 rounded-lg border border-border px-2 text-xs hover:border-lime/30"
                      onClick={() => startEdit(layer)}
                    >
                      {t("layerEdit")}
                    </button>
                    <button
                      className="h-8 rounded-lg border border-border px-2 text-xs hover:border-danger/40"
                      title={t("layerDelete")}
                      onClick={() => onDelete(layer.id)}
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
                placeholder={t("layerName")}
                aria-label={t("layerName")}
                className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
              />
              <select
                value={switchValue}
                onChange={(event) => setSwitchKey(event.target.value)}
                aria-label={t("layerSwitchKey")}
                className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
              >
                {switchOptions.map((k) => (
                  <option key={k.code} value={k.code}>
                    {k.label}
                  </option>
                ))}
              </select>
              <select
                value={mode}
                onChange={(event) => setMode(event.target.value as LayerMode)}
                aria-label={t("layerMode")}
                className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
              >
                <option value="hold">{t("layerModeHold")}</option>
                <option value="toggle">{t("layerModeToggle")}</option>
              </select>
            </div>

            <p className="text-xs text-muted">{t("layerDisabledHint")}</p>
            <div className="flex flex-wrap items-center gap-2">
              <select
                value={addDisabled}
                onChange={(event) => setAddDisabled(event.target.value)}
                aria-label={t("layerAddDisabled")}
                className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
              >
                <option value="">{t("layerAddDisabled")}</option>
                {disabledOptions.map((k) => (
                  <option key={k.code} value={k.code}>
                    {k.label}
                  </option>
                ))}
              </select>
              <button
                className="h-9 rounded-lg border border-border bg-panel px-4 text-sm hover:border-lime/30 disabled:opacity-40"
                disabled={!addDisabled}
                onClick={() => {
                  if (addDisabled) {
                    setDisabledKeys((prev) => [...prev, addDisabled]);
                    setAddDisabled("");
                  }
                }}
              >
                {t("add")}
              </button>
              {disabledKeys.map((code) => (
                <button
                  key={code}
                  className="h-8 rounded-lg border border-border px-2 text-xs hover:border-danger/40"
                  title={t("remove")}
                  onClick={() =>
                    setDisabledKeys((prev) => prev.filter((c) => c !== code))
                  }
                >
                  {labelOf(code)} ✕
                </button>
              ))}
            </div>

            <p className="text-xs text-muted">{t("layerRemapHint")}</p>
            <div className="flex flex-wrap items-center gap-2">
              <select
                value={remapFrom}
                onChange={(event) => setRemapFrom(event.target.value)}
                aria-label={t("remapFrom")}
                className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
              >
                <option value="">{t("remapFrom")}</option>
                {remapFromOptions.map((k) => (
                  <option key={k.code} value={k.code}>
                    {k.label}
                  </option>
                ))}
              </select>
              <span className="text-xs text-muted">→</span>
              <select
                value={remapTo}
                onChange={(event) => setRemapTo(event.target.value)}
                aria-label={t("remapTo")}
                className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
              >
                <option value="">{t("remapTo")}</option>
                {keyOptions.map((k) => (
                  <option key={k.code} value={k.code}>
                    {k.label}
                  </option>
                ))}
              </select>
              <button
                className="h-9 rounded-lg border border-border bg-panel px-4 text-sm hover:border-lime/30 disabled:opacity-40"
                disabled={!remapFrom || !remapTo || remapFrom === remapTo}
                onClick={() => {
                  if (remapFrom && remapTo && remapFrom !== remapTo) {
                    setRemappings((prev) => [...prev, { from: remapFrom, to: remapTo }]);
                    setRemapFrom("");
                    setRemapTo("");
                  }
                }}
              >
                {t("add")}
              </button>
            </div>
            {remappings.length > 0 && (
              <ul className="flex flex-col gap-2">
                {remappings.map((rule) => (
                  <li
                    key={rule.from}
                    className="flex items-center justify-between gap-3 rounded-lg border border-border bg-panel px-3 py-2 text-xs"
                  >
                    <span>
                      {labelOf(rule.from)} → {labelOf(rule.to)}
                    </span>
                    <button
                      className="h-8 rounded-lg border border-border px-2 text-xs hover:border-danger/40"
                      onClick={() =>
                        setRemappings((prev) => prev.filter((r) => r.from !== rule.from))
                      }
                    >
                      ✕
                    </button>
                  </li>
                ))}
              </ul>
            )}

            <div className="flex flex-wrap items-center gap-2">
              <button
                className="h-9 rounded-lg border border-border bg-panel px-4 text-sm hover:border-lime/30 disabled:opacity-40"
                disabled={!name.trim() || !switchValue}
                onClick={submit}
              >
                {editingId ? t("layerSave") : t("layerAdd")}
              </button>
              {editingId && (
                <button
                  className="h-9 rounded-lg border border-border bg-panel px-4 text-sm"
                  onClick={startNew}
                >
                  {t("layerCancelEdit")}
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
