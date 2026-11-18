# Manual test checklist

Use a physical keyboard. Open Notepad as the target app unless noted.

## Automated

```bash
npm run gen:layouts
npm test
npm run test:rust
```

These cover layout integrity, key-id roundtrips, the blocking state machine (including stuck-modifier prevention and emergency unlock), and settings load/save with a corrupt-file fallback.

Safety convergence concurrency (P2) is covered by `cargo test` in
`src-tauri/src/keyboard/safety.rs` — deterministic, no sleeps:

- `reconciled_epoch_does_not_advance_before_work_completes` — claiming an
  epoch must not advance the completion marker before the work ran.
- `pending_epoch_survives_claim_without_work` — an elected-but-unworked
  epoch stays pending so another path converges it (never silently lost).
- `complete_reconcile_is_monotonic` — a late straggler never moves the
  completion marker backwards.
- `convergence_lock_serializes_worker_and_pump` — while one path holds the
  convergence lock, another path's `try_lock` fails (channel-coordinated).
- `worker_protocol_converges_latest_epoch_exactly_once` — five rapid
  triggers coalesce into one convergence of the latest epoch.
- `newer_epoch_during_work_is_not_lost` — a trigger landing mid-work is
  picked up by the next loop iteration.

## Test 1 — Disable A

1. Click `A` on the KeyGlow keyboard. It should look disabled (dark, dashed, strike).
2. Focus Notepad. Press `A`.
3. Expected: no `a` appears.
4. Click `A` again to re-enable.
5. Press `A`. Expected: `a` appears.

## Test 2 — Left Windows

1. Disable Left Win.
2. Press Left Win. Expected: Start menu does not open.
3. Re-enable. Expected: Start menu opens.

## Test 3 — Caps Lock

1. Disable Caps Lock.
2. Press Caps Lock. Expected: caps state does not toggle.
3. Re-enable and confirm Caps Lock works.

## Test 4 — Layout switching

Switch 데스크톱 키보드 → 슬림 데스크톱 → 컴팩트 → 미니 → 노트북형 without restarting. Each layout must render, scale with the window, and keep key alignment.

## Test 5 — Profile persistence

1. On a profile, disable Caps Lock and Insert.
2. Exit KeyGlow from the tray (Exit, not just close).
3. Start KeyGlow again.
4. Expected: the same profile and both keys still disabled.

## Test 6 — Press visualization

Hold `W`. The virtual `W` should glow and depress. Release: glow gone.

## Test 7 — Emergency unlock

Disable several keys. Press `Ctrl + Shift + F12`. Expected: all keys enabled, notice shown, Default profile selected.

## Test 7b — Emergency unlock under a saturated event queue (HF-01)

Goal: prove the unlock converges even when the `EmergencyUnlock` queue event is dropped.

1. Disable several keys and turn on Cat Lock.
2. Stall the UI event consumer (e.g. break into the debugger on the main thread, or hold the main thread busy) so the bounded event queue fills and `try_send` starts dropping.
3. Press `Ctrl + Shift + F12`.
4. Expected: physical input recovers immediately (type in Notepad at once). After the UI thread resumes: notice shown, Default profile selected, all keys enabled, Cat Lock off, tray menu refreshed. `settings.json` on disk holds the converged state.
5. Regression check: no duplicate persists per single press in the log (one `emergency unlock converged` line).

## Test 7c — Stale command after emergency (HF-01)

Goal: an in-flight profile/key-disable command must not re-disable keys after the unlock.

1. Disable several keys on the Gaming profile and select it.
2. Start a profile switch (or a key toggle) and press `Ctrl + Shift + F12` while the command is in flight. Manual approximation: invoke `select_profile` for Gaming from the tray menu, then immediately hit the chord.
3. Expected: Default profile selected, all keys enabled, notice shown. The stale command's disabled set is discarded, not applied.
4. Restart the app. Expected: Default profile, all keys still enabled (no stale disable set resurrected from disk).

## Test 7d — Emergency with unwritable settings (HF-01)

Goal: physical recovery must not depend on config persist.

1. Make `settings.json` unwritable (read-only file, or read-only config dir).
2. Disable several keys. Press `Ctrl + Shift + F12`.
3. Expected: all keys enabled immediately; an error is logged (`settings persist failed ... input remains enabled`) instead of silently dropped. Keys stay enabled.

