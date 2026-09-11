import { useState } from "react";
import type { Profile } from "../types/keyboard";
import { profileLabel } from "../lib/layouts";
import { useLocale, useT } from "../hooks/useT";

interface ProfileSelectorProps {
  profiles: Profile[];
  value: string;
  onChange: (id: string) => void;
  onCreate: (name: string) => void;
  onDuplicate: () => void;
  onRename: (name: string) => void;
  onDelete: (id: string) => void;
}

export function ProfileSelector({
  profiles,
  value,
  onChange,
  onCreate,
  onDuplicate,
  onRename,
  onDelete,
}: ProfileSelectorProps) {
  const t = useT();
  const locale = useLocale();
  const current = profiles.find((p) => p.id === value);
  const [draft, setDraft] = useState("");
  const [mode, setMode] = useState<"idle" | "create" | "rename">("idle");

  function submit() {
    const name = draft.trim();
    if (!name) return;
    if (mode === "create") onCreate(name);
    if (mode === "rename") onRename(name);
    setDraft("");
    setMode("idle");
  }

  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-[11px] uppercase tracking-[0.14em] text-muted">{t("profile")}</span>
      <div className="flex items-center gap-2">
        <select
          value={value}
          onChange={(event) => onChange(event.target.value)}
          className="h-9 w-44 rounded-lg border border-border bg-panel-2 px-3 text-sm outline-none focus:border-lime/40"
        >
          {profiles.map((profile) => (
            <option key={profile.id} value={profile.id}>
              {profileLabel(profile, locale)}
            </option>
          ))}
        </select>
        <IconButton label={t("newProfile")} onClick={() => setMode("create")}>
          +
        </IconButton>
        <IconButton label={t("duplicateProfile")} onClick={onDuplicate}>
          ⧉
        </IconButton>
        <IconButton
          label={t("renameProfile")}
          onClick={() => {
            setDraft(current?.name ?? "");
            setMode("rename");
          }}
        >
          ✎
        </IconButton>
        <IconButton
          label={t("deleteProfile")}
          onClick={() => {
            if (current && current.id !== "default") onDelete(current.id);
          }}
        >
          ⌫
        </IconButton>
      </div>
      {mode !== "idle" && (
        <form
          className="mt-1 flex gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <input
            autoFocus
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder={mode === "create" ? t("profileName") : t("newName")}
            className="h-8 flex-1 rounded-md border border-border bg-bg px-2 text-sm outline-none"
          />
          <button type="submit" className="h-8 rounded-md bg-lime/20 px-3 text-xs text-lime">
            {t("save")}
          </button>
          <button type="button" className="h-8 rounded-md px-2 text-xs text-muted" onClick={() => setMode("idle")}>
            {t("cancel")}
          </button>
        </form>
      )}
    </div>
  );
}

function IconButton({
  children,
  onClick,
  label,
}: {
  children: string;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      className="grid h-9 w-8 place-items-center rounded-lg border border-border bg-panel-2 text-sm text-muted hover:border-lime/30 hover:text-text"
    >
      {children}
    </button>
  );
}
