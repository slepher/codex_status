# Bridge multi-instance launch

## Contract

- Keep the default instance's data path, ports, tray square, and owner ID. The main worktree Bridge is started after this change; no old-instance compatibility or migration is required.
- A named instance uses its own process lock, data directory, HTTP/MCP ports, owner ID, and selectable tray background (`square`, `circle`, `diamond`). Reject invalid names, duplicate ports, and a second launch of the same instance.
- The launcher accepts instance, ports, icon shape, and optional exact device MAC/IP. It records a PID per instance and never treats another `bridge-app` process as its own.
- The watchdog inherits the instance settings on restart. Named instances cannot change the default instance's autostart registry entry.
- No device firmware or protocol change. Existing MAC, token, claim, and lease rules stay in force.

## Validation

- Build and test Bridge app without replacing the running executable.
- Launch a named instance on distinct ports with isolated data; check both listeners, process identity, tray icon, and duplicate launch. Leave the existing instance running.
- Do not claim, publish, or OTA a device as part of the launcher test.
