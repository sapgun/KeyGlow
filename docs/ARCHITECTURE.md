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
- Convergence protocol (P2): `SafetyState` separates election from completion.
  `claim_reconcile(epoch)` elects a converger (exactly once per epoch);
  the actual work runs under `convergence_lock()` (blocking) or
  `try_convergence_lock()` (never blocks; UI thread), and only then does
  `complete_reconcile(epoch)` advance the truthful completion marker
  (`reconciled_epoch()`). The worker loops on `pending_epoch()`, so an
  epoch elected-but-never-worked is never silently lost, and convergence
  work for different epochs never overlaps. See "ADR: safety convergence
  claim/completion (P2)" below.

## ADR: safety convergence claim/completion (P2)

**Defect (reproduced):** `claim_reconcile` advanced the completion marker
(`reconciled_epoch()`) before the config/controller work actually ran.
Claim unit tests passing did not prove that different epochs' full
convergence was serialized — and the worker's claim-then-skip pattern meant
an epoch elected-but-never-worked could be silently lost.

**Design:** `SafetyState` now carries three markers:

- `epoch` — bumped by the hook (one atomic increment).
- `claimed` — election marker, written by `claim_reconcile` (exactly-once).
- `completed` — truthful completion marker, written only by
  `complete_reconcile`, after the work, under the convergence lock.
  Monotonic: a late straggler never moves it backwards.

Plus `convergence: Mutex<()>` serializing the work across the safety
worker, the event-pump fallback, and the stale-command path:

- Worker (background thread): loops on `pending_epoch()`; takes the
  blocking lock; re-checks; claims; runs `emergency_unlock()`; completes;
  repeats if a newer epoch landed mid-work.
- Pump fallback (UI thread): only if `reconciled_epoch() < epoch`, and only
  via `try_convergence_lock()` — never blocks the UI. If the worker is
  mid-convergence, the pump skips; the worker picks up the latest epoch.
- Stale-command path (command pool): takes the blocking lock and forces
  re-convergence, then completes.

**Wake channel evaluation (P2):** the wake path is an unbounded
`std::mpsc::Sender<()>`. It never blocks the hook (no capacity to wait on),
but it is *not* allocation-free (one node allocation per `send`) and *not*
lock-free (brief internal queue lock). The worker's `try_recv` drain
coalesces rapid presses, so the queue stays ~empty at human press rates; a
failed `send` (worker gone) is ignored because the epoch latch plus the pump
fallback still converge. Measured on Linux (std mpsc, release): 2000 rapid
`trigger()+send()` calls cost p50 90ns / p99 230ns / max 6.7us per call —
negligible against the hook timeout, but the stress run also showed the
queue reaching depth 938 when the worker is slow, proving it is *not*
bounded. No change: the cost is negligible at human press rates and the
hook already does more (engine lock, key identification). Do not claim
allocation-free/lock-free/bounded.

## ADR: UI snapshot/event ordering (P3)

**Defect (reproduced in tests):** the local command counter (`cmdRev`)
could not order `hydrate()` or native events — they bypass it. A hydrate
requested before an emergency but resolved after it overwrote the safe
state with pre-emergency disabled keys; a `keyboard:state-changed` event
from the previous profile arriving after a profile switch overwrote the
new profile's state; and a pressed-snapshot response older than an
already-processed key-down dropped the physically held key.

**Design:** native issues one ordering stamp; the UI applies data only
when the stamp is not older than what is displayed.

Native (`AppState`):

- `safety.epoch()` — latest emergency recorded by the hook.
- `safety.reconciled_epoch()` — highest epoch fully converged (P2).
- `runtime_revision: AtomicU64` — bumped once per runtime-state mutation
  (every mutating command, emergency convergence), regardless of whether
  the change reached the disk.

`AppSnapshot` carries `safetyEpoch`, `safetyReconciled`, `runtimeRevision`.
State-affecting events carry the same stamp:

- `keyboard:state-changed` → `{ keys, safetyEpoch, runtimeRevision }`
- `keyboard:cat-lock` → `{ locked, safetyEpoch, runtimeRevision }`
- `keyboard:emergency-unlock` → `{ epoch, runtimeRevision }` — emitted
  only after the epoch fully converged, so it is authoritative truth.
- `keyboard:key-down` / `keyboard:key-up` → `{ code, seq }` — the
  engine's event sequence for that press.

