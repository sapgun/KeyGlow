// S6: layout import validation (frontend mirror of
// src-tauri/src/profiles/layout_import.rs). The backend re-validates on
// import, so this is a fast pre-check for the import dialog: the user
// sees the rejection reason before any IPC call. Keep the two in sync;
// scripts/test-layout-import.mjs covers both sides with the same cases.
import { isKnownKey } from "./keycodes";
import { BUILTIN_LAYOUTS } from "./layouts";
import type { KeyboardLayout } from "../types/keyboard";

export const MAX_IMPORT_BYTES = 64 * 1024;
export const MAX_IMPORT_KEYS = 120;
export const MAX_COORD = 64;
export const MAX_KEY_SIZE = 16;

const DIMENSION_TOLERANCE = 0.01;
const LAYOUT_ID_RE = /^[a-z0-9][a-z0-9-]{0,63}$/;
const BUILTIN_IDS = new Set(BUILTIN_LAYOUTS.map((l) => l.id));

const LAYOUT_FIELDS = new Set([
  "id",
  "name",
  "hint",
  "description",
  "category",
  "unitWidth",
  "gap",
  "width",
  "height",
  "keys",
]);

const KEY_FIELDS = new Set([
  "code",
  "vk",
  "vkCode",
  "label",
  "subLabel",
  "x",
  "y",
  "w",
  "h",
  "category",
  "unsupported",
]);

export type ImportValidationResult =
  | { ok: true; layout: KeyboardLayout }
  | { ok: false; error: string };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

/** Validate an already-parsed JSON value as an imported layout. */
export function validateImportedLayout(value: unknown): ImportValidationResult {
  const fail = (error: string): ImportValidationResult => ({ ok: false, error });
  if (!isRecord(value)) return fail("layout must be a JSON object");
  for (const field of Object.keys(value)) {
    if (!LAYOUT_FIELDS.has(field)) return fail(`unsupported layout field: ${field}`);
  }
  const { id, name, hint, description, category, unitWidth, gap, width, height, keys } = value;
  if (typeof id !== "string" || !LAYOUT_ID_RE.test(id)) {
    return fail(`invalid layout id: ${JSON.stringify(id)}`);
  }
  if (BUILTIN_IDS.has(id)) return fail(`layout id ${id} collides with a builtin layout`);
  if (typeof name !== "string" || name.length === 0 || name.length > 128) {
    return fail("layout name must be 1-128 characters");
  }
  if (hint !== undefined && typeof hint !== "string") return fail("layout hint must be a string");
  if (description !== undefined && typeof description !== "string") {
    return fail("layout description must be a string");
  }
  if (typeof category !== "string" || category.length === 0 || category.length > 64) {
    return fail("layout category must be 1-64 characters");
  }
  if (!isFiniteNumber(unitWidth) || unitWidth <= 0 || unitWidth > 200) {
    return fail(`invalid unitWidth: ${String(unitWidth)}`);
  }
  if (!isFiniteNumber(gap) || gap < 0 || gap > 50) return fail(`invalid gap: ${String(gap)}`);
  if (!isFiniteNumber(width) || width <= 0 || width > MAX_COORD) {
    return fail(`invalid width: ${String(width)}`);
  }
  if (!isFiniteNumber(height) || height <= 0 || height > MAX_COORD) {
    return fail(`invalid height: ${String(height)}`);
  }
  if (!Array.isArray(keys) || keys.length === 0 || keys.length > MAX_IMPORT_KEYS) {
    return fail(`key count must be 1-${MAX_IMPORT_KEYS}, got ${Array.isArray(keys) ? keys.length : "not an array"}`);
  }
  const seen = new Set<string>();
  let maxX = 0;
  let maxY = 0;
  for (const rawKey of keys) {
    if (!isRecord(rawKey)) return fail("each key must be an object");
    for (const field of Object.keys(rawKey)) {
      if (!KEY_FIELDS.has(field)) return fail(`unsupported key field: ${field}`);
    }
    const { code, vk, vkCode, label, subLabel, x, y, w, h, category: keyCategory, unsupported } = rawKey;
    if (typeof code !== "string" || code.length === 0) return fail("key code must be a non-empty string");
    if (code === "Fn") {
      if (unsupported !== true) return fail("Fn key must be marked unsupported");
    } else {
      if (unsupported === true) return fail(`key ${code} must not be marked unsupported`);
      if (!isKnownKey(code)) return fail(`unknown key code: ${code}`);
    }
    if (typeof vk !== "string" || vk.length === 0 || vk.length > 32) {
      return fail(`invalid vk for ${code}`);
    }
    if (!isFiniteNumber(vkCode) || !Number.isInteger(vkCode) || vkCode < 0 || vkCode > 0xff) {
      return fail(`invalid vkCode for ${code}`);
    }
    // The Space key has an empty label by design (blank spacebar);
    // only the length is bounded.
    if (typeof label !== "string" || label.length > 32) {
      return fail(`invalid label for ${code}`);
    }
    if (subLabel !== undefined && (typeof subLabel !== "string" || subLabel.length > 32)) {
      return fail(`invalid subLabel for ${code}`);
    }
    if (typeof keyCategory !== "string" || keyCategory.length === 0 || keyCategory.length > 32) {
      return fail(`invalid category for ${code}`);
    }
    if (!isFiniteNumber(x) || x < 0 || x > MAX_COORD || !isFiniteNumber(y) || y < 0 || y > MAX_COORD) {
      return fail(`key ${code} out of range: ${String(x)},${String(y)}`);
    }
    if (!isFiniteNumber(w) || w <= 0 || w > MAX_KEY_SIZE || !isFiniteNumber(h) || h <= 0 || h > MAX_KEY_SIZE) {
      return fail(`key ${code} has invalid size: ${String(w)}x${String(h)}`);
    }
    if (x + w > width + DIMENSION_TOLERANCE || y + h > height + DIMENSION_TOLERANCE) {
      return fail(`key ${code} exceeds layout bounds`);
    }
    if (seen.has(code)) return fail(`duplicate key code: ${code}`);
    seen.add(code);
    maxX = Math.max(maxX, x + w);
    maxY = Math.max(maxY, y + h);
  }
  if (Math.abs(maxX - width) > DIMENSION_TOLERANCE || Math.abs(maxY - height) > DIMENSION_TOLERANCE) {
    return fail(`declared size ${width}x${height} does not match keys ${maxX}x${maxY}`);
  }
  for (let i = 0; i < keys.length; i++) {
    const a = keys[i] as Record<string, number>;
    for (let j = i + 1; j < keys.length; j++) {
      const b = keys[j] as Record<string, number>;
      if (a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h) {
        return fail(`overlapping keys: ${String(a.code)} / ${String(b.code)}`);
      }
    }
  }
  return { ok: true, layout: value as unknown as KeyboardLayout };
}

/** Parse a raw JSON string and validate it as an imported layout. */
export function parseImportedLayout(raw: string): ImportValidationResult {
  if (new TextEncoder().encode(raw).length > MAX_IMPORT_BYTES) {
    return { ok: false, error: `layout JSON too large (max ${MAX_IMPORT_BYTES} bytes)` };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (err) {
    return { ok: false, error: `invalid layout JSON: ${err instanceof Error ? err.message : String(err)}` };
  }
  return validateImportedLayout(parsed);
}
