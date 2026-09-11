/**
 * Generates data-driven keyboard layout JSON files.
 * Run: node scripts/generate-layouts.mjs
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "src", "layouts");
mkdirSync(outDir, { recursive: true });
mkdirSync(join(root, "src", "lib"), { recursive: true });

const META = {
  Escape: { vk: "VK_ESCAPE", vkCode: 0x1b, label: "Esc", category: "function" },
  F1: { vk: "VK_F1", vkCode: 0x70, label: "F1", category: "function" },
  F2: { vk: "VK_F2", vkCode: 0x71, label: "F2", category: "function" },
  F3: { vk: "VK_F3", vkCode: 0x72, label: "F3", category: "function" },
  F4: { vk: "VK_F4", vkCode: 0x73, label: "F4", category: "function" },
  F5: { vk: "VK_F5", vkCode: 0x74, label: "F5", category: "function" },
  F6: { vk: "VK_F6", vkCode: 0x75, label: "F6", category: "function" },
  F7: { vk: "VK_F7", vkCode: 0x76, label: "F7", category: "function" },
  F8: { vk: "VK_F8", vkCode: 0x77, label: "F8", category: "function" },
  F9: { vk: "VK_F9", vkCode: 0x78, label: "F9", category: "function" },
  F10: { vk: "VK_F10", vkCode: 0x79, label: "F10", category: "function" },
  F11: { vk: "VK_F11", vkCode: 0x7a, label: "F11", category: "function" },
  F12: { vk: "VK_F12", vkCode: 0x7b, label: "F12", category: "function" },
  PrintScreen: {
    vk: "VK_SNAPSHOT",
    vkCode: 0x2c,
    label: "PrtSc",
    subLabel: "SysRq",
    category: "system",
  },
  ScrollLock: { vk: "VK_SCROLL", vkCode: 0x91, label: "ScrLk", category: "system" },
  Pause: { vk: "VK_PAUSE", vkCode: 0x13, label: "Pause", subLabel: "Break", category: "system" },
  Backquote: { vk: "VK_OEM_3", vkCode: 0xc0, label: "`", subLabel: "~", category: "punctuation" },
  Digit1: { vk: "VK_1", vkCode: 0x31, label: "1", subLabel: "!", category: "punctuation" },
  Digit2: { vk: "VK_2", vkCode: 0x32, label: "2", subLabel: "@", category: "punctuation" },
  Digit3: { vk: "VK_3", vkCode: 0x33, label: "3", subLabel: "#", category: "punctuation" },
  Digit4: { vk: "VK_4", vkCode: 0x34, label: "4", subLabel: "$", category: "punctuation" },
  Digit5: { vk: "VK_5", vkCode: 0x35, label: "5", subLabel: "%", category: "punctuation" },
  Digit6: { vk: "VK_6", vkCode: 0x36, label: "6", subLabel: "^", category: "punctuation" },
  Digit7: { vk: "VK_7", vkCode: 0x37, label: "7", subLabel: "&", category: "punctuation" },
  Digit8: { vk: "VK_8", vkCode: 0x38, label: "8", subLabel: "*", category: "punctuation" },
  Digit9: { vk: "VK_9", vkCode: 0x39, label: "9", subLabel: "(", category: "punctuation" },
  Digit0: { vk: "VK_0", vkCode: 0x30, label: "0", subLabel: ")", category: "punctuation" },
  Minus: { vk: "VK_OEM_MINUS", vkCode: 0xbd, label: "-", subLabel: "_", category: "punctuation" },
  Equal: { vk: "VK_OEM_PLUS", vkCode: 0xbb, label: "=", subLabel: "+", category: "punctuation" },
  Backspace: { vk: "VK_BACK", vkCode: 0x08, label: "Backspace", category: "system" },
  Tab: { vk: "VK_TAB", vkCode: 0x09, label: "Tab", category: "system" },
  KeyQ: { vk: "VK_Q", vkCode: 0x51, label: "Q", category: "alpha" },
  KeyW: { vk: "VK_W", vkCode: 0x57, label: "W", category: "alpha" },
  KeyE: { vk: "VK_E", vkCode: 0x45, label: "E", category: "alpha" },
  KeyR: { vk: "VK_R", vkCode: 0x52, label: "R", category: "alpha" },
  KeyT: { vk: "VK_T", vkCode: 0x54, label: "T", category: "alpha" },
  KeyY: { vk: "VK_Y", vkCode: 0x59, label: "Y", category: "alpha" },
  KeyU: { vk: "VK_U", vkCode: 0x55, label: "U", category: "alpha" },
  KeyI: { vk: "VK_I", vkCode: 0x49, label: "I", category: "alpha" },
  KeyO: { vk: "VK_O", vkCode: 0x4f, label: "O", category: "alpha" },
  KeyP: { vk: "VK_P", vkCode: 0x50, label: "P", category: "alpha" },
  BracketLeft: { vk: "VK_OEM_4", vkCode: 0xdb, label: "[", subLabel: "{", category: "punctuation" },
  BracketRight: { vk: "VK_OEM_6", vkCode: 0xdd, label: "]", subLabel: "}", category: "punctuation" },
  Backslash: { vk: "VK_OEM_5", vkCode: 0xdc, label: "\\", subLabel: "|", category: "punctuation" },
  CapsLock: { vk: "VK_CAPITAL", vkCode: 0x14, label: "Caps", category: "modifier" },
  KeyA: { vk: "VK_A", vkCode: 0x41, label: "A", category: "alpha" },
  KeyS: { vk: "VK_S", vkCode: 0x53, label: "S", category: "alpha" },
  KeyD: { vk: "VK_D", vkCode: 0x44, label: "D", category: "alpha" },
  KeyF: { vk: "VK_F", vkCode: 0x46, label: "F", category: "alpha" },
  KeyG: { vk: "VK_G", vkCode: 0x47, label: "G", category: "alpha" },
  KeyH: { vk: "VK_H", vkCode: 0x48, label: "H", category: "alpha" },
  KeyJ: { vk: "VK_J", vkCode: 0x4a, label: "J", category: "alpha" },
  KeyK: { vk: "VK_K", vkCode: 0x4b, label: "K", category: "alpha" },
  KeyL: { vk: "VK_L", vkCode: 0x4c, label: "L", category: "alpha" },
  Semicolon: { vk: "VK_OEM_1", vkCode: 0xba, label: ";", subLabel: ":", category: "punctuation" },
  Quote: { vk: "VK_OEM_7", vkCode: 0xde, label: "'", subLabel: '"', category: "punctuation" },
  Enter: { vk: "VK_RETURN", vkCode: 0x0d, label: "Enter", category: "system" },
  ShiftLeft: { vk: "VK_LSHIFT", vkCode: 0xa0, label: "Shift", category: "modifier" },
  KeyZ: { vk: "VK_Z", vkCode: 0x5a, label: "Z", category: "alpha" },
  KeyX: { vk: "VK_X", vkCode: 0x58, label: "X", category: "alpha" },
  KeyC: { vk: "VK_C", vkCode: 0x43, label: "C", category: "alpha" },
  KeyV: { vk: "VK_V", vkCode: 0x56, label: "V", category: "alpha" },
  KeyB: { vk: "VK_B", vkCode: 0x42, label: "B", category: "alpha" },
  KeyN: { vk: "VK_N", vkCode: 0x4e, label: "N", category: "alpha" },
  KeyM: { vk: "VK_M", vkCode: 0x4d, label: "M", category: "alpha" },
  Comma: { vk: "VK_OEM_COMMA", vkCode: 0xbc, label: ",", subLabel: "<", category: "punctuation" },
  Period: { vk: "VK_OEM_PERIOD", vkCode: 0xbe, label: ".", subLabel: ">", category: "punctuation" },
  Slash: { vk: "VK_OEM_2", vkCode: 0xbf, label: "/", subLabel: "?", category: "punctuation" },
  ShiftRight: { vk: "VK_RSHIFT", vkCode: 0xa1, label: "Shift", category: "modifier" },
  ControlLeft: { vk: "VK_LCONTROL", vkCode: 0xa2, label: "Ctrl", category: "modifier" },
  MetaLeft: { vk: "VK_LWIN", vkCode: 0x5b, label: "Win", category: "modifier" },
  AltLeft: { vk: "VK_LMENU", vkCode: 0xa4, label: "Alt", category: "modifier" },
  Space: { vk: "VK_SPACE", vkCode: 0x20, label: "", category: "modifier" },
  AltRight: { vk: "VK_RMENU", vkCode: 0xa5, label: "Alt", category: "modifier" },
  MetaRight: { vk: "VK_RWIN", vkCode: 0x5c, label: "Win", category: "modifier" },
  ContextMenu: { vk: "VK_APPS", vkCode: 0x5d, label: "Menu", category: "modifier" },
  ControlRight: { vk: "VK_RCONTROL", vkCode: 0xa3, label: "Ctrl", category: "modifier" },
  Insert: { vk: "VK_INSERT", vkCode: 0x2d, label: "Ins", category: "navigation" },
  Delete: { vk: "VK_DELETE", vkCode: 0x2e, label: "Del", category: "navigation" },
  Home: { vk: "VK_HOME", vkCode: 0x24, label: "Home", category: "navigation" },
  End: { vk: "VK_END", vkCode: 0x23, label: "End", category: "navigation" },
  PageUp: { vk: "VK_PRIOR", vkCode: 0x21, label: "PgUp", category: "navigation" },
  PageDown: { vk: "VK_NEXT", vkCode: 0x22, label: "PgDn", category: "navigation" },
  ArrowLeft: { vk: "VK_LEFT", vkCode: 0x25, label: "◀", category: "navigation" },
  ArrowUp: { vk: "VK_UP", vkCode: 0x26, label: "▲", category: "navigation" },
  ArrowRight: { vk: "VK_RIGHT", vkCode: 0x27, label: "▶", category: "navigation" },
  ArrowDown: { vk: "VK_DOWN", vkCode: 0x28, label: "▼", category: "navigation" },
  NumLock: { vk: "VK_NUMLOCK", vkCode: 0x90, label: "Num", category: "numpad" },
  NumpadDivide: { vk: "VK_DIVIDE", vkCode: 0x6f, label: "/", category: "numpad" },
  NumpadMultiply: { vk: "VK_MULTIPLY", vkCode: 0x6a, label: "*", category: "numpad" },
  NumpadSubtract: { vk: "VK_SUBTRACT", vkCode: 0x6d, label: "-", category: "numpad" },
  NumpadAdd: { vk: "VK_ADD", vkCode: 0x6b, label: "+", category: "numpad" },
  NumpadEnter: { vk: "VK_RETURN", vkCode: 0x0d, label: "Enter", category: "numpad" },
  NumpadDecimal: { vk: "VK_DECIMAL", vkCode: 0x6e, label: ".", category: "numpad" },
  Numpad0: { vk: "VK_NUMPAD0", vkCode: 0x60, label: "0", category: "numpad" },
  Numpad1: { vk: "VK_NUMPAD1", vkCode: 0x61, label: "1", category: "numpad" },
  Numpad2: { vk: "VK_NUMPAD2", vkCode: 0x62, label: "2", category: "numpad" },
  Numpad3: { vk: "VK_NUMPAD3", vkCode: 0x63, label: "3", category: "numpad" },
  Numpad4: { vk: "VK_NUMPAD4", vkCode: 0x64, label: "4", category: "numpad" },
  Numpad5: { vk: "VK_NUMPAD5", vkCode: 0x65, label: "5", category: "numpad" },
  Numpad6: { vk: "VK_NUMPAD6", vkCode: 0x66, label: "6", category: "numpad" },
  Numpad7: { vk: "VK_NUMPAD7", vkCode: 0x67, label: "7", category: "numpad" },
  Numpad8: { vk: "VK_NUMPAD8", vkCode: 0x68, label: "8", category: "numpad" },
  Numpad9: { vk: "VK_NUMPAD9", vkCode: 0x69, label: "9", category: "numpad" },
};

function key(code, x, y, w = 1, h = 1, extra = {}) {
  if (code === "Fn") {
    return {
      code: "Fn",
      vk: "N/A",
      vkCode: 0,
      label: "Fn",
      x,
      y,
      w,
      h,
      category: "modifier",
      unsupported: true,
      ...extra,
    };
  }
  const meta = META[code];
  if (!meta) throw new Error(`Unknown key code: ${code}`);
  const out = {
    code,
    vk: meta.vk,
    vkCode: meta.vkCode,
    label: extra.label ?? meta.label,
    x,
    y,
    w,
    h,
    category: extra.category ?? meta.category,
  };
  const sub = extra.subLabel ?? meta.subLabel;
  if (sub) out.subLabel = sub;
  if (extra.unsupported) out.unsupported = true;
  return out;
}

function rowKeys(codes, x, y, w = 1) {
  return codes.map((code, i) => key(code, x + i * w, y, w, 1));
}

const LETTERS_Q = ["KeyQ", "KeyW", "KeyE", "KeyR", "KeyT", "KeyY", "KeyU", "KeyI", "KeyO", "KeyP"];
const LETTERS_A = ["KeyA", "KeyS", "KeyD", "KeyF", "KeyG", "KeyH", "KeyJ", "KeyK", "KeyL"];
const LETTERS_Z = ["KeyZ", "KeyX", "KeyC", "KeyV", "KeyB", "KeyN", "KeyM"];
const DIGITS = ["Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9", "Digit0"];

function ansiMain(y0, options = {}) {
  const {
    topLeft = "Backquote",
    rShift = 2.75,
    includeMenu = true,
    space = 6.25,
    bottomExtra = [],
  } = options;
  const y1 = y0;
  const y2 = y0 + 1;
  const y3 = y0 + 2;
  const y4 = y0 + 3;
  const y5 = y0 + 4;
  const keys = [];

  keys.push(key(topLeft, 0, y1));
  keys.push(...rowKeys(DIGITS, 1, y1));
  keys.push(key("Minus", 11, y1));
  keys.push(key("Equal", 12, y1));
  keys.push(key("Backspace", 13, y1, 2));

  keys.push(key("Tab", 0, y2, 1.5));
  keys.push(...rowKeys(LETTERS_Q, 1.5, y2));
  keys.push(key("BracketLeft", 11.5, y2));
  keys.push(key("BracketRight", 12.5, y2));
  keys.push(key("Backslash", 13.5, y2, 1.5));

  keys.push(key("CapsLock", 0, y3, 1.75));
  keys.push(...rowKeys(LETTERS_A, 1.75, y3));
  keys.push(key("Semicolon", 10.75, y3));
  keys.push(key("Quote", 11.75, y3));
  keys.push(key("Enter", 12.75, y3, 2.25));

  keys.push(key("ShiftLeft", 0, y4, 2.25));
  keys.push(...rowKeys(LETTERS_Z, 2.25, y4));
  keys.push(key("Comma", 9.25, y4));
  keys.push(key("Period", 10.25, y4));
  keys.push(key("Slash", 11.25, y4));
  keys.push(key("ShiftRight", 12.25, y4, rShift));

  keys.push(key("ControlLeft", 0, y5, 1.25));
  keys.push(key("MetaLeft", 1.25, y5, 1.25));
  keys.push(key("AltLeft", 2.5, y5, 1.25));
  keys.push(key("Space", 3.75, y5, space));
  let x = 3.75 + space;
  keys.push(key("AltRight", x, y5, 1.25));
  x += 1.25;
  if (includeMenu) {
    keys.push(key("MetaRight", x, y5, 1.25));
    x += 1.25;
    keys.push(key("ContextMenu", x, y5, 1.25));
    x += 1.25;
    keys.push(key("ControlRight", x, y5, 1.25));
  }
  keys.push(...bottomExtra);
  return keys;
}

function fRowStandard() {
  const keys = [key("Escape", 0, 0)];
  ["F1", "F2", "F3", "F4"].forEach((c, i) => keys.push(key(c, 2 + i, 0)));
  ["F5", "F6", "F7", "F8"].forEach((c, i) => keys.push(key(c, 6.5 + i, 0)));
  ["F9", "F10", "F11", "F12"].forEach((c, i) => keys.push(key(c, 11 + i, 0)));
  return keys;
}

function navCluster() {
  const x = 15.5;
  return [
    key("PrintScreen", x, 0),
    key("ScrollLock", x + 1, 0),
    key("Pause", x + 2, 0),
    key("Insert", x, 1.25),
    key("Home", x + 1, 1.25),
    key("PageUp", x + 2, 1.25),
    key("Delete", x, 2.25),
    key("End", x + 1, 2.25),
    key("PageDown", x + 2, 2.25),
    key("ArrowUp", x + 1, 4.25),
    key("ArrowLeft", x, 5.25),
    key("ArrowDown", x + 1, 5.25),
    key("ArrowRight", x + 2, 5.25),
  ];
}

function numpad() {
  const x = 19.0;
  const y = 1.25;
  return [
    key("NumLock", x, y),
    key("NumpadDivide", x + 1, y),
    key("NumpadMultiply", x + 2, y),
    key("NumpadSubtract", x + 3, y),
    key("Numpad7", x, y + 1),
    key("Numpad8", x + 1, y + 1),
    key("Numpad9", x + 2, y + 1),
    key("NumpadAdd", x + 3, y + 1, 1, 2),
    key("Numpad4", x, y + 2),
    key("Numpad5", x + 1, y + 2),
    key("Numpad6", x + 2, y + 2),
    key("Numpad1", x, y + 3),
    key("Numpad2", x + 1, y + 3),
    key("Numpad3", x + 2, y + 3),
    key("NumpadEnter", x + 3, y + 3, 1, 2),
    key("Numpad0", x, y + 4, 2),
    key("NumpadDecimal", x + 2, y + 4),
  ];
}

function bounds(keys) {
  let width = 0;
  let height = 0;
  for (const k of keys) {
    width = Math.max(width, k.x + k.w);
    height = Math.max(height, k.y + k.h);
  }
  return { width, height };
}

function layout(id, name, category, keys, extra = {}) {
  const { width, height } = bounds(keys);
  return {
    id,
    name,
    category,
    unitWidth: 54,
    gap: 5,
    width: Number(width.toFixed(3)),
    height: Number(height.toFixed(3)),
    keys,
    ...extra,
  };
}

const tkl = layout("tkl-ansi", "슬림 데스크톱", "tkl", [
  ...fRowStandard(),
  ...ansiMain(1.25),
  ...navCluster(),
], {
  hint: "숫자패드 없음",
  description: "숫자 키패드만 없는 일반 데스크톱 키보드",
});

const full = layout("fullsize-ansi", "데스크톱 키보드", "fullsize", [
  ...fRowStandard(),
  ...ansiMain(1.25),
  ...navCluster(),
  ...numpad(),
], {
  hint: "숫자패드 있음",
  description: "숫자 키패드가 있는 일반 PC 키보드",
});

function compact75() {
  const keys = [...fRowStandard(), key("Delete", 15.5, 0)];
  keys.push(...ansiMain(1.25, { rShift: 1.75, includeMenu: false, space: 5.25 }));
  // Replace default 15u bottom (without menu/win-right) by constructing extras.
  // ansiMain with includeMenu false only places Ctrl/Win/Alt/Space/AltRight — add remaining.
  // Filter those we will replace on the bottom row and rebuild bottom.
  const kept = keys.filter((k) => k.y !== 5.25);
  const y = 5.25;
  kept.push(
    key("ControlLeft", 0, y, 1.25),
    key("MetaLeft", 1.25, y, 1.25),
    key("AltLeft", 2.5, y, 1.25),
    key("Space", 3.75, y, 5.5),
    key("AltRight", 9.25, y, 1.25),
    key("Fn", 10.5, y, 1.25),
    key("ControlRight", 11.75, y, 1.25),
    key("ArrowLeft", 13.5, y),
    key("ArrowDown", 14.5, y),
    key("ArrowRight", 15.5, y),
  );
  // Number / Q / A / Z right column and arrows
  kept.push(key("PageUp", 15.5, 1.25));
  kept.push(key("PageDown", 15.5, 2.25));
  kept.push(key("Home", 15.5, 3.25));
  kept.push(key("End", 15.5, 4.25));
  kept.push(key("ArrowUp", 14.5, 4.25));
  return layout("75-ansi", "컴팩트 키보드", "75", kept, {
    hint: "작은 배열",
    description: "키를 가깝게 모은 작은 키보드 · 방향키와 F키 있음",
  });
}

function compact65() {
  const keys = [];
  const y0 = 0;
  keys.push(key("Escape", 0, y0));
  keys.push(...rowKeys(DIGITS, 1, y0));
  keys.push(key("Minus", 11, y0));
  keys.push(key("Equal", 12, y0));
  keys.push(key("Backspace", 13, y0, 2));
  keys.push(key("Delete", 15.5, y0));

  keys.push(key("Tab", 0, 1, 1.5));
  keys.push(...rowKeys(LETTERS_Q, 1.5, 1));
  keys.push(key("BracketLeft", 11.5, 1));
  keys.push(key("BracketRight", 12.5, 1));
  keys.push(key("Backslash", 13.5, 1, 1.5));
  keys.push(key("PageUp", 15.5, 1));

  keys.push(key("CapsLock", 0, 2, 1.75));
  keys.push(...rowKeys(LETTERS_A, 1.75, 2));
  keys.push(key("Semicolon", 10.75, 2));
  keys.push(key("Quote", 11.75, 2));
  keys.push(key("Enter", 12.75, 2, 2.25));
  keys.push(key("PageDown", 15.5, 2));

  keys.push(key("ShiftLeft", 0, 3, 2.25));
  keys.push(...rowKeys(LETTERS_Z, 2.25, 3));
  keys.push(key("Comma", 9.25, 3));
  keys.push(key("Period", 10.25, 3));
  keys.push(key("Slash", 11.25, 3));
  keys.push(key("ShiftRight", 12.25, 3, 1.75));
  keys.push(key("ArrowUp", 14.5, 3));
  keys.push(key("End", 15.5, 3));

  keys.push(key("ControlLeft", 0, 4, 1.25));
  keys.push(key("MetaLeft", 1.25, 4, 1.25));
  keys.push(key("AltLeft", 2.5, 4, 1.25));
  keys.push(key("Space", 3.75, 4, 5.5));
  keys.push(key("AltRight", 9.25, 4, 1.25));
  keys.push(key("Fn", 10.5, 4, 1.25));
  keys.push(key("ControlRight", 11.75, 4, 1.25));
  keys.push(key("ArrowLeft", 13.5, 4));
  keys.push(key("ArrowDown", 14.5, 4));
  keys.push(key("ArrowRight", 15.5, 4));

  return layout("65-ansi", "미니 키보드", "65", keys, {
    hint: "방향키 있음",
    description: "방향키는 있고 F키 줄은 없는 미니 키보드",
  });
}

function compact60() {
  const keys = [];
  keys.push(key("Escape", 0, 0));
  keys.push(...rowKeys(DIGITS, 1, 0));
  keys.push(key("Minus", 11, 0));
  keys.push(key("Equal", 12, 0));
  keys.push(key("Backspace", 13, 0, 2));

  keys.push(key("Tab", 0, 1, 1.5));
  keys.push(...rowKeys(LETTERS_Q, 1.5, 1));
  keys.push(key("BracketLeft", 11.5, 1));
  keys.push(key("BracketRight", 12.5, 1));
  keys.push(key("Backslash", 13.5, 1, 1.5));

  keys.push(key("CapsLock", 0, 2, 1.75));
  keys.push(...rowKeys(LETTERS_A, 1.75, 2));
  keys.push(key("Semicolon", 10.75, 2));
  keys.push(key("Quote", 11.75, 2));
  keys.push(key("Enter", 12.75, 2, 2.25));

  keys.push(key("ShiftLeft", 0, 3, 2.25));
  keys.push(...rowKeys(LETTERS_Z, 2.25, 3));
  keys.push(key("Comma", 9.25, 3));
  keys.push(key("Period", 10.25, 3));
  keys.push(key("Slash", 11.25, 3));
  keys.push(key("ShiftRight", 12.25, 3, 2.75));

  keys.push(key("ControlLeft", 0, 4, 1.25));
  keys.push(key("MetaLeft", 1.25, 4, 1.25));
  keys.push(key("AltLeft", 2.5, 4, 1.25));
  keys.push(key("Space", 3.75, 4, 6.25));
  keys.push(key("AltRight", 10, 4, 1.25));
  keys.push(key("Fn", 11.25, 4, 1.25));
  keys.push(key("ContextMenu", 12.5, 4, 1.25));
  keys.push(key("ControlRight", 13.75, 4, 1.25));

  return layout("60-ansi", "노트북형 키보드", "60", keys, {
    hint: "가장 작음",
    description: "노트북처럼 작은 배열 · 방향키와 F키 없음",
  });
}

const layouts = [full, tkl, compact75(), compact65(), compact60()];

for (const l of layouts) {
  const file = join(outDir, `${l.id}.json`);
  writeFileSync(file, JSON.stringify(l, null, 2) + "\n");
  console.log(`wrote ${file} (${l.keys.length} keys, ${l.width} x ${l.height})`);
}

const catalog = {
  codes: Object.keys(META),
  unsupportedVisual: ["Fn"],
};
writeFileSync(join(root, "src", "lib", "key-catalog.json"), JSON.stringify(catalog, null, 2) + "\n");
console.log("wrote key-catalog.json");
