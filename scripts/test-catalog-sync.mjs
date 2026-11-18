// S6: Rust KeyCode enum, TS key catalog, and the generator META must
// name exactly the same key set. Run as `node scripts/test-catalog-sync.mjs`.
// The Rust side is checked by keycodes.rs::catalog_matches_ts_key_catalog
// (Windows CI); this side checks META against the generated catalog and
// the Rust source, so a drift in any of the three fails.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const catalog = JSON.parse(readFileSync(join(root, "src", "lib", "key-catalog.json"), "utf8"));
const catalogCodes = new Set(catalog.codes);

// META keys: parse the generator source (single source of truth for
// vk/vkCode/label/category). Lines look like:
//   IntlBackslash: { vk: "VK_OEM_102", vkCode: 0xe2, label: "\\", ... },
const genSource = readFileSync(join(root, "scripts", "generate-layouts.mjs"), "utf8");
const metaBlock = genSource.slice(genSource.indexOf("const META = {"), genSource.indexOf("};", genSource.indexOf("const META = {")));
const metaCodes = new Set([...metaBlock.matchAll(/^  ([A-Za-z0-9]+): \{/gm)].map((m) => m[1]));

// Rust enum variants: lines like `    IntlBackslash,` inside the enum,
// plus the COUNT constant.
const rustSource = readFileSync(join(root, "src-tauri", "src", "keyboard", "keycodes.rs"), "utf8");
const enumBlock = rustSource.slice(rustSource.indexOf("pub enum KeyCode {"), rustSource.indexOf("}", rustSource.indexOf("pub enum KeyCode {")));
const rustCodes = new Set([...enumBlock.matchAll(/^\s+([A-Z][A-Za-z0-9]+)(?:\s*=\s*\d+)?,$/gm)].map((m) => m[1]));
const countMatch = rustSource.match(/pub const COUNT: usize = (\d+);/);
const rustCount = countMatch ? Number(countMatch[1]) : null;

let failures = 0;
function check(cond, msg) {
  if (cond) {
    console.log(`ok ${msg}`);
  } else {
    failures++;
    console.error(`FAIL ${msg}`);
  }
}

function diff(a, b) {
  return [...a].filter((x) => !b.has(x));
}

check(metaCodes.size > 0, `META parsed (${metaCodes.size} codes)`);
check(catalogCodes.size > 0, `catalog parsed (${catalogCodes.size} codes)`);
check(rustCodes.size > 0, `Rust enum parsed (${rustCodes.size} codes)`);

const metaOnly = diff(metaCodes, catalogCodes);
const catalogOnly = diff(catalogCodes, metaCodes);
check(metaOnly.length === 0, `META == catalog (META-only: ${metaOnly.join(", ") || "none"})`);
check(catalogOnly.length === 0, `catalog == META (catalog-only: ${catalogOnly.join(", ") || "none"})`);

const rustOnly = diff(rustCodes, catalogCodes);
const catalogOnlyRust = diff(catalogCodes, rustCodes);
check(rustOnly.length === 0, `Rust enum == catalog (Rust-only: ${rustOnly.join(", ") || "none"})`);
check(catalogOnlyRust.length === 0, `catalog == Rust enum (catalog-only: ${catalogOnlyRust.join(", ") || "none"})`);

check(rustCount === rustCodes.size, `KeyCode::COUNT (${rustCount}) == enum variants (${rustCodes.size})`);

// S6: the six new codes must be present in all three places.
for (const code of ["IntlBackslash", "IntlYen", "IntlRo", "Convert", "NonConvert", "KanaMode"]) {
  check(
    metaCodes.has(code) && catalogCodes.has(code) && rustCodes.has(code),
    `S6 code ${code} in META + catalog + Rust`,
  );
}
// IntlHash must NOT exist as a code: scan 0x2B is reported as Backslash.
check(!catalogCodes.has("IntlHash") && !rustCodes.has("IntlHash"), "IntlHash is not a code (scan 0x2B = Backslash)");

if (failures > 0) {
  throw new Error(`catalog sync failed (${failures} checks)`);
}
console.log("catalog sync passed");
