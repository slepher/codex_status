# Generic display platform architecture — status

Status: design complete; implementation not started

- 2026-09-21: independent context-free Astra design commissioned.
- Existing working-tree changes are outside this task and must not be touched.
- 2026-09-21: Astra independently produced
  `docs/history/generic-display-platform-design-v1.md` (originally at the docs
  root, now archived). Primary review confirmed coverage
  of the four-tab product model, active-only compiled requirements, observed
  device import and deduplication, immutable deployments, multi-target ROMs,
  typed snapshots, atomic storage, power integration, migration, and tests.
- Validation: `git diff --no-index --check -- NUL
  docs/history/generic-display-platform-design-v1.md` reported no whitespace errors
  (exit 1 is expected because the new file differs from `NUL`). No source,
  runtime, device, deployment, or existing design document was changed by
  this task.
- Next: obtain product approval for the architecture decisions, then split M0
  compatibility fixtures from M1 application-service/frozen-queue work.
- 2026-09-22: product decision simplified template management: each template
  ID + render target keeps only the current latest content. Profile references
  IDs; publish freezes a temporary ReleaseArtifact for queue consistency, not
  a selectable version. Device template recovery/import is a corner flow.
- 2026-09-22: Astra medium produced the accepted simplification pass in
  `docs/generic-display-platform-design-v2.md`. It supersedes conflicting v1
  capacity, revision, entity, wake-check, and power-decision structures with an
  8-item Profile, CompiledTemplate, one active context, full A/B Bundle, one
  PublishJob, Bridge-owned push/pull scheduling and PowerPlan, and device-owned
  bounded execution/framebuffer refresh. Implementation remains unstarted.
- 2026-09-22: v1 moved to `docs/history/` and all active architecture references
  moved to v2. v2 is the only current platform architecture; v1 is retained
  solely as decision history.
