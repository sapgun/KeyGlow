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
│   ├── keyboard/safety.rs     emergency-unlock safety epoch + claim
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

`Ctrl + Shift + F12` is evaluated from **physical** down state, even if those keys are disabled. The chord itself cannot be turned off in a profile. Completing the chord:

1. `enable_all()` inside the hook — physical input recovers immediately, on the hook thread.
2. The hook bumps the **safety epoch** (one atomic increment) and wakes the safety worker over an unbounded channel. No blocking sends, no file or UI work in the callback.
3. The **safety worker** (`keyglow-safety` thread) converges config + controller + persist on its own thread, then posts UI/tray updates to the main thread. It claims each epoch exactly once, so rapid repeated presses coalesce and a delayed UI thread can never lose the unlock.
4. The `EmergencyUnlock` queue event is only a best-effort UI hint now: if the event queue is saturated and drops it, the epoch latch is authoritative. The event pump keeps an exactly-once fallback claim in case the worker is gone.

Converged state: Default profile selected, its disabled list cleared, all keys enabled, settings persisted (best-effort: a persist failure is logged but never re-blocks input), UI notice + tray refreshed.

### Stale commands after emergency

Mutating commands (`set_key_enabled`, `enable_all_keys`, `set_cat_lock`, profile select/create/duplicate/delete/reset) record the safety epoch on entry and re-check it after mutating and after persisting. If an emergency press landed in between, the command's intent is discarded and the safe state is force re-asserted (controller + config + persist + UI/tray), so a stale profile/key-disable command can never re-disable keys after the unlock — including on the next launch (a stale write that raced the worker's persist is repaired, not left on disk).

## Profiles

Stored as JSON. Layout and disabled-key set are separate fields. Built-in profiles:

- Default — all keys enabled
- Gaming — Left/Right Windows disabled (demonstration)
- Coding — all keys enabled

## Concurrency

- Hook callback: `parking_lot::Mutex<FilterEngine>` held only for the decision. On emergency the callback additionally performs exactly one atomic epoch increment, one non-blocking wake send, and one `try_send` — never file, network, UI, or blocking channel work.
- Events: `sync_channel(1024)` + `try_send` (drops if the UI is stuck; input is never delayed). Key down/up pulses are lossy by design; the emergency signal is not — it rides the safety epoch latch.
- Safety wake: unbounded `mpsc::channel::<()>`; `send` from the hook never blocks.
- Persistence: only from command/event/safety-worker threads, never from the hook.
- Convergence exactly-once: `SafetyState::claim_reconcile(epoch)` elects a single converger per epoch across the safety worker, the event pump fallback, and stale-command repair.

## ADR: safe settings replace (HF-02)

`profiles/storage.rs::atomic_write` replaces `settings.json` as follows:

1. Serialize to a temp file **next to the target** (same volume) under a
   unique name (`settings.json.tmp.<pid>.<counter>.<nanos>`). Concurrent
   writers never share a temp file.
2. `sync_all` the temp file (best-effort durability, not a power-loss
   proof — do not claim power-loss durability from unit tests).
3. `std::fs::rename` the temp over the target. On Windows this maps to
   `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`; on the same volume the
   destination is replaced atomically — readers see either the old or the
   new file, never a torn or missing one.
4. The existing file is **never deleted first**. If the rename fails, the
   original is untouched, the temp is cleaned up, and an error is returned.

All writes go through `AppState::persist`, which holds a single writer
mutex across the config clone + save, so concurrent commands serialize
instead of interleaving. Each successful persist bumps
`persisted_revision`; failures are recorded (`persist_error`,
`persist_error_kind`) and surfaced to the UI with a retry action — the
physical input state is never rolled back because the disk write failed.

## ADR: settings migration and activation policy (HF-06)

- `AppConfig::migrate` runs at load: older versions normalize to the
  current schema (sequential per-version arms can be added later);
  **newer versions are never silently downgraded** — the config is kept
  read-only in memory (`future_version`), and `persist` refuses to
  overwrite the file until the app is updated.
- `AppConfig::normalize` repairs a current-version file: profiles exist,
  profile ids are unique, the Default profile is restored if missing,
  per-profile layouts are validated, unknown key ids are dropped, and the
  selected profile exists.
- Activation invariant: `selected_layout == current_profile.layout`,
  enforced by the single `AppConfig::activate_profile` policy used by
  select / duplicate / create / delete-fallback / emergency unlock.

## Rollback policy (HF-03)

- Normal edits apply optimistically to runtime + in-memory config, then
  persist. On persist failure the applied state stays (it is the truth the
  user sees) and the UI shows an error banner with Retry and Enable-All
  actions; the unsaved state persists across hydrates until a retry
  succeeds. Restart loads the last good file from disk.
- Emergency unlock and Enable-All are never cancelled by a persist
  failure: physical input recovery is unconditional (HF-01 contract).
