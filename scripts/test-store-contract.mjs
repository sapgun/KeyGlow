// P3 contract tests: the REAL src/stores/appStore.ts and src/lib/tauri.ts
// run against a programmable fake IPC layer (scripts/test-mocks/).
//
// These are not replicas of a counter function: they drive the actual store
// actions, the actual IPC payload parsing, and the actual ordering gates,
// with native timing simulated (deferred promises, late/duplicate events).
//
// Run: node scripts/test-store-contract.mjs   (node >= 22, type stripping)
// CI: wired into `npm test`.
import { register } from "node:module";

register("./test-mocks/loader.mjs", import.meta.url);

// Minimal browser globals the store touches at import/call time.
globalThis.window = globalThis;
globalThis.document = { documentElement: { lang: "", dataset: {} } };
const lsStore = new Map();
globalThis.localStorage = {
  getItem: (k) => (lsStore.has(k) ? lsStore.get(k) : null),
  setItem: (k, v) => lsStore.set(k, String(v)),
  removeItem: (k) => lsStore.delete(k),
};

const core = await import("./test-mocks/mock-tauri-core.mjs");
const events = await import("./test-mocks/mock-tauri-event.mjs");
const { __mockInvoke, __resetInvoke } = core;
const { __fire, __resetEvents } = events;

let caseNo = 0;
/** Fresh store module instance per test (module-level ordering state). */
async function freshStore() {
  caseNo += 1;
  const mod = await import(`../src/stores/appStore.ts?case=${caseNo}`);
  return mod.useAppStore;
}

