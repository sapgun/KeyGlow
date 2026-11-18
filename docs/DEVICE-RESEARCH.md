# Device identification research (S7)

Status: research only. No product code changed in this branch. The
conclusions below decide what S7 may build next (an observer) and what
it must not promise (per-device blocking from Raw Input alone).

Sources checked 2026-11-18: Microsoft Learn "About Raw Input",
`RegisterRawInputDevices`, `RAWINPUTDEVICE` reference; Raymond Chen,
"If I have multiple attached keyboards, how can I read input from each
one individually?" (The Old New Thing, 2016-06-27); the vendored
`windows-sys 0.59.0` crate source for the exact feature names and
symbols; `tauri 2.11.5` source for the window-handle path; the
KeyGlow code itself (`src-tauri/src/keyboard/hook.rs`,
`src-tauri/src/platform/foreground.rs`, `docs/LIMITATIONS.md`).

## The two features, kept apart

**Identification** (which keyboard produced this input) and **blocking**
(stopping one keyboard's input while letting another through) are
different features with different feasibility. This document treats
them separately throughout, because the existing docs blur them:
`docs/LIMITATIONS.md` lists "Per-device filtering via Raw Input" as a
recommended improvement, which reads as if Raw Input could do the
filtering. It cannot. The correction is recorded in §5 and in
`LIMITATIONS.md`.

## 1. Enumeration: what Raw Input can tell us about devices

Feasible. The API surface is small and fully available in the
`windows-sys` version the project already uses:

| Step | API | What it returns |
| --- | --- | --- |
| List devices | `GetRawInputDeviceList` | `RAWINPUTDEVICELIST` entries: a device handle (`HANDLE`) plus a type (`RIM_TYPEMOUSE`, `RIM_TYPEKEYBOARD`, `RIM_TYPEHID`) |
| Interface path | `GetRawInputDeviceInfo(hDevice, RIDI_DEVICENAME, …)` | A device interface path string, e.g. `\\?\HID#VID_046D&PID_C33B&MI_00#…`. VID/PID can be parsed out of it |
| Device info | `GetRawInputDeviceInfo(hDevice, RIDI_DEVICEINFO, …)` | `RID_DEVICE_INFO`. For keyboards: `RID_DEVICE_INFO_KEYBOARD` (subtype, keyboard mode, function-key count, indicator count, total key count). For generic HID: `RID_DEVICE_INFO_HID` (vendor id, product id, version, usage page, usage) |
| Friendly name | Not a Raw Input API. Two options: SetupAPI (`SetupDiGetClassDevs` + `SetupDiGetDeviceRegistryProperty` for `SPDRP_FRIENDLYNAME` / `SPDRP_DEVICEDESC`, matched by the interface path) or HID (`HidD_GetManufacturerString` / `HidD_GetProductString`, after opening the device with `CreateFile`) | A human-readable name, when the device reports one |

All of the above are present in `windows-sys 0.59.0` under the
feature `Win32_UI_Input_KeyboardAndMouse` (which pulls in
`Win32_UI_Input`): `RegisterRawInputDevices`, `GetRawInputData`,
`GetRawInputDeviceList`, `GetRawInputBuffer`,
`GetRegisteredRawInputDevices`, `DefRawInputProc`, the `RIDEV_*`,
`RIDI_*`, `RIM_TYPE*` constants, and the `RAWINPUT`, `RAWKEYBOARD`,
`RID_DEVICE_INFO*` structs. `WM_INPUT` and
`WM_INPUT_DEVICE_CHANGE` already come with the existing
`Win32_UI_WindowsAndMessaging` feature. Friendly names need two
further features: `Win32_Devices_HumanInterfaceDevice`
(`HidD_GetManufacturerString`, `HidD_GetProductString`,
`HidD_GetAttributes`) and `Win32_Devices_DeviceAndDriverInstallation`
(`SetupDi*`). No new crate is needed.

Limits of enumeration (documented, not fixable from userspace):

- **Phantom keyboards.** A modern machine exposes many
  keyboard-class devices that are not physical keyboards: laptop
  embedded controllers, Bluetooth stacks, remote-desktop virtual
  keyboards, software keyboards, some mice with a keyboard TLC.
  Raymond Chen's note applies directly: `GetRawInputDeviceList`
  "is not useful in practice" as a raw device picker, because the
  list is full of special-purpose keyboards hiding inside the
  machine. Any device list UI must filter or rank: show only
  devices that have actually produced input, or that report a
  keyboard HID usage, and let the user rename/ignore the rest.
  Showing the raw list as-is would be a confusing picker.
- **Handles are not stable identities.** An `hDevice` value is only
  meaningful while the device is attached; re-enumeration after a
  reconnect can hand out a different value. The stable key is the
  device interface path (`RIDI_DEVICENAME`), plus VID/PID as a
  fallback. A profile that references "keyboard X" must store the
  interface path (or VID/PID), never the handle.
- **No layout from the device.** Nothing in `RID_DEVICE_INFO`
  says ISO vs JIS vs ANSI, or which physical layout a keyboard
  has. The S6 ADR already records that the registry cannot tell
  whether a connected keyboard is ISO or JIS; Raw Input does not
  change that. A "press a key to identify" wizard can confirm
  which device is which, but it cannot infer the layout.
- **Friendly names are best-effort.** Some devices report no
  manufacturer/product string. The UI must fall back to
  VID/PID or the interface path, and allow a manual override
  (LIMITATIONS.md already anticipates a manual override).

## 2. Input-source observation: what Raw Input can tell us per keystroke

Feasible, with one integration constraint (see §2.3).

### 2.1 Registration

- Register the keyboard top-level collection: usage page `0x01`
  (Generic Desktop), usage `0x06` (Keyboard), via
  `RegisterRawInputDevices`.
- Flags: `RIDEV_INPUTSINK` (receive input while the app is in the
  background; required, KeyGlow is a tray app) plus
  `RIDEV_DEVNOTIFY` (receive `WM_INPUT_DEVICE_CHANGE` for
  arrival/removal, i.e. hotplug). `RIDEV_INPUTSINK` requires a
  non-NULL `hwndTarget`.
- `RegisterRawInputDevices` may be called for a device class that
  is not currently attached; the system starts delivering when the
  device appears. Only one window per device class may be
  registered within a process.

### 2.2 Reading

- Each keystroke arrives as `WM_INPUT`. `GetRawInputData` with
  `RID_INPUT` returns a `RAWINPUT` whose `header` carries the
  source device handle (`hDevice`), a timestamp (`dwTime`, a
  millisecond tick count from the same clock as the hook's
  `KBDLLHOOKSTRUCT.dwTime`), and a type. The `keyboard` member is
  a `RAWKEYBOARD`: `MakeCode` (scan code), `VKey` (virtual key),
  `Message` (`WM_KEYDOWN` / `WM_KEYUP` / sys variants), `Flags`
  (`RI_KEY_BREAK` for release, `RI_KEY_E0` / `RI_KEY_E1` for
  extended keys), `ExtraInformation`.
- The `(VKey, MakeCode, Flags)` triple is the same triple the
  existing hook feeds into `identify_key`, so a Raw Input event
  can be mapped to a `KeyCode` with the same function, including
  the S6 scan-code disambiguation (0xE2/0x56 vs 0xE2/0x73,
  0xDC/0x2B vs 0xDC/0x7D).
- The buffered read (`GetRawInputBuffer`) exists for
  high-frequency devices; for keyboards the standard one-event
  read is adequate.
- Injected input (`SendInput`) arrives with a NULL device handle
  in the raw-input header. That is a second, independent way to
  spot injected input, next to the hook's `LLKHF_INJECTED` flag.
  (Relevant to the S8 ADR on injected input.)

### 2.3 Integration constraint: Raw Input needs a window, the hook thread has none

`WM_INPUT` is delivered to a window's message queue. KeyGlow's
hook thread deliberately has no window: it owns only a message
queue created by `PeekMessageW`, and `PostThreadMessageW(WM_QUIT)`
is its shutdown path (P4). Raw Input therefore cannot be bolted
onto the hook thread.

Two routes exist; only one fits the codebase:

- (a) **Dedicated invisible window on its own thread.** A new
  observer thread creates a message-only window, registers it
  with `RIDEV_INPUTSINK | RIDEV_DEVNOTIFY`, and runs its own
  `GetMessageW` loop for `WM_INPUT` / `WM_INPUT_DEVICE_CHANGE`.
  Shutdown mirrors the hook thread: unregister
  (`RIDEV_REMOVE`), post `WM_QUIT`, join. This keeps the hook
  thread, the safety worker, and the foreground observer
  untouched, and a dead observer cannot take the hook down
  with it. This is the recommended route.
- (b) **Reuse the Tauri main window.** Tauri 2.11.5's `Window`
  implements `raw_window_handle::HasWindowHandle`, so the HWND
  is obtainable. But Tauri/wry owns that window's message loop
  and exposes no raw `WndProc` hook for `WM_INPUT`, so this
  route would need framework-level plumbing and would couple
  observation to the UI window's lifetime (tray minimise/close
  behaviour). Not recommended.

