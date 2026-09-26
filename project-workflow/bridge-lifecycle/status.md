# Status — 2026-09-27

The standard default bridge is now launched through the on-demand Windows Scheduled Task `CodexStatusBridge`. Task Scheduler reports Running; bridge PID 24568 has parent `svchost.exe` PID 2860, watchdog PID 2704, and owns HTTP 8765 / MCP 8766. `tools/start-bridge.ps1` syntax and `git diff --check` passed; an idempotent invocation reported PID 24568.

The remaining acceptance check is to close Codex and confirm the task and listeners remain active. The user observed that the previous `UseShellExecute=true` launch still exited with Codex; the watchdog had no corresponding abnormal exit entry.
