import { LAYOUTS, layoutDescription, layoutName } from "../lib/layouts";
import { useLocale, useT } from "../hooks/useT";

interface OnboardingProps {
  onChoose: (layoutId: string) => void;
}

export function Onboarding({ onChoose }: OnboardingProps) {
  const t = useT();
  const locale = useLocale();

  return (
    <div className="flex h-full flex-col items-center justify-center px-10">
      <div className="mb-8 text-center">
        <div className="text-xs uppercase tracking-[0.22em] text-lime">KeyGlow</div>
        <h1 className="mt-3 text-3xl font-semibold">{t("onboardTitle")}</h1>
        <p className="mt-2 max-w-xl text-sm text-muted">{t("onboardBody")}</p>
      </div>
      <div className="flex w-full max-w-5xl flex-wrap justify-center gap-3">
        {LAYOUTS.map((layout) => (
          <button
            key={layout.id}
            type="button"
            onClick={() => onChoose(layout.id)}
            className="w-48 rounded-2xl border border-border bg-panel p-4 text-left transition hover:border-lime/40 hover:bg-panel-2"
          >
            <div className="text-sm font-medium">{layoutName(layout.id, locale)}</div>
            <div className="mt-1 text-xs leading-snug text-muted">
              {layoutDescription(layout.id, locale)}
            </div>
            <div className="mt-2 text-[11px] text-muted/80">{t("keysCount", { n: layout.keys.length })}</div>
            <LayoutMini id={layout.category} />
          </button>
        ))}
      </div>
    </div>
  );
}

function LayoutMini({ id }: { id: string }) {
  const widths: Record<string, string> = {
    fullsize: "w-full",
    tkl: "w-[82%]",
    "75": "w-[72%]",
    "65": "w-[64%]",
    "60": "w-[56%]",
  };
  return (
    <div className="mt-4 h-10 rounded-md bg-overlay p-2">
      <div className={`h-full rounded-sm bg-lime/20 ${widths[id] ?? "w-2/3"}`} />
    </div>
  );
}
