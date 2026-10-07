# Final stabilization candidate (2026-10-07)

Stack: #8 -> #9 -> #10 -> #11 -> #13 -> #14 -> #15 -> final-validation.
The final-validation branch carries #12's source-map-js 1.2.2 patch as well.
All PRs remain drafts. No merge, public release, or deployment is authorized.

## Candidate changes

- Source-map-js 1.2.2 is included in the cumulative candidate.
- Node support matches Vite/plugin-react: `^20.19.0 || >=22.12.0`.
  `>=20.19.0` alone would incorrectly admit unsupported Node 21 and 22.0.
- The real-store test loader uses the installed TypeScript compiler.
  Node 22.14 previously failed with ERR_UNKNOWN_FILE_EXTENSION even though
  the newer Node used in CI passed. No new dependency was added.
- CI additionally tests npm ci, real-store contracts, and frontend build
  on Windows with minimum supported Node 20.19.0 and 22.12.0.
- App/installer version remains 0.1.0; do not label it 0.1.1.

## Automated evidence

Local Windows: Node 22.14.0; full Rust MSVC tests: 62 passed; layout
validation: five layouts; real-store/IPC contracts: 28 assertions passed;
frontend build passed. npm ci reports zero vulnerabilities after #12.
Record final source SHA, CI run URLs, artifact checksums, and any later
runtime evidence in PROJECTS/KeyGlow/01_검증_릴리스_증거.

## Physical and installation protocol

Do not overwrite a daily-use installation or settings to run this protocol.
Use a disposable Windows account/VM for fresh install, corruption, autostart,
and upgrade tests. Back up settings before any approved existing-account test.
Record candidate SHA/hash, OS build, expected/observed behavior, and PASS/FAIL
for each item; unchecked items remain NOT RUN.

| Input test | Expected result | Status |
|---|---|---|
| A disabled/enabled in Notepad | Only disabled A is filtered | NOT RUN |
| Win/CapsLock filtering | Selected keys filtered; restore works | NOT RUN |
| Ctrl+Shift+F12 including disabled F12 | All keys enabled, Cat Lock off | NOT RUN |
| Emergency during delayed UI/saturated event queue | Physical input restored, state converges | NOT RUN |
| Stale profile command after emergency | Cannot restore pre-emergency lock | NOT RUN |
| Cat Lock and emergency escape | Input blocked then restored | NOT RUN |
| Hold key for 12 seconds | Glow retained while physically held | NOT RUN |
| Lost key-up/focus return | Native snapshot repairs pressed display | NOT RUN |
| Disable modifier while held | Matching release forwarded; no stuck modifier | NOT RUN |
| Immediate tray Exit repeated ten times | Process exits; input usable | NOT RUN |
| Sleep/resume and unexpected hook-thread failure | Recovery/status follows documented contract | NOT RUN |

| Installation test | Expected result | Status |
|---|---|---|
| Clean per-user NSIS install | App/tray open, filtering works | NOT RUN |
| Portable execution | App opens, expected config location | NOT RUN |
| Upgrade from v0.1.0 | Profiles/layout/locale/theme preserved | NOT RUN |
| Autostart enable/disable, sign-out/in | Setting and actual startup agree | NOT RUN |
| Corrupt/future-version configuration | Backup/read-only protection; usable input | NOT RUN |
| Uninstall and force exit | No remaining hook or unwanted startup entry | NOT RUN |

Same-version 0.1.0 reinstall does not prove a 0.1.1 upgrade. Re-run the
upgrade test after an approved coordinated version bump.

## CSP gate

`csp: null` is still deferred. A successful installer build does not verify
WebView CSP behavior. On an isolated test candidate, apply the ADR policy,
then observe main window, keyboard assets, EN/KO/JA, theme changes, IPC,
profiles, emergency status, and browser console violations. Include Tauri
IPC/asset origins required by the actual production WebView. If any check
fails, keep the release blocked and repair the policy; do not mark this
gate passed from a static scan. Native UI observation is required.

F-stage features remain outside this stabilization task until device,
installation, CSP, and final integration review gates are cleared.
