# Emergency shortcut and reported device failure

The user tested the f24e217 candidate: key filtering (Test 1) passed;
Ctrl+Shift+F12 unlock (Test 2) and Cat Lock escape (Test 3) failed. Treat this
as a blocking device result, regardless of earlier green CI. The exact cause
is not established: missing firmware/Fn F12 input, modifier delivery, physical
recovery versus stale UI need observation. Do not claim this report fixed from
engine tests alone.

## User flow

Click **Emergency shortcut**, select at least two of Ctrl/Shift/Alt and a main
key, then Save. Ctrl+Shift+U is a convenient function-row-free choice. Active
shortcut labels in the status bar and Cat Lock hints come from native state.
Ctrl+Shift+F12 remains an always-available fallback; it still requires Windows
to receive an F12 event. Fn itself is firmware-controlled and cannot be bound.

The dialog shows keys from `get_pressed_snapshot`, not browser keydown events.
Hold the active chord long enough to see received keys (roughly one second).
It reports success only after the native safety epoch increases AND its
completion marker catches up. Testing a chord enables all keys. The dialog
does not certify hardware behavior from simulated events.

## Native contract

- Bindings compile during editing/startup; the hook path checks only booleans
  and a KeyCode. No config parsing, allocation, or disk work is added there.
- Matching uses physical state before filtering, accepts either modifier side
  and any press order, and works under Cat Lock or disabled chord keys.
- A held chord triggers once until released rather than flooding safety wakes
  on repeats or unrelated presses while held.
- Invalid bindings are rejected before changing runtime state. Competing edits
  serialize under the convergence lock. Save failure leaves the applied binding
  visible with the existing unsaved banner; retry persists it.
- Config schema v2 persists the global binding, migrates v0/v1 with the legacy
  default, repairs semantically invalid chords, and preserves future-version
  read-only protection. Older v1 builds read v2 settings without overwriting.
- Emergency convergence clears locks/profiles as before and keeps the chosen
  global shortcut. The default fallback remains independent of the edit.

## Regression and device gate

Automated engine tests: blocked chord/Cat Lock, right modifiers, alternate press
orders, repeat debouncing/rearm, incomplete chord, invalid edits, legacy fallback.
Config test: v1 migration, locale/theme preserved, custom shortcut round trip,
invalid shortcut repaired. Store/IPC tests: selected chord sent, authoritative
response shown, failed save reported with applied runtime state.
Browser UI verification uses mocked IPC and must be labeled as such.

Device retest must record OS build, new executable SHA256, and:
1. All three original keys visibly received? Does A type afterward? Does the UI
   clear its disabled keys and Cat Lock? Distinguish those observations.
2. Set/save Ctrl+Shift+U; restart; confirm label persists.
3. Disable A, then press Ctrl+Shift+U: A types, safe UI/Default profile converges.
4. Enable Cat Lock and press Ctrl+Shift+U: all physical input restored.
5. Disable the selected main key and modifiers, then repeat: escape still works.
6. Try the unchanged Ctrl+Shift+F12 fallback with either modifier side. If F12
   is absent, compare Fn+F12 and firmware Fn-lock setting; record what is received.
7. Ensure incomplete chords do not unlock; held chord shows only one unlock until
   release/repress; save failure retains runtime escape and unsaved indication.

Keep a mouse-accessible tray Exit/Task Manager recovery path while retesting.
No release, merge, or F-stage feature expansion until the blocking physical
failure is actually retested and resolved.