## 3. Blocking: what Raw Input cannot do

**Raw Input cannot block, suppress, or redirect input.** It is a
notification channel: by the time `WM_INPUT` is delivered, the
input has already been dispatched to the system. There is no
return value that consumes an event, and no Raw Input flag that
stops delivery to other applications.

The tempting flag does not help:

- `RIDEV_NOLEGACY` only stops the *registering application's own
  windows* from receiving legacy messages (`WM_KEYDOWN` etc.) for
  the registered device class. It changes nothing for other
  processes, and nothing system-wide. It is a per-app legacy
  suppression switch, not a filter.

So the blocking options for per-device filtering are:

| Option | How it would work | Verdict |
| --- | --- | --- |
| Existing `WH_KEYBOARD_LL` hook alone | The hook already blocks globally by returning 1 — but `KBDLLHOOKSTRUCT` carries no device handle, so it cannot tell which keyboard a key came from | Cannot do per-device blocking by itself |
| Hook + Raw Input correlation | Match each hook event to the nearest `WM_INPUT` event by `(VKey, scan, up/down)` and timestamp (`dwTime` on both sides), then apply the block decision for the matched device | Best-effort only, and racy by construction (see §3.1). Not a correctness-grade mechanism |
| Kernel filter driver (Interception-style) | A signed filter driver sits below the input stack and drops reports per device | The only robust per-device blocking path, but it needs driver installation, a signed driver, admin rights, and ongoing signing/HVCI compatibility work. It contradicts the project's local-first, no-admin, no-driver premise (LIMITATIONS.md; S8 ADR requirement that driver install/admin never be smuggled into the default path). Separate user decision required; not part of S7 |
| `BlockInput` | Win32 `BlockInput` | Blocks all input at thread level, is not per-device, and is the wrong tool. Rejected |

