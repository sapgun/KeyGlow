import { create } from "zustand";
import type {
  AppProfileStatus,
  AppRule,
  AppSnapshot,
  AutoSwitchedPayload,
  EmergencyPayload,
  KeyboardLayout,
  LayerDef,
  MacroDef,
  MatchReason,
  Profile,
  RemapRule,
} from "../types/keyboard";
import * as api from "../lib/tauri";
import { setCustomLayouts } from "../lib/layouts";
import { detectLocale, parseLocale, type Locale } from "../i18n";
import { applyTheme, parseTheme, readStoredTheme, type Theme } from "../lib/theme";

// HF-04: after this long without a key-up, the UI does NOT drop the key.
// It marks the key stale and re-checks the native pressed snapshot instead.
const PRESS_STALE_CHECK_MS = 8000;

// P3: authoritative ordering. Native stamps every snapshot and
// state-affecting event with (safetyEpoch, runtimeRevision). The UI applies
// data only when it is not older than what is already displayed:
//   (epoch, rev) >= (appliedEpoch, appliedRev), epoch compared first.
// The local command counter (cmdRev) cannot cover hydrate() or native
// events — they bypass it — so a late hydrate or a stale state-changed
// event could overwrite newer (e.g. post-emergency) state. This stamp
// covers command responses, hydrate snapshots, and native events uniformly.
//
// configRevision (persisted_revision) is a different number: it counts
// successful disk writes, not runtime mutations. Never compare the two.
let appliedEpoch = 0;
let appliedRev = 0;
const isFreshStamp = (epoch: number, rev: number) =>
  epoch > appliedEpoch || (epoch === appliedEpoch && rev >= appliedRev);
const markAppliedStamp = (epoch: number, rev: number) => {
  if (isFreshStamp(epoch, rev)) {
    appliedEpoch = epoch;
    appliedRev = rev;
  }
};

// P3: native event sequence per key. Key-down/up events carry the engine's
// sequence; a pressed-snapshot response carries the sequence it was taken
// at. A snapshot older than an already-processed key event must not drop
// that key from the display.
const keyPressSeq = new Map<string, number>();

