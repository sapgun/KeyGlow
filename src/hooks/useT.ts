import { useCallback } from "react";
import { t, type MessageKey } from "../i18n";
import { useAppStore } from "../stores/appStore";

export function useT() {
  const locale = useAppStore((state) => state.locale);
  return useCallback(
    (key: MessageKey, vars?: Record<string, string | number>) => t(locale, key, vars),
    [locale],
  );
}

export function useLocale() {
  return useAppStore((state) => state.locale);
}
