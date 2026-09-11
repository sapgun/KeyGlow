# KeyGlow

**See your keyboard. Control every key.**

A lightweight universal visual keyboard controller for Windows. KeyGlow shows a graphical keyboard, lets you click any key to enable or disable it, and enforces that state at the Windows input level using a low-level keyboard hook.

It is not tied to Logitech, Razer, or any other manufacturer. v0.1 works with ordinary Windows keyboards.

## Supported platforms

- Windows 10
- Windows 11

macOS and Linux are not supported in v0.1. OS-specific input control is isolated behind a platform layer for later ports.

## Requirements

- Node.js 20+
- Rust stable (1.77+)
- WebView2 (included with Windows 10/11 and Microsoft Edge)
- Visual Studio Build Tools with the MSVC C++ toolchain

## Development

```bash
cd KeyGlow
npm install
npm run tauri dev
```

The Vite dev server listens on `127.0.0.1:5173` (Windows Hyper-V excludes port 1420 on this machine).

Regenerate layout JSON after editing `scripts/generate-layouts.mjs`:

```bash
npm run gen:layouts
```

Run tests:

```bash
npm run test:all
```

## Build a Windows installer

```bash
npm run tauri build
```

The NSIS installer is written to:

- `src-tauri/target/release/bundle/nsis/KeyGlow_0.1.0_x64-setup.exe`
- `release/KeyGlow_0.1.0_x64-setup.exe` (copy)

The portable executable is `src-tauri/target/release/keyglow.exe` (also copied to `release/KeyGlow.exe`).

Installation does not require administrator rights (current-user install). Closing the window hides to the tray; choose **Exit** in the tray menu to quit and unload the keyboard hook.

## Keyboard hook architecture

```
Physical keyboard
        ↓
WH_KEYBOARD_LL  (SetWindowsHookExW)
        ↓
KeyGlow FilterEngine  (in-memory, mutex held only for a cheap lookup)
        ↓
    disabled?  ──yes──► consume event (return 1)
        │
        no
        ↓
    CallNextHookEx  → Windows applications
```

- Hook callback does no disk I/O, no network, no React IPC, and no logging of typed keys.
- UI updates are sent on a bounded `sync_channel` with `try_send`.
- Closing or killing KeyGlow uninstalls the hook, so Windows keyboard behavior is restored immediately.
- Disabled state is **not** written to the registry and does not survive process exit.

## Layouts vs profiles

| Concept | Meaning | Examples |
| --- | --- | --- |
| Layout | Physical shape | 데스크톱 키보드, 슬림 데스크톱, 컴팩트, 미니, 노트북형 |
| Profile | Which keys are disabled | Default, Gaming, Coding, custom |

Layouts are data files in `src/layouts/`. The React renderer does not hardcode keys.

## Emergency unlock

`Ctrl + Shift + F12` cannot be disabled by a profile. It immediately enables every key, switches to the Default profile, and shows a notice in the UI.

The main window also has **Enable All**.

## Privacy

- Local-only. No telemetry, analytics, accounts, or network requirement.
- Typed keys are never written to disk.
- Real-time key-down/up events are sent only to the local UI so keycaps can light up.

Settings live in the app config directory, typically:

`%APPDATA%\com.keyglow.app\settings.json`

## Current limitations

See [docs/LIMITATIONS.md](docs/LIMITATIONS.md). In short:

- Filtering is **global** for the Windows session, not per physical keyboard.
- `Ctrl + Alt + Delete` and other Secure Attention Sequence behavior is OS-protected.
- `Fn` is firmware-controlled on most keyboards and cannot be intercepted.
- Administrator rights are not required for normal operation.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Manual test checklist

See [docs/TEST-CHECKLIST.md](docs/TEST-CHECKLIST.md).
