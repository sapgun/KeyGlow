import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppSnapshot, Profile } from "../types/keyboard";

export function getAppState() {
  return invoke<AppSnapshot>("get_app_state");
}

export function setKeyEnabled(code: string, enabled: boolean) {
  return invoke<string[]>("set_key_enabled", { code, enabled });
}

export function enableAllKeys() {
  return invoke<string[]>("enable_all_keys");
}

export function setCatLock(locked: boolean) {
  return invoke<boolean>("set_cat_lock", { locked });
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

export function setOnboarded(onboarded: boolean) {
  return invoke<void>("set_onboarded", { onboarded });
}

export function setStartWithWindows(enabled: boolean) {
  return invoke<boolean>("set_start_with_windows", { enabled });
}

export function setLocale(locale: string) {
  return invoke<string>("set_locale", { locale });
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

export function onKeyDown(handler: (code: string) => void): Promise<UnlistenFn> {
  return listen<unknown>("keyboard:key-down", (event) => {
    const code = payloadCode(event.payload);
    if (code) handler(code);
  });
}

export function onKeyUp(handler: (code: string) => void): Promise<UnlistenFn> {
  return listen<unknown>("keyboard:key-up", (event) => {
    const code = payloadCode(event.payload);
    if (code) handler(code);
  });
}

export function onStateChanged(handler: (keys: string[]) => void): Promise<UnlistenFn> {
  return listen<string[]>("keyboard:state-changed", (event) => handler(event.payload));
}

export function onEmergencyUnlock(handler: () => void): Promise<UnlistenFn> {
  return listen("keyboard:emergency-unlock", () => handler());
}

export function onProfileChanged(handler: () => void): Promise<UnlistenFn> {
  return listen("profile:changed", () => handler());
}

export function onCatLock(handler: (locked: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("keyboard:cat-lock", (event) => handler(Boolean(event.payload)));
}
