# Project Workflow Status

## Initiative
- Artifact directory: project-workflow/live-template-delivery
- Repository: codex_status
- Updated: 2026-09-16 Asia/Hong_Kong (late evening)
- Plan revision: 4 (task2 quad template; authoritative missing5h 100 rule)

## Repository snapshot
- HEAD: 2bfa034 (auth) + docs update pending
- Worktree: PROGRESS/status edits pending commit; source clean otherwise
- Expected paths: PROGRESS.md, project-workflow/live-template-delivery/status.md
- Unexpected paths: none

## Progress
- Current task: task-3 (runtime/hardware acceptance) — closed
- Current phase: closing
- Latest boundary: 0.7.0-bw running: quad template active, partial refresh enabled, BLE-negotiated token gates Wi-Fi operations; end-to-end verified
- Exact next action: commit docs; optional BOOT-cycle manual check
- Blocker: none

## Active task evidence
- Device: 0.7.0-bw, 192.168.1.50, templates 3, active quad 4827fea3, running ota_0 (next ota_1), reset reason software, EPD partial ready
- Bridge: PID 43156 hidden via artifacts/start-rust-live.cmd; BLE push verified with positive ACKs (artifacts/ble-0.7.0-final.log)
- ROM artifact: artifacts/codex-status-0.7.0-bw.bin SHA256 F8CC10E0CC679537B2A7A3ED233266B36D334A718846B7FA7C9A088A22F4CC5D
- Auth evidence: tools/device-auth/request_token.py negotiated token (TTL 3600s); /update?token= 200, Bearer 200, bad token 401, /update without token 401, dummy /doUpdate 401
- OTA evidence: authorized OTA returned UPDATE OK (deferred reboot); unauthorized checks after reboot still 401

## Commit
- State: firmware/tool commits done (8ffc304, 812170a, 2fb5d95, 2bfa034); docs uncommitted
- Expected subject: Document partial refresh and BLE token gating
- Latest committed checkpoint: 2bfa034 Gate Wi-Fi operations behind a BLE-negotiated token

## Continuity
- Next task: none planned; optional BOOT-cycle manual check and partial-refresh contrast observation
- Next Sol: none
- Reason: user-directed scope closed; dispatcher executed directly
- Last child: none

## Completed boundaries
- task1 e89b0b1/f5e8e2e/0a30c58 immutable.
- task2 committed 62bd15e; display revision 6c39ead.
- task3 acceptance: 0.5.0 OTA, BLE quad ACK, JSON-only demo, username label, zero reset credits.
- Post-task3 user features: partial refresh (0.6.0/0.6.1, adaptive full on >12.5% change or every 30 partials), OTA diagnostics and deferred reboot (0.6.2), BLE-token-gated Wi-Fi operations + ArduinoOTA password randomization (0.7.0), token tool.
- Windows GATT cache after adding a characteristic required one manual unpair/re-pair; recovered and verified.

## Constraints and pending limits
- User authorized declared role configs; dispatcher executed directly, no subagents.
- BOOT short-press template cycling not manually verified.
- Any future GATT table change needs Windows re-pair (or a Service Changed mechanism).
- Full refresh still occurs periodically (every 30 partials) by design.
