import { useRef, useState } from "react";
import type { KeyboardLayout, QmkImportPreview } from "../types/keyboard";
import * as api from "../lib/tauri";
import { useAppStore } from "../stores/appStore";
import { useT } from "../hooks/useT";

interface QmkImportDialogProps {
  layout: KeyboardLayout;
  onClose: () => void;
}

/** Position order the backend expects: layout keys row-major, (y, x) sorted. */
function keyOrderOf(layout: KeyboardLayout): string[] {
  return [...layout.keys]
    .sort((a, b) => a.y - b.y || a.x - b.x)
    .map((k) => k.code);
}

function download(filename: string, text: string) {
  const blob = new Blob([text], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

/** S9: QMK keymap.json / VIA JSON import and export. File -> validate
 * (backend) -> preview -> apply/cancel. Import replaces the profile's
 * base keymap and layers; macros are untouched. Cancelling (or closing)
 * drops the preview and leaves the profile untouched. Export downloads
 * a JSON file. Actual QMK flashing is out of scope (docs/QMK-VIA.md). */
export function QmkImportDialog({ layout, onClose }: QmkImportDialogProps) {
  const t = useT();
  const profileId = useAppStore((s) => s.profileId);
  const error = useAppStore((s) => s.error);
  const importQmkKeymap = useAppStore((s) => s.importQmkKeymap);
  const importViaKeymap = useAppStore((s) => s.importViaKeymap);
  const exportQmkKeymap = useAppStore((s) => s.exportQmkKeymap);
  const exportViaKeymap = useAppStore((s) => s.exportViaKeymap);
  const [format, setFormat] = useState<"qmk" | "via">("qmk");
  const [fileName, setFileName] = useState<string | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const [json, setJson] = useState<string | null>(null);
  const [preview, setPreview] = useState<QmkImportPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const keyOrder = keyOrderOf(layout);

  const onFile = async (file: File | undefined) => {
    if (!file) return;
    setFileName(file.name);
    setFileError(null);
    setPreview(null);
    const raw = await file.text();
    setBusy(true);
    try {
      const result =
        format === "qmk"
          ? await api.validateQmkKeymap(profileId, raw, layout.id, keyOrder)
          : await api.validateViaKeymap(profileId, raw, layout.id, keyOrder);
      setJson(raw);
      setPreview(result);
    } catch (err) {
      setFileError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    if (!json) return;
    setBusy(true);
    try {
      if (format === "qmk") {
        await importQmkKeymap(json, layout.id, keyOrder);
      } else {
        await importViaKeymap(json, layout.id, keyOrder);
      }
      onClose();
    } finally {
      setBusy(false);
    }
  };

  const exportFile = async (kind: "qmk" | "via") => {
    setBusy(true);
    try {
      const text =
        kind === "qmk"
          ? await exportQmkKeymap(layout.id, keyOrder)
          : await exportViaKeymap(layout.id, keyOrder);
      download(`keyglow-${profileId}-${kind === "qmk" ? "keymap" : "via"}.json`, text);
    } catch {
      // The store surfaces the error through the shared error banner.
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6" onClick={onClose}>
      <div
        className="flex max-h-full w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-border bg-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-5 py-3">
          <span className="text-sm font-medium">{t("qmkTitle")}</span>
          <button className="text-xs text-muted" onClick={onClose}>
            {t("dismiss")}
          </button>
        </div>
        <div className="flex flex-col gap-4 overflow-auto p-5">
          <p className="text-sm text-muted">{t("qmkHint")}</p>
          <div className="flex flex-wrap items-center gap-3">
            <select
              value={format}
              onChange={(event) => {
                setFormat(event.target.value as "qmk" | "via");
                setPreview(null);
                setJson(null);
                setFileError(null);
                setFileName(null);
              }}
              aria-label={t("qmkFormat")}
              className="h-9 rounded-lg border border-border bg-panel px-3 text-sm outline-none"
            >
              <option value="qmk">{t("qmkFormatQmk")}</option>
              <option value="via">{t("qmkFormatVia")}</option>
            </select>
            <input
              ref={inputRef}
              type="file"
              accept=".json,application/json"
              className="hidden"
              onChange={(e) => void onFile(e.target.files?.[0])}
            />
            <button
              className="h-9 rounded-lg border border-border bg-panel-2 px-3 text-sm hover:border-lime/40 disabled:opacity-40"
              disabled={busy}
              onClick={() => inputRef.current?.click()}
            >
              {t("chooseFile")}
            </button>
            {fileName && <span className="text-sm text-muted">{fileName}</span>}
          </div>
          {(fileError || error) && (
            <div className="rounded-xl border border-danger/30 bg-danger/10 px-4 py-3 text-sm text-danger">
              {fileError ?? error}
            </div>
          )}
          {preview && (
            <div className="flex flex-col gap-2">
              <p className="text-sm font-medium">{t("qmkPreviewTitle")}</p>
              <p className="text-xs text-muted">
                {preview.keyboard ?? "—"} · {preview.keymap ?? "—"} · {preview.layout}
              </p>
              <p className="text-xs text-muted">
                {t("qmkBase")}: {preview.baseRemapCount} {t("qmkRemaps")} ·{" "}
                {preview.baseDisabledCount} {t("qmkDisabled")}
              </p>
              <ul className="flex flex-col gap-2">
                {preview.layers.map((layer) => (
                  <li
                    key={layer.index}
                    className="rounded-xl border border-border bg-panel-2 px-4 py-2 text-sm"
                  >
                    {t("qmkLayer")} {layer.index}
                    {layer.switchKey && (
                      <> · {t("qmkSwitchKey")}: {layer.switchKey} ({layer.switchMode})</>
                    )}{" "}
                    · {layer.remapCount} {t("qmkRemaps")} · {layer.disabledCount}{" "}
                    {t("qmkDisabled")}
                  </li>
                ))}
              </ul>
              <p className="text-xs text-muted">{t("qmkReplacesWarning")}</p>
              <div className="flex gap-2">
                <button
                  className="h-9 rounded-lg border border-lime/40 bg-lime/10 px-4 text-sm text-lime disabled:opacity-40"
                  disabled={busy}
                  onClick={() => void apply()}
                >
                  {t("qmkApply")}
                </button>
                <button
                  className="h-9 rounded-lg border border-border bg-panel px-4 text-sm"
                  onClick={onClose}
                >
                  {t("qmkCancel")}
                </button>
              </div>
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2 border-t border-border pt-4">
            <button
              className="h-9 rounded-lg border border-border bg-panel px-4 text-sm hover:border-lime/30 disabled:opacity-40"
              disabled={busy}
              onClick={() => void exportFile("qmk")}
            >
              {t("qmkExportQmk")}
            </button>
            <button
              className="h-9 rounded-lg border border-border bg-panel px-4 text-sm hover:border-lime/30 disabled:opacity-40"
              disabled={busy}
              onClick={() => void exportFile("via")}
            >
              {t("qmkExportVia")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
