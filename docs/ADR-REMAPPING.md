# ADR: key remapping (S8)

Status: **decided** (ADR only — no product code in this branch).
Implementation follows after approval; the acceptance tests in §10 are
part of the implementation, not of this ADR.

Sources checked 2026-11-18: the KeyGlow code itself
(`src-tauri/src/keyboard/{hook,engine,safety,keycodes}.rs`,
`src-tauri/src/{lib,state}.rs`, `src-tauri/src/profiles/models.rs`,
`docs/DEVICE-RESEARCH.md` §2.2/§3, `docs/ARCHITECTURE.md`,
`docs/LIMITATIONS.md`); Microsoft Learn "`SendInput` function",
"`KBDLLHOOKSTRUCT` structure" (`LLKHF_INJECTED`,
`LLKHF_LOWER_IL_INJECTED`), "`GetAsyncKeyState` function"; the
vendored `windows-sys 0.59.0` crate source (symbols below verified
present, not assumed).

## 0. What S8 v1 is

One-to-one, single-key remapping: pressing physical key **S** (source)
sends key **T** (target) to the foreground application instead, while
**S** is held. Chord/combination remaps (e.g. `CapsLock+X` → `Y`),
macros, and layers are **out of scope** for v1; they are S9 work and
must not be smuggled into this implementation.

Scope decisions:

- **Per-profile rules.** Rules live on `Profile` (next to
  `disabled_keys`), not globally. The effective profile's rules apply;
  switching profiles (manual, S5 app rule, or emergency fallback to
  Default) swaps the rule set. A global rule set would break the S5
  mental model (a gaming profile wants different remaps from Default)
  and was rejected.
- **Session-global, not per-device.** Remapping applies to input from
  every keyboard (S7 verdict: Raw Input cannot block per device, and
  hook + Raw Input correlation is best-effort and unproven). Per-device
  remapping is out of scope; the UI must not offer it.
- **No driver, no admin, no registry.** Remapping is done in userspace
  with `SendInput` (user32). No kernel filter driver, no registry
  remap, no `uiAccess`, no elevation. This is a hard premise, not a
  tuning knob: if a remap cannot be done as a normal user, it is not
  done in S8.
- **v0.1 changes.** ARCHITECTURE.md says "no `SendInput` in v0.1".
  S8 introduces the first `SendInput` call in the codebase. Nothing
  else in v0.1's input path changes: the hook, the engine's
  disable/Cat Lock logic, and the safety worker are untouched except
  for the additive paths described below.

## 1. Injected-input identification

Two independent signals exist; S8 uses both, for different purposes.

### 1.1 Hook path (authoritative for the loop guard)

`KBDLLHOOKSTRUCT.flags` carries `LLKHF_INJECTED` (0x10, set when
the event was injected by `SendInput`/`keybd_event`) and
`LLKHF_LOWER_IL_INJECTED` (0x02, injected from a lower-integrity
process). Both constants exist in `windows-sys 0.59.0`
(`Win32_UI_WindowsAndMessaging`). The current callback
(`hook.rs::low_level_keyboard_proc`) does **not** check them; S8 adds
the check.

**Decision D1 — remapping applies to physical input only.** An event
with either injected flag set is never a remap source: it never
triggers a new injection. This is the loop guard (see §2). The flag
— not the `dwExtraInfo` tag (§1.3) — is the guard, because the flag
covers every injector, including other software, and cannot be
forged by a stale tag.

### 1.2 Raw Input path (diagnostic, not a guard)

Per S7 (`DEVICE-RESEARCH.md` §2.2), injected input arrives at a
`WM_INPUT` consumer with a NULL device handle. The future S7
observer will therefore see remapped keys as device-less input. That
is correct behaviour — an injected key has no keyboard — and the
observer must treat it as injected, never attribute it to a device.
The observer is not part of S8 and the hook never waits for it.

### 1.3 Our own injections are tagged

Every `SendInput` call S8 makes sets `dwExtraInfo` to a fixed
KeyGlow magic constant (`KEYGLOW_INJECTED_TAG`, value fixed at
implementation time, e.g. an ASCII-derived constant). Purpose:
diagnostics only — logs and the S7 observer can recognise KeyGlow's
own injections. The tag is **not** a security boundary and **not**
the loop guard: any process can set any `dwExtraInfo`, and the
guard is the flag in §1.1.