## Test 7e — Emergency edge cases

- With `Ctrl`, `Shift`, or `F12` themselves disabled in the profile: the chord still fires (evaluated from physical state).
- During Cat Lock: the chord fires and clears the lock.
- Holding a modifier mid-hold, then completing the chord: no stuck modifier afterwards (release still forwards the up).
- Five rapid chord presses: single convergence per press, no duplicate UI notices stacking, queue stays bounded.
- Chord immediately followed by a fresh key-disable: the fresh (post-emergency) command applies normally.

## Test 8 — Exit restores input

Disable `A`. Choose tray **Exit**. Press `A` in Notepad. Expected: `A` types. The hook is gone with the process.

## Test 9 — Modifiers with everything enabled

With the Default profile (all enabled), confirm no regression:

- Shift+A
- Ctrl+C / Ctrl+V
- Alt+Tab
- Win+R

## Extra safety

- Disable Left Shift while holding it, then release. Shift must not stick.
- Hide the window with the close button; the app stays in the tray and filtering stays active.
- Open a second KeyGlow instance; it should focus the existing window (single instance).

## Test 10 — Settings survive a failed write (HF-02/HF-03)

1. Make the settings file unwritable (e.g. revoke write permission on
   `%APPDATA%\KeyGlow\settings.json`, or point the app at a read-only
   location).
2. Disable a key in the UI. Expected: the key is disabled in the running
   app, and a settings error banner appears with the failure reason plus
   **Retry** and **Enable All** actions.
3. Press **Retry** after restoring write permission. Expected: the banner
   clears and the file on disk matches the UI state.
4. With the file still unwritable, trigger emergency unlock
   (Ctrl+Shift+F12). Expected: all keys work immediately; the unlock is
   NOT cancelled by the failed write. The banner remains until Retry.

## Test 11 — Profile activation keeps layout in sync (HF-06)

1. Select the Gaming profile, then duplicate it. Expected: the copy is
   selected and the layout selector shows the copy's layout (not the
   previous one).
2. Rapidly create two profiles with the same name. Expected: two distinct
   ids, no collision.
3. Quit, delete the Default profile entry from `settings.json` by hand,
   and restart. Expected: the app starts with a restored Default profile;
   no crash, no empty profile list.

## Test 12 — Settings error banner languages (HF-03)

Switch language to KO and JA, then repeat Test 10 step 2. Expected: the
banner header and Retry button are translated; only the technical detail
stays in English.

## Test 13 — Long hold keeps its glow (HF-04)

Automated (CI, `npm test` → `scripts/test-store-contract.mjs` T6): the
ordering of key events vs. pressed-snapshot responses is verified —
a snapshot older than a processed key-down must not drop the key, a newer
snapshot governs, stale key events are ignored.

Physical (not covered by automation — needs the rig):

1. Open Notepad (or any other app) and hold the `A` key for 12 seconds.
   Expected: the KeyGlow window keeps `A` lit the whole time. It must NOT
   go dark at 8 seconds while the key is still physically held.
2. While still holding, note the keycap style: after ~8s it briefly shows a
   dashed "verifying hold" pulse, then returns to the normal pressed glow.
   Expected: no stuck dark key, no flicker loop.
3. Release the key. Expected: the glow clears within a second.

## Test 14 — Dropped key-up recovery (HF-04)

Automated (CI, `npm test` → `scripts/test-store-contract.mjs` T6): snapshot
merge logic is verified — keys touched by events newer than the snapshot
are kept, the rest follow the snapshot.

Physical (not covered by automation — needs the rig):

1. Hold a key in an external app, then hide the KeyGlow window
   (close to tray) and release the key.
2. Restore the window. Expected: no ghost pressed key remains; if the key
   is still physically held, it shows pressed, otherwise it is clear.
3. Put the machine to sleep with a key held, wake it, release the key.
   Expected: the pressed state resynchronizes on wake; no permanently lit
   key.

## Test 15 — Rapid toggle converges (HF-05)

1. Double-click (or triple-click) the same key as fast as possible.
   Expected: the final key state always matches the last click; the UI
   never ends up inverted because an older response arrived late.
