# Task 1 — independent Astra architecture design

Owner: context-free Astra subagent.

Historical exclusive write ownership:
`docs/history/generic-display-platform-design-v1.md`. The accepted current
design is `docs/generic-display-platform-design-v2.md`.

The full task contract is supplied directly to the subagent. It must inspect
the repository and may use existing designs as references, but must derive and
justify its own final architecture. No source, existing documentation,
PROGRESS, tests, runtime state, or external systems may be changed.

Acceptance:

- Resolves the meaning and ownership of current Template, one-shot
  ReleaseArtifact, Profile, Deployment, Dataset, ViewSnapshot, Device,
  firmware target, render target, and compiled template plans. The accepted
  simplification has no user-facing template revision history.
- Preserves the fact that a device may store multiple templates but has exactly
  one active template, and runtime data requirements come only from that active
  template without reparsing template JSON on each request.
- Provides a staged migration that keeps the current device operational.
- Identifies decisions that require product or hardware confirmation instead
  of silently guessing.
