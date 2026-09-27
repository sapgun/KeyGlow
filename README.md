<div align="center">

[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md)

  <img src="public/icon.png" alt="KeyGlow icon" width="112" />

# KeyGlow

### See your keyboard. Control every key.

A lightweight, local-first visual keyboard controller for Windows.

Click a key in the on-screen keyboard to disable it. Click again to bring it back. KeyGlow enforces the state at the Windows input layer — not just in the UI.

[![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?logo=windows11&logoColor=white)](#supported-platforms)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-native%20input-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=111)](https://react.dev/)
[![Local First](https://img.shields.io/badge/privacy-local--first-B6F25C)](#privacy)

</div>

<p align="center">
  <img src="docs/assets/keyglow-main.webp" alt="KeyGlow main interface" width="100%" />
</p>

---

## Why KeyGlow?

Most keyboard software is tied to one manufacturer. KeyGlow takes a different approach: the visual controller is independent from Logitech, Razer, Keychron, Leopold, Akko, or any other specific vendor.

Choose the keyboard type closest to the physical keyboard you use, then control individual keys visually.

- Click a key → disable it
- Click again → enable it
- Press a physical key → see it light up in real time
- Switch keyboard types without restarting
- Save different profiles for work, gaming, or custom setups
- Keep everything local — no account, telemetry, or cloud required

> KeyGlow is a real input controller, not a keyboard mockup. Disabled keys are intercepted with a Windows low-level keyboard hook.

## Interface

KeyGlow is designed around one simple mental model:

| State | Meaning |
| --- | --- |
| 🟢 Glowing | Key is enabled |
| ✨ Bright / pressed | Physical key is currently being pressed |
| ⚫ Dark | Key is disabled |

## Features

### Visual per-key control

Click any supported key directly on the keyboard graphic to toggle whether Windows receives that input.

### Real-time key glow

Physical key-down and key-up events are reflected in the UI so the virtual keyboard behaves like a live control surface.

### Multiple keyboard types

KeyGlow separates the physical keyboard type from the behavior profile.

| Keyboard type | Description |
| --- | --- |
| Desktop keyboard | Standard PC keyboard with a number pad |
| Slim desktop | Desktop keyboard without a number pad |
| Compact keyboard | Smaller layout with arrow keys and a function-key row |
| Mini keyboard | Compact layout with arrow keys and no dedicated function-key row |
| Laptop-style keyboard | Smallest layout, without dedicated arrow or function-key rows |

The UI intentionally uses familiar keyboard-type names instead of enthusiast percentage labels such as TKL, 75%, 65%, or 60%.

Internally, these remain data-driven layout definitions such as `fullsize-ansi`, `tkl-ansi`, `75-ansi`, `65-ansi`, and `60-ansi`, so additional keyboard types can be added without rebuilding the renderer.

### Profiles

Keep separate key states for different situations.

- Default
- Gaming
- Coding
- Custom profiles

Profiles can be created, duplicated, renamed, reset, and persisted across restarts.

### 🐈 Cat Lock

A small but practical feature: if a cat, child, cleaning cloth, or anything else lands on the keyboard, Cat Lock blocks every key immediately.

Unlock with the Cat button or the emergency shortcut:

```text
Ctrl + Shift + F12
```

### Emergency unlock

`Ctrl + Shift + F12` cannot be disabled by a normal profile. It immediately re-enables all keys and switches back to the Default profile.

### System tray

Closing the main window hides KeyGlow to the tray. Exiting from the tray unloads the keyboard hook and restores normal keyboard behavior.

### Multilingual UI

Current UI languages:

- English
- 한국어
- 日本語

## How it works

```text
Physical keyboard
        ↓
WH_KEYBOARD_LL  (SetWindowsHookExW)
        ↓
KeyGlow FilterEngine
        ↓
    disabled?
      /    \
    yes     no
     ↓       ↓
 consume   CallNextHookEx
  event         ↓
           Windows apps
```

The hook callback is deliberately minimal:

- no disk I/O
- no network requests
- no persistent key logging
- no React work inside the callback
- only inexpensive in-memory state lookup and event routing

UI updates are delivered asynchronously so keyboard filtering stays responsive.

## Supported platforms

| Platform | Status |
| --- | --- |
| Windows 11 | ✅ Supported |
| Windows 10 | ✅ Supported |
| macOS | ⏳ Not yet |
| Linux | ⏳ Not yet |

Windows-specific input code is isolated behind the native platform layer to leave room for future ports.

## Quick start

### Requirements

- Node.js 20+
- Rust stable 1.77+
- WebView2
- Visual Studio Build Tools with the MSVC C++ toolchain

### Development

```bash
git clone https://github.com/sapgun/KeyGlow.git
cd KeyGlow
npm install
npm run tauri dev
```

Regenerate keyboard layout JSON after editing the layout generator:

```bash
npm run gen:layouts
```

Run all tests:

```bash
npm run test:all
```

## Build for Windows

```bash
npm run tauri build
```

Expected outputs:

```text
src-tauri/target/release/bundle/nsis/KeyGlow_0.1.0_x64-setup.exe
release/KeyGlow_0.1.0_x64-setup.exe

src-tauri/target/release/keyglow.exe
release/KeyGlow.exe
```

The current installer uses a per-user installation and does not require administrator rights for normal use.

## Privacy

KeyGlow is intentionally local-first.

- No telemetry
- No analytics
- No cloud account
- No network dependency
- No typed-key history written to disk
- No registry-based permanent key disabling

Real-time key-down/up events exist only so the local keyboard UI can light up.

Settings are stored locally, typically at:

```text
%APPDATA%\com.keyglow.app\settings.json
```

## Safety & limitations

KeyGlow intentionally stays in normal user mode.

- Filtering currently applies globally to the Windows session, not independently per physical keyboard.
- `Ctrl + Alt + Delete` and Secure Attention Sequence behavior remain protected by Windows.
- `Fn` is firmware-controlled on most keyboards and usually cannot be intercepted as a normal Windows key.
- If KeyGlow exits or crashes, its hook disappears and normal keyboard behavior returns.

More details: [docs/LIMITATIONS.md](docs/LIMITATIONS.md)

## Project docs

- [Architecture](docs/ARCHITECTURE.md)
- [Known limitations](docs/LIMITATIONS.md)
- [Manual test checklist](docs/TEST-CHECKLIST.md)

## Roadmap

Possible next steps after the Windows v0.1 foundation is stable:

- Per-application profiles
- Raw Input / per-device keyboard distinction
- ISO and JIS layouts
- HHKB / Alice / split keyboard layouts
- Importable custom layouts
- Key remapping
- Macro and layer support
- Optional QMK / VIA integrations

## Contributing

Issues, bug reports, keyboard type/layout contributions, UX feedback, and pull requests are welcome.

When reporting keyboard-hook bugs, include:

- Windows version
- keyboard type / profile
- affected key(s)
- whether the issue reproduces after emergency unlock

## ❤️ Support KeyGlow

If KeyGlow is useful to you, you can support continued development by starring the repository, reporting reproducible bugs, contributing keyboard layouts, sharing the project, or sending a donation.

[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support%20KeyGlow-FF5E5B?logo=kofi&logoColor=white)](https://ko-fi.com/sapgun)

### Crypto

| Network / method | Donation address |
| --- | --- |
| Ethereum | `0xDF2930264Cf2285eB76C232b3a1233f0c5D4b471` |
| Solana | `BzsE914REG8op1uonEv7rz2NxiS9k3Jcrivz84NdNd5H` |
| Tether ID | `sapgun98@tether.me` |

> Please verify the destination and network before sending. Crypto transfers are irreversible. For the Ethereum address, use the Ethereum network; for the Solana address, use the Solana network. Use the Tether ID only from a service that explicitly supports `tether.me` identifiers.

PayPal support can be added once a public PayPal payment or PayPal.Me URL is available. A PayPal account dashboard URL is not a public donation link.

---

<div align="center">

Built with Tauri · Rust · React · TypeScript

**KeyGlow — See your keyboard. Control every key.**

</div>
