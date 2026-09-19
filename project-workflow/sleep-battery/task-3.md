# task-3 — M2 firmware DEEP windows (firmware 0.10.0)

Status: implemented + OTA'd 2026-09-17; device acceptance (T1–T8) pending.
Owned: `src/main.cpp`, `src/{bridge_store,template_store}.{h,cpp}`,
`src/EPD_SSD1681.*` (M1), `src/template_engine.*` (task-2).

## Implemented

- Window model (`docs/history/sleep-plan-v4.md §4.2`): every boot is a window; Wi-Fi scan + best
  saved slot (last-used preference, single 9 s attempt), fetch via active
  endpoint when fresh else same-BSSID MRU first (2 s timeout), BLE advertising
  in parallel; window end always deep-sleeps (`finishWindowAndSleep`), fetch
  success or not.
- `RTC_DATA_ATTR`: activeMac/activeAt, fail count, accelerated-window counter,
  idle reason, BSSID hash, usage-cache fingerprint.
- Backoff: 60 s first 3 windows, then 300 s; 3 consecutive failures → 900 s;
  success resets; network change restores accelerated windows.
- IDLE policy: 2 consecutive failed windows or never synced → render the idle
  template (`mode:idle`) from the cached usage; empty cache → built-in default
  screen (CODEX STATUS / IDLE - NO LINK / BATT / IP / SYNC --:-- / FW).
- Usage cache: NVS `ucache/json` (fingerprint-deduped, <4000 B), adopted
  `server_time` when the clock is unset.
- `active` rules (`§2`): explicit `"activate": true`; same bridge; T=600 s
  (`active_hold_seconds`); active endpoint BSSID mismatch.
- Envelope: `idle_template` pins a store id (eviction-protected, persisted in
  `tpl/index.json`), `active_hold_seconds` overrides T.
- AP policy D10: AP only when no saved slots or BOOT held 5 s; 5 min idle
  timeout; no automatic AP fallback.
- Wake render skips `EPD_SSD1681_Clear()` (M1 sleep/wake already in place).
- Kept for field recovery (beyond the spec, documented deviation): BLE pairing
  window, a live BLE peer, and an OTA upload hold the window open (bounded
  10 min); issuing an OTA token holds it 10 min so the existing token-gated OTA
  remains usable in DEEP.
- `/status.json` now reports `mode`, `idle_reason`, `active_mac`, `active_at`,
  `fail_count`, `window_synced`.

## Device evidence so far

- OTA 0.10.0-bw `UPDATE OK`; `/status.json` shows the new fields
  (`idle_reason: boot`, `fail_count: 0`) and the device enters deep sleep
  between windows (subsequent probes only succeed during a window).
- ROM `artifacts/codex-status-0.10.0-bw.bin` SHA256
  `6CA11D955363ED71B9012FF7F8397163FB47814C33FA8A840AA7008D5E703FDF`.

## Remaining acceptance

T1 battery overnight, T2 window fetch timing with the bridge running, T3
no-growth across windows, T4 network revisit, T5 active rules, T6 bridge
restart, T7 IDLE template (needs quad v4 pushed), T8 three-way hash + previews
(previews already verified; device hash pending push).