interface AppStore {
  hydrated: boolean;
  hookActive: boolean;
  hookError: string | null;
  layoutId: string;
  profileId: string;
  profiles: Profile[];
  /** S6: imported custom layouts (mirrored into the layout registry). */
  customLayouts: KeyboardLayout[];
  /**
   * S6: layout waiting in the import preview. Set by the import dialog
   * after validation; applying it is a separate step, and cancelling
   * leaves the current layout untouched.
   */
  importPreview: KeyboardLayout | null;
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
  /** S5: why the effective profile is active. */
  matchReason: MatchReason;
  /** S5: automation paused after an emergency until an explicit resume. */
  autoProfilePaused: boolean;
  /** S5: the manual selection, if any. Null when automation decides. */
  manualOverride: string | null;
  /** S5: normalized exe of the current foreground app, if known. */
  activeExe: string | null;
  /** S5: persisted app rules. */
  appRules: AppRule[];
  /** S5: master switch for automatic profile switching. */
  autoProfileEnabled: boolean;
  /** S9: id of the currently active layer, if any. */
  activeLayerId: string | null;
  hydrate: () => Promise<void>;
  applySnapshot: (snapshot: AppSnapshot) => void;
  /**
   * HF-07: apply a hook liveness change from the native `hook:status-changed`
   * event. Deliberately NOT gated by the P3 ordering stamp: the hook cannot
   * be restarted in-process, so this signal is monotonic (healthy -> dead)
   * and a late event can never revive a dead hook or overwrite newer state.
   */
  setHookStatus: (hookActive: boolean, hookError: string | null) => void;
  /**
   * P3: apply a native event's partial state only when its stamp is not
   * older than what is displayed. Late/duplicate events are dropped, so
   * they can never overwrite newer (e.g. post-emergency) state.
   */
  applyNativeEvent: (partial: Partial<AppStore>, epoch: number, rev: number) => void;
  /**
   * S5: apply a foreground auto-switch event through the same ordering
   * gate. A late auto-switch can never overwrite newer (manual or
   * post-emergency) state.
   */
  applyAutoSwitched: (payload: AutoSwitchedPayload) => void;
  toggleKey: (code: string) => Promise<void>;
  enableAll: () => Promise<void>;
  chooseLayout: (id: string) => Promise<void>;
  /** S6: hold a validated layout in the import preview (no backend call). */
  beginImportPreview: (layout: KeyboardLayout) => void;
  /** S6: drop the import preview; the current layout is untouched. */
  cancelImportPreview: () => void;
  /** S6: persist the previewed layout, then select it. */
  confirmImportPreview: () => Promise<void>;
  /** S6: delete an imported custom layout (refused while in use). */
  deleteCustomLayout: (id: string) => Promise<void>;
  chooseProfile: (id: string) => Promise<void>;
  newProfile: (name: string) => Promise<void>;
  copyProfile: () => Promise<void>;
  renameCurrent: (name: string) => Promise<void>;
  removeProfile: (id: string) => Promise<void>;
  resetCurrent: () => Promise<void>;
  /** S5: create or update one app rule. */
  saveAppRule: (rule: AppRule) => Promise<void>;
  /** S5: delete one app rule. */
  removeAppRule: (id: string) => Promise<void>;
  /** S8: create or update one remap rule (1:1) for the current profile. */
  saveRemapRule: (rule: RemapRule) => Promise<void>;
  /** S8: delete one remap rule (by source key) from the current profile. */
  removeRemapRule: (from: string) => Promise<void>;
  /** S9: create or replace one macro on the current profile. */
  saveMacro: (macroDef: MacroDef) => Promise<void>;
  /** S9: delete one macro (by id) from the current profile. */
  removeMacro: (macroId: string) => Promise<void>;
  /** S9: create or replace one layer on the current profile. */
  saveLayer: (layer: LayerDef) => Promise<void>;
  /** S9: delete one layer (by id) from the current profile. */
  removeLayer: (layerId: string) => Promise<void>;
  /** S9: import a QMK keymap into the current profile (replaces its
   * base keymap and layers). */
  importQmkKeymap: (json: string, layoutId: string, keyOrder: string[]) => Promise<void>;
  /** S9: import a VIA keymap into the current profile. */
  importViaKeymap: (json: string, layoutId: string, keyOrder: string[]) => Promise<void>;
  /** S9: export the current profile as a QMK keymap.json string. */
  exportQmkKeymap: (layoutId: string, keyOrder: string[]) => Promise<string>;
  /** S9: export the current profile as a VIA JSON string. */
  exportViaKeymap: (layoutId: string, keyOrder: string[]) => Promise<string>;
  /** S5: master switch for automatic profile switching. */
  setAutoProfileEnabled: (enabled: boolean) => Promise<void>;
  /** S5: clear the manual override and the safety pause, re-evaluating
   * the current foreground app. */
  resumeAuto: () => Promise<void>;
  markOnboarded: (layoutId: string) => Promise<void>;
  setAutostart: (enabled: boolean) => Promise<void>;
  setLocale: (locale: Locale) => Promise<void>;
  setTheme: (theme: Theme) => Promise<void>;
  toggleCatLock: () => Promise<void>;
  retryPersist: () => Promise<void>;
  notePress: (code: string, down: boolean, seq?: number) => void;
  verifyHeldKey: (code: string) => Promise<void>;
  resyncPressed: () => Promise<void>;
  clearPressTracking: () => void;
  clearEmergency: () => void;
  showEmergency: (payload: EmergencyPayload) => void;
}

