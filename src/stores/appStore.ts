import { create } from "zustand";
import type { AppSnapshot, Profile } from "../types/keyboard";
import * as api from "../lib/tauri";
import { detectLocale, parseLocale, type Locale } from "../i18n";
import { applyTheme, parseTheme, readStoredTheme, type Theme } from "../lib/theme";

const PRESS_TIMEOUT_MS = 8000;

interface AppStore {
  hydrated: boolean;
  hookActive: boolean;
  hookError: string | null;
  layoutId: string;
  profileId: string;
  profiles: Profile[];
  disabledKeys: string[];
  pressedKeys: string[];
  lastPressed: string | null;
  catLock: boolean;
  locale: Locale;
  theme: Theme;
  onboarded: boolean;
  startWithWindows: boolean;
  deviceName: string;
  emergencyNotice: boolean;
  error: string | null;
  persisted: boolean;
  persistError: string | null;
  persistErrorKind: string | null;
  configRevision: number;
  hydrate: () => Promise<void>;
  applySnapshot: (snapshot: AppSnapshot) => void;
  toggleKey: (code: string) => Promise<void>;
  enableAll: () => Promise<void>;
  chooseLayout: (id: string) => Promise<void>;
  chooseProfile: (id: string) => Promise<void>;
  newProfile: (name: string) => Promise<void>;
  copyProfile: () => Promise<void>;
  renameCurrent: (name: string) => Promise<void>;
  removeProfile: (id: string) => Promise<void>;
  resetCurrent: () => Promise<void>;
  markOnboarded: (layoutId: string) => Promise<void>;
  setAutostart: (enabled: boolean) => Promise<void>;
  setLocale: (locale: Locale) => Promise<void>;
  setTheme: (theme: Theme) => Promise<void>;
  toggleCatLock: () => Promise<void>;
  retryPersist: () => Promise<void>;
  notePress: (code: string, down: boolean) => void;
  clearEmergency: () => void;
  showEmergency: () => void;
}

function apply(snapshot: AppSnapshot): Partial<AppStore> {
  return {
    hydrated: true,
    hookActive: snapshot.hookActive,
    hookError: snapshot.hookError,
    layoutId: snapshot.layoutId,
    profileId: snapshot.profileId,
    profiles: snapshot.profiles,
    disabledKeys: snapshot.disabledKeys,
    onboarded: snapshot.onboarded,
    startWithWindows: snapshot.startWithWindows,
    deviceName: snapshot.deviceName,
    catLock: Boolean(snapshot.catLock),
    locale: parseLocale(snapshot.locale) ?? detectLocale(),
    theme: parseTheme(snapshot.theme) ?? readStoredTheme(),
    error: null,
    persisted: snapshot.persisted,
    persistError: snapshot.persistError,
    persistErrorKind: snapshot.persistErrorKind,
    configRevision: snapshot.configRevision,
  };
}

const pressTimers = new Map<string, number>();

