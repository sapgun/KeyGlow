# Known limitations (v0.1)

KeyGlow is a user-mode application. It does not install a driver and does not remap keys in the registry.

## Administrator rights

Not required for standard operation. The NSIS installer uses current-user mode.

A low-level keyboard hook (`WH_KEYBOARD_LL`) works for the current Windows session at user integrity level. Processes running at a higher integrity level (elevated apps) may not observe the same filtering, which is a Windows integrity rule, not a KeyGlow setting.

## What cannot be intercepted (and why)

| Input | Why |
| --- | --- |
| `Ctrl + Alt + Delete` | Secure Attention Sequence. Handled by Winlogon / the secure desktop before (or instead of) user-mode hooks. KeyGlow does not attempt to bypass this. |
| Other SAS / secure-desktop input | Same reason. |
| `Fn` | Almost always handled inside keyboard firmware. Windows never sees a standalone Fn virtual key. The key is shown on compact layouts as **unsupported**. |
| Some vendor extra keys / media keys | Often HID consumer-page usages rather than standard VK codes. Deferred. |
| Per-device filtering | `WH_KEYBOARD_LL` is session-global. KeyGlow does **not** claim per-keyboard control in v0.1. Raw Input could be added later. |

## Global, not per keyboard

If two keyboards are plugged in, disable rules apply to both. The UI device name is `Generic Keyboard` unless a later version adds HID identification.

## Auto-repeat and mid-hold disable

Disabling a key while it is held stops further auto-repeat. The matching key-up is still forwarded if the original key-down reached Windows, so modifiers cannot stick.

## Injected input

KeyGlow filters injected events the same way as physical ones. It does not inject keystrokes.

## Persistence

Disabled keys persist **inside KeyGlow profiles**. They do not persist as a Windows-wide remap after the app exits.

## Next recommended improvements

1. Per-device filtering via Raw Input (`GetRawInputDeviceList` + device handle on `RAWINPUT`)
2. ISO / JIS / HHKB / Alice layout packs
3. Optional key remapping (in addition to enable/disable)
4. Per-application profiles
5. Custom layout importer
6. HID product-name detection with manual override
7. Media / consumer-page keys
