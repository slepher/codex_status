# Note4 96px template status — 2026-09-24

## Completed

- Generated and registered Note4-only built-in `ntreg96` font; 200×200 target validation still rejects it. All usage numbers in the 400×300 `codex-status-a` variant use it, including dual-bucket and weekly-only layouts.
- Weekly-only value and reset time are centered. Visible `RESET` labels were removed while the reset times remain. Bluetooth, Wi-Fi and Bridge connection icons show their state, and a `zzz` icon shows deep mode.
- Compiled-template ABI is 2, with 64 operation and 16 bitmap-resource limits. This template uses 57 operations and 16 resources. Existing ABI1 devices remain accepted for Bridge discovery/data, while publishing requires an exact ABI match.
- Six host previews were checked at 400×300. JSON-render versus compiled-render and compiled serialization round trip pixel differences were all zero. The Bridge template library now holds source CRC `11455c65` and compiled CRC `442007ea`; MCP save reports `published=false` until explicit publication. The later full-Bundle publish `4905c76c` was acknowledged as `displayed` by Note4.
- The Bridge UI now receives `templates-changed` after an MCP v2 template save and uses its cached usage envelope for previews without an explicit test envelope. This fixes stale or quota-empty Bridge previews after a save.

## Limit

- Device status confirms rendering and data ACK. The user separately observed a full screen refresh on clock updates; this is recorded in the live-sync status and is left unchanged by request.
