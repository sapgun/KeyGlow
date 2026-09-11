import { LOCALE_OPTIONS, type Locale } from "../i18n";
import { useAppStore } from "../stores/appStore";
import { useT } from "../hooks/useT";

export function LanguageSelector() {
  const locale = useAppStore((state) => state.locale);
  const setLocale = useAppStore((state) => state.setLocale);
  const t = useT();

  return (
    <div className="flex items-center rounded-lg border border-white/8 bg-panel-2 p-0.5" title={t("language")}>
      {LOCALE_OPTIONS.map((option) => (
        <button
          key={option.id}
          type="button"
          onClick={() => void setLocale(option.id as Locale)}
          className={`rounded-md px-2 py-1 text-[11px] ${
            locale === option.id ? "bg-lime/20 text-lime" : "text-muted hover:text-text"
          }`}
        >
          {option.native}
        </button>
      ))}
    </div>
  );
}
