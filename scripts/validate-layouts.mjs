import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dir = join(root, "src", "layouts");
const known = new Set(JSON.parse(readFileSync(join(root, "src", "lib", "key-catalog.json"), "utf8")).codes);
const files = readdirSync(dir).filter((f) => f.endsWith(".json"));

if (files.length !== 5) {
  throw new Error(`expected 5 layouts, found ${files.length}`);
}

for (const file of files) {
  const layout = JSON.parse(readFileSync(join(dir, file), "utf8"));
  const codes = new Set();
  if (!layout.id || !layout.name || !Array.isArray(layout.keys)) {
    throw new Error(`${file} is missing required fields`);
  }
  for (const key of layout.keys) {
    if (key.w <= 0 || key.h <= 0 || key.x < 0 || key.y < 0) {
      throw new Error(`${layout.id}: invalid geometry for ${key.code}`);
    }
    if (codes.has(key.code)) {
      throw new Error(`${layout.id}: duplicate key ${key.code}`);
    }
    codes.add(key.code);
    if (!key.unsupported && !known.has(key.code)) {
      throw new Error(`${layout.id}: unknown key ${key.code}`);
    }
  }
  console.log(`ok ${layout.id} (${layout.keys.length} keys)`);
}

console.log("layout validation passed");
