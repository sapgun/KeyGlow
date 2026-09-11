import catalog from "./key-catalog.json";

export const KEY_CODES: readonly string[] = catalog.codes;
export const UNSUPPORTED_VISUAL: readonly string[] = catalog.unsupportedVisual;

export function isKnownKey(code: string): boolean {
  return KEY_CODES.includes(code);
}