2. Trigger emergency unlock while a toggle request is still in flight.
   Expected: the unlock wins; the late toggle response does not re-disable
   the key.

## Test 16 — Event subscription failure (HF-09)

1. (Code review / fault injection) Make one Tauri event subscription fail.
   Expected: the listeners that did register are unlistened again (no
   leak), and a translated banner explains that live key display may not
   update.
2. In all three languages (EN/KO/JA), check the banner text renders.

## Test 17 — Fresh install (HF-08)

1. On a clean Windows machine (state the OS: Win10 / Win11, build), run
   the NSIS installer (`KeyGlow_0.1.1_x64-setup.exe`) as a normal user.
   Expected: per-user install completes, app starts, tray icon appears,
   keyboard filtering works in Notepad.
2. Run the portable exe (`keyglow.exe`) from a folder without installing.
   Expected: same behavior, no files written outside the config dir and
   the exe's own folder.
3. Record the OS build that was actually tested. Never mark an untested
   OS as verified.

## Test 18 — Upgrade over v0.1.0, settings preserved (HF-08)

1. Install v0.1.0, disable a few keys, create a custom profile, enable
   start-with-Windows.
2. Install the new version over it (NSIS upgrade).
   Expected: `settings.json` is preserved — disabled keys, profiles,
   layout, language, theme, and autostart setting are all intact.
3. Corrupt `settings.json` by hand, then start the app.
   Expected: app starts with defaults, the corrupt file is backed up
   (`.bak`), no crash, input works.
4. Tray Exit, then force-kill the process while keys are disabled.
   Expected: no stuck keys after either; the low-level hook is released.
5. Sleep/resume with the app running.
   Expected: filtering resumes; no ghost pressed keys (see Test 14).

## Test 19 — Stale snapshot/event ordering (P3)

Automated (CI, `npm test` → `scripts/test-store-contract.mjs`, 28
assertions; Rust unit tests in `state.rs` run via `npm run test:rust` on
Windows CI): the real store and the real IPC payload parsing are driven
against a fake native layer with hostile timing —

- T1: hydrate requested before an emergency, resolved after → dropped.
- T2: older-revision `state-changed` after a newer one → dropped.
- T3: toggle invalidated by emergency → late response dropped, unlock kept.
- T4: failed command → optimistic update rolled back, stale recovery
  snapshot dropped, error still surfaced.
- T5: command response stamped older than displayed state → dropped.
- T6: pressed-snapshot older than a processed key-down → key kept;
  newer snapshot governs; stale key events ignored.
- T7: older safety epoch never wins, even with a higher revision.
- T8: malformed event payloads ignored; a malformed emergency payload
  never triggers the unlock path.
- Rust: `runtime_revision` increments per mutation; emergency convergence
  bumps it even when persist fails; `persisted_revision` (disk) and
  `runtime_revision` (state) stay independent.

Physical (not covered by automation — needs the rig):

1. Disable `A`, then trigger emergency unlock, then (before the UI
   settles) switch profiles rapidly. Expected: the UI ends on Default,
   all keys enabled; no disabled key from the old profile reappears.
2. With DevTools throttling the network/IPC, hold a key for 12s and
   release. Expected: no stuck glow, no dropped held key.

## Test 20 — Hook lifecycle (HF-07)

Automated (Windows CI, `npm run test:rust` →
`src-tauri/src/keyboard/hook_lifecycle.rs`, 11 tests, deterministic — no
sleeps, no thread races; the transition functions are the same ones the
real `hook.rs` drives through its statics):

- Start → ready → running → shutdown requested → requested exit → stopped
  (requested shutdown raises no alarm).
- A second `start_hook` is rejected with the restart guidance; the running
  hook is undisturbed.
- No restart after stop or after install failure.
- Install failure records the Win32 error; a following thread-exit report
  does not overwrite it or announce twice.
- Unexpected thread death (panic, message-loop error) moves the lifecycle
  to Failed exactly once and surfaces the reason.
- Thread death during stopping without a requested shutdown is a failure.
- Shutdown request is idempotent; ready/shutdown signals outside their
  states are ignored (total transition table).