function apply(snapshot: AppSnapshot): Partial<AppStore> | null {
  // P3: a snapshot that predates what is displayed is stale — drop it
  // instead of overwriting newer (e.g. post-emergency) state.
  if (!isFreshStamp(snapshot.safetyEpoch, snapshot.runtimeRevision)) return null;
  markAppliedStamp(snapshot.safetyEpoch, snapshot.runtimeRevision);
  // S6: mirror the backend custom layouts into the layout registry so
  // getLayout/allLayouts see imported layouts.
  const customLayouts = snapshot.customLayouts ?? [];
  setCustomLayouts(customLayouts);
  return {
    hydrated: true,
    hookActive: snapshot.hookActive,
    hookError: snapshot.hookError,
    layoutId: snapshot.layoutId,
    profileId: snapshot.profileId,
    profiles: snapshot.profiles,
    customLayouts,
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
    matchReason: snapshot.matchReason,
    autoProfilePaused: snapshot.autoProfilePaused,
    manualOverride: snapshot.manualOverride,
    activeExe: snapshot.activeExe,
    appRules: snapshot.appRules,
    autoProfileEnabled: snapshot.autoProfileEnabled,
    activeLayerId: snapshot.activeLayerId ?? null,
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
  // P3: the recovery snapshot is stamped too — a stale one must not
  // overwrite newer state, but the error is still surfaced.
  const fail = async (err: unknown) => {
    try {
      const snapshot = await api.getAppState();
      const next = apply(snapshot);
      set({ ...(next ?? {}), error: String(err) });
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
  customLayouts: [],
  importPreview: null,
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
  matchReason: "default",
  autoProfilePaused: false,
  manualOverride: null,
  activeExe: null,
  appRules: [],
  autoProfileEnabled: true,
  activeLayerId: null,

  applySnapshot: (snapshot) => {
    const next = apply(snapshot);
    if (next) set(next);
  },
  setHookStatus: (hookActive, hookError) => {
    // Monotonic health signal (see interface docs): direct set, no stamp.
    // Never resurrects: once false it stays false until the app restarts.
    if (!hookActive) set({ hookActive: false, hookError });
  },
  applyNativeEvent: (partial, epoch, rev) => {
    if (!isFreshStamp(epoch, rev)) return;
    markAppliedStamp(epoch, rev);
    set(partial);
  },
  applyAutoSwitched: (payload) => {
    get().applyNativeEvent(
      {
        profileId: payload.profileId,
        matchReason: payload.matchReason,
        activeExe: payload.activeExe,
      },
      payload.safetyEpoch,
      payload.runtimeRevision,
    );
  },
  hydrate: async () => {
    const snapshot = await api.getAppState();
    const next = apply(snapshot);
    // P3: a hydrate requested before an emergency (or a profile switch)
    // may resolve after newer state was applied — drop it.
    if (!next) return;
    const locale = parseLocale(snapshot.locale) ?? detectLocale();
    const theme = parseTheme(snapshot.theme) ?? readStoredTheme();
    set({ ...next, locale, theme });
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
    const prevKeys = get().disabledKeys;
    const disabled = prevKeys.includes(code);
    const nextEnabled = disabled;
    set({
      disabledKeys: disabled
        ? prevKeys.filter((k) => k !== code)
        : [...prevKeys, code],
    });
    try {
      const res = await api.setKeyEnabled(code, nextEnabled);
      if (!isCurrentCommand(rev)) return;
      // P3: the response carries its native stamp. Apply it through the
      // same ordering gate as events/snapshots: a newer event or snapshot
      // may already have covered (or superseded) this command.
      get().applyNativeEvent(
        { disabledKeys: res.keys, error: null },
        res.safetyEpoch,
        res.runtimeRevision,
      );
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      // Roll back the optimistic update; fail() then re-pulls the
      // authoritative snapshot (guarded, so a stale recovery snapshot
      // cannot overwrite newer state either).
      set({ disabledKeys: prevKeys });
      await fail(err);
    }
  },

  enableAll: async () => {
    const rev = beginCommand();
    try {
      const res = await api.enableAllKeys();
      if (!isCurrentCommand(rev)) return;
      get().applyNativeEvent(
        { disabledKeys: res.keys, catLock: false, error: null },
        res.safetyEpoch,
        res.runtimeRevision,
      );
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
      const res = await api.setCatLock(next);
      if (!isCurrentCommand(rev)) return;
      get().applyNativeEvent(
        { catLock: res.locked, error: null },
        res.safetyEpoch,
        res.runtimeRevision,
      );
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

  beginImportPreview: (layout) => {
    // Pure local state: no backend call, no layout change. The dialog
    // validates before calling this.
    set({ importPreview: layout, error: null });
  },

  cancelImportPreview: () => {
    // The preview never touched layoutId, so cancelling restores
    // nothing — the original layout is still selected.
    set({ importPreview: null });
  },

  confirmImportPreview: async () => {
    const preview = get().importPreview;
    if (!preview) return;
    const rev = beginCommand();
    try {
      await api.importLayout(JSON.stringify(preview));
      if (!isCurrentCommand(rev)) return;
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
      // Select the imported layout after it is stored.
      await api.selectLayout(preview.id);
      if (!isCurrentCommand(rev)) return;
      set({ layoutId: preview.id, importPreview: null, onboarded: true, error: null });
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  deleteCustomLayout: async (id) => {
    const rev = beginCommand();
    try {
      await api.deleteCustomLayout(id);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
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
      // P3: the post-command pull is stamped; drop it if newer state
      // (event/snapshot) was applied while awaiting.
      const next = apply(snapshot);
      if (next) set(next);
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
      // P3: the post-command pull is stamped; drop it if newer state
      // (event/snapshot) was applied while awaiting.
      const next = apply(snapshot);
      if (next) set(next);
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
      // P3: the post-command pull is stamped; drop it if newer state
      // (event/snapshot) was applied while awaiting.
      const next = apply(snapshot);
      if (next) set(next);
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
      // P3: the post-command pull is stamped; drop it if newer state
      // (event/snapshot) was applied while awaiting.
      const next = apply(snapshot);
      if (next) set(next);
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
      // P3: the post-command pull is stamped; drop it if newer state
      // (event/snapshot) was applied while awaiting.
      const next = apply(snapshot);
      if (next) set(next);
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
      // P3: the post-command pull is stamped; drop it if newer state
      // (event/snapshot) was applied while awaiting.
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  saveAppRule: async (rule) => {
    const rev = beginCommand();
    try {
      await api.saveAppRule(rule);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  removeAppRule: async (id) => {
    const rev = beginCommand();
    try {
      await api.deleteAppRule(id);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  saveRemapRule: async (rule) => {
    const rev = beginCommand();
    try {
      // S9 fix: the backend needs the profile id (the earlier client
      // sent { rule } and the invoke always failed on a real backend).
      await api.saveRemapRule(get().profileId, rule);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  removeRemapRule: async (from) => {
    const rev = beginCommand();
    try {
      await api.deleteRemapRule(get().profileId, from);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  saveMacro: async (macroDef) => {
    const rev = beginCommand();
    try {
      await api.saveMacro(get().profileId, macroDef);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  removeMacro: async (macroId) => {
    const rev = beginCommand();
    try {
      await api.deleteMacro(get().profileId, macroId);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  saveLayer: async (layer) => {
    const rev = beginCommand();
    try {
      await api.saveLayer(get().profileId, layer);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  removeLayer: async (layerId) => {
    const rev = beginCommand();
    try {
      await api.deleteLayer(get().profileId, layerId);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  importQmkKeymap: async (json, layoutId, keyOrder) => {
    const rev = beginCommand();
    try {
      await api.importQmkKeymap(get().profileId, json, layoutId, keyOrder);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  importViaKeymap: async (json, layoutId, keyOrder) => {
    const rev = beginCommand();
    try {
      await api.importViaKeymap(get().profileId, json, layoutId, keyOrder);
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  exportQmkKeymap: async (layoutId, keyOrder) => {
    return api.exportQmkKeymap(get().profileId, layoutId, keyOrder);
  },

  exportViaKeymap: async (layoutId, keyOrder) => {
    return api.exportViaKeymap(get().profileId, layoutId, keyOrder);
  },

  setAutoProfileEnabled: async (enabled) => {
    const rev = beginCommand();
    const prev = get().autoProfileEnabled;
    set({ autoProfileEnabled: enabled });
    try {
      await api.setAutoProfileEnabled(enabled);
      if (!isCurrentCommand(rev)) return;
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      set({ autoProfileEnabled: prev });
      await fail(err);
    }
  },

  resumeAuto: async () => {
    const rev = beginCommand();
    try {
      const status: AppProfileStatus = await api.resumeAutoProfile();
      if (!isCurrentCommand(rev)) return;
      get().applyNativeEvent(
        {
          profileId: status.activeProfileId,
          matchReason: status.matchReason,
          manualOverride: status.manualOverride,
          autoProfilePaused: status.safetyPaused,
          activeExe: status.currentExe,
          error: null,
        },
        status.safetyEpoch,
        status.runtimeRevision,
      );
      // The resume may have switched the effective profile; pull the
      // authoritative snapshot so disabled keys follow it.
      const snapshot = await api.getAppState();
      if (!isCurrentCommand(rev)) return;
      const next = apply(snapshot);
      if (next) set(next);
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
      // P3: the post-command pull is stamped; drop it if newer state
      // (event/snapshot) was applied while awaiting.
      const next = apply(snapshot);
      if (next) set(next);
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
      // P3: the post-command pull is stamped; drop it if newer state
      // (event/snapshot) was applied while awaiting.
      const next = apply(snapshot);
      if (next) set(next);
    } catch (err) {
      if (!isCurrentCommand(rev)) return;
      await fail(err);
    }
  },

  notePress: (code, down, seq) => {
    // P3: native key events carry the engine's sequence. A stale or
    // duplicate event (seq not newer than the last one seen for this key)
    // is ignored so event reordering can never resurrect a released key.
    // Window DOM listeners pass no seq and stay unordered.
    if (seq !== undefined) {
      const last = keyPressSeq.get(code) ?? -1;
      if (seq <= last) return;
      keyPressSeq.set(code, seq);
    }
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
  // P3: the snapshot carries the sequence it was taken at. A key touched
  // by a key event NEWER than the snapshot wins over the snapshot — the
  // snapshot predates the event, so applying it blindly would drop a
  // physically held key (order inversion).
  resyncPressed: async () => {
    try {
      const snap = await api.getPressedSnapshot();
      const native = new Set(snap.pressed);
      const current = get().pressedKeys;
      const next: string[] = [];
      for (const c of current) {
        if ((keyPressSeq.get(c) ?? -1) > snap.sequence) {
          if (!next.includes(c)) next.push(c);
          continue;
        }
        if (native.has(c)) {
          if (!next.includes(c)) next.push(c);
        } else {
          disarmStaleCheck(c);
        }
      }
      for (const c of snap.pressed) {
        if (!next.includes(c)) {
          next.push(c);
          armStaleCheck(c);
        }
      }
      set({ pressedKeys: next, staleKeys: [] });
    } catch {
      // Native unreachable: keep the current display rather than guessing.
    }
  },

  clearPressTracking: () => {
    for (const code of [...pressTimers.keys()]) disarmStaleCheck(code);
    keyPressSeq.clear();
    set({ pressedKeys: [], staleKeys: [] });
  },

  clearEmergency: () => set({ emergencyNotice: false }),
  showEmergency: (payload) => {
    // HF-05: an emergency invalidates every in-flight command response, so
    // a stale toggle/disable can never undo the unlock afterwards. This
    // mirrors the native discard_stale_command at the UI layer.
    // P3: the event fires only after the epoch fully converged, so its
    // (epoch, revision) is authoritative truth. Floor the ordering stamp
    // on it: anything older (a late hydrate, a stale response) is dropped
    // instead of overwriting the safe state.
    invalidateCommands();
    markAppliedStamp(payload.epoch, payload.runtimeRevision);
    set({
      emergencyNotice: true,
      disabledKeys: [],
      profileId: "default",
      catLock: false,
    });
  },
}});
