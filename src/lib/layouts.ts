import type { KeyboardLayout } from "../types/keyboard";
import type { MessageKey } from "../i18n";
import { t, type Locale } from "../i18n";
import fullsize from "../layouts/fullsize-ansi.json";
import tkl from "../layouts/tkl-ansi.json";
import compact75 from "../layouts/75-ansi.json";
import compact65 from "../layouts/65-ansi.json";
import compact60 from "../layouts/60-ansi.json";

export const LAYOUTS: KeyboardLayout[] = [
  fullsize as KeyboardLayout,
  tkl as KeyboardLayout,
  compact75 as KeyboardLayout,
  compact65 as KeyboardLayout,
  compact60 as KeyboardLayout,
];

export function getLayout(id: string): KeyboardLayout {
  return LAYOUTS.find((layout) => layout.id === id) ?? (tkl as KeyboardLayout);
}

export function layoutName(id: string, locale: Locale): string {
  return t(locale, `layout.${id}.name` as MessageKey);
}

export function layoutHint(id: string, locale: Locale): string {
  return t(locale, `layout.${id}.hint` as MessageKey);
}

export function layoutDescription(id: string, locale: Locale): string {
  return t(locale, `layout.${id}.description` as MessageKey);
}

export function layoutOptionLabel(id: string, locale: Locale): string {
  return `${layoutName(id, locale)} (${layoutHint(id, locale)})`;
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
