# Note4 live sync and clock status — 2026-09-24

## Completed

- Bridge Codex source was healthy. Note4 Profile had `sync_enabled=false` and four quota bindings to `static1`; saved the same Profile with those four bindings changed to `codex` and sync enabled. Note4 data ACK reached 16/16 after the final template publish.
- Bridge delivery now resolves the requested device MAC to its own saved IP. This prevents a Note4 delivery from using the globally selected 1.54 address. The 1.54 device remains separately registered.
- The common deep timer clock path now attempts a retained clock-window update and, if that is unavailable, renders locally from the installed template and cached usage before sleeping. It does not fetch Bridge data or advance the data sequence. The Note4 SSD2683 driver has the vendor OTP partial path with a valid full-frame baseline requirement; Note4 capability advertises partial refresh.
- Authenticated one-shot OTA installed `0.18.20-note4-b` in `ota_0`, retaining the installed Bundle and job. ROM: `artifacts/codex-status-0.18.20-note4-b-abi2.bin`, SHA256 `DDA7F214127814FC93B03B06D4C50818D77847CCCDF18938701E12C69C3F094B`; evidence: `artifacts/note4-ota-01820-b-abi2.json`.

## Checks and open issue

- Note4 B PlatformIO build and the Bridge focused tests passed; Bridge data source was `good`. The device reported ABI2, `partial=true`, new Bundle job `4905c76c`, `display_state=displayed`, and data sequence/applied sequence 16/16.
- User observed that clock updates currently perform a full screen refresh. **Record only; do not adjust this behavior in this task.** Deep sleep cuts panel power and can invalidate the partial baseline, so the local full-render fallback is one plausible cause; this observation was not isolated to a specific wake path.
- 1.54 was still registered and ABI1 capability accepted for data/discovery, but its Bridge coordinator showed a pending/full-sync-due state during the final check; it was not used as evidence for Note4 delivery.
