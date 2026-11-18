import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dir = join(root, "src", "layouts");
const known = new Set(JSON.parse(readFileSync(join(root, "src", "lib", "key-catalog.json"), "utf8")).codes);
const files = readdirSync(dir).filter((f) => f.endsWith(".json"));

// S6: 5 ANSI + fullsize ISO + fullsize JIS.
const EXPECTED_LAYOUTS = [
  "fullsize-ansi",
  "tkl-ansi",
  "75-ansi",
  "65-ansi",
  "60-ansi",
  "fullsize-iso",
  "fullsize-jis",
];

// S6: golden SHA-256 of the 5 ANSI layout files. Any regeneration that
// changes a key count, coordinate, or id fails here. Recompute only
// after a deliberate, reviewed ANSI change:
//   sha256sum src/layouts/{fullsize,tkl,75,65,60}-ansi.json
const ANSI_GOLDEN = {
  "fullsize-ansi": "05186728c57f904244af7c6f9db1422659ab3980562e81f19696a9e30907407f",
  "tkl-ansi": "dca5ca802f36ad67adb2d3ba1cd951898a6ab972d3bfdb41a47317e4d581d832",
  "75-ansi": "4e6857010a3f9dae6f2e928315aae053ea07e6a8bb6e411641be97f5bbb2f43f",
  "65-ansi": "620ff1b94aa2e20f5216127c7e3a4fa9b42c9bb31517829d1cf4b59fa50bc03b",
  "60-ansi": "aaae3ba9fe92eb0e08e9010ee6fa8e3b5d791a6f084fa50edf55e05340ad5a2b",
};

// S6: expected key counts and required extra keys per layout.
const EXPECTED_KEYS = {
  "fullsize-ansi": { count: 104, required: [] },
  "tkl-ansi": { count: 87, required: [] },
  "75-ansi": { count: 82, required: [] },
  "65-ansi": { count: 68, required: [] },
  "60-ansi": { count: 61, required: [] },
  "fullsize-iso": {
    count: 105,
    required: ["IntlBackslash"],
    // The ISO # key is scan 0x2B, reported as Backslash, label "#".
    requiredLabels: ["#"],
  },
  "fullsize-jis": {
    count: 109,
    required: ["IntlYen", "IntlRo", "Convert", "NonConvert", "KanaMode"],
  },
};

if (files.length !== EXPECTED_LAYOUTS.length) {
  throw new Error(`expected ${EXPECTED_LAYOUTS.length} layouts, found ${files.length}`);
}

for (const file of files) {
  const raw = readFileSync(join(dir, file), "utf8");
  const layout = JSON.parse(raw);
  const codes = new Set();
  if (!layout.id || !layout.name || !Array.isArray(layout.keys)) {
    throw new Error(`${file} is missing required fields`);
  }
  const expected = EXPECTED_KEYS[layout.id];
  if (!expected) {
    throw new Error(`${file}: unexpected layout id ${layout.id}`);
  }
  if (layout.keys.length !== expected.count) {
    throw new Error(`${layout.id}: expected ${expected.count} keys, found ${layout.keys.length}`);
  }
  for (const key of layout.keys) {
    if (key.w <= 0 || key.h <= 0 || key.x < 0 || key.y < 0) {
      throw new Error(`${layout.id}: invalid geometry for ${key.code}`);
    }
    if (key.x + key.w > layout.width || key.y + key.h > layout.height) {
      throw new Error(`${layout.id}: key ${key.code} exceeds layout bounds`);
    }
    if (codes.has(key.code)) {
      throw new Error(`${layout.id}: duplicate key ${key.code}`);
    }
    codes.add(key.code);
    if (!key.unsupported && !known.has(key.code)) {
      throw new Error(`${layout.id}: unknown key ${key.code}`);
    }
  }
  // Overlap check: no two keys may share area.
  for (let i = 0; i < layout.keys.length; i++) {
    for (let j = i + 1; j < layout.keys.length; j++) {
      const a = layout.keys[i];
      const b = layout.keys[j];
      if (a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h) {
        throw new Error(`${layout.id}: overlapping keys ${a.code} / ${b.code}`);
      }
    }
  }
  for (const code of expected.required) {
    if (!codes.has(code)) {
      throw new Error(`${layout.id}: missing required key ${code}`);
    }
  }
  for (const label of expected.requiredLabels ?? []) {
    if (!layout.keys.some((k) => k.label === label)) {
      throw new Error(`${layout.id}: missing required label ${label}`);
    }
  }
  // S6: golden hash pins the 5 ANSI layouts byte-for-byte.
  if (ANSI_GOLDEN[layout.id]) {
    const hash = createHash("sha256").update(raw).digest("hex");
    if (hash !== ANSI_GOLDEN[layout.id]) {
      throw new Error(
        `${layout.id}: golden hash mismatch (got ${hash}); ` +
          `an ANSI layout changed — review the diff and update ANSI_GOLDEN only deliberately`,
      );
    }
  }
  console.log(`ok ${layout.id} (${layout.keys.length} keys)`);
}

console.log("layout validation passed");