UI rule (single rule, applied to command responses, hydrate snapshots,
and native events): apply only if
`(epoch, rev) >= (appliedEpoch, appliedRev)`, epoch compared first.
Otherwise discard. The emergency event floors the stamp to the converged
epoch, so anything older (a late hydrate, a stale response) is dropped
instead of overwriting the safe state.

Two revisions, never confused:

- `runtimeRevision` — orders runtime state for the UI. Bumped on every
  mutation, even when persist fails (emergency best-effort) or never
  happens (cat lock). Never compare against `configRevision`.
- `configRevision` (`persisted_revision`) — counts successful disk writes
  only. Answers "is the file in sync?", not "what is the newest state?".

Pressed keys live in a separate sequence domain: the UI keeps the last
native `seq` per key. A snapshot older than a processed key event must
not drop that key (order inversion); a newer snapshot governs. Missing
events are recovered by the next snapshot, which always carries a stamp
at least as new as any event it postdates.

Command responses for the three value-returning commands
(`set_key_enabled`, `enable_all_keys`, `set_cat_lock`) now return their
payload with the stamp (`{ keys, safetyEpoch, runtimeRevision }` /
`{ locked, safetyEpoch, runtimeRevision }`) and go through the same gate
— no separate "response is newer than the event" logic, no extra IPC.

**Not claimed:** physical 10-second hold display and dropped key-up
recovery are covered by automated ordering tests
(`scripts/test-store-contract.mjs`, run in CI via `npm test`), but the
real 10s hold and the real dropped key-up still need the physical rig
(Test 13/14) — automated and physical are recorded separately.

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

## Pressed-state sync (HF-04)

- The 8-second UI timer is NOT a release timer anymore. After 8s without a
  key-up, the UI marks the key stale (dashed pulse + "verifying hold" label)
  and queries the native pressed snapshot (`get_pressed_snapshot` IPC).
- The native side exposes `FilterEngine::pressed_keys()` (the hook's
  `physical_down` array) and a per-event `event_seq` sequence number through
  the `KeyboardController` trait. The snapshot is authoritative; the event
  channel stays lossy for glow hints.
- Native confirmation keeps the glow and re-arms the check; a native
  "released" answer clears it. A failed query keeps the glow and retries:
  the UI never drops a physically held key on suspicion.
- `resyncPressed()` reconciles on window focus, visibility change, and
  after emergency unlock — the moments key-up events are most likely lost.
- Repeat events are still not emitted by the engine (first-down pulse
  only); long holds are covered by the stale-check loop instead.

## Command revision guard (HF-05)

- Every mutating store command takes a monotonically increasing revision
  (`cmdRev`) when it starts and applies its response only if no newer
  command began meanwhile. Rapid double-clicks therefore always converge to
  the latest click; a late response can never overwrite fresher state.
- `showEmergency()` invalidates all in-flight commands, mirroring the
  native `discard_stale_command` at the UI layer: a stale toggle response
  can never undo an emergency unlock.
- Authoritative snapshots (`hydrate`, native `state-changed` events) bypass
  the guard — they are the truth, not a guess.
- `setLocale`/`setTheme`/`setAutostart` roll back to the previous value on
  failure (HF-03 pattern reused).
- Event subscription uses `Promise.allSettled`: a single failing listener
  no longer leaks the already-registered ones, and the failure is shown in
  a translated banner (HF-09 follow-up).

## ADR: Content Security Policy (deferred — HF-09)

`tauri.conf.json` ships with `csp: null`, meaning Tauri injects no CSP.
This is a deliberate deferral, not an oversight:

- The frontend (`index.html` + Vite production bundle) uses no inline
  scripts and no `innerHTML`/`dangerouslySetInnerHTML`, so a strict policy
  like `default-src 'self'` is *plausible*.
- But Tauri v2 IPC and the asset protocol have scheme requirements that
  can only be confirmed against a real Windows build. Applying a guessed
  CSP risks silently breaking IPC or the keyboard UI with no test catching
  it on this branch.
- Decision: keep `csp: null` until a Windows `tauri build` exists, then
  apply the minimal verified policy and smoke-test every IPC command
  (`get_pressed_snapshot`, profile CRUD, autostart, locale/theme) plus the
  Cat Lock and emergency flows before keeping it.

Candidate policy to verify (do not apply blind):
`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'`
(`unsafe-inline` for styles only, because Tailwind/runtime style
attributes may need it — confirm during the real-build verification and
tighten if possible).
