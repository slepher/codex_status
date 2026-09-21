# BLE rendezvous power design — plan

## Objective

Produce an implementation-ready design for replacing the deep-mode per-minute
Wi-Fi pull with a short BLE rendezvous. The bridge may choose no-op, direct BLE
data delivery, or a leased Wi-Fi light-sleep session. The design must also
define a ghosting-safe partial-refresh policy for the large black usage blocks.

## Scope

1. Reconcile the proposal with `docs/power-state.md` and the current SSD1681
   refresh implementation.
2. Write one complete target-design document under `docs/`.
3. Review the document for protocol completeness, state-machine termination,
   power accounting, compatibility, failure recovery, and testability.

## Non-goals

- No firmware or bridge implementation in this milestone.
- No GATT table change is selected without explicitly documenting the Windows
  attribute-cache cost and a reuse-first alternative.
- No universal BLE discovery latency or energy claim without measured data.

## Additional requirement (2026-09-21, revised): silent BOOT wake

Any wake from deep sleep (BOOT/PWR, or a retry-timer wake while the panel
still shows the sleep image) must not show the full-screen `Connecting: <SSID>`
page. The panel leaves the sleep frame immediately: Zzz is removed before
association, and the Wi-Fi icon appears only after the connection succeeds
(template `device.state` condition). Entering deep draws Zzz again and hides
the Wi-Fi icon. If the battery connect fails or times out, the sleep frame is
restored and the device returns to deep within the bounded timeout. The
`Connecting:` page remains allowed on cold boot and explicit setup paths. This
does not need the v2 BLE protocol: it is implemented ahead of it as a
firmware + template change (task-2, revised by task-3).

## Acceptance

- Every wake path has a bounded timeout and deterministic return to deep sleep.
- BOOT wake defaults to a 300-second light lease; only explicit bridge commands
  renew or terminate it.
- A wake from deep draws no Wi-Fi connecting page; Zzz is removed by the first
  wake render (full-refresh baseline) and the Wi-Fi icon follows the actual
  link state: hidden while associating, shown after connect, hidden again in
  deep.
- A failed/timed-out battery connect restores the sleep frame and still
  returns to deep within the bounded timeout.
- Direct BLE update is atomic, idempotent, acknowledged, and does not start
  Wi-Fi or renew a light lease.
- The large-black-block policy covers dirty-region detection, black/white
  polarity changes, partial/full escalation, ghost counters, and recovery.
- Current behavior, proposed behavior, assumptions, and measurements are
  clearly distinguished.
