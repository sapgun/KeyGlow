import { create } from "zustand";
import type { AppSnapshot, Profile } from "../types/keyboard";
import * as api from "../lib/tauri";
import { detectLocale, parseLocale, type Locale } from "../i18n";
import { applyTheme, parseTheme, readStoredTheme, type Theme } from "../lib/theme";

// HF-04: after this long without a key-up, the UI does NOT drop the key.
// It marks the key stale and re-checks the native pressed snapshot instead.
const PRESS_STALE_CHECK_MS = 8000;

interface AppStore {
  hydrated: boolean;
  hookActive: boolean;
  hookError: string | null;
  layoutId: string;
  profileId: string;
  profiles: Profile[];
  disabledKeys: string[];
  pressedKeys: string[];
  staleKeys: string[];
  lastPressed: string | null;
  catLock: boolean;
  locale: Locale;
  theme: Theme;
  onboarded: boolean;
  startWithWindows: boolean;
  deviceName: string;
  emergencyNotice: boolean;
  error: string | null;
  eventError: string | null;
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
  verifyHeldKey: (code: string) => Promise<void>;
  resyncPressed: () => Promise<void>;
  clearPressTracking: () => void;
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

// HF-05: monotonically increasing revision for mutating commands. A response
// is applied only if no newer command (or emergency) started meanwhile, so a
// late/duplicate response can never overwrite fresher state. Authoritative
// snapshots (hydrate, native events) bypass this guard: they are the truth.
let cmdRev = 0;
const beginCommand = () => ++cmdRev;
const isCurrentCommand = (rev: number) => rev === cmdRev;
const invalidateCommands = () => {
  cmdRev++;
};

const pressTimers = new Map<string, number>();

function armStaleCheck(code: string) {
  disarmStaleCheck(code);
  const timer = window.setTimeout(() => {
    pressTimers.delete(code);
    void useAppStore.getState().verifyHeldKey(code);
  }, PRESS_STALE_CHECK_MS);
  pressTimers.set(code, timer);
}

function disarmStaleCheck(code: string) {
  const existing = pressTimers.get(code);
  if (existing !== undefined) {
    window.clearTimeout(existing);
    pressTimers.delete(code);
  }
}

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
  staleKeys: [],
  lastPressed: null,
  catLock: false,
  locale: detectLocale(),
  theme: readStoredTheme(),
  onboarded: false,
  startWithWindows: false,
  deviceName: "Generic Keyboard",
  emergencyNotice: false,
  error: null,
  eventError: null,
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
    const rev = beginCommand();
    const disabled = get().disabledKeys.includes(code);
    const nextEnabled = disabled;
    set({
      disabledKeys: disabled
        ? get().disabledKeys.filter((k) => k !== code)
        : [...get().disabledKeys, code],
    });
    try {
      const keys = await api.setKeyEnabled(code, nextEnabled);
      if (!isCurrentCommand(rev)) return;
      set({ disabledKeys: keys, error: null });
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  enableAll: async () => {
    const rev = beginCommand();
    try {
      const keys = await api.enableAllKeys();
      if (!isCurrentCommand(rev)) return;
      set({ disabledKeys: keys, catLock: false, error: null });
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  toggleCatLock: async () => {
    const rev = beginCommand();
    const next = !get().catLock;
    set({ catLock: next });
    try {
      const locked = await api.setCatLock(next);
      if (!isCurrentCommand(rev)) return;
      set({ catLock: locked, error: null });
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      set({ catLock: !next });
      await fail(err);
    }
  },

  chooseLayout: async (id) => {
    const rev = beginCommand();
    try {
      await api.selectLayout(id);
      if (!isCurrentCommand(rev)) return;
      set({ layoutId: id, onboarded: true, error: null });
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  chooseProfile: async (id) => {
    const rev = beginCommand();
    try {
      await api.selectProfile(id);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      set(apply(snapshot));
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  newProfile: async (name) => {
    const rev = beginCommand();
    try {
      await api.createProfile(name);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      set(apply(snapshot));
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  copyProfile: async () => {
    const rev = beginCommand();
    try {
      await api.duplicateProfile(get().profileId);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      set(apply(snapshot));
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  renameCurrent: async (name) => {
    const rev = beginCommand();
    try {
      await api.renameProfile(get().profileId, name);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      set(apply(snapshot));
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  removeProfile: async (id) => {
    const rev = beginCommand();
    try {
      await api.deleteProfile(id);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      set(apply(snapshot));
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  resetCurrent: async () => {
    const rev = beginCommand();
    try {
      await api.resetProfile(get().profileId);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      set(apply(snapshot));
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  markOnboarded: async (layoutId) => {
    const rev = beginCommand();
    try {
      await api.selectLayout(layoutId);
      await api.setOnboarded(true);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      set(apply(snapshot));
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  setAutostart: async (enabled) => {
    const rev = beginCommand();
    const prev = get().startWithWindows;
    set({ startWithWindows: enabled });
    try {
      await api.setStartWithWindows(enabled);
      if (!isCurrentCommand(rev)) return;
      set({ error: null });
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      set({ startWithWindows: prev });
      await fail(err);
    }
  },

  setLocale: async (locale) => {
    const rev = beginCommand();
    const prev = get().locale;
    set({ locale });
    document.documentElement.lang = locale;
    try {
      await api.setLocale(locale);
      if (!isCurrentCommand(rev)) return;
      set({ error: null });
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      // Rollback: the persisted locale is still `prev`.
      set({ locale: prev });
      document.documentElement.lang = prev;
      await fail(err);
    }
  },

  setTheme: async (theme) => {
    const rev = beginCommand();
    const prev = get().theme;
    set({ theme });
    applyTheme(theme);
    try {
      await api.setTheme(theme);
      if (!isCurrentCommand(rev)) return;
      set({ error: null });
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      // Rollback: the persisted theme is still `prev`.
      set({ theme: prev });
      applyTheme(prev);
      await fail(err);
    }
  },

  retryPersist: async () => {
    const rev = beginCommand();
    try {
      await api.retryPersist();
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      set(apply(snapshot));
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  notePress: (code, down) => {
    const current = get().pressedKeys;
    if (down) {
      if (current.includes(code)) {
        set({ lastPressed: code });
      } else {
        set({ pressedKeys: [...current, code], lastPressed: code });
      }
      // HF-04: the timer no longer releases the key. It only schedules a
      // native re-check (see verifyHeldKey).
      armStaleCheck(code);
    } else {
      disarmStaleCheck(code);
      if (current.includes(code)) {
        set({
          pressedKeys: current.filter((k) => k !== code),
          staleKeys: get().staleKeys.filter((k) => k !== code),
        });
      } else if (get().staleKeys.includes(code)) {
        set({ staleKeys: get().staleKeys.filter((k) => k !== code) });
      }
    }
  },

  // HF-04: a key was pressed for PRESS_STALE_CHECK_MS without a key-up.
  // Instead of dropping the glow, mark it stale and ask native what is
  // really held. Native confirmation keeps the glow (and re-arms the
  // check); otherwise the key is released. A failed query keeps the glow
  // and retries later: we never drop a physically held key on suspicion.
  verifyHeldKey: async (code) => {
    if (!get().pressedKeys.includes(code)) return;
    const staleKeys = get().staleKeys.includes(code)
      ? get().staleKeys
      : [...get().staleKeys, code];
    set({ staleKeys });
    try {
      const snap = await api.getPressedSnapshot();
      if (!get().pressedKeys.includes(code)) {
        set({ staleKeys: get().staleKeys.filter((k) => k !== code) });
        return;
      }
      if (snap.pressed.includes(code)) {
        set({ staleKeys: get().staleKeys.filter((k) => k !== code) });
        armStaleCheck(code);
      } else {
        set({
          pressedKeys: get().pressedKeys.filter((k) => k !== code),
          staleKeys: get().staleKeys.filter((k) => k !== code),
        });
      }
    } catch {
      set({ staleKeys: get().staleKeys.filter((k) => k !== code) });
      armStaleCheck(code);
    }
  },

  // HF-04: reconcile the displayed pressed keys with the native snapshot.
  // Used after window focus/visibility changes and emergency unlock, where
  // key-up events may have been missed while the UI was not watching.
  resyncPressed: async () => {
    try {
      const snap = await api.getPressedSnapshot();
      const native = new Set(snap.pressed);
      const current = get().pressedKeys;
      const next = current.filter((c) => native.has(c));
      for (const c of snap.pressed) {
        if (!next.includes(c)) next.push(c);
      }
      for (const c of next) armStaleCheck(c);
      for (const c of current) {
        if (!native.has(c)) disarmStaleCheck(c);
      }
      set({ pressedKeys: next, staleKeys: [] });
    } catch {
      // Native unreachable: keep the current display rather than guessing.
    }
  },

  clearPressTracking: () => {
    for (const code of [...pressTimers.keys()]) disarmStaleCheck(code);
    set({ pressedKeys: [], staleKeys: [] });
  },

  clearEmergency: () => set({ emergencyNotice: false }),
  showEmergency: () => {
    // HF-05: an emergency invalidates every in-flight command response, so
    // a stale toggle/disable can never undo the unlock afterwards. This
    // mirrors the native discard_stale_command at the UI layer.
    invalidateCommands();
    set({
      emergencyNotice: true,
      disabledKeys: [],
      profileId: "default",
      catLock: false,
    });
  },
}});
