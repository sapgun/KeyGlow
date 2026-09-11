# KeyGlow architecture

```
KeyGlow
│
├── Frontend (React + TypeScript + Vite + Tailwind + Zustand)
│   ├── Keyboard renderer (data-driven)
│   ├── Layout / profile selectors
│   ├── Status, tray-adjacent window chrome
│   └── layouts/*.json
│
├── Tauri IPC
│   ├── commands: get_app_state, set_key_enabled, enable_all_keys,
│   │             profiles, layout, autostart
│   └── events: keyboard:key-down, keyboard:key-up,
│               keyboard:state-changed, keyboard:emergency-unlock,
│               profile:changed
│
├── Rust core
│   ├── keyboard/keycodes.rs   stable IDs ↔ Windows VK/scan
│   ├── keyboard/engine.rs     enable/disable + stuck-key state machine
│   ├── keyboard/hook.rs       WH_KEYBOARD_LL callback + hook thread
│   ├── profiles/              JSON persistence
│   └── commands/              IPC
│
└── Platform
    └── Windows input backend (SetWindowsHookExW)
```

## Windows APIs actually used

| API | Role |
| --- | --- |
| `SetWindowsHookExW(WH_KEYBOARD_LL, …)` | Install the low-level keyboard hook |
| `CallNextHookEx` | Forward events that should reach Windows |
| `UnhookWindowsHookEx` | Clean shutdown |
| `GetMessageW` / `TranslateMessage` / `DispatchMessageW` | Hook thread message loop |
| `PostThreadMessageW(WM_QUIT)` | Ask the hook thread to exit |
| `KBDLLHOOKSTRUCT` (`vkCode`, `scanCode`, `flags`) | Identify the physical key |

No registry remapping, no kernel driver, no `MapVirtualKey` injection, and no `SendInput` in v0.1.

## How a disabled key is intercepted

1. The UI calls `set_key_enabled(code, false)`.
2. Rust stores that key in `FilterEngine.disabled` (a 104-slot boolean array).
3. The same list is saved on the active profile in `settings.json`.
4. The next `WH_KEYBOARD_LL` callback maps `(vk, scan, extended)` to a `KeyCode`.
5. If the key is disabled and this is a down/repeat whose matching down was not previously forwarded, the callback **returns 1** and does not call `CallNextHookEx`.
6. Ordinary applications therefore receive nothing.

## Key-down / key-up state machine

Each key tracks:

- `physical_down` — the key is currently held on the hardware
- `forwarded_down` — a down event was already delivered to Windows

| Event | Rule |
| --- | --- |
| Down, enabled | Forward, set `forwarded_down` |
| Down, disabled | Consume |
| Repeat while held after disable | Consume extra repeats, keep `forwarded_down` |
| Up, down was forwarded | **Always forward** (prevents stuck Ctrl/Shift/Alt/Win) |
| Up, down was consumed | Consume |
| Up with no prior down | Forward (safer than swallowing an unmatched up) |

## Emergency unlock

`Ctrl + Shift + F12` is evaluated from **physical** down state, even if those keys are disabled. Completing the chord:

1. `enable_all()` inside the hook
2. emit `keyboard:emergency-unlock` on the event thread
3. switch to the Default profile and clear its disabled list
4. persist settings

The chord itself cannot be turned off in a profile.

## Profiles

Stored as JSON. Layout and disabled-key set are separate fields. Built-in profiles:

- Default — all keys enabled
- Gaming — Left/Right Windows disabled (demonstration)
- Coding — all keys enabled

## Concurrency

- Hook callback: `parking_lot::Mutex<FilterEngine>` held only for the decision
- Events: `sync_channel(256)` + `try_send` (drops if the UI is stuck; input is never delayed)
- Persistence: only from command/event threads, never from the hook
