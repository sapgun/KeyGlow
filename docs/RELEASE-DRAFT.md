# Release draft — v0.1.1 (candidate)

> DRAFT ONLY. Do not publish. Fill in the TBD fields from a real Windows
> build (CI `installer` job artifacts) and real device testing before any
> public release. See the release gate in `.github/workflows/ci.yml`.

## Version

- Version: 0.1.1
- Source SHA: TBD (the merge commit of PRs #8, #9, #10 onto main)
- Branch state at draft time:
  - HF-01: `codex/hf01-emergency-unlock-safety` → PR #8 (DRAFT)
  - HF-02/03/06: `codex/hf02-settings-atomicity` → PR #9 (DRAFT, stacked on #8)
  - HF-04/05: `codex/hf04-ui-state-accuracy` → PR #10 (DRAFT, stacked on #9)
- Version consistency at draft time: package.json 0.1.0 / Cargo.toml 0.1.0 /
  tauri.conf.json 0.1.0 (all match; bump to 0.1.1 together at release).

## What changed (by hotfix)

- HF-01: emergency unlock no longer rides the lossy UI event queue. A
  safety epoch + dedicated worker converges config/UI/tray even when the
  event channel is saturated. Stale profile/key commands after an unlock
  are discarded instead of re-locking input.
- HF-02: settings are written to a unique temp file (same volume),
  synced, and atomically renamed over the target. The original file is
  never deleted first; a failed replace keeps the last good settings.
  Concurrent saves serialize through a single writer.
- HF-03: commands distinguish "applied in memory" from "persisted to
  disk". Save failures surface as a translated error banner (EN/KO/JA)
  with retry; emergency/enable-all are never cancelled by a disk failure.
- HF-04: the 8-second forced key release is gone. Long-held keys show a
  "verifying" state and re-check the native pressed snapshot instead of
  visually releasing a physically held key.
- HF-05: command revisions guard against out-of-order responses; rapid
  toggles always converge on the last action. Event-subscription failures
  clean up already-registered listeners.
- HF-06: profile activation keeps `selected_layout` in sync;
  versioned config migration with non-destructive handling of future
  versions; collision-resistant profile IDs.

## Binaries

| Asset | SHA256 |
|---|---|
| `KeyGlow_0.1.1_x64-setup.exe` (NSIS, per-user) | TBD — from CI `installer` job |
| `keyglow.exe` (portable) | TBD — from CI `installer` job |

## Code signing

- None. Binaries are unsigned. (Stated truthfully; do not imply otherwise.)

## Upgrade notes

- Settings format unchanged. An existing v0.1.0 `settings.json` is
  preserved on upgrade and auto-repaired by migration if needed.
- No account, no network, no telemetry — same local-first contract.

## Not verified at draft time

- Windows compilation of the full Rust crate on the release branch.
- `cargo test` on Windows.
- Device tests 7b–7e (HF-01), 10/11/12 (HF-02/03/06), 13–16 (HF-04/05),
  17–18 (install/upgrade) in `docs/TEST-CHECKLIST.md`.
- Any OS not explicitly listed as tested: do not claim it as verified.

## Release checklist (all must be true before publishing)

1. PRs #8, #9, #10 merged to main in order (rebase the stack first).
2. CI green on main: version-check, build, installer, release-gate.
3. Device evidence recorded for every checklist item the release touches.
4. SHA256 table above filled from the CI artifacts of the release commit.
5. Explicit human approval to publish.
