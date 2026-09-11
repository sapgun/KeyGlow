import { useT } from "../hooks/useT";

interface StatusBarProps {
  hookActive: boolean;
  hookError: string | null;
  layoutName: string;
  profileName: string;
  disabledCount: number;
}

export function StatusBar({
  hookActive,
  hookError,
  layoutName,
  profileName,
  disabledCount,
}: StatusBarProps) {
  const t = useT();
  return (
    <div className="flex flex-shrink-0 flex-wrap items-center justify-between gap-3 border-t border-border px-6 py-3 text-xs text-muted">
      <div className="flex items-center gap-4">
        <span className="inline-flex items-center gap-2">
          <span
            className="h-2 w-2 rounded-full"
            style={{
              background: hookActive ? "#b6f25c" : "#f87171",
              boxShadow: hookActive ? "0 0 8px #b6f25c" : "none",
            }}
          />
          {hookActive ? t("hookActive") : t("hookUnavailable")}
        </span>
        <span>{layoutName}</span>
        <span>{profileName}</span>
        <span>{t("keysDisabled", { n: disabledCount })}</span>
      </div>
      <div>{t("emergencyUnlock")}</div>
      {hookError && <div className="w-full text-danger">{hookError}</div>}
    </div>
  );
}