export const useAppStore = create<AppStore>((set, get) => {
  // Shared failure path (HF-03): refresh the authoritative snapshot so the
  // UI shows the real applied state, then surface the error. The snapshot
  // also carries the sticky persist state (persisted/persistError), so an
  // unsaved-settings banner survives until a retry succeeds. No rejected
  // promise ever escapes a store action.
  const fail = async (err: unknown) => {
    try {
      const snapshot = await api.getAppState();
      set({ ...apply(snapshot), error: String(err) });
    } catch {
      set({ error: String(err) });
    }
  };

  return {
  hydrated: false,
  hookActive: false,
  hookError: null,
  layoutId: "tkl-ansi",
  profileId: "default",
  profiles: [],
  disabledKeys: [],
  pressedKeys: [],
  lastPressed: null,
  catLock: false,
  locale: detectLocale(),
  theme: readStoredTheme(),
  onboarded: false,
  startWithWindows: false,
  deviceName: "Generic Keyboard",
  emergencyNotice: false,
  error: null,
  persisted: true,
  persistError: null,
  persistErrorKind: null,
  configRevision: 0,

  applySnapshot: (snapshot) => set(apply(snapshot)),
  hydrate: async () => {
    const snapshot = await api.getAppState();
    const locale = parseLocale(snapshot.locale) ?? detectLocale();
    const theme = parseTheme(snapshot.theme) ?? readStoredTheme();
    set({ ...apply(snapshot), locale, theme });
    document.documentElement.lang = locale;
    applyTheme(theme);
    if (!parseLocale(snapshot.locale)) {
      void api.setLocale(locale).catch(() => {});
    }
    if (!parseTheme(snapshot.theme)) {
      void api.setTheme(theme).catch(() => {});
    }
  },

  toggleKey: async (code) => {
    if (get().catLock) return;
    const disabled = get().disabledKeys.includes(code);
    const nextEnabled = disabled;
    set({
      disabledKeys: disabled
        ? get().disabledKeys.filter((k) => k !== code)
        : [...get().disabledKeys, code],
    });
    try {
      const keys = await api.setKeyEnabled(code, nextEnabled);
      set({ disabledKeys: keys, error: null });
    } catch (err) {
      await fail(err);
    }
  },

  enableAll: async () => {
    try {
      const keys = await api.enableAllKeys();
      set({ disabledKeys: keys, catLock: false, error: null });
    } catch (err) {
      await fail(err);
    }
  },

  toggleCatLock: async () => {
    const next = !get().catLock;
    set({ catLock: next });
    try {
      const locked = await api.setCatLock(next);
      set({ catLock: locked, error: null });
    } catch (err) {
      set({ catLock: !next });
      await fail(err);
    }
  },

  chooseLayout: async (id) => {
    try {
      await api.selectLayout(id);
      set({ layoutId: id, onboarded: true, error: null });
    } catch (err) {
      await fail(err);
    }
  },

  chooseProfile: async (id) => {
    try {
      await api.selectProfile(id);
      const snapshot = await api.getAppState();
      set(apply(snapshot));
    } catch (err) {
      await fail(err);
    }
  },

  newProfile: async (name) => {
    try {
      await api.createProfile(name);
      const snapshot = await api.getAppState();
      set(apply(snapshot));
    } catch (err) {
      await fail(err);
    }
  },

  copyProfile: async () => {
    try {
      await api.duplicateProfile(get().profileId);
      const snapshot = await api.getAppState();
      set(apply(snapshot));
    } catch (err) {
      await fail(err);
    }
  },

  renameCurrent: async (name) => {
    try {
      await api.renameProfile(get().profileId, name);
      const snapshot = await api.getAppState();
      set(apply(snapshot));
    } catch (err) {
      await fail(err);
    }
  },

  removeProfile: async (id) => {
    try {
      await api.deleteProfile(id);
      const snapshot = await api.getAppState();
      set(apply(snapshot));
    } catch (err) {
      await fail(err);
    }
  },

  resetCurrent: async () => {
    try {
      await api.resetProfile(get().profileId);
      const snapshot = await api.getAppState();
      set(apply(snapshot));
    } catch (err) {
      await fail(err);
    }
  },

  markOnboarded: async (layoutId) => {
    try {
      await api.selectLayout(layoutId);
      await api.setOnboarded(true);
      const snapshot = await api.getAppState();
      set(apply(snapshot));
    } catch (err) {
      await fail(err);
    }
  },

  setAutostart: async (enabled) => {
    const prev = get().startWithWindows;
    set({ startWithWindows: enabled });
    try {
      await api.setStartWithWindows(enabled);
      set({ error: null });
    } catch (err) {
      set({ startWithWindows: prev });
      await fail(err);
    }
  },

  setLocale: async (locale) => {
    const prev = get().locale;
    set({ locale });
    document.documentElement.lang = locale;
    try {
      await api.setLocale(locale);
      set({ error: null });
    } catch (err) {
      // Rollback: the persisted locale is still `prev`.
      set({ locale: prev });
      document.documentElement.lang = prev;
      await fail(err);
    }
  },

  setTheme: async (theme) => {
    const prev = get().theme;
    set({ theme });
    applyTheme(theme);
    try {
      await api.setTheme(theme);
      set({ error: null });
    } catch (err) {
      // Rollback: the persisted theme is still `prev`.
      set({ theme: prev });
      applyTheme(prev);
      await fail(err);
    }
  },

  retryPersist: async () => {
    try {
      await api.retryPersist();
      const snapshot = await api.getAppState();
      set(apply(snapshot));
    } catch (err) {
      await fail(err);
    }
  },

  notePress: (code, down) => {
    const current = get().pressedKeys;
    if (down) {
      const pressedKeys = current.includes(code) ? current : [...current, code];
      set({ pressedKeys, lastPressed: code });
      const existing = pressTimers.get(code);
      if (existing) window.clearTimeout(existing);
      const timer = window.setTimeout(() => {
        set({ pressedKeys: get().pressedKeys.filter((k) => k !== code) });
        pressTimers.delete(code);
      }, PRESS_TIMEOUT_MS);
      pressTimers.set(code, timer);
    } else {
      const existing = pressTimers.get(code);
      if (existing) window.clearTimeout(existing);
      pressTimers.delete(code);
      set({ pressedKeys: current.filter((k) => k !== code) });
    }
  },

  clearEmergency: () => set({ emergencyNotice: false }),
  showEmergency: () =>
    set({
      emergencyNotice: true,
      disabledKeys: [],
      profileId: "default",
      catLock: false,
    }),
  };
});
