import type { KeyboardLayout } from "../types/keyboard";
import type { MessageKey } from "../i18n";
import { t, type Locale } from "../i18n";
import fullsize from "../layouts/fullsize-ansi.json";
import tkl from "../layouts/tkl-ansi.json";
import compact75 from "../layouts/75-ansi.json";
import compact65 from "../layouts/65-ansi.json";
import compact60 from "../layouts/60-ansi.json";
import fullsizeIso from "../layouts/fullsize-iso.json";
import fullsizeJis from "../layouts/fullsize-jis.json";

// S6: layout registry. Builtin layouts ship as generated JSON; custom
// layouts arrive through the importer and are held here as a mirror of
// the backend config (the store owns the truth and calls
// setCustomLayouts after hydrate / import / delete).
export const BUILTIN_LAYOUTS: KeyboardLayout[] = [
  fullsize as KeyboardLayout,
  tkl as KeyboardLayout,
  compact75 as KeyboardLayout,
  compact65 as KeyboardLayout,
  compact60 as KeyboardLayout,
  fullsizeIso as KeyboardLayout,
  fullsizeJis as KeyboardLayout,
];

/** Back-compat alias: the builtin registry. */
export const LAYOUTS: KeyboardLayout[] = BUILTIN_LAYOUTS;

export type LayoutSource = "builtin" | "pack" | "custom" | "unknown";

// S6: layout packs (HHKB, Alice, split) register here later. A pack is
// just more builtin layouts: registerLayoutPack validates the ids and
// appends them. No other code needs to change.
const packs: KeyboardLayout[][] = [];

export function registerLayoutPack(layouts: KeyboardLayout[]): void {
  const known = new Set(allLayouts().map((l) => l.id));
  for (const layout of layouts) {
    if (known.has(layout.id)) {
      throw new Error(`layout pack id collides: ${layout.id}`);
    }
    known.add(layout.id);
  }
  packs.push([...layouts]);
}

let customLayouts: KeyboardLayout[] = [];

export function setCustomLayouts(layouts: KeyboardLayout[]): void {
  customLayouts = [...layouts];
}

export function getCustomLayouts(): KeyboardLayout[] {
  return [...customLayouts];
}

export function allLayouts(): KeyboardLayout[] {
  return [...BUILTIN_LAYOUTS, ...packs.flat(), ...customLayouts];
}

export function getLayout(id: string): KeyboardLayout {
  return allLayouts().find((layout) => layout.id === id) ?? (tkl as KeyboardLayout);
}

export function layoutSource(id: string): LayoutSource {
  if (BUILTIN_LAYOUTS.some((l) => l.id === id)) return "builtin";
  if (packs.flat().some((l) => l.id === id)) return "pack";
  if (customLayouts.some((l) => l.id === id)) return "custom";
  return "unknown";
}

export function isBuiltinLayoutId(id: string): boolean {
  return BUILTIN_LAYOUTS.some((l) => l.id === id);
}

export function layoutName(id: string, locale: Locale): string {
  const source = layoutSource(id);
  if (source === "custom" || source === "pack") {
    return getLayout(id).name;
  }
  return t(locale, `layout.${id}.name` as MessageKey);
}

export function layoutHint(id: string, locale: Locale): string {
  const source = layoutSource(id);
  if (source === "custom" || source === "pack") {
    return getLayout(id).hint ?? "";
  }
  return t(locale, `layout.${id}.hint` as MessageKey);
}

export function layoutDescription(id: string, locale: Locale): string {
  const source = layoutSource(id);
  if (source === "custom" || source === "pack") {
    return getLayout(id).description ?? "";
  }
  return t(locale, `layout.${id}.description` as MessageKey);
}

export function layoutOptionLabel(id: string, locale: Locale): string {
  const hint = layoutHint(id, locale);
  return hint ? `${layoutName(id, locale)} (${hint})` : layoutName(id, locale);
}

export function profileLabel(
  profile: { id: string; name: string; builtin?: boolean },
  locale: Locale,
): string {
  if (profile.id === "default" || profile.id === "gaming" || profile.id === "coding") {
    return t(locale, `profile.${profile.id}` as MessageKey);
  }
  return profile.name;
}
