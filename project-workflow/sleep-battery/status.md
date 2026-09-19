# Project Workflow Status

## Initiative
- Artifact directory: project-workflow/sleep-battery
- Repository: codex_status
- Updated: 2026-09-17 ~21:05 Asia/Hong_Kong
- Plan revision: 2 (task-6 done; task-7 M3 spike in progress)

## Repository snapshot
- HEAD: de5c09d (`Replace the handover prompt with the task-7 M3 execution brief`)
- Worktree (uncommitted): `platformio.ini` (pm env), `.gitignore` (pioarduino
  droppings), `src/ble_bridge.cpp` (NimBLE 1.x/2.x dual build),
  `src/main.cpp` (PM light sleep, GPIO sleep retention, `pm_light_sleep`)
- Expected paths: platformio.ini, .gitignore, src/main.cpp, src/ble_bridge.cpp,
  project-workflow/sleep-battery/*

## Progress
- Current task: task-7 — M3 custom core (pioarduino) + PM light sleep (T10)
- Current phase: build spike. Env + code landed; build now passes the
  whitespace check and CMake configure; the Arduino-libs recompile has not yet
  completed (last attempt was killed after the GBK log-reader hang, see below)
- Latest boundary: stock env (0.10.3, rollback) builds and links clean from the
  modified tree; device still runs 0.10.3-bw on ota_1
- Exact next action: rerun `pio run -e esp32-s3-epaper-154g-pm` detached, cwd =
  junction (see below), **without** `-v`; then flash/OTA and run T10/T9/T11
- Blocker: none hard, but the pm env only builds from a space-free cwd

## Build environment (new constraint)
- Repo path `...\codex status` contains a space; pioarduino's
  `custom_sdkconfig` flow rejects it (`espidf.py:2629`).
- Workaround: junction `C:\Users\user\AppData\Local\Temp\opencode\codex-status`
  → repo root, and start the build process with that junction as its real cwd
  (detached `System.Diagnostics.ProcessStartInfo`). Shell `workdir`/
  `Set-Location` resolve to the real path and still fail. A subst/drive root
  fails too because `os.path.basename("P:\\")` is empty and pioarduino writes
  `project()` into the generated CMakeLists.
- `pio run -v` hangs the build on the GBK console (UnicodeEncodeError in the
  output reader thread): do not use `-v` for the pm env.
- SCons 4.11.1 is installed on demand into
  `~/.platformio/packages/tool-scons/scons-local-4.11.1`; a short missing
  window produces `No module named 'SCons.Tool.FortranCommon'` — rerun fixes it.

## Active task evidence
- `pio run -e esp32-s3-epaper-154g` → SUCCESS (337 s) after the source changes,
  so the shared source stays compatible with the stock NimBLE 1.4.3 core.
- pm run reached LDF (“Found 46 compatible libraries”) — the path/CMake gate is
  passed; the libs recompile is the remaining long pole.
- Logs: `artifacts/pm-build.log`, `artifacts/pm-build-v.log` (gitignored).

## Commit
- State: uncommitted (per rule “no commit without an explicit user request”)
- Proposed subject: `Add pioarduino PM light sleep env; dual-build BLE for NimBLE 2.x`

## Continuity
- Next task: task-7 (resume at the libs recompile)
- Next Sol: none
- Reason: dispatcher executes directly per AGENTS.md
- Last child: none

## Constraints and pending limits
- Do not stop the running bridge (`bridge-app` PID 43768) or device services.
- Device: 0.10.3-bw on ota_1, LIVE on battery, ~30 % / 3.59 V and falling; PM
  light sleep is the mitigation, so land 0.11.0-bw early. ROM rollback:
  `artifacts/codex-status-0.10.3-bw.bin`
  (SHA256 C0AABDC117D88D8FB073306C27F0BAD5DB7967600A1ACFAF019B7EFE5780B61D).
- Keep template hash parity and preview pixel parity (templates untouched).
- Do not commit; do not write tokens/secrets anywhere.
