# Note4 panel power switch — 2026-09-24

- Scope: Note4 firmware only. Bridge multi-device work is independent.
- `pm/panel_pwr` is persisted in NVS. `keep` is the default and active choice; `off_cache` retains the rail-off experiment. Token-gated `POST /diag?panel_power=keep|off_cache`, USB serial `panelpower keep|off_cache`, and `/status.json.panel_power_mode` provide write and read paths for a future configuration UI.
- Both choices turn off the SSD2683 internal high-voltage supply between refreshes. Deep-sleep GPIO6 hold/release follows the chosen rail level.
- The displayed 15 KB frame is saved to LittleFS once on entry to deep sleep, with a frame hash mirrored in RTC memory. On deep wake, the cache is verified and overlaid with RTC clock-window pixels before seeding the driver shadow. Thin minute wakes do not write flash. Invalid/missing cache leaves the existing full-refresh fallback in control.
- Note4 B final build passed as `0.18.21-note4-b`. Candidate ROM: `artifacts/codex-status-0.18.21-note4-b-panel-modes.bin`, 1,742,144 B, SHA256 `917FAFADA6886A797FC9C2390990286A90E8DB173BF1DF1A50E3778879C9FD6A`.
- 1.54 regression build passed after Luna removed the single zero-byte generated `dl_base_sub2d.cpp.o` left by the interrupted build. Full logs: `artifacts/panel-154-retry-build.log` and `artifacts/note4-panel-final-build.log`.
- Pending: bench measurements for current draw, minute partial waveform, ghosting, and repeated button/timer wakes. No firmware has been flashed.
