# Task 9 — Stage 6: staged enablement, measurements, dual-phase experiment

Owner: implementation + measurement (rollout; no new protocol surface).

Inputs: design §9 (power model/measurement plan), §10 (faults/compat),
§12.6, §13 (acceptance matrix, photo metrics, wireless admission gate).

## Deliverables

1. **Staged enablement**
   - Keep `ble_rendezvous_v2` opt-in per device; the legacy 60 s Wi-Fi pull
     remains the default until the gate passes. Enable on one device first,
     with BOOT-based legacy recovery verified *before* the switch is treated
     as safe.
   - Rollback triggers honored automatically or by the operator: persistent
     ghosting, baseline drift, non-atomic transaction, owner/token
     regressions, missed-rendezvous rate, PM-lock leak. Black-tile partial
     refresh and the dual-phase experiment can each be disabled independently
     (module switches), and the whole transport can fall back to legacy while
     keeping diagnostic evidence.

2. **Measurements (design §9)**
   - Battery-side integration with USB disconnected; record at least 30
     samples per event class: pure deep floor, clock only, advertisement
     1/1.5/2/5 s, encrypted reconnect, same-revision ACK, 512/2048 B update,
     failure timeout, WAKE_LIGHT association, BOOT 300 s, black-tile partial
     / full / dual-phase.
   - 24 h same-scenario run for the release decision; report error bars,
     full-refresh counts, and missed-rendezvous ratio. Improvement inside the
     measurement error is not claimed as a win; `q_*` and deep floor stay
     explicit unknowns until measured.

3. **Wireless/Windows admission (design §13)**
   - ≥1000 opportunities on a stable desk setup: first-opportunity success
     ≥95%, two-opportunity cumulative ≥99%, with RSSI/adapter/PC state
     recorded; weak results tune the scan/1–2 s window first (5 s is a
     comparison only). All no-command/invalid-traffic tests must close the
     radio within the hard cutoff (100 ms clock tolerance).

4. **Photo acceptance + dual-phase gate (design §8.3/§13)**
   - Fixed-rig photos with the full-refresh reference; ghost metric normalized
     to the same-run dynamic range (old-stroke residual ≤0.05 D, black-tile
     lightening ≤0.05 D). Failure demotes the region to full refresh.
   - The dual-phase (white-clean-then-write) experiment is last, behind its
     own switch, and only after the conservative full-refresh path ships;
     it must never block the conservative release.

5. **Documentation/closeout**
   - `PROGRESS.md` records the ROM hashes, measured tables, the default-mode
     decision, and every rollback that was exercised; the design §14 open
     questions are updated with the measured answers (RTC capacity, Windows
     discovery latency, low-temperature budget, real capacity, ACK
     durability).

## Validation

- All stage-specific tests in §13 pass or are explicitly deferred with the
  reason; `git diff --check`; the template triple-hash tests stay green.
- The final default is only switched after the power, discovery, and photo
  gates pass; otherwise the legacy transport stays.

## Rollback

Per-device config back to `legacy_wifi_pull`, black-tile partial and
dual-phase switches off; ROM rollback to `artifacts/codex-status-0.15.2-bw.bin`
remains valid throughout (GATT unchanged).
