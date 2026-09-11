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

export interface AppSnapshot {
  hookActive: boolean;
  hookError: string | null;
  layoutId: string;
  profileId: string;
  profiles: Profile[];
  disabledKeys: string[];
  onboarded: boolean;
  startWithWindows: boolean;
  deviceName: string;
  emergencyShortcut: string;
  catLock: boolean;
  locale: string;
}
