import type { KeyDef, VisualKeyState } from "../../types/keyboard";
import { useT } from "../../hooks/useT";

interface KeyboardKeyProps {
  keyDef: KeyDef;
  state: VisualKeyState;
  unit: number;
  gap: number;
  onToggle: (code: string) => void;
}

export function KeyboardKey({ keyDef, state, unit, gap, onToggle }: KeyboardKeyProps) {
  const t = useT();
  const width = Math.max(28, keyDef.w * unit - gap);
  const height = Math.max(28, keyDef.h * unit - gap);
  const left = keyDef.x * unit;
  const top = keyDef.y * unit;
  const compact = width < 46 || height < 36;
  const unsupported = Boolean(keyDef.unsupported);

  const className = [
    "keycap",
    unsupported ? "unsupported" : state,
  ].join(" ");

  const stateLabel =
    state === "disabled" ? t("keyDisabled") : state === "pressed" ? t("keyPressed") : t("keyEnabled");
  const title = unsupported
    ? t("fnUnsupported", { label: keyDef.label || keyDef.code })
    : t("keyTooltip", {
        label: keyDef.label || keyDef.code,
        state: stateLabel,
        action: state === "disabled" ? t("keyEnable") : t("keyDisable"),
      });

  return (
    <div
      role="button"
      className={className}
      title={title}
      aria-label={title}
      aria-pressed={state !== "disabled"}
      aria-disabled={unsupported}
      tabIndex={-1}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => {
        if (!unsupported) onToggle(keyDef.code);
      }}
      style={{
        position: "absolute",
        left,
        top,
        width,
        height,
        padding: compact ? "4px 5px" : "6px 8px",
        zIndex: state === "pressed" ? 2 : 1,
      }}
    >
      <span
        className="block text-[10px] leading-none tracking-wide"
        style={{
          fontSize: compact ? 9 : keyDef.subLabel ? 9 : 11,
          opacity: 0.55,
          minHeight: keyDef.subLabel ? 11 : 0,
        }}
      >
        {keyDef.subLabel ?? ""}
      </span>
      <span
        className="font-medium leading-none"
        style={{
          fontSize: labelSize(keyDef, compact),
          letterSpacing: keyDef.label.length > 4 ? "-0.02em" : "0.02em",
        }}
      >
        {keyDef.label}
      </span>
    </div>
  );
}

function labelSize(keyDef: KeyDef, compact: boolean) {
  if (compact) return 10;
  if (keyDef.label.length > 6) return 10;
  if (keyDef.w >= 2) return 11;
  return 12;
}
