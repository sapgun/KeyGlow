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
