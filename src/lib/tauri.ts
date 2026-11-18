import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppProfileStatus,
  AppRule,
  AppSnapshot,
  AutoSwitchedPayload,
  CatLockPayload,
  EmergencyPayload,
  HookStatusPayload,
  PressedSnapshot,
  Profile,
  StateChangedPayload,
} from "../types/keyboard";

export function getAppState() {
  return invoke<AppSnapshot>("get_app_state");
}

export function getPressedSnapshot() {
  return invoke<PressedSnapshot>("get_pressed_snapshot");
}

export function setKeyEnabled(code: string, enabled: boolean) {
  // P3: the response carries the native ordering stamp (keys + stamp),
  // applied through the same gate as events/snapshots.
  return invoke<StateChangedPayload>("set_key_enabled", { code, enabled });
}

export function enableAllKeys() {
  return invoke<StateChangedPayload>("enable_all_keys");
}

export function setCatLock(locked: boolean) {
  return invoke<CatLockPayload>("set_cat_lock", { locked });
}

export function selectLayout(id: string) {
  return invoke<void>("select_layout", { id });
}

export function selectProfile(id: string) {
  return invoke<Profile>("select_profile", { id });
}

export function createProfile(name: string) {
  return invoke<Profile>("create_profile", { name });
}

export function duplicateProfile(id: string) {
  return invoke<Profile>("duplicate_profile", { id });
}

export function renameProfile(id: string, name: string) {
  return invoke<Profile>("rename_profile", { id, name });
}

export function deleteProfile(id: string) {
  return invoke<void>("delete_profile", { id });
}

export function resetProfile(id: string) {
  return invoke<Profile>("reset_profile", { id });
}

// S5: app-profile rules and automation control.
export function getAppRules() {
  return invoke<AppRule[]>("get_app_rules");
}

export function getAppProfileStatus() {
  return invoke<AppProfileStatus>("get_app_profile_status");
}

export function saveAppRule(rule: AppRule) {
  return invoke<AppRule>("save_app_rule", { rule });
}

export function deleteAppRule(id: string) {
  return invoke<void>("delete_app_rule", { id });
}

export function setAutoProfileEnabled(enabled: boolean) {
  return invoke<boolean>("set_auto_profile_enabled", { enabled });
}

export function resumeAutoProfile() {
  return invoke<AppProfileStatus>("resume_auto_profile");
}

export function retryPersist() {
  return invoke<void>("retry_persist", {});
}

export function setOnboarded(onboarded: boolean) {
  return invoke<void>("set_onboarded", { onboarded });
}

export function setStartWithWindows(enabled: boolean) {
  return invoke<boolean>("set_start_with_windows", { enabled });
}

export function setLocale(locale: string) {
  return invoke<string>("set_locale", { locale });
}

export function setTheme(theme: string) {
  return invoke<string>("set_theme", { theme });
}

function payloadCode(payload: unknown): string | null {
  if (typeof payload === "string" && payload.length > 0) {
    return payload;
  }
  if (payload && typeof payload === "object" && "code" in payload) {
    const code = (payload as { code: unknown }).code;
    if (typeof code === "string" && code.length > 0) {
      return code;
    }
  }
  return null;
}

/** Extract {code, seq} from a key event payload (P3). Legacy bare-string
 * payloads yield an undefined seq, which the store treats as unordered. */
function payloadPress(payload: unknown): { code: string; seq?: number } | null {
  const code = payloadCode(payload);
  if (!code) return null;
  if (payload && typeof payload === "object" && "seq" in payload) {
    const seq = (payload as { seq: unknown }).seq;
    if (typeof seq === "number" && Number.isFinite(seq)) {
      return { code, seq };
    }
  }
  return { code };
}

function payloadStateChanged(payload: unknown): StateChangedPayload | null {
  if (payload && typeof payload === "object" && "keys" in payload) {
    const p = payload as Partial<StateChangedPayload>;
    if (
      Array.isArray(p.keys) &&
      typeof p.safetyEpoch === "number" &&
      typeof p.runtimeRevision === "number"
    ) {
      return { keys: p.keys, safetyEpoch: p.safetyEpoch, runtimeRevision: p.runtimeRevision };
    }
  }
  return null;
}

