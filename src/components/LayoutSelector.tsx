import { allLayouts, layoutDescription, layoutOptionLabel, layoutSource } from "../lib/layouts";
import { useLocale, useT } from "../hooks/useT";

interface LayoutSelectorProps {
  value: string;
  onChange: (id: string) => void;
}

export function LayoutSelector({ value, onChange }: LayoutSelectorProps) {
  const t = useT();
  const locale = useLocale();
  const layouts = allLayouts();

  return (
    <label className="flex min-w-56 flex-col gap-1">
      <span className="text-[11px] uppercase tracking-[0.14em] text-muted">{t("layout")}</span>
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-9 max-w-xs rounded-lg border border-border bg-panel-2 px-3 text-sm outline-none focus:border-lime/40"
        title={layoutDescription(value, locale)}
      >
        {layouts.map((layout) => (
          <option key={layout.id} value={layout.id}>
            {layoutOptionLabel(layout.id, locale)}
            {layoutSource(layout.id) === "custom" ? ` [${t("customLayoutBadge")}]` : ""}
          </option>
        ))}
      </select>
    </label>
  );
}
