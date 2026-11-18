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

// S6: golden SHA-256 of the 5 ANSI layouts. Any regeneration that
// changes a key count, coordinate, or id fails here. The hash is taken
// over JSON.stringify of the parsed layout, not the raw file bytes, so
// line endings (LF on Linux, CRLF after a Windows checkout) and
// trailing whitespace do not matter. Recompute only after a
// deliberate, reviewed ANSI change:
//   node -e 'const fs=require("fs"),c=require("node:crypto");for(const f of ["fullsize","tkl","75","65","60"])console.log(f, c.createHash("sha256").update(JSON.stringify(JSON.parse(fs.readFileSync(`src/layouts/${f}-ansi.json`,"utf8")))).digest("hex"))'
const ANSI_GOLDEN = {
  "fullsize-ansi": "468fff35965f94abc0e2f8fe2347a1725898f855b154d42887ed11a8271c382f",
  "tkl-ansi": "6a024c4cd54428857e4f3f0030fa5509a7d15ba1bbc57cea4cb903433f40c447",
  "75-ansi": "f410071496e06d5d851f165a7dbb40deb3000b533d74991b3be9f28fd2f98f84",
  "65-ansi": "1ffa027d8eb2d5534b473335e5cb9a46facec8681fb4f4397d1dbb303ccad6f3",
  "60-ansi": "f519e1559418f6929c3d96bcc56d720993f8bccf1d770961269350509bfc33b0",
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
  // S6: golden hash pins the 5 ANSI layouts. Hash the normalized
  // content (see ANSI_GOLDEN), not the raw bytes, so a Windows
  // checkout (CRLF) passes exactly like a Linux one (LF).
  if (ANSI_GOLDEN[layout.id]) {
    const hash = createHash("sha256").update(JSON.stringify(layout)).digest("hex");
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