Compile-verified on Linux against faithful `windows_sys` 0.59 stubs
(`--cfg windows`): the queue-readiness `PeekMessageW`, the checked
`PostThreadMessageW`, and the `catch_unwind` exit-report plumbing
type-check; behavior on real Windows is covered by CI.

Physical (needs the rig):

1. Start the app, then Exit from the tray immediately (within a second).
   Expected: the process terminates promptly — no hang in hook shutdown
   (the pre-fix race lost WM_QUIT and hung the join forever).
2. Repeat start → immediate Exit 10 times. Expected: no hangs, no leftover
   `KeyGlow.exe` processes, no hook installed after exit (keys type
   normally in Notepad).
3. If the hook thread dies unexpectedly (e.g. kill its thread via a
   debugger): expected — the status bar flips to "keyboard control
   unavailable" with the error, without waiting for user interaction; a
   second start attempt is refused with the restart guidance.
4. Windows silently removing the hook while the thread lives is NOT
   detectable from userspace (documented limitation): if keys stop being
   filtered while the UI still shows active, restart the app from the tray.

## Test 21 — Per-app profiles (S5)

Automated (pure-logic tests in `src-tauri/src/profiles/app_profiles.rs`,
15 tests, deterministic with a virtual clock — no sleeps; S5 integration
tests in `state.rs`, 5 tests; frontend contract in
`scripts/test-store-contract.mjs`, T9, 12 assertions):

- Notepad → browser → game fast switching: only the app still in front
  after the 200ms debounce commits; intermediate apps never become
  effective.
- Rapid Alt+Tab flurry: the debounce timer restarts on every change, so
  only the final app switches.
- App exit / unknown foreground: resolves to no match → Default fallback.
- Inaccessible process (query failure): same Default fallback, no error
  state.
- Manual override survives foreground changes; resume clears the override
  and the safety pause and re-evaluates the current app immediately.
- Deleting a profile removes the rules that reference it and falls the
  effective profile back to Default.
- Saving a rule that points at a non-existent profile is rejected.
- Duplicate rules: highest priority wins; ties go to the first rule in
  the list; exact duplicates collapse at normalize time.
- Emergency unlock pauses automation on Default; foreground changes do
  not resume it until an explicit `resume_auto_profile`.
- Cat Lock survives foreground switches (the lock flag is never touched
  by a switch).
- Foreground switches never write `settings.json` (state test asserts
  the file bytes are unchanged) and never change `selected_profile`.
- Frontend: the snapshot carries the S5 fields; the `profile:auto-switched`
  event applies through the P3 ordering gate (stale switches dropped, an
  emergency floors the stamp); malformed auto-switch payloads are ignored;
  rule CRUD drives the real invoke layer and refreshes the snapshot.

Physical (needs the rig):

1. Open Notepad, then Alt+Tab to the browser, then to a game, pausing
   less than 200ms on each. Expected: only the game's profile becomes
   effective; the browser's profile never flashes in.
2. With a rule for the game exe, Alt+Tab to the game and type. Expected:
   the game profile's disabled keys apply; switching back to Notepad
   restores Default within ~250ms.
3. Pick a profile manually, then Alt+Tab between apps. Expected: the
   manual profile stays effective regardless of the foreground app.
4. Trigger the emergency unlock while a game rule is active, then Alt+Tab
   to the game. Expected: Default stays effective and the UI offers
   "Resume automatic switching"; the game profile does not come back
   until Resume is pressed.
5. Enable Cat Lock, then Alt+Tab to the game. Expected: all keys stay
   blocked; the lock is not lifted by the switch.
6. Close the game while its profile is effective. Expected: Default
   returns; no error banner appears.

## Test 22 — ISO/JIS layouts and layout import (S6)

Automated (layout validation in `scripts/validate-layouts.mjs`, 7
layouts incl. golden SHA-256 content hashes pinning the 5 ANSI files
(line-ending-insensitive, so a Windows CRLF checkout passes); catalog sync in `scripts/test-catalog-sync.mjs` and
`keycodes.rs::catalog_matches_ts_key_catalog`, 110 codes in Rust,
META, and the TS catalog; import validation in
`scripts/test-layout-import.mjs`, 18 assertions, and
`layout_import.rs`, 13 tests; store contract in
`scripts/test-store-contract.mjs`, T10, 12 assertions; model tests in
`models.rs`, 2 tests):

