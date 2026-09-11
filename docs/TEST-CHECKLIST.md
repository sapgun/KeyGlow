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
