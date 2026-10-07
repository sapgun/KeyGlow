export type KeyCategory =
  | "alpha"
  | "modifier"
  | "navigation"
  | "function"
  | "system"
  | "numpad"
  | "punctuation";

export type VisualKeyState = "enabled" | "pressed" | "disabled";

export interface KeyDef {
  code: string;
  vk: string;
  vkCode: number;
  label: string;
  subLabel?: string;
  x: number;
  y: number;
  w: number;
  h: number;
  category: KeyCategory;
  unsupported?: boolean;
}

export interface KeyboardLayout {
  id: string;
  name: string;
  hint?: string;
  description?: string;
  category: string;
  unitWidth: number;
  gap: number;
  width: number;
  height: number;
  keys: KeyDef[];
}

export interface Profile {
  id: string;
  name: string;
  layout: string;
  disabledKeys: string[];
  builtin: boolean;
}

export interface PressedSnapshot {
  pressed: string[];
  sequence: number;
}

export interface EmergencyShortcutConfig {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  key: string;
}

export interface AppSnapshot {  hookActive: boolean;
  hookError: string | null;
  layoutId: string;
  profileId: string;
  profiles: Profile[];
  disabledKeys: string[];
  onboarded: boolean;
  startWithWindows: boolean;
  deviceName: string;
  emergencyShortcut: string;
  emergencyShortcutConfig: EmergencyShortcutConfig;
  catLock: boolean;
  locale: string;
  theme: string;
  persisted: boolean;
  persistError: string | null;
  persistErrorKind: string | null;
  configRevision: number;
  /** Latest emergency epoch recorded by native at snapshot time (P3). */
  safetyEpoch: number;
  /** Highest emergency epoch fully converged at snapshot time (P3). */
  safetyReconciled: number;
  /**
   * Monotonic runtime-state revision (P3). Orders runtime state for the
   * UI; deliberately distinct from configRevision, which only tracks
   * successful disk writes. Never compare the two against each other.
   */
  runtimeRevision: number;
}

/**
 * Ordering envelope on state-affecting native events (P3). The UI applies
 * an event only when (safetyEpoch, runtimeRevision) is not older than what
 * it already shows.
 */
export interface StateChangedPayload {
  keys: string[];
  safetyEpoch: number;
  runtimeRevision: number;
}

export interface CatLockPayload {
  locked: boolean;
  safetyEpoch: number;
  runtimeRevision: number;
}

/** Emitted only after an emergency epoch fully converged (P3). */
export interface EmergencyPayload {
  epoch: number;
  runtimeRevision: number;
}

/** Hook thread health (HF-07). Emitted once when the thread dies on its own. */
export interface HookStatusPayload {
  hookActive: boolean;
  hookError: string | null;
}

/** Physical key event with the native event sequence (P3). */
export interface KeyPressPayload {
  code: string;
  seq: number;
}