- 5 ANSI layouts regenerate byte-identical (`npm run gen:layouts`
  leaves no diff); ISO has 105 keys, JIS 109 keys, no duplicates, no
  out-of-range coordinates, no overlaps.
- ISO/JIS required keys present: IntlBackslash + `#` label (Backslash
  code) on ISO; IntlYen, IntlRo, Convert, NonConvert, KanaMode on JIS.
- Rust/TS catalogs agree on all 110 codes, including the 6 new ones;
  IntlHash is not a code.
- `identify_key` disambiguates the shared VKs by scan: 0xE2/0x56
  IntlBackslash, 0xE2/0x73 IntlRo, 0xDC/0x7D IntlYen, 0xDC/0x2B
  Backslash; Convert 0x1C, NonConvert 0x1D, KanaMode 0x15.
- Importer rejects: oversize JSON, >120 keys, out-of-range
  coordinates, duplicate codes, unknown codes, unknown fields,
  builtin id collision, bad id format, overlapping keys, dimension
  mismatch, Fn without `unsupported`. A rejected import leaves the
  config and the selected layout untouched.
- Import preview: beginning a preview does not change the selected
  layout; cancelling keeps the original layout; confirming imports,
  refreshes the snapshot, and selects the imported layout; the
  registry resolves the imported layout afterwards.
- Imported layout round-trips through save/load; a profile may
  reference it. A stored custom layout that no longer validates is
  dropped at normalize time and its profiles fall back to Default.
- Deleting a custom layout refreshes the snapshot; deleting one in
  use is refused by the backend.

Physical (needs the rig + real ISO/JIS keyboards):

1. Select fullsize ISO. Expected: 105 keys render; the tall Enter
   shows as one rectangle; the `#` key sits on the A row and the
   extra key sits between Left Shift and Z.
2. Select fullsize JIS. Expected: 109 keys render; Yen sits between
   `^` and Backspace; the bottom row shows 無変換, Space, 変換, かな.
3. On a real ISO keyboard, press the extra key, then the `#` key.
   Expected: the extra key glows IntlBackslash and `#` glows
   Backslash. Record the scan/VK pairs the hook reports.
4. On a real JIS keyboard, press Yen, Ro, 変換, 無変換, かな.
   Expected: each glows its own code. Record the scan/VK pairs; in
   particular confirm whether Yen reports VK_OEM_5 (provisional).
5. Import a valid layout JSON from a file. Expected: the preview
   renders the layout; Apply selects it; the profile keeps it after
   restart.
6. Import an invalid JSON (e.g. an unknown key code). Expected: the
   dialog shows the rejection reason and the current layout is
   unchanged.
7. In the import dialog, cancel after a valid preview. Expected: the
   original layout is still selected; nothing was stored.
8. Delete an imported layout that no profile uses. Expected: it
   disappears from the selector. Try to delete one a profile uses.
   Expected: the backend refuses and the layout stays.

## Test 23 — Device identification via Raw Input (S7)

Research only so far (`docs/DEVICE-RESEARCH.md`): no observer code
exists yet. The automated checks below apply once an observer is
built; the physical checks settle the open questions in the research
doc. Nothing here is verified yet.

Automated (once the observer exists):

- `GetRawInputDeviceList` enumeration matches the devices the
  observer reports; every reported keyboard has a stable identity
  (device interface path, never the raw `hDevice` value).
- Registering the keyboard TLC (usage page 0x01, usage 0x06) with
  `RIDEV_INPUTSINK | RIDEV_DEVNOTIFY` delivers `WM_INPUT` while
  another app is in the foreground.
- Each `WM_INPUT` maps through `identify_key` to the same `KeyCode`
  the hook reports for the same physical press, including the S6
  scan-code disambiguation.
- `WM_INPUT_DEVICE_CHANGE` fires on plug and unplug; the device
  list refreshes without restart.
- The observer never touches `SafetyState`, never holds
  `convergence_lock()`, and never writes `settings.json`.
  The hook callback is unchanged (no disk/network/UI/blocking
  send added).

