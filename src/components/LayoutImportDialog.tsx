import { useRef, useState } from "react";
import { Keyboard } from "./Keyboard/Keyboard";
import { parseImportedLayout } from "../lib/layout-import";
import { useAppStore } from "../stores/appStore";
import { useT } from "../hooks/useT";

interface LayoutImportDialogProps {
  onClose: () => void;
}

/**
 * S6: layout import dialog — file -> validate -> preview -> apply/cancel.
 * Validation runs locally first (parseImportedLayout); the backend
 * re-validates on confirm. Cancelling (or closing) drops the preview
 * and leaves the current layout untouched.
 */
export function LayoutImportDialog({ onClose }: LayoutImportDialogProps) {
  const t = useT();
  const preview = useAppStore((s) => s.importPreview);
  const beginImportPreview = useAppStore((s) => s.beginImportPreview);
  const cancelImportPreview = useAppStore((s) => s.cancelImportPreview);
  const confirmImportPreview = useAppStore((s) => s.confirmImportPreview);
  const customLayouts = useAppStore((s) => s.customLayouts);
  const profiles = useAppStore((s) => s.profiles);
  const deleteCustomLayout = useAppStore((s) => s.deleteCustomLayout);
  const error = useAppStore((s) => s.error);
  const [fileError, setFileError] = useState<string | null>(null);
  const [fileName, setFileName] = useState<string | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const close = () => {
    cancelImportPreview();
    onClose();
  };

  const onFile = async (file: File | undefined) => {
    if (!file) return;
    setFileName(file.name);
    setFileError(null);
    const raw = await file.text();
    const result = parseImportedLayout(raw);
    if (!result.ok) {
      setFileError(result.error);
      return;
    }
    beginImportPreview(result.layout);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6" onClick={close}>
      <div
        className="flex max-h-full w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-border bg-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-5 py-3">
          <span className="text-sm font-medium">{t("importLayout")}</span>
          <button className="text-xs text-muted" onClick={close}>
            {t("dismiss")}
          </button>
        </div>
        <div className="flex flex-col gap-4 overflow-auto p-5">
          <p className="text-sm text-muted">{t("importLayoutHint")}</p>
          <div className="flex items-center gap-3">
            <input
              ref={inputRef}
              type="file"
              accept=".json,application/json"
              className="hidden"
              onChange={(e) => void onFile(e.target.files?.[0])}
            />
            <button
              className="h-9 rounded-lg border border-border bg-panel-2 px-3 text-sm hover:border-lime/40"
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
          {customLayouts.length > 0 && (
            <div className="flex flex-col gap-2">
              {customLayouts.map((layout) => {
                const inUse = profiles.some((p) => p.layout === layout.id);
                return (
                  <div
                    key={layout.id}
                    className="flex items-center justify-between gap-3 rounded-xl border border-border bg-panel-2 px-4 py-2 text-sm"
                  >
                    <span>
                      {layout.name} · {t("keysCount", { n: layout.keys.length })}
                    </span>
                    {inUse ? (
                      <span className="text-xs text-muted">{t("importInUse")}</span>
                    ) : (
                      <button
                        className="h-8 shrink-0 rounded-lg border border-danger/40 px-3 text-xs text-danger hover:bg-danger/10"
                        onClick={() => {
                          if (confirmDeleteId === layout.id) {
                            setConfirmDeleteId(null);
                            void deleteCustomLayout(layout.id);
                          } else {
                            setConfirmDeleteId(layout.id);
                          }
                        }}
                      >
                        {confirmDeleteId === layout.id ? t("importDeleteConfirm") : t("importDelete")}
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          )}
          {preview && (
            <>
              <div className="text-[11px] uppercase tracking-[0.14em] text-muted">
                {t("importPreviewTitle")} · {preview.name} ·{" "}
                {t("keysCount", { n: preview.keys.length })}
              </div>
              <div className="h-64 rounded-xl border border-border bg-panel-2 p-2">
                <Keyboard
                  layout={preview}
                  disabledKeys={[]}
                  pressedKeys={[]}
                  onToggle={() => {}}
                />
              </div>
            </>
          )}
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <button
            className="h-9 rounded-lg border border-border px-4 text-sm hover:border-lime/40"
            onClick={close}
          >
            {t("importCancel")}
          </button>
          <button
            className="h-9 rounded-lg bg-lime px-4 text-sm font-medium text-black disabled:opacity-40"
            disabled={!preview}
            onClick={() => {
              void confirmImportPreview().then(() => {
                if (useAppStore.getState().importPreview === null) onClose();
              });
            }}
          >
            {t("importApply")}
          </button>
        </div>
      </div>
    </div>
  );
}
