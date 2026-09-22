# Task 1 — Bridge simplified domain model (M1)

Modules added to `bridge/crates/core` (no new crates):

| Module | Model |
|---|---|
| `datasource.rs` | `FieldSpec` (type + push/pull trigger), `DataSource` (`kind`, `config`, `credential_ref`), `SourceSnapshot` (values, `observed_at`, `valid_until`, `last_success_at`, `quality`, error), Codex + Static JSON providers |
| `model.rs` | `DeviceCapabilities`, `TemplateKey {template_id, render_target}`, `Profile` (1–8 ordered ids, `initial_active_id`, `bindings`, no enabled subset, no revision), `PublishJob`, `PowerPlan`, `BundleMeta` |
| `compile.rs` | `CompiledTemplate`: requirements (bounded field index), render ops, local dependencies, resources, compiler ABI, canonical bytes + CRC, load-time revalidation |
| `coordinator.rs` | per-device serial coordinator: one active context, data snapshot + push/full fingerprints, acked fingerprints, `full_sync_deadline`, monotonic `data_seq`, in-flight snapshot pin, single mutex publish/activate/OTA, plan idempotency |
| `service.rs` | one application service used by UI and MCP: template save/validate/preview, profile save, explicit publish, data source config/probe, device power view, claim/release, recovery import |
| `store.rs` | atomic JSON persistence under `<exe>/data/platform/` |

Truth table (binding contract, v2 §6):

| Change | Push? | PowerPlan? | Deadline? |
|---|---|---|---|
| push field value/missing/quality changes | yes, sends full snapshot | may escalate if reachable | no |
| pull-only change | no (piggybacks later) | no | no |
| full_sync_deadline reached | on next reachable opportunity | bridge decides | reset only after ACK |

Tests (task-1 acceptance): profile 1–8, empty/>8 rejected, order preserved, all
items cycle, save≠publish, publish freeze, queued edit does not drift, ACK loss,
stale ACK, duplicate seq idempotent/conflict/out-of-order, A→B→A contexts.
