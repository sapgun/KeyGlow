# Manual test checklist

Use a physical keyboard. Open Notepad as the target app unless noted.

## Automated

```bash
npm run gen:layouts
npm test
npm run test:rust
```

These cover layout integrity, key-id roundtrips, the blocking state machine (including stuck-modifier prevention and emergency unlock), and settings load/save with a corrupt-file fallback.

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