function payloadCatLock(payload: unknown): CatLockPayload | null {
  if (payload && typeof payload === "object" && "locked" in payload) {
    const p = payload as Partial<CatLockPayload>;
    if (
      typeof p.locked === "boolean" &&
      typeof p.safetyEpoch === "number" &&
      typeof p.runtimeRevision === "number"
    ) {
      return { locked: p.locked, safetyEpoch: p.safetyEpoch, runtimeRevision: p.runtimeRevision };
    }
  }
  return null;
}

function payloadEmergency(payload: unknown): EmergencyPayload | null {
  if (payload && typeof payload === "object" && "epoch" in payload) {
    const p = payload as Partial<EmergencyPayload>;
    if (typeof p.epoch === "number" && typeof p.runtimeRevision === "number") {
      return { epoch: p.epoch, runtimeRevision: p.runtimeRevision };
    }
  }
  return null;
}

export function onKeyDown(handler: (code: string, seq?: number) => void): Promise<UnlistenFn> {
  return listen<unknown>("keyboard:key-down", (event) => {
    const press = payloadPress(event.payload);
    if (press) handler(press.code, press.seq);
  });
}

export function onKeyUp(handler: (code: string, seq?: number) => void): Promise<UnlistenFn> {
  return listen<unknown>("keyboard:key-up", (event) => {
    const press = payloadPress(event.payload);
    if (press) handler(press.code, press.seq);
  });
}

export function onStateChanged(
  handler: (payload: StateChangedPayload) => void,
): Promise<UnlistenFn> {
  return listen<unknown>("keyboard:state-changed", (event) => {
    const payload = payloadStateChanged(event.payload);
    if (payload) handler(payload);
  });
}

export function onEmergencyUnlock(handler: (payload: EmergencyPayload) => void): Promise<UnlistenFn> {
  return listen<unknown>("keyboard:emergency-unlock", (event) => {
    const payload = payloadEmergency(event.payload);
    // The event always carries the converged epoch; a missing payload
    // means a protocol mismatch, which must not silently unlock.
    if (payload) handler(payload);
  });
}

export function onProfileChanged(handler: () => void): Promise<UnlistenFn> {
  return listen("profile:changed", () => handler());
}

export function onCatLock(handler: (payload: CatLockPayload) => void): Promise<UnlistenFn> {
  return listen<unknown>("keyboard:cat-lock", (event) => {
    const payload = payloadCatLock(event.payload);
    if (payload) handler(payload);
  });
}

function payloadHookStatus(payload: unknown): HookStatusPayload | null {
  if (payload && typeof payload === "object" && "hookActive" in payload) {
    const p = payload as Partial<HookStatusPayload>;
    if (
      typeof p.hookActive === "boolean" &&
      (typeof p.hookError === "string" || p.hookError === null)
    ) {
      return { hookActive: p.hookActive, hookError: p.hookError ?? null };
    }
  }
  return null;
}

function payloadAutoSwitched(payload: unknown): AutoSwitchedPayload | null {
  if (payload && typeof payload === "object" && "profileId" in payload) {
    const p = payload as Partial<AutoSwitchedPayload>;
    if (
      typeof p.profileId === "string" &&
      (p.matchReason === "manual" ||
        p.matchReason === "app-rule" ||
        p.matchReason === "default" ||
        p.matchReason === "safety-paused") &&
      (typeof p.activeExe === "string" || p.activeExe === null) &&
      typeof p.safetyEpoch === "number" &&
      typeof p.runtimeRevision === "number"
    ) {
      return {
        profileId: p.profileId,
        matchReason: p.matchReason,
        activeExe: p.activeExe ?? null,
        safetyEpoch: p.safetyEpoch,
        runtimeRevision: p.runtimeRevision,
      };
    }
  }
  return null;
}

export function onHookStatusChanged(
  handler: (payload: HookStatusPayload) => void,
): Promise<UnlistenFn> {
  return listen<unknown>("hook:status-changed", (event) => {
    const payload = payloadHookStatus(event.payload);
    // The backend only emits this when the hook thread died on its own;
    // a missing payload means a protocol mismatch and is dropped.
    if (payload) handler(payload);
  });
}

export function onAutoSwitched(
  handler: (payload: AutoSwitchedPayload) => void,
): Promise<UnlistenFn> {
  return listen<unknown>("profile:auto-switched", (event) => {
    const payload = payloadAutoSwitched(event.payload);
    // The backend only emits this when the observer switched the
    // effective profile; a malformed payload is dropped.
    if (payload) handler(payload);
  });
}