Physical (needs the rig + two keyboards):

1. Plug in two keyboards. Expected: the observer lists both, each
   with a distinct identity (interface path / VID/PID); phantom
   entries (embedded controller, remote-desktop virtual keyboard)
   are filtered or clearly marked, not presented as real keyboards.
2. Press keys on keyboard A, then keyboard B. Expected: the
   observer attributes each press to the right device.
3. Check friendly names. Expected: each keyboard shows its real
   product/manufacturer name, or a VID/PID fallback where the
   device reports none. No admin prompt appears.
4. Unplug keyboard B while typing on keyboard A, then plug B back
   in. Expected: a device-change event fires, B disappears and
   reappears with the same stored identity (interface path), and
   attribution keeps working without restarting the app.
5. Disable a key in KeyGlow, then press it on either keyboard.
   Expected: the key is blocked as before. Record whether a
   `WM_INPUT` arrives for the blocked press at all. This settles
   whether hook + Raw Input correlation is even possible (research
   doc §3.1); if no `WM_INPUT` arrives for blocked keys,
   correlation cannot identify blocked keys.
6. Press the same key on both keyboards within ~50 ms of each
   other. Expected: record whether the observer can still tell
   the two presses apart, or whether they collide. This bounds
   how reliable timestamp correlation could ever be.
7. Minimise the app to the tray, switch to another app, and type.
   Expected: the observer still attributes input while in the
   background.
8. Sleep/resume the machine with both keyboards attached.
   Expected: enumeration and attribution resume without restart,
   and stored device identities are still valid.

## Test 24 — Key remapping (S8)

Implemented (`docs/ADR-REMAPPING.md` + engine/inject/models/UI).
Automated checks below are covered by the Rust engine tests
(`engine.rs`, `keycodes.rs`, `models.rs`, `state.rs`) and the
frontend contract (T11 in `scripts/test-store-contract.mjs`);
they pass in the local harness — see the S8 report for the
Windows CI result. The physical checks settle the open questions
in the ADR (§11). Nothing physical is verified yet.

Automated (engine unit tests in `engine.rs`, frontend contract in
`scripts/test-store-contract.mjs`, T11; Windows CI):

- Down injects the target once; repeat injects nothing; up injects
  the target up. A source up after a rule change releases the
  down-time target, never the new one.
- Shared target refcount: two sources held, first release injects
  nothing, second release injects the up.
- Flush on rule change / profile switch / emergency injects ups
  for held targets only, is idempotent, and installs new rules for
  the next press only.
- Injected events never remap, never set `physical_down`, never
  count toward the emergency chord, and still respect disable /
  Cat Lock.
- Validation: duplicate source, self-map, disabled participant in
  both directions (disable a rule participant is rejected; create
  a rule naming a disabled key is rejected), `Pause` as source and
  target, toggle keys as targets, unknown ids dropped at load.
- Reverse-table roundtrip: `identify_key(to_vk_scan(k)) == k` for
  all 110 codes, including the S6 scan disambiguations.
- Frontend: rule CRUD drives the real invoke layer and refreshes
  the snapshot; a stale rule-change response is dropped by the
  ordering gate; an emergency floors a pending rule-change
  response.

Physical (needs the rig):

1. Create rule `A → B` on the Default profile. Focus Notepad, press
   `A`. Expected: `b` appears. Press `B`. Expected: `b` appears
   (B itself is unmapped). The rule is listed in the UI; `A`'s
   keycap shows the remap.
2. Hold the remapped `A` for 5 seconds. Expected: `b` repeats (or
   record that it does not — ADR open question 1). Release.
   Expected: no further `b` appears; no stuck key.
3. With `A → B`, hold Shift and press `A`. Expected: `B` appears.
   With a rule `ShiftLeft → ControlLeft`, hold Shift and press `C`
   in an editor. Expected: copy behaviour (Ctrl+C).
4. With `ShiftLeft → ControlLeft`, press and release Shift alone.
   Expected: nothing visible happens; the next plain keypress is
   unaffected (no stuck Control).
5. Hold `A` (mapped to `B`), delete the rule while holding, then
   release `A`. Expected: no extra `b` after the release; the next
   `A` press types `a`.
