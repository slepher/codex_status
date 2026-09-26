# Bridge independent lifecycle

## Problem

On 2026-09-27, a bridge launched by `tools/start-bridge.ps1` was confirmed listening with its watchdog, but both exited when Codex closed. The watchdog log has no new abnormal exit entry. `UseShellExecute=true` detached standard I/O but did not establish an independent process lifetime.

## Plan

1. Launch the default bridge through an on-demand Windows Scheduled Task under the current interactive user, with no login trigger or time limit. Task Scheduler, rather than the Codex process tree, starts the executable.
2. Keep the existing duplicate-instance and port checks, and make the default script reuse the task on later starts. Preserve named-instance behavior unless independently validated.
3. Start the task, verify main process, watchdog, HTTP/MCP listeners, task state, and process parent. Record operational evidence and limits in `PROGRESS.md`.

## Boundary

Do not rebuild the executable, modify runtime data, stop unrelated processes, or enable login autostart. Closing Codex must be checked by the user after this turn; process ancestry and Task Scheduler ownership are the in-session evidence.
