// S6: TS import validator tests (src/lib/layout-import.ts). Same cases
// as the Rust validator tests in layout_import.rs, run against the real
// generated layout JSONs. The two validators must agree: a layout the
// dialog accepts must also pass the backend, and vice versa.
//
// Run: node scripts/test-layout-import.mjs   (node >= 22, type stripping)
// CI: wired into `npm test`.
import { register } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

register("./test-mocks/loader.mjs", import.meta.url);

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const { parseImportedLayout, validateImportedLayout, MAX_IMPORT_BYTES, MAX_IMPORT_KEYS } =
  await import("../src/lib/layout-import.ts");

let passed = 0;
function check(name, cond) {
  if (!cond) {
    console.error(`FAIL: ${name}`);
    process.exitCode = 1;
  } else {
    passed += 1;
    console.log(`ok: ${name}`);
  }
}

const isoJson = JSON.parse(readFileSync(join(root, "src", "layouts", "fullsize-iso.json"), "utf8"));
const jisJson = JSON.parse(readFileSync(join(root, "src", "layouts", "fullsize-jis.json"), "utf8"));
const ansi75Json = JSON.parse(readFileSync(join(root, "src", "layouts", "75-ansi.json"), "utf8"));

const withId = (layout, id) => ({ ...layout, id });
const mutate = (layout, fn) => {
  const copy = JSON.parse(JSON.stringify(layout));
  fn(copy);
  return copy;
};

// Valid layouts import.
{
  const r = parseImportedLayout(JSON.stringify(withId(isoJson, "my-iso")));
  check("valid ISO layout imports", r.ok && r.layout.keys.length === 105);
  const r2 = parseImportedLayout(JSON.stringify(withId(jisJson, "my-jis")));
  check("valid JIS layout imports", r2.ok && r2.layout.keys.length === 109);
  const r3 = parseImportedLayout(JSON.stringify(withId(ansi75Json, "my-75")));
  check("valid 75 layout with Fn imports", r3.ok === true);
}

// Rejections (same cases as the Rust tests).
{
  check(
    "rejects builtin id collision",
    !parseImportedLayout(JSON.stringify(withId(isoJson, "fullsize-iso"))).ok,
  );
  check(
    "rejects bad id format",
    !parseImportedLayout(JSON.stringify(withId(isoJson, "My_Layout"))).ok,
  );
  check("rejects oversize JSON", !parseImportedLayout(" ".repeat(MAX_IMPORT_BYTES + 1)).ok);
  check(
    "rejects too many keys",
    !validateImportedLayout(
      mutate(withId(isoJson, "my-iso"), (v) => {
        const extra = JSON.parse(JSON.stringify(v.keys)).slice(0, MAX_IMPORT_KEYS + 1 - v.keys.length);
        v.keys = [...v.keys, ...extra];
      }),
    ).ok,
  );
  check(
    "rejects out-of-range coordinate",
    !validateImportedLayout(mutate(withId(isoJson, "my-iso"), (v) => { v.keys[0].x = 99; })).ok,
  );
  check(
    "rejects duplicate code",
    !validateImportedLayout(
      mutate(withId(isoJson, "my-iso"), (v) => { v.keys[1] = JSON.parse(JSON.stringify(v.keys[0])); }),
    ).ok,
  );
  check(
    "rejects unknown key code",
    !validateImportedLayout(mutate(withId(isoJson, "my-iso"), (v) => { v.keys[0].code = "NotAKey"; })).ok,
  );
  check(
    "rejects unknown layout field",
    !validateImportedLayout(mutate(withId(isoJson, "my-iso"), (v) => { v.evil = true; })).ok,
  );
  check(
    "rejects unknown key field",
    !validateImportedLayout(mutate(withId(isoJson, "my-iso"), (v) => { v.keys[0].evil = true; })).ok,
  );
  check(
    "rejects overlapping keys",
    !validateImportedLayout(
      mutate(withId(isoJson, "my-iso"), (v) => {
        v.keys[1].x = v.keys[0].x;
        v.keys[1].y = v.keys[0].y;
      }),
    ).ok,
  );
  check(
    "rejects dimension mismatch",
    !validateImportedLayout(mutate(withId(isoJson, "my-iso"), (v) => { v.width = 99; })).ok,
  );
  check(
    "Fn without unsupported rejected",
    !validateImportedLayout(mutate(withId(isoJson, "my-iso"), (v) => { v.keys[0].code = "Fn"; })).ok,
  );
  check("rejects malformed JSON", !parseImportedLayout("{not json").ok);
  check("rejects empty input", !parseImportedLayout("").ok);
  check("rejects non-object JSON", !parseImportedLayout("[1,2,3]").ok);
}

console.log(`\n${passed} assertions passed`);
process.exit(process.exitCode ?? 0);