### 3.1 Why hook + Raw Input correlation is only best-effort

- **Two threads, two queues, no shared sequence.** Hook events
  arrive on the hook thread; `WM_INPUT` arrives on the observer
  thread's window. Matching is by timestamp and key identity,
  not by an ID the OS guarantees. Two keyboards pressed at the
  same moment, or the same key pressed twice quickly on two
  keyboards, are ambiguous.
- **Consumed keys never produce a `WM_INPUT`.** Community
  observation (and the input-path ordering: the low-level hook
  runs before the input is dispatched to raw-input consumers)
  says a key the hook swallows generates no `WM_INPUT` at all.
  That means the correlation source disappears exactly for the
  keys a per-device block would care about: the observer would
  see the hook event but no matching raw event, and could not
  tell whether the key came from the blocked device or whether
  the raw event was merely delayed. This is the load-bearing
  unknown; Test 23 item 5 exists to settle it on the rig.
  Until it is settled, no design may assume the correlation
  works.
- **Ordering is not guaranteed across the two paths.** Even for
  keys the hook passes through, the hook callback and the
  `WM_INPUT` delivery are not synchronised; a burst of input can
  reorder the two streams relative to each other.
- **The hook callback must stay lean.** The P2/P4 contract
  forbids disk, network, UI, or blocking channel sends in the
  callback. Any correlation bookkeeping would have to happen on
  the observer side (matching a hook event the callback already
  emitted), never inside the callback. Correlation state is also
  new shared state between two threads, i.e. new locking and new
  failure modes for the safety-critical path. That cost is only
  worth paying if the rig proves the correlation is reliable
  enough to use — it has not been proven.

Conclusion for blocking: **do not promise per-device blocking.**
S7 delivers identification only. If per-device blocking is ever
wanted, the S8 ADR must choose between the correlation heuristic
(best-effort, racy, needs rig proof) and a kernel driver (robust,
heavy, needs a separate user decision) — or drop the feature.

## 4. What identification alone can deliver

Without any blocking, a Raw Input observer enables:

