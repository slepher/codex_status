# AP01 firmware/application boundary

2026-10-22. User requests preserving implemented features while moving OTA, slot selection, Wi-Fi menu and switching into cuktech-ap01-firmware. codex_status owns only Bridge pairing/protocol and template application/display; native platform services expose narrow callbacks. No device writes, deployment, commit or push in this task.

Extract maintenance authentication/router/lifecycle and native flash/slot/network executor from Ap01Application/Runtime. Keep pairing and binding resolution in App, passed as callbacks to firmware. Move shared HTTP transport, maintenance tests and ROM menu packaging under firmware ownership; main build/test tools may orchestrate shared modules. Preserve routes, code rotation/rate limit, Bridge fallback, transaction receipts/replay, boot recovery, nonactive slot checks, menu behavior and saved-list formats. Wi-Fi menu acceptance remains backlog C9; this refactor must not mark it accepted.

Validate host sanitizer App + firmware integration tests and switch-core tests; RV32 XIP audit/link/package; architecture ownership check; native ROM maintenance and App smoke where supported. Record exact candidate identity and limitations, deployed06/rescue unchanged.

User follow-up: commit and push both repositories; do not flash any device. Companion firmware revision41541a2.