function deferred() {
  let resolve, reject;
  const promise = new Promise((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Full AppSnapshot-shaped object with the P3 ordering fields. */
const snap = (over = {}) => ({
  hookActive: true,
  hookError: null,
  layoutId: "tkl-ansi",
  profileId: "default",
  profiles: [],
  customLayouts: [],
  disabledKeys: [],
  onboarded: true,
  startWithWindows: false,
  deviceName: "Generic Keyboard",
  emergencyShortcut: "Ctrl+Shift+F12",
  catLock: false,
  locale: "en",
  theme: "dark",
  persisted: true,
  persistError: null,
  persistErrorKind: null,
  configRevision: 1,
  safetyEpoch: 0,
  safetyReconciled: 0,
  runtimeRevision: 1,
  matchReason: "default",
  autoProfilePaused: false,
  manualOverride: null,
  activeExe: null,
  appRules: [],
  autoProfileEnabled: true,
  ...over,
});

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
const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// ---------------------------------------------------------------- T1
// A hydrate requested BEFORE an emergency must not overwrite the safe
// state when its response lands AFTER the emergency.
{
  const useAppStore = await freshStore();
  __resetInvoke();
  const d = deferred();
  __mockInvoke("get_app_state", () => d.promise);
  const hp = useAppStore.getState().hydrate(); // requested pre-emergency
  useAppStore.getState().showEmergency({ epoch: 6, runtimeRevision: 101 });
  d.resolve(snap({ safetyEpoch: 5, safetyReconciled: 5, runtimeRevision: 100, disabledKeys: ["KeyA"] }));
  await hp;
  const s = useAppStore.getState();
  check("T1 late hydrate after emergency does not restore disabled keys", eq(s.disabledKeys, []));
  check("T1 late hydrate does not clear the emergency floor", s.profileId === "default");
}

// ---------------------------------------------------------------- T2
// A state-changed event from an older revision (e.g. the previous profile)
// arriving after a newer one is dropped; a newer one applies.
{
  const useAppStore = await freshStore();
  const st = useAppStore.getState();
  st.applyNativeEvent({ disabledKeys: ["KeyA"] }, 5, 100);
  check("T2 newer event applies", eq(useAppStore.getState().disabledKeys, ["KeyA"]));
  st.applyNativeEvent({ disabledKeys: ["KeyB"] }, 5, 90); // stale
  check("T2 stale event (older rev) dropped", eq(useAppStore.getState().disabledKeys, ["KeyA"]));
  st.applyNativeEvent({ disabledKeys: [] }, 5, 101);
  check("T2 newest event applies", eq(useAppStore.getState().disabledKeys, []));
}

// ---------------------------------------------------------------- T3
// Rapid toggle invalidated by an emergency: the late command response must
// not undo the unlock, and no recovery fetch may resurrect it.
{
  const useAppStore = await freshStore();
  __resetInvoke();
  let getStateCalls = 0;
  const d = deferred();
  __mockInvoke("set_key_enabled", () => d.promise);
  __mockInvoke("get_app_state", () => {
    getStateCalls += 1;
    return Promise.resolve(snap());
  });
  const p = useAppStore.getState().toggleKey("KeyA"); // optimistic: [KeyA]
  check("T3 optimistic update applied", eq(useAppStore.getState().disabledKeys, ["KeyA"]));
  useAppStore.getState().showEmergency({ epoch: 6, runtimeRevision: 101 });
  d.resolve({ keys: ["KeyA"], safetyEpoch: 5, runtimeRevision: 100 });
  await p;
  check("T3 stale toggle response does not undo the unlock", eq(useAppStore.getState().disabledKeys, []));
  check("T3 no recovery fetch after invalidated command", getStateCalls === 0);
}

// ---------------------------------------------------------------- T4
// A failed command rolls back its optimistic update; the stale recovery
// snapshot must not overwrite newer state, but the error is still shown.
{
  const useAppStore = await freshStore();
  __resetInvoke();
  const st = useAppStore.getState();
  st.applyNativeEvent({ disabledKeys: ["KeyB"] }, 5, 103); // newer truth
  __mockInvoke("set_key_enabled", () => Promise.reject(new Error("boom")));
  __mockInvoke("get_app_state", () =>
    Promise.resolve(snap({ safetyEpoch: 5, runtimeRevision: 100, disabledKeys: ["KeyA"] })),
  );
  await useAppStore.getState().toggleKey("KeyC");
  const s = useAppStore.getState();
  check("T4 optimistic update rolled back on failure", eq(s.disabledKeys, ["KeyB"]));
  check("T4 stale recovery snapshot did not overwrite newer state", eq(s.disabledKeys, ["KeyB"]));
  check("T4 error still surfaced", typeof s.error === "string" && s.error.includes("boom"));
}

// ---------------------------------------------------------------- T5
// A command response stamped older than the displayed state is dropped
// (e.g. a newer profile switch already applied).
{
  const useAppStore = await freshStore();
  __resetInvoke();
  const st = useAppStore.getState();
  st.applyNativeEvent({ disabledKeys: ["KeyA"] }, 5, 102);
  __mockInvoke("set_key_enabled", () =>
    Promise.resolve({ keys: [], safetyEpoch: 5, runtimeRevision: 100 }),
  );
  await useAppStore.getState().toggleKey("KeyB");
  check(
    "T5 stale command response dropped; optimistic state stands",
    eq(useAppStore.getState().disabledKeys, ["KeyA", "KeyB"]),
  );
}

// ---------------------------------------------------------------- T6
// Pressed-snapshot order inversion: a snapshot taken BEFORE a processed
// key-down must not drop that key; a newer snapshot governs.
{
  const useAppStore = await freshStore();
  __resetInvoke();
  const st = useAppStore.getState();
  st.notePress("KeyB", true, 105);
  check("T6 key-down tracked", useAppStore.getState().pressedKeys.includes("KeyB"));
  __mockInvoke("get_pressed_snapshot", () => Promise.resolve({ pressed: ["KeyA"], sequence: 100 }));
  await st.resyncPressed();
  const afterOld = useAppStore.getState().pressedKeys;
  check(
    "T6 older snapshot does not drop the newer key-down",
    afterOld.includes("KeyA") && afterOld.includes("KeyB"),
  );
  __mockInvoke("get_pressed_snapshot", () => Promise.resolve({ pressed: ["KeyA"], sequence: 110 }));
  await useAppStore.getState().resyncPressed();
  check(
    "T6 newer snapshot drops the released key",
    eq(useAppStore.getState().pressedKeys, ["KeyA"]),
  );
  // Stale / duplicate key events are ignored.
  const s2 = useAppStore.getState();
  s2.notePress("KeyC", true, 120);
  s2.notePress("KeyC", false, 125);
  s2.notePress("KeyC", true, 122); // stale: older than the key-up
  check("T6 stale key-down ignored", !useAppStore.getState().pressedKeys.includes("KeyC"));
}

// ---------------------------------------------------------------- T7
// Epoch dominates revision: an older epoch never wins, even with a huge rev.
{
  const useAppStore = await freshStore();
  const st = useAppStore.getState();
  st.showEmergency({ epoch: 6, runtimeRevision: 101 });
  st.applyNativeEvent({ disabledKeys: ["KeyX"] }, 5, 9999);
  check("T7 older epoch dropped despite higher rev", eq(useAppStore.getState().disabledKeys, []));
  st.applyNativeEvent({ disabledKeys: ["KeyY"] }, 6, 101); // same stamp: applies
  check("T7 same stamp applies", eq(useAppStore.getState().disabledKeys, ["KeyY"]));
}

// ---------------------------------------------------------------- T8
// IPC payload parsing in the real tauri.ts: sequenced key events, legacy
// bare-string payloads, and strict validation of stamped payloads. A
// malformed emergency payload must never trigger the unlock path.
{
  const tauri = await import(`../src/lib/tauri.ts?case=payload`);
  __resetEvents();
  let got = "unset";
  await tauri.onKeyDown((code, seq) => {
    got = [code, seq];
  });
  __fire("keyboard:key-down", { code: "KeyA", seq: 42 });
  check("T8 key-down carries seq", eq(got, ["KeyA", 42]));
  __fire("keyboard:key-down", "KeyB"); // legacy native without seq
  check("T8 legacy string payload still works", eq(got, ["KeyB", undefined]));
  got = "unset";
  __fire("keyboard:key-down", { nope: 1 });
  check("T8 malformed key payload ignored", got === "unset");

  let sc = "unset";
  await tauri.onStateChanged((p) => {
    sc = p;
  });
  __fire("keyboard:state-changed", { keys: ["KeyA"], safetyEpoch: 5, runtimeRevision: 100 });
  check("T8 state-changed parsed", eq(sc, { keys: ["KeyA"], safetyEpoch: 5, runtimeRevision: 100 }));
  sc = "unset";
  __fire("keyboard:state-changed", { keys: ["KeyA"] }); // missing stamp
  check("T8 unstamped state-changed ignored", sc === "unset");
  __fire("keyboard:state-changed", ["KeyA"]); // legacy shape
  check("T8 legacy array state-changed ignored", sc === "unset");

  let em = "unset";
  await tauri.onEmergencyUnlock((p) => {
    em = p;
  });
  __fire("keyboard:emergency-unlock", { epoch: 7, runtimeRevision: 200 });
  check("T8 emergency payload parsed", eq(em, { epoch: 7, runtimeRevision: 200 }));
  em = "unset";
  __fire("keyboard:emergency-unlock", {});
  check("T8 malformed emergency payload never unlocks", em === "unset");

  let cl = "unset";
  await tauri.onCatLock((p) => {
    cl = p;
  });
  __fire("keyboard:cat-lock", { locked: true, safetyEpoch: 5, runtimeRevision: 100 });
  check("T8 cat-lock parsed", eq(cl, { locked: true, safetyEpoch: 5, runtimeRevision: 100 }));
  cl = "unset";
  __fire("keyboard:cat-lock", true); // legacy boolean
  check("T8 legacy boolean cat-lock ignored", cl === "unset");

  let as = "unset";
  await tauri.onAutoSwitched((p) => {
    as = p;
  });
  __fire("profile:auto-switched", {
    profileId: "gaming",
    matchReason: "app-rule",
    activeExe: "c:\\games\\game.exe",
    safetyEpoch: 5,
    runtimeRevision: 100,
  });
  check(
    "T8 auto-switched parsed",
    eq(as, {
      profileId: "gaming",
      matchReason: "app-rule",
      activeExe: "c:\\games\\game.exe",
      safetyEpoch: 5,
      runtimeRevision: 100,
    }),
  );
  as = "unset";
  __fire("profile:auto-switched", { profileId: "gaming", matchReason: "bogus" });
  check("T8 malformed auto-switched ignored", as === "unset");
  __fire("profile:auto-switched", { profileId: "gaming" }); // missing stamp
  check("T8 unstamped auto-switched ignored", as === "unset");
}

// ---------------------------------------------------------------- T9
// S5: the snapshot carries the app-profile status, and the auto-switch
// event applies through the same P3 ordering gate as every other native
// event — a late switch can never overwrite newer (manual or
// post-emergency) state.
{
  const useAppStore = await freshStore();
  __resetInvoke();
  const rule = {
    id: "r1",
    executablePath: "C:\\Games\\game.exe",
    profileId: "gaming",
    enabled: true,
    priority: 0,
  };
  __mockInvoke("get_app_state", () =>
    snap({
      profileId: "gaming",
      matchReason: "app-rule",
      autoProfilePaused: false,
      manualOverride: null,
      activeExe: "c:\\games\\game.exe",
      appRules: [rule],
      autoProfileEnabled: true,
      runtimeRevision: 10,
    }),
  );
  await useAppStore.getState().hydrate();
  const s = useAppStore.getState();
  check("T9 snapshot carries match reason", s.matchReason === "app-rule");
  check("T9 snapshot carries active exe", s.activeExe === "c:\\games\\game.exe");
  check("T9 snapshot carries rules", eq(s.appRules, [rule]));
  check("T9 snapshot carries auto enabled", s.autoProfileEnabled === true);
  check("T9 snapshot effective profile", s.profileId === "gaming");
}

// T9b/c: auto-switch event gating.
{
  const useAppStore = await freshStore();
  const st = useAppStore.getState();
  st.applyAutoSwitched({
    profileId: "gaming",
    matchReason: "app-rule",
    activeExe: "c:\\games\\game.exe",
    safetyEpoch: 0,
    runtimeRevision: 5,
  });
  check("T9 fresh auto-switch applies", useAppStore.getState().profileId === "gaming");
  check("T9 auto-switch reason applies", useAppStore.getState().matchReason === "app-rule");
  check("T9 auto-switch exe applies", useAppStore.getState().activeExe === "c:\\games\\game.exe");
  // A stale switch (older stamp) is dropped.
  st.applyAutoSwitched({
    profileId: "coding",
    matchReason: "app-rule",
    activeExe: "c:\\windows\\notepad.exe",
    safetyEpoch: 0,
    runtimeRevision: 3,
  });
  check("T9 stale auto-switch dropped", useAppStore.getState().profileId === "gaming");
  // An emergency floors the stamp: anything older is dropped.
  st.showEmergency({ epoch: 6, runtimeRevision: 101 });
  st.applyAutoSwitched({
    profileId: "gaming",
    matchReason: "app-rule",
    activeExe: "c:\\games\\game.exe",
    safetyEpoch: 0,
    runtimeRevision: 200,
  });
  check("T9 auto-switch after emergency dropped", useAppStore.getState().profileId === "default");
}

// T9d: rule CRUD store actions drive the real invoke layer and refresh.
{
  const useAppStore = await freshStore();
  __resetInvoke();
  const rule = {
    id: "r1",
    executablePath: "C:\\Games\\game.exe",
    profileId: "gaming",
    enabled: true,
    priority: 0,
  };
  let saved = null;
  __mockInvoke("save_app_rule", (args) => {
    saved = args.rule;
    return { ...args.rule, id: "r1" };
  });
  __mockInvoke("get_app_state", () => snap({ appRules: [rule], runtimeRevision: 7 }));
  await useAppStore.getState().saveAppRule(rule);
  check("T9 save rule invokes backend", eq(saved, rule));
  check("T9 save rule refreshes snapshot", eq(useAppStore.getState().appRules, [rule]));
  __mockInvoke("get_app_state", () => snap({ appRules: [], runtimeRevision: 8 }));
  __mockInvoke("delete_app_rule", () => undefined);
  await useAppStore.getState().removeAppRule("r1");
  check("T9 delete rule refreshes snapshot", eq(useAppStore.getState().appRules, []));
}

// ---------------------------------------------------------------- T10
// S6: import preview. The preview is pure local state: beginning it
// changes nothing about the selected layout, and cancelling leaves the
// original layout selected. Confirming persists through import_layout,
// refreshes the snapshot (custom layouts land in the store), and then
// selects the imported layout.
{
  const useAppStore = await freshStore();
  __resetInvoke();
  const preview = {
    id: "my-mini",
    name: "My Mini",
    category: "custom",
    unitWidth: 54,
    gap: 5,
    width: 2,
    height: 1,
    keys: [
      { code: "KeyA", vk: "VK_A", vkCode: 0x41, label: "A", x: 0, y: 0, w: 1, h: 1, category: "alpha" },
      { code: "KeyB", vk: "VK_B", vkCode: 0x42, label: "B", x: 1, y: 0, w: 1, h: 1, category: "alpha" },
    ],
  };
  const st = useAppStore.getState();
  st.beginImportPreview(preview);
  check("T10 preview held without changing layout", useAppStore.getState().importPreview === preview);
  check("T10 preview does not change selected layout", useAppStore.getState().layoutId === "tkl-ansi");
  st.cancelImportPreview();
  check("T10 cancel clears preview", useAppStore.getState().importPreview === null);
  check("T10 cancel keeps original layout", useAppStore.getState().layoutId === "tkl-ansi");

  // Confirm: import, refresh, select.
  let importedJson = null;
  let selectedId = null;
  __mockInvoke("import_layout", (args) => {
    importedJson = args.json;
    return JSON.parse(args.json);
  });
  __mockInvoke("select_layout", (args) => {
    selectedId = args.id;
    return undefined;
  });
  __mockInvoke("get_app_state", () =>
    snap({ customLayouts: [preview], layoutId: "tkl-ansi", runtimeRevision: 10 }),
  );
  st.beginImportPreview(preview);
  await useAppStore.getState().confirmImportPreview();
  const s = useAppStore.getState();
  check("T10 confirm sends preview JSON to backend", JSON.parse(importedJson).id === "my-mini");
  check("T10 confirm refreshes custom layouts", eq(s.customLayouts, [preview]));
  check("T10 confirm selects imported layout", s.layoutId === "my-mini");
  check("T10 confirm selects via backend", selectedId === "my-mini");
  check("T10 confirm clears preview", s.importPreview === null);

  // Hydrate carries custom layouts into the store (and the registry).
  __mockInvoke("get_app_state", () =>
    snap({ customLayouts: [preview], layoutId: "my-mini", runtimeRevision: 11 }),
  );
  await useAppStore.getState().hydrate();
  check("T10 hydrate carries custom layouts", eq(useAppStore.getState().customLayouts, [preview]));
  // Plain import (no query): the same layouts.ts instance the store
  // mirrors custom layouts into. A ?case= import would be a second
  // module instance with an empty registry.
  const { getLayout, setCustomLayouts } = await import(`../src/lib/layouts.ts`);
  check("T10 registry resolves imported layout", getLayout("my-mini").id === "my-mini");
  // Reset the shared registry so later tests see a clean builtin set.
  setCustomLayouts([]);

  // Delete: backend call + snapshot refresh.
  __mockInvoke("delete_custom_layout", () => undefined);
  __mockInvoke("get_app_state", () => snap({ customLayouts: [], runtimeRevision: 12 }));
  await useAppStore.getState().deleteCustomLayout("my-mini");
  check("T10 delete refreshes custom layouts", eq(useAppStore.getState().customLayouts, []));
}

console.log(`\n${passed} assertions passed`);
// The store arms 8s stale-check timers (PRESS_STALE_CHECK_MS); they keep
// the event loop alive long after the tests finish. Exit explicitly.
process.exit(process.exitCode ?? 0);