6. Hold `A` on the Gaming profile (maps `A → B`), switch to Default
   mid-hold, release `A`. Expected: no stuck `B`; `A` types `a`
   afterwards.
7. Hold `A` (mapped to `B`), then press `Ctrl + Shift + F12` with
   the other hand. Expected: all keys enabled, no stuck `b`, the
   rule is still listed in the UI, Default is effective.
8. Enable Cat Lock, press the remapped `A`. Expected: nothing is
   typed. Disable Cat Lock. Expected: the remap works again.
9. With rule `A → B`, try to disable `B` in the UI. Expected: the
   disable is refused with a message naming the rule. Disable `A`
   in a fresh profile, then try to create `A → B`. Expected: the
   rule is refused.
10. Create rules `A → B` and `B → C`. Press `A`. Expected: `b`
    appears, not `c` (no chaining).
11. Try to create `A → A`, two rules with the same source, `Pause`
    as a source, `Pause` as a target, and `A → CapsLock`. Expected:
    all five are refused. Create `CapsLock → ControlLeft`.
    Expected: accepted; pressing CapsLock no longer toggles caps.
12. With `A` disabled, use the on-screen keyboard (or AutoHotkey)
    to send `A`. Expected: still blocked (v0.1 behaviour kept).
    Use it to send a remap source. Expected: the physical key
    passes through un-remapped (remap is physical-only).
13. Open an elevated (Run as administrator) Notepad. Press the
    remapped key. Expected: record whether the target arrives.
    If it does not, the physical key is forwarded instead (UIPI —
    ADR open question 2). No admin prompt from KeyGlow itself.
14. Hold a remapped key, choose tray **Exit** while holding.
    Expected: the target is released cleanly (no stuck key after
    exit). Repeat with a force-kill while holding, then restart
    the app. Expected: the target is not stuck after restart
    (startup sweep).
15. Remap `CapsLock → ControlLeft` and press CapsLock. Expected:
    caps does not toggle and no Control sticks. Remap
    `MetaLeft → ControlLeft`, press Left Win. Expected: Start menu
    does not open. Press `Win + L`. Expected: the machine still
    locks (secure combos are not remappable).
16. Create `A → B` and `C → B`. Hold `A`, hold `C`, release `A`.
    Expected: `B` is still held (a held-`b` repeat or a test app
    shows it down). Release `C`. Expected: `B` is released.

## Test 25 — Macros, layers, QMK/VIA (S9)

Implemented (`keyboard/macro_runner.rs`, `keyboard/engine.rs`,
`profiles/qmk.rs`, `profiles/models.rs`, `commands/profiles.rs`,
`commands/keyboard.rs`, `MacroPanel.tsx`, `LayerPanel.tsx`,
`QmkImportDialog.tsx`). Automated checks below are covered by the
Rust engine tests (`macro_runner.rs`, `engine.rs`, `models.rs`,
`qmk.rs`, `state.rs`) and the frontend contract (T12 in
`scripts/test-store-contract.mjs`); they pass in the local harness —
see the S9 report for the Windows CI result. Nothing physical is
verified yet.

Automated (engine unit tests, frontend contract T12; Windows CI):

- Macro runner: start/tick/cancel, held keys released in reverse
  order, second trigger during a run consumed and ignored, 30 s
  run limit aborts, cancel key works while disabled.
- Engine: macro start on trigger down, 0-delay leading steps run
  immediately, other physical keys interleave during a run, cancel
  and emergency flush held steps, profile switch installs the new
  profile's macros and layers, Cat Lock on cancels a running macro.
- Layers: hold activates on switch down and deactivates on up,
  toggle toggles on switch down and consumes the up, switch key is
  consumed, layer switch flushes held keys, rule edits reset to
  the base, a layer remap source works even when the base disables
  it (QMK meaning), disabled layer targets are rejected.
- Validation: unbalanced steps, Pause step, toggle-key step,
  disabled step key, role overlaps (remap source/target, macro
  trigger, macro cancel, layer switch), caps (64 steps, 5000 ms
  per step, 16 macros, 8 layers per profile).
