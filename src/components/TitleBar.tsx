import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { LanguageSelector } from "./LanguageSelector";
import { ThemeToggle } from "./ThemeToggle";
import { useT } from "../hooks/useT";

export function TitleBar() {
  const [maximized, setMaximized] = useState(false);
  const t = useT();

  useEffect(() => {
    const window = getCurrentWindow();
    window.isMaximized().then(setMaximized).catch(() => {});
  }, []);

  async function minimize() {
    await getCurrentWindow().minimize();
  }

  async function toggleMax() {
    const window = getCurrentWindow();
    const isMax = await window.isMaximized();
    if (isMax) await window.unmaximize();
    else await window.maximize();
    setMaximized(!isMax);
  }

  async function closeToTray() {
    await getCurrentWindow().close();
  }

  return (
    <div className="flex h-11 items-center border-b border-border px-2">
      <div className="flex items-center gap-3 pl-1" data-tauri-drag-region>
        <div className="grid h-6 w-6 place-items-center rounded-md bg-lime/15 text-[11px] font-semibold text-lime">
          K
        </div>
        <div>
          <div className="text-[13px] font-semibold tracking-wide">KeyGlow</div>
          <div className="text-[10px] text-muted">{t("tagline")}</div>
        </div>
      </div>
      <div className="h-full flex-1" data-tauri-drag-region />
      <ThemeToggle />
      <LanguageSelector />
      <div className="flex items-center gap-0.5 pr-1 pl-2">
        <button className="titlebar-btn" onClick={minimize} aria-label={t("minimize")} title={t("minimize")}>
          <span className="text-base leading-none">−</span>
        </button>
        <button
          className="titlebar-btn"
          onClick={toggleMax}
          aria-label={maximized ? t("restore") : t("maximize")}
          title={maximized ? t("restore") : t("maximize")}
        >
          <span className="text-xs leading-none">{maximized ? "❐" : "□"}</span>
        </button>
        <button className="titlebar-btn close" onClick={closeToTray} aria-label={t("hideToTray")} title={t("hideToTray")}>
          <span className="text-sm leading-none">×</span>
        </button>
      </div>
    </div>
  );
}
