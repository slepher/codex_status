# Live Template Delivery

## Goal and baseline

Deliver real Codex data through Rust, enlarge the quad layout, and demonstrate template replacement without changing ROM after one capability upgrade. Baseline HEAD `0fd3f03`, initially clean. Device COM4 / 192.168.1.50 / 0.4.0-bw. Rust workspace baseline 9 tests passed. Outside the sandbox, existing bridge-core reads real prolite usage; snapshot codex weekly used82, codex has no5h, Spark has a separate5h. Device serial confirms WIFI rendered wk82. Values are snapshots, not acceptance constants.

## Constraints

- Wi-Fi normal channel; BLE fallback. Local hash controls304. Verify hash, structure and min_fw before template replacement.
- Template success needs device acknowledgements. Preserve NVS, bonds, storage, partition map and factory backup. No erase or factory restore.
- Fixed codex bucket; never mix Spark windows or infer unlimited from missing. Missing displays `--`.
- Quad blocks approximately90px tall, compact one-line date/time, plan top-right. Full/mini remain available.
- Minimal template extensions with firmware/Rust validator parity. After one final firmware OTA, layouts update through template data.
- No Tauri redesign/installer/security redesign. Full refresh retained pending separate contrast investigation.
- Sequential coding self-test → separate runner verification → Sol review/rework → dispatcher commit per task.
- Native roles per delegate skill; user explicitly accepts declared role configuration despite unavailable runtime model/effort metadata (2026-09-16).

## Priority revision: ROM first (user request)

First deliver firmware0.4.1-bw to test WiFi/BLE after USB unplug. Task1 is narrowed to firmware HTTP/BLE corrections, compile/review/commit, then dispatcher OTA and live wireless verification. Sender fragmentation/ACK enhancements move to the later capability work. Existing full/mini templates can be replaced under this ROM; enlarged quad capability remains pending. This supersedes the earlier single-install sequence; final goal remains styles change without firmware changes once capabilities are installed.

## task-1: Reliable firmware transport

Only firmware files in revised task-1.md. Correct local conditional hash, shared validation before storage, BLE string byte serialization and receive resets. Version0.4.1-bw, accurate B/W model. No display/schema extension, host source changes, or partition changes. Compile and independent review before dispatcher installs.

## Deferred sender work (include in later capability tasks)

Owned: firmware HTTP fetch/shared acceptance/BLE receive buffers; Rust BLE; Python test bridge; focused tests. Fix conditional local hash, reject invalid downloads before store, fragment JSON writes, reset partial receives, wait for positive template ACKs and propagate failures. No schema/layout changes or flashing. Verify firmware build, Rust workspace and Python transport tests, diff hygiene. Complete after both test layers and review; commit `Fix template sync and BLE transfer reliability`.

## task-2: Quad template capability and asset

Prerequisite task1 committed. Owned: firmware template engine/environment/main integration, Rust validator, quad template/preview/tests. Add bounded scaling and alignment/fit, battery binding and required date display; explicit codex weekly/5h, missing--. Blocks approximately90px high, layout controlled by JSON. Preserve old template compatibility. Include coherent refresh triggers for displayed local values and retained latest usage. Verify compile, Rust parity/reject tests, deterministic preview bounds and missing-window fixtures. Complete both test layers/review and commit.

## task-3: Repeatable runtime and hardware acceptance

Prerequisite task2 committed. Owned: narrow startup/status tooling or operational docs, hardware evidence and PROGRESS.md. Build final binaries; start Rust with correct CLI and template path; report process/HTTP/data freshness/templates honestly. One web OTA, no erase. Verify first HTTP install, valid conditional304, acknowledged BLE usage/template delivery, template changes under unchanged ROM, BOOT cycling and persistence. Record actual live values/time, serial, firmware SHA256 and photos. Camera may be insufficient for small text; distinguish visual evidence from logs. Do not claim manual BOOT verification without evidence. Complete tests/review/commit, leave real Rust bridge running.

## Evidence and recovery

Baseline runtime artifacts are ignored files under artifacts/: rust-live.*, rust-live-usage.json, rust-device-first-sync.log, device-before-update.html. Runtime PID initially36828; always recheck identity before stopping. Factory backup SHA256 EE024E3E2D73E5CF80AEB72245025E4CD41391BC06F0D9C4DC5F91FE727138EC. Device status page requires direct LAN access (-NoProxy). Existing old PNG initiative is complete and unrelated.

## Live gate rework
Task1 manualpair succeeded but BLE template END overflowed nimble_host stack. Complete bounded0.4.2 hotfix in task1 before battery handoff or task2; retain completed pairing and HTTP evidence.

## Task2 user clarification
Weekly always present. Codex5h missing means infinity glyph and hidden5hresetline; present means quotaandreset. Generic conditionalvisibility inJSON, no crossbucketfallback. Supersedes prior missing5h--constraint.

## Latest user display revision
AbsentCodex5h displays100 instead ofinfinity; keep5hresetlinehidden. Present5hrealremaining/reset, weeklynormal. Task2 latestaddendum authoritative.
