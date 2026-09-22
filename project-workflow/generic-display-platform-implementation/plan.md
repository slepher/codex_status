# Generic display platform v2 — implementation plan

Authority: `docs/generic-display-platform-design-v2.md` (target design),
`docs/ble-rendezvous-power-design.md` (still-applicable low-level evidence),

`AGENTS.md` (security/compat invariants). Supersedes the v1 model entirely.

Goal: turn Codex Status into a working generic multi-device e-paper platform:
Codex is one DataSource; the Bridge owns sources, per-device Profile/coordination,
push/pull decisions, Bundle freeze and PowerPlan; devices execute bounded commands,
compiled templates, framebuffer diff and panel safety limits. One shared
application service backs both the four-page UI and MCP.

## Non-goals (v2 §14)

No template revision/history or rollback UI, no Deployment/ReleaseIntent/
ReleaseArtifact, no ProviderType→SourceInstance→Dataset layering, no
content-addressed resource graph/GC, no multi-stage release events, no enabled
subset / separate quick-cycle capacity, no device short/long checks, no implicit
data-packet lease renewal, no scripting plugins, no patch/event data protocol.

## Stages

| Stage | Deliverable | Acceptance |
|---|---|---|
| M0 | Regression baseline: 3-way canonical hash, whole-template rejection, host/firmware pixel parity, ASCII, Codex display rules, token/claim/owner/MAC, save≠push, legacy fixture tests | Existing tests green; legacy interop fixture test added |
| M1 | Bridge domain: DataSource+SourceSnapshot (Codex+Static), Template, per-device Profile 1–8, CompiledTemplate, Bundle, single PublishJob, data snapshot (push/pull), PowerPlan; shared application service | Unit/integration tests for the §21 truth table (bridge side) |
| M2 | CompiledTemplate compile/serialize/validate, ABI + bounds; no JSON parse on wake/render/switch; host+firmware same render semantics | Host parity test compiled-vs-JSON 0 pixel diff; 8-template compile peak test |
| M3 | Full Bundle + A/B slots + one active_context_id + data_seq idempotency + simple ACK | Power-loss stage tests, A→B→A rejection, old-seq rejection, dup-seq idempotent |
| M4 | Rendezvous/PowerPlan: device state machine (timer/manual BOOT 300 s provisional), Bridge NOOP/sleep/BLE Data/light/Bundle/activate decisions, plan idempotency, no implicit renewal, PM lock release | Host-simulated state machine tests + on-device verification |
| M5 | Refresh: framebuffer diff, dirty rect alignment, ghost budgets, clean full refresh, zero-refresh identical frames, BUSY failure baseline invalidation | Existing host policy tests extended; device photo/soak evidence |
| M6 | Second target: capability/target contract, independent ROM env, template variant path, OTA dual-side target reject, controller/panel adapter, offline host tests. New hardware arrival blocked for physical verification | Target mismatch rejects in tests; ROM builds; blocked_by_hardware_arrival list |
| M7 | Four-page UI (templates/devices incl. power/data/MCP) + MCP tools over the same service; migration; persistence/recovery | UI/MCP consistency tests; migration tests; `git diff --check` |

## Constraints

- No new dependencies unless unavoidable.
- Do not disturb unrelated working-tree changes; no destructive git commands.
- Firmware `FW_VERSION` bump only for a real release; record ROM + SHA256 in PROGRESS.
- New hardware constants must not be guessed; unresolved facts stay explicit blockers.