- **"Which keyboard is this?" attribution.** The glow/UI can show
  which connected keyboard produced the last input, and a
  press-a-key wizard can label devices ("press any key on the
  keyboard you want to name").
- **Per-device input log/stats.** Counts per device, last-seen
  device, hotplug history. Display-only; no safety impact.
- **Device list with real names.** VID/PID plus friendly name
  (or manual override) replaces the current `Generic Keyboard`
  label.
- **Injected-input signal.** A NULL `hDevice` on a raw event is
  a second injected-input indicator for the S8 ADR.

None of these needs the hook callback to change, and none of them
touches the safety epoch, the snapshot stamp, or the settings file.

## 5. Contract compatibility with P2 / P3 / P4 / S5

Checked against the current code. An observer built as in §2.3(a)
(a separate thread + message-only window, observation only) does
not conflict with any existing contract:

- **P2 safety convergence.** The observer never touches
  `SafetyState`, never claims or completes an epoch, and never
  holds `convergence_lock()`. It is read-only with respect to
  safety. No change to `safety.rs`.
- **P3 snapshot/event ordering.** If device attribution ever
  reaches the UI as state (not just a log line), it must travel
  with the existing `(safetyEpoch, runtimeRevision)` stamp and
  pass the same ordering gate as every other snapshot/event;
  an unstamped device event must never overwrite a newer
  snapshot. For a first observer that only logs/attributes, the
  gate does not apply. No change to the stamping rules.
- **P4 hook lifecycle.** The observer is a separate thread with
  its own lifecycle (register → message loop → unregister →
  join). It must not share the hook thread's queue, must not
  post to it, and must not reuse `HOOK_THREAD_ID` or the hook's
  shutdown flag. A dead observer must be reported the same way
  a dead hook thread is (status event, not a silent stall), and
  must never take the hook down with it. No change to
  `hook.rs` or `hook_lifecycle.rs`.
- **S5 foreground observer.** The foreground observer is a
  polling thread (`GetForegroundWindow` every 50 ms + 200 ms
  debounce) that reports exe-path changes. A Raw Input observer
  is orthogonal: device identity is independent of which exe is
  in the foreground, so app-rule evaluation is unaffected.
  The two observers share only process lifetime and shutdown
  ordering; they must not share mutable state. No change to
  `foreground.rs` or `app_profiles.rs`.
- **Hook callback constraint (standing).** No disk, network, UI,
  or blocking channel send in `low_level_keyboard_proc`. The
  observer adds no work to the callback. If correlation (§3.1)
  is ever attempted, matching happens on the observer thread
  from events the callback already emits, never in the callback.

## 6. Correction to LIMITATIONS.md

`LIMITATIONS.md` previously said per-device filtering could be
added later via Raw Input ("Raw Input could be added later", and
"Per-device filtering via Raw Input" as recommended improvement
#1). That is now corrected: Raw Input gives per-device
*identification* (observation); per-device *filtering* needs a
separate blocking mechanism that Raw Input does not provide (hook
+ correlation heuristic, or a kernel driver). The doc now says
so, and points here.

## 7. Open questions (all need the rig)

1. Does a hook-consumed key really produce no `WM_INPUT`?
   (Test 23.5 — decides whether correlation is even possible.)
2. In practice, how many phantom keyboards does
   `GetRawInputDeviceList` return on a typical machine, and can
   the "only devices that produced input" filter keep the picker
   usable?
3. Do `HidD_GetProductString` / SetupAPI friendly names resolve
   for common keyboards without admin rights, or do some
   devices refuse `CreateFile`?
4. Does the device interface path survive sleep/resume and USB
   re-enumeration unchanged, so a stored profile reference
   stays valid?
5. Is the `dwTime` tick on `RAWINPUTHEADER` and on
   `KBDLLHOOKSTRUCT` close enough in practice to correlate
   events, and how often do two keyboards collide in the same
   window?
6. Does registering `RIDEV_INPUTSINK` on a message-only window
   keep delivering while the app is minimised to the tray and
   while another app is in the foreground?

## 8. Recommendation

Build S7 as an **observer only**: enumerate devices, resolve
names, register the keyboard TLC on a dedicated message-only
window, and attribute input to devices. Ship that as a
display/identification feature. Do not build per-device blocking
on top of it until Test 23 settles question 1, and do not build
a kernel driver without a separate user decision. The S8 ADR
(remapping) should take the injected-input signal (§2.2) and
the blocking verdict (§3) as inputs.
