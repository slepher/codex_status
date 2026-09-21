# Task 1 — complete target-design document

Owner: Astra documentation subagent (exclusive ownership of
`docs/ble-rendezvous-power-design.md`).

Inputs:

- `AGENTS.md`
- latest section of `PROGRESS.md`
- `docs/power-state.md`, especially §13
- `src/main.cpp` refresh policy and deep/light state transitions
- `src/EPD_SSD1681.cpp/.h`
- bridge BLE/activity implementation where relevant
- accepted conversation decisions summarized in the task contract

Deliverable: `docs/ble-rendezvous-power-design.md`, an implementation-ready
Chinese design document. Do not edit firmware, bridge source, existing docs, or
project status files.

Validation: internally check state/command tables, timeout coverage, revision
and ACK semantics, refresh escalation rules, compatibility, telemetry, rollout,
and acceptance tests. Report exact files read and changed.