### 1.4 What injected events still go through

D1 does not mean injected events bypass the filter entirely.
v0.1 behaviour must be preserved: today the hook processes injected
events as if physical, so an on-screen keyboard sending a disabled
key is blocked, and Cat Lock blocks injected input too. S8 keeps
that, with the engine split into two lanes:

| | Physical event | Injected event |
| --- | --- | --- |
| Disable / Cat Lock check | yes | yes |
| Remap lookup + injection | yes | **no** |
| `physical_down` tracking | yes | **no** |
| UI pulse (glow) | yes | **no** |
| Counts toward emergency chord (§5) | yes | **no** |

Injected down/up decision is the same enabled-check as a physical
down (forward iff enabled and not Cat Locked); no per-key injected
state is kept, because a stray injected up for a key that is not
down is harmless to forward and forwarding it can only clear a
stuck modifier. Injected events never mutate `physical_down` /
`forwarded_down`, so the engine's existing up rule
(forward iff `was_forwarded || !was_physical`) keeps working on
physical state only.

**Consequence to record:** the glow shows physical presses only.
An injected key (ours or another tool's) never lights a keycap.
That is a deliberate, visible behaviour change from v0.1, where
injected keys glowed.

## 2. SendInput loop prevention

**Decision D2 — the loop is impossible by construction, not by a
counter.** The chain is:

1. Physical **S** down → callback → engine → inject **T** down
   (§4) → decision for **S** is Consume.
2. The injected **T** down re-enters the same callback with
   `LLKHF_INJECTED` set → D1: no remap lookup, no new injection →
   disable check only → **T** is enabled by validation (§7, V3),
   so it is forwarded.
3. No third event is ever produced.

There is no chain rule: rules `A→B` and `B→C` do **not** compose,
because the injected **B** never remaps. There is no recursion
depth, no visited-set, no loop counter to tune — and none may be
added later as a substitute for D1.

**Decision D3 — injection happens synchronously in the callback,
exactly once per triggering event.** The engine (pure logic, no
Win32) returns the injection as data; the callback executes it with
`SendInput` immediately after the engine call, still inside the
callback, and only then returns the hook decision. One physical
event produces at most one `INPUT` struct (one key event). Rationale
and bounds:

- The deferred alternative (callback enqueues, a separate injector
  thread sends) was rejected: a physical up could overtake a
  deferred injected down, producing a down-after-up ordering the
  engine cannot repair. Ordering is only exact if the injection is
  issued in the same callback invocation as the physical event.
- `SendInput` from inside a low-level hook callback is the standard
  pattern (used by AutoHotkey-class tools): it only queues the input
  and returns; the injected event's own hook invocation is delivered
  to the hook thread's message queue and runs after the current
  callback returns, so there is no reentrancy and no deadlock from
  the hook itself.
- Residual risk, stated plainly: `SendInput` is a Win32 call inside
  the callback, which the P2/P4 contract otherwise keeps lean. The
  bound is one queued input per callback, no waiting on delivery,
  no disk/network/UI. If `SendInput` ever blocked (full input
  queue), the callback would be slow and Windows could silently
  unhook it — the P4 documented limitation (silent removal is
  undetectable from userspace) applies unchanged. This is accepted
  because there is no ordering-correct alternative.
- **Failure is fail-open.** If `SendInput` returns 0 / fewer events
  than requested, the callback logs loudly and forwards the original
  physical event instead of consuming it (decision becomes Forward).
  The user gets the physical key rather than silence; the failure is
  visible in logs. Swallowing a failed injection was rejected:
  it would lose input with no trace.

**Decision D4 — scan-code injection.** The injected `INPUT` uses
`KEYEVENTF_SCANCODE` with the target's scan code, plus
`KEYEVENTF_EXTENDEDKEY` for E0 keys (Right Ctrl/Alt, NumpadEnter,
etc.), and `KEYEVENTF_KEYUP` for releases. Position — not the
virtual key — identifies the target, which is exactly the S6
disambiguation (shared VKs split by scan: 0xE2/0x56 vs 0xE2/0x73,
0xDC/0x2B vs 0xDC/0x7D). A new reverse table
`to_vk_scan(KeyCode) -> (vk, scan, extended)` is derived from the
same source of truth as `identify_key`; a roundtrip test must prove
`identify_key(to_vk_scan(k)) == k` for all 110 codes (§10). All
symbols (`SendInput`, `INPUT`, `KEYBDINPUT`, `KEYEVENTF_KEYUP`,
`KEYEVENTF_SCANCODE`, `KEYEVENTF_EXTENDEDKEY`) exist in
`windows-sys 0.59.0` under `Win32_UI_Input_KeyboardAndMouse` — a
feature the crate does not currently enable, so the implementation
adds that feature to `Cargo.toml`. No new crate is needed.

## 3. Modifier pairing

**Decision D5 — pairing is per source, with a per-target refcount.**
The engine keeps two tables (in addition to the existing
`physical_down` / `forwarded_down`):

- `remap_held[S]`: for each source **S** currently physically held,
  the target **T** that was injected for it — a snapshot of the
  mapping taken at down time. A rule change mid-hold never changes
  what the held key releases as (§4).
- `target_refcount[T]`: how many held sources currently map to **T**.

Rules:

- **Down (first, not repeat):** if **S** has a rule **S→T** and
  `remap_held[S]` is empty, inject **T** down, set
  `remap_held[S] = T`, `refcount[T] += 1`. Consume **S**.
- **Repeat:** consume **S**, inject nothing. The target is already
  down; OS typematic repeat for the injected key, if any, comes from
  the system, not from re-injection. Whether a held injected key
  repeats in Notepad is a rig question (Test 24.2); if the rig shows
  no repeat, a repeat-injection fallback is a separate decision, not
  part of v1.
- **Up:** if `remap_held[S] == T`, `refcount[T] -= 1`; inject **T**
  up only if the refcount reached 0; clear `remap_held[S]`. If
  another source still holds **T**, no up is injected — the target
  stays down until its last holder releases.
- **Modifier alone:** a remapped modifier pressed and released on
  its own injects the target down/up pair exactly as above. A lone
  injected modifier has no visible effect by itself (same as a lone
  physical modifier), and no stuck state results.
- **Modifier released mid-combination:** physical modifier up
  injects the target modifier up immediately, even while a regular
  key is still held — the OS sees the combination end, exactly as
  with a physical modifier. The still-held regular key's own up is
  unaffected.
- **Regular key while a remapped modifier is held:** the physical
  regular key is forwarded normally; the OS already sees the
  injected modifier down, so the combination is correct with no
  extra work.
- **Shared target:** `A→B` and `C→B` are both allowed. Hold **A**,
  hold **C**, release **A** → **B** stays down (refcount 1);
  release **C** → **B** up. The refcount is what makes this safe;
  a single boolean per target would stick or drop **B**.

## 4. Source up / target up when rules change mid-hold

**Decision D6 — a held key always releases what its down injected,
and a rule change never injects a new key for a held key.**

- The up always uses `remap_held[S]` (the mapping snapshotted at
  down time), never the current rule set. Changing `A→B` to `A→C`
  while **A** is held does not inject **C** and does not change what
  the release sends: the release injects **B** up (or nothing, if a
  flush already released it — see below).
- **Flush on rule change / profile switch / rule deletion.**
  Before a new rule set is applied, the engine flushes: for every
  **S** with `remap_held[S] == T`, `refcount[T] -= 1`, inject **T**
  up iff the refcount reached 0, clear all `remap_held`. Only then
  is the new mapping installed. The new rules apply to the *next*
  press, never retroactively to a held key. Rationale: predictable —
  a held key never produces a surprise keystroke from a rule the
  user just deleted.
- **Flush is idempotent.** After a flush all refcounts are 0 and
  all `remap_held` are empty, so a second flush injects nothing.
  This matters because the emergency path (§5) can flush from two
  threads (hook thread and safety worker) and must never double-
  inject an up. (An injected up for a key that is not down is
  harmless to the OS in any case; idempotence is belt and braces.)

## 5. Emergency unlock priority

**Decision D7 — emergency is evaluated from physical state only,
before any remap decision, and it always wins.**

- In `FilterEngine::process`, the order is fixed: (1) update
  `physical_down`; (2) evaluate `emergency_held()` from
  `physical_down` (Ctrl+Shift+F12, either side); (3) only then the
  remap/disable decision. A remapped F12, Ctrl, or Shift still
  counts toward the chord, because the physical press is recorded
  before remapping is considered. An *injected* F12/Ctrl/Shift
  never counts (§1.4): the emergency chord cannot be triggered by
  automation.
- **Emergency flushes held remaps.** `enable_all()` — called on
  the hook thread when the chord fires, and re-asserted by the
  safety worker via `state.emergency_unlock()` — flushes all held
  remaps (§4) as part of clearing the disabled set and Cat Lock.
  Every injected target that is down gets its up; the rules stay in
  the config. A user still holding a remapped key after the
  emergency gets no surprise: the key's release injects nothing
  (`remap_held` was cleared), and the next press uses the rules
  normally.
- **Rules survive the emergency.** Remap rules are config, not
  runtime disable state. `emergency_unlock()` clears the disabled
  set and Cat Lock but never touches the rules. After the emergency
  the effective profile is Default (S5 `on_emergency`), so
  Default's rules apply; the pre-emergency profile's rules return
  only on explicit resume, exactly like its disabled keys.
- **Cat Lock beats remap.** While Cat Lock is on, a remapped
  source is consumed and nothing is injected (`is_enabled` is false
  under Cat Lock). Remap is inert under the lock; the rules stay
  listed in the UI, just inert. The emergency chord clears Cat Lock
  (§5, first bullet) and therefore re-enables remapping.

## 6. Stuck-key cleanup

**Decision D8 — every exit path releases injected keys; the one
path that cannot is covered by a startup sweep.**

| Trigger | Cleanup |
| --- | --- |
| Rule change / profile switch / rule deletion | §4 flush (inject ups, then install new rules) |
| Emergency unlock | §5 flush (hook thread + worker re-assert) |
| Tray Exit / normal shutdown | `hook_thread_main`, after the message loop breaks and **before** `UnhookWindowsHookEx`: flush held remaps, execute the injections, then unhook. The ups pass through the still-installed hook as injected events and are forwarded (§1.4). |
| Hook thread unexpected death (panic / message-loop error) | No cleanup is possible — the thread is dead. The app enters the P4 Failed state (`hook_active=false`, `hook_error` set). A target held at that moment can stick. Residual risk, accepted: it needs a remapped key held at the exact moment the thread dies. |
| Force-kill / crash (no code runs) | No cleanup is possible. Covered by the startup sweep below. |
| App start | **Startup sweep.** Before `start_hook`, for every `KeyCode`, if `GetAsyncKeyState` reports it *not* currently down, inject a key-up for it in a single batched `SendInput` call. Keys physically held at startup are skipped (checked first), so the sweep can never cancel a key the user is actually holding. This releases anything a previous force-killed run left down. The sweep covers all codes, not just stored rule targets, so a rule deleted after the crash is still covered. |

Notes:

- The sweep's `GetAsyncKeyState` check is load-bearing: without it,
  an injected up for a physically-held modifier (e.g. Shift held
  during app start) would make the OS believe the modifier was
  released while it is still held. `GetAsyncKeyState` exists in the
  same `Win32_UI_Input_KeyboardAndMouse` feature as `SendInput`.
- `enable_all()` returning injection actions: the engine is pure
  logic — it returns *what* to inject; callers execute it. So
  `process_event` returns the per-event inject list, `enable_all()`
  returns the flush list, and rule application returns its flush
  list. The `KeyboardController` trait gains the return values (it
  is already KeyGlow-specific). The hook callback executes actions
  immediately; the state.rs controller implementation executes
  actions returned on worker/command threads — `SendInput` is
  callable from any thread. On non-Windows the injection helper is
  a stub (there is no hook there, so no actions ever occur; engine
  tests still assert the action lists).
- The engine's existing up rule is unchanged for physical keys:
  an up is forwarded iff it was forwarded or never observed down
  (hook started mid-hold). A remapped source's up is consumed
  (the OS never saw the source down); the injected target up is
  what the OS receives.

## 7. Validation invariants

Rules are validated at save time (command layer, reusing the
`models.rs::normalize` pattern that already drops unknown disabled
keys) and rejected with a surfaced error — never silently fixed.
A stored file with an invalid rule drops that rule at load and
keeps the rest of the profile.

- **V1 — well-formed.** `from != to`; both are valid `KeyCode` ids
  (`KeyCode::from_id`, the same 110-code catalog the S6 sync test
  pins in Rust, META, and TS). Unknown ids are dropped at load,
  rejected at save.
- **V2 — one rule per source per profile.** A second rule with the
  same `from` in one profile is rejected. (Shared targets are
  allowed — §3 refcount.)
- **V3 — no participant is disabled.** Neither a rule's source nor
  its target may be in the same profile's `disabled_keys`, and
  disabling a key that participates in any rule of that profile
  (source or target) is rejected by the disable command with a
  message telling the user to remove the rule first. Why: injected
  events pass the disable check (§1.4), so a disabled target would
  be delivered anyway — the combination would be a lie. Total
  invariant, enforced on both write paths.
- **V4 — excluded keys.** `Pause`/`Break` is excluded as source
  *and* target: it has no distinct key-up at the PS/2 level (the
  hook reports it as make-only), so there is nothing to pair the
  release with. Toggle keys (`CapsLock`, `NumLock`, `ScrollLock`)
  are excluded as *targets*: injecting a toggle down flips LED
  state the engine does not track, and a missed up would leave the
  LED inverted against the UI. They are allowed as *sources*
  (consuming the physical toggle simply never toggles it). `Fn` has
  no `KeyCode` (firmware-handled, never reaches Windows) and so
  cannot be referenced at all.
- **V5 — storage.** Rules are stored on `Profile` as
  `remappings: [{from, to}]` (`#[serde(default)]`, camelCase),
  config version stays 1 — additive, same precedent as
  `custom_layouts` / `app_rules`. Old settings files load with an
  empty rule list.

## 8. Contract compatibility

Checked against the current code. No existing contract changes;
remap is additive on top of each.

- **P2 safety convergence.** Rule CRUD commands are mutating
  commands: they observe the safety epoch at start and are
  discarded via the existing stale-command path
  (`discard_stale_command`) if an emergency landed mid-command —
  a late rule change must never resurrect a disabled state after
  the unlock. The emergency flush (§5) is idempotent (§4), so the
  worker, pump, and stale-command paths can all flush without
  double-injecting. The hook callback never takes
  `convergence_lock()`; it only returns injection actions for the
  caller to execute. No change to `safety.rs`.
- **P3 snapshot/event ordering.** A rule change is a runtime state
  mutation: the command bumps `runtime_revision` exactly once,
  persists, and emits with the new `(safetyEpoch, runtimeRevision)`
  stamp; the UI ordering gate applies unchanged — a stale
  rule-change response is dropped like a stale toggle (T3/T5
  pattern). The snapshot's disabled-keys list is unaffected.
  Runtime remap state (`remap_held`, refcounts) is engine-internal
  and never appears in a snapshot. The pressed snapshot
  (`physical_down`) never includes injected targets. No change to
  the stamping rules.
- **P4 hook lifecycle.** No restart; remap state lives in the
  engine, which lives as long as the process. The hook thread's
  shutdown path gains the pre-unhook flush (§6); the lifecycle
  state machine itself is unchanged. A dead hook thread cannot
  flush — documented residual risk (§6). No change to
  `hook.rs`'s lifecycle handling or `hook_lifecycle.rs`.
- **S5 app profiles.** Rules are per-profile (§0). Applying a
  profile applies its disabled keys *and* its rules, after a flush
  of the old profile's held remaps (§4). Emergency falls back to
  Default and its rules (§5). Foreground switches never write
  `settings.json` and never change `selected_profile` — unchanged.
  No change to `foreground.rs` or the resolver.
- **S6 layout registry / import.** Rules address `KeyCode` ids —
  the same id space as `disabled_keys` and the layout JSON `code`
  fields. A rule's source may not exist on the active layout
  (e.g. `IntlYen` on an ANSI 60%): the rule stays valid, it is
  just unreachable from that layout, and the UI lists it by code
  id. Layouts are display-only for remap; importing a layout never
  creates, changes, or deletes a rule. The reverse table (§2, D4)
  is covered by the catalog roundtrip test, reusing the S6 sync
  infrastructure. No change to the registry or importer.
- **S7 observer (future).** The observer sees injected keys as
  device-less input (NULL `hDevice` + KeyGlow `dwExtraInfo` tag,
  §1.2/§1.3) and must not attribute them to a device. The observer
  and the remapper share nothing but process lifetime. No change
  to the hook callback's contract with the observer.
- **Hook callback constraint (standing).** No disk, network, UI, or
  blocking channel send in `low_level_keyboard_proc`. `SendInput`
  is the single, bounded exception (§2, D3): one queued input per
  callback, no waiting on delivery. If a future change needs more
  than one input per event, that is a new ADR decision, not a
  quiet expansion.

## 9. Known limitations (documented, not fixable in S8)

- **UIPI: injected keys may not reach elevated apps.** `SendInput`
  needs no admin, but Windows UIPI can keep input injected by a
  normal-user process from reaching a foreground app running
  elevated (admin). If the foreground app is elevated, a remapped
  key may not arrive there; the fail-open path (§2, D3) forwards
  the physical key instead, so the user still gets *a* keypress.
  Test 24.13 records the actual behaviour on the rig. Admin is
  **not** required by S8 and must not be smuggled in as a fix.
- **Win+L, Ctrl+Alt+Del and other secure combos cannot be
  remapped.** The OS handles them below the hook. Remapping a Meta
  key does not capture them; the UI must not imply otherwise.
- **No per-device remapping** (§0, S7 verdict).
- **Force-kill / hook-thread death can stick a held target** (§6);
  the startup sweep is the mitigation, not a guarantee — a target
  held at the moment of death *and* physically held again at the
  next startup (skipped by the `GetAsyncKeyState` check) stays
  down until its physical key is pressed and released once.
- **Repeat behaviour of a held injected key is unconfirmed**
  (Test 24.2). If the rig shows no typematic repeat for injected
  keys, v1 ships without repeat injection and the gap is recorded.
- **Toggle-key targets are rejected** (V4) — remapping *to*
  CapsLock/NumLock/ScrollLock is not offered.

## 10. Acceptance criteria for the implementation

The implementation PR must prove all of these; the ADR is not
done until they pass on Windows CI.

Engine unit tests (pure logic, `engine.rs`, no Win32):

- Down injects the target once; repeat injects nothing; up injects
  the target up; source up after a rule change releases the
  down-time target, never the new one.
- Shared target refcount: two sources held, first release injects
  nothing, second release injects the up.
- Flush on rule change / profile switch / emergency injects ups
  for held targets only, is idempotent, and installs the new rules
  for the next press only.
- Injected events never remap, never set `physical_down`, never
  count toward the emergency chord, and still respect
  disable/Cat Lock.
- Validation: V1–V5 (duplicate source, self-map, disabled
  participant in both directions, `Pause` both ways, toggle keys
  as targets, unknown ids dropped at load).
- Reverse-table roundtrip: `identify_key(to_vk_scan(k)) == k` for
  all 110 codes, including the S6 scan disambiguations.

Frontend contract tests (store contract, T11 pattern):

- Rule CRUD drives the real invoke layer and refreshes the
  snapshot; a stale rule-change response is dropped by the
  ordering gate; an emergency floors a pending rule-change
  response (same pattern as T3/T5).

Windows CI: Build and test + Version consistency green (installer
packaging pending is the usual non-code gate). Physical rig:
Test 24 (§11 in TEST-CHECKLIST.md), all items unverified until run.

## 11. Open questions (all need the rig)

1. Does a held injected key auto-repeat in Notepad (Test 24.2)?
2. Does injected input reach an elevated foreground app (Test
   24.13, UIPI)?
3. Does any common app (game with anti-cheat, remote desktop)
   reject `LLKHF_INJECTED` input in practice, and does the
   fail-open path cover it?
4. Is one `SendInput` per callback fast enough under a burst
   (e.g. 10 keys/s) that the hook never approaches the system's
   slow-hook removal?
