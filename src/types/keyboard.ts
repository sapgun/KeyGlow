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
  /** 1:1 key remapping rules for this profile (S8). */
  remappings: RemapRule[];
  builtin: boolean;
}

/** One remap rule (S8): pressing `from` sends `to` instead, 1:1. */
export interface RemapRule {
  from: string;
  to: string;
}

/** One app rule (S5): foreground exe -> profile. */
export interface AppRule {
  id: string;
  executablePath: string;
  profileId: string;
  enabled: boolean;
  priority: number;
}

/** Why the effective profile is active (S5). */
export type MatchReason = "manual" | "app-rule" | "default" | "safety-paused";

/** Runtime app-profile status (S5). */
export interface AppProfileStatus {
  activeProfileId: string;
  manualOverride: string | null;
  matchReason: MatchReason;
  safetyPaused: boolean;
  autoEnabled: boolean;
  currentExe: string | null;
  revision: number;
  safetyEpoch: number;
  runtimeRevision: number;
}

export interface PressedSnapshot {
  pressed: string[];
  sequence: number;
}

export interface AppSnapshot {  hookActive: boolean;
  hookError: string | null;
  layoutId: string;
  /** Effective profile (S5): what actually filters keys now. */
  profileId: string;
  profiles: Profile[];
  /** Imported custom layouts (S6). Builtin layouts come from the frontend registry. */
  customLayouts: KeyboardLayout[];
  disabledKeys: string[];
  onboarded: boolean;
  startWithWindows: boolean;
  deviceName: string;
  emergencyShortcut: string;
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
  /** Why the effective profile is active (S5). */
  matchReason: MatchReason;
  /** Automation paused after an emergency until an explicit resume (S5). */
  autoProfilePaused: boolean;
  /** The manual selection, if any (S5). Null when automation decides. */
  manualOverride: string | null;
  /** Normalized exe of the current foreground app, if known (S5). */
  activeExe: string | null;
  /** Persisted app rules (S5). */
  appRules: AppRule[];
  /** Master switch for automatic profile switching (S5). */
  autoProfileEnabled: boolean;
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

/** Foreground observer switched the effective profile (S5). */
export interface AutoSwitchedPayload {
  profileId: string;
  matchReason: MatchReason;
  activeExe: string | null;
  safetyEpoch: number;
  runtimeRevision: number;
}