- QMK: base-only import, layer with MO + TRNS inheritance,
  missing switch key, unreachable switch key, unsupported
  compound keycode, layout mismatch, length mismatch, export
  roundtrip; VIA basic keycodes and 16-bit layer code refusal;
  VIA export refused for profiles with layers.
- Frontend: macro/layer CRUD drives the real invoke layer with
  (profile_id, ...) args and refreshes the snapshot; QMK import
  sends profile, JSON, layout, and key order; the snapshot carries
  the active layer id; a stale macro-save response is dropped by
  the ordering gate; an emergency floors a pending macro save.
  The S8 remap invoke fix (profile_id, from, to) is asserted in
  T11.

Physical (needs the rig):

1. Create a macro triggered by `F9` with steps `H` down, `H` up
   (delay 10 ms), name it "Hello". Focus Notepad, press `F9`.
   Expected: `h` appears once. Press `F9` again. Expected: `h`
   appears again. The macro is listed in the UI.
2. Create a macro with a long delay (e.g. `A` down, 2000 ms,
   `A` up). Press the trigger, then press `Escape` after 500 ms.
   Expected: `a` stops — no further `a` appears after the cancel,
   and no key is stuck (the next plain `A` types `a` once).
3. With a macro running (long delay), press `Ctrl + Shift + F12`
   with the other hand. Expected: the run stops immediately, no
   stuck key, all keys are enabled afterwards.
4. Create a macro triggered by `F9` with 70 steps (or one step
   with a 6000 ms delay, or a total run over 30 s). Expected: the
   save is refused with a message naming the limit (or, for a run
   that exceeds the time limit, it aborts mid-run and the log
   says so).
5. Hold a macro step key down (the macro holds `A` for 5 s),
   switch to another profile mid-run. Expected: the held `A` is
   released (no stuck `a`), the new profile's macros are the ones
   listed.
6. Hold a macro step key down, choose tray **Exit** while holding.
   Expected: the key is released cleanly (no stuck key after
   exit). Repeat with a force-kill while holding, then restart.
   Expected: the key is not stuck after restart (startup sweep).
7. Create a layer switched by `CapsLock` (hold) that disables `A`
   and remaps `B → C`. Hold `CapsLock`, press `A`. Expected:
   nothing types. Press `B`. Expected: `c` appears. Release
   `CapsLock`, press `A`. Expected: `a` appears again (base
   restored). The active layer is shown in the UI while held.
8. Create a layer switched by `F10` (toggle). Press `F10` once,
   press `A` (disabled in the layer). Expected: nothing types.
   Press `F10` again, press `A`. Expected: `a` appears.
9. With a layer active, press its switch key's own position in a
   text field. Expected: nothing types (the switch key is
   consumed). Press the layer's remap target key directly.
   Expected: it still types its own letter (no chaining).
10. Create a layer whose remap source is a key the base profile
    disables. Expected: the save is accepted, and while the layer
    is active the remap works (layer replaces the base at that
    position). Create a layer whose remap *target* is a key the
    base disables. Expected: the save is refused.
11. Try to use one key as both a macro trigger and a layer switch
    key (or as a remap source). Expected: the second save is
    refused with a message naming the conflict.
12. Import the sample QMK keymap.json from `docs/QMK-VIA.md`
    (layer 0 base + layer 1 with `MO(1)`). Expected: the preview
    shows both layers with the switch key and counts; applying
    replaces the base keymap and adds the layer; the profile's
    macros are still listed. Cancel an import. Expected: the
    profile is unchanged.
13. Import a VIA JSON file with a 16-bit layer keycode.
    Expected: refused with a reason pointing at QMK keymap.json.
14. Export the profile as QMK, then import the exported file
    into a fresh profile. Expected: the roundtrip reproduces the
    same base keymap and layers. Export as VIA with a layered
    profile. Expected: refused with a message pointing at QMK.
15. Run a macro while its step key is disabled in the profile.
    Expected: the step is dropped (nothing types for that step)
    and the run continues or aborts cleanly — record which.
    (Injected-lane disable check; see ARCHITECTURE.md S9.)
16. Open an elevated (Run as administrator) Notepad, trigger a
    macro. Expected: record whether the injected steps arrive.
    If they do not, the physical trigger still works (UIPI — same
    open question as S8). No admin prompt from KeyGlow itself.
