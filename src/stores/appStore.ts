import { create } from "zustand";
import type { AppSnapshot, Profile } from "../types/keyboard";
import * as api from "../lib/tauri";
import { detectLocale, parseLocale, type Locale } from "../i18n";

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
  onboarded: boolean;
  startWithWindows: boolean;
  deviceName: string;
  emergencyNotice: boolean;
  error: string | null;
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
  toggleCatLock: () => Promise<void>;
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
    error: null,
  };
}

const pressTimers = new Map<string, number>();

export const useAppStore = create<AppStore>((set, get) => ({
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
  onboarded: false,
  startWithWindows: false,
  deviceName: "Generic Keyboard",
  emergencyNotice: false,
  error: null,

  applySnapshot: (snapshot) => set(apply(snapshot)),

  hydrate: async () => {
    const snapshot = await api.getAppState();
    const locale = parseLocale(snapshot.locale) ?? detectLocale();
    set({ ...apply(snapshot), locale });
    document.documentElement.lang = locale;
    if (!parseLocale(snapshot.locale)) {
      void api.setLocale(locale);
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
      const snapshot = await api.getAppState();
      set({ ...apply(snapshot), error: String(err) });
    }
  },

  enableAll: async () => {
    const keys = await api.enableAllKeys();
    set({ disabledKeys: keys, catLock: false });
  },

  toggleCatLock: async () => {
    const next = !get().catLock;
    set({ catLock: next });
    try {
      const locked = await api.setCatLock(next);
      set({ catLock: locked });
    } catch (err) {
      set({ catLock: !next, error: String(err) });
    }
  },

  chooseLayout: async (id) => {
    await api.selectLayout(id);
    set({ layoutId: id, onboarded: true });
  },

  chooseProfile: async (id) => {
    await api.selectProfile(id);
    const snapshot = await api.getAppState();
    set(apply(snapshot));
  },

  newProfile: async (name) => {
    await api.createProfile(name);
    const snapshot = await api.getAppState();
    set(apply(snapshot));
  },

  copyProfile: async () => {
    await api.duplicateProfile(get().profileId);
    const snapshot = await api.getAppState();
    set(apply(snapshot));
  },

  renameCurrent: async (name) => {
    await api.renameProfile(get().profileId, name);
    const snapshot = await api.getAppState();
    set(apply(snapshot));
  },

  removeProfile: async (id) => {
    await api.deleteProfile(id);
    const snapshot = await api.getAppState();
    set(apply(snapshot));
  },

  resetCurrent: async () => {
    await api.resetProfile(get().profileId);
    const snapshot = await api.getAppState();
    set(apply(snapshot));
  },

  markOnboarded: async (layoutId) => {
    await api.selectLayout(layoutId);
    await api.setOnboarded(true);
    const snapshot = await api.getAppState();
    set(apply(snapshot));
  },

  setAutostart: async (enabled) => {
    await api.setStartWithWindows(enabled);
    set({ startWithWindows: enabled });
  },

  setLocale: async (locale) => {
    set({ locale });
    document.documentElement.lang = locale;
    await api.setLocale(locale);
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
}));
