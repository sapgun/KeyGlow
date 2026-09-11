export type Theme = "dark" | "light";

const STORAGE_KEY = "keyglow-theme";

export function parseTheme(value: string | null | undefined): Theme | null {
  if (value === "dark" || value === "light") return value;
  return null;
}

export function readStoredTheme(): Theme {
  try {
    return parseTheme(localStorage.getItem(STORAGE_KEY)) ?? "dark";
  } catch {
    return "dark";
  }
}

export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    /* ignore quota / private mode */
  }
}
