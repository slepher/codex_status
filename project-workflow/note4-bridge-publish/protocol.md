# Note4 incremental asset publish protocol — draft for both implementers

**Status: pending joint Bridge/device confirmation. No Note4 firmware implements this protocol yet.** Note4 A/B application ROM OTA is independent and does not establish support for incremental fonts or manifests. Neither side may infer these endpoints from a target name. The authenticated device status must advertise `asset_publish_protocol: 1` before the Bridge uses them.

## 1. Transport and identity

HTTP is the asset transport. Every request uses the existing v2 endpoint token, nonce/session checks, MAC identity, bridge ID and explicit owner lease. The token is never embedded in a manifest or a persisted job. Reads and writes do not claim ownership or renew a PowerPlan. BLE remains for rendezvous and small control data. 401 (token/session), 409 `owner_conflict`, and 409 `context_changed` stop writes; the Bridge obtains fresh authenticated state before retrying. The exact URL paths below are **proposed**, not existing endpoints.

`protocol=1` is the manifest wire version. `compiler_abi` is a separate version; asset font references require a new ABI agreed with the device, tentatively **2**. A firmware advertising only ABI 1 or no `asset_publish_protocol` keeps the existing full Bundle path, with its existing size limit; a Profile selecting asset fonts is rejected on that path. The Bridge must never switch paths within a job.

## 2. Canonical content and manifest

All JSON objects use UTF-8, lexically sorted keys, no insignificant whitespace, and the same canonical escaping as `bridge/crates/core/src/template.rs::canonical_bytes`. Numeric fields are nonnegative integers. Hex CRC fields use eight lowercase digits. `crc32` is IEEE CRC-32 over exact bytes; it detects damage and is not authentication. A content ID is `kind:byte_length:crc32:sha256`, with a 64-digit lowercase SHA-256 over the same bytes. The receiver checks kind, length, CRC, SHA-256 and final bytes. If a stored object has the same ID but different bytes, return `id_collision` and do not commit. No object path is accepted from the peer.

The canonical manifest payload (before its envelope length/CRC) is:

```json
{"bindings":[],"compiler_abi":2,"device_mac":"7C4FADB93408","firmware_target":"zectrix-note4-400x300","fonts":[{"font_id":"18c2e4ed","name":"ntthin18","object_id":"font:12345:18c2e4ed:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],"full_sync_s":3600,"initial_active_id":"codex-status-a","job_id":"4b91d30a","objects":[{"crc32":"721fcd11","id":"compiled:9842:721fcd11:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","kind":"compiled","length":9842,"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"crc32":"18c2e4ed","id":"font:12345:18c2e4ed:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","kind":"font","length":12345,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"crc32":"df1353a2","id":"source:4302:df1353a2:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","kind":"source","length":4302,"sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}],"profile_order":["codex-status-a"],"protocol":1,"render_target":"epd-ssd2683-400x300-1bpp","templates":[{"compiled_id":"compiled:9842:721fcd11:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","source_id":"source:4302:df1353a2:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","template_id":"codex-status-a"}]}
```

Lengths and hashes above are illustrative; the example shows the complete wire fields and encoding shape. `source` is canonical template JSON, `compiled` is the exact device-readable compiled artifact, and `font` is the complete validated CSFN container. Object IDs are deduplicated within a manifest. Every template in `profile_order` has exactly one template entry; every asset-font name used by a template has exactly one selected `font_id`. Built-in small-screen fonts have no asset object. The manifest envelope contains `manifest_length`, `manifest_crc32`, and `manifest_id = manifest:<length>:<crc32>:<sha256>` computed over the payload only. The receiver verifies exact bytes, target, ABI, font pixel format and every reference before visibility.

The envelope also carries `target_id`: the `target:` content ID of the same canonical manifest with only `job_id` replaced by an empty string. It distinguishes a new explicit job from a changed display target. When authenticated status reports the same active `target_id`, the device returns a durable idempotent ACK for the new job without retransmitting objects, switching context or refreshing the panel.

## 3. Authenticated status digest

`GET /v2/asset-publish/status` (proposed) returns:

```json
{"asset_publish_protocol":1,"compiler_abi":2,"device_mac":"7C4FADB93408","firmware_target":"zectrix-note4-400x300","render_target":"epd-ssd2683-400x300-1bpp","pixel_format":"1bpp","max_object_bytes":4194304,"max_manifest_bytes":65536,"free_bytes":2949120,"install_peak_bytes":2949120,"filesystem_overhead_bytes":4096,"active_context_id":"ctx-42","active":{"manifest_id":"manifest:850:abcd1234:<sha256>","target_id":"target:840:5678abcd:<sha256>","job_id":"old-job","object_ids":["font:12345:18c2e4ed:<sha256>"]},"rollback":{"manifest_id":"manifest:810:ef567890:<sha256>","target_id":"target:800:7890ef12:<sha256>","job_id":"prior-job","object_ids":[]},"pending":{"job_id":"4b91d30a","manifest_id":"manifest:900:12345678:<sha256>","offsets":{"font:12345:18c2e4ed:<sha256>":4096}}}
```

`active` and `rollback` can be null. Their object IDs are authoritative committed references, not a directory inventory. `pending` is resumable staged state and may be null. The device reports both free space and its conservative install peak budget, including filesystem reserve. The Bridge estimates retained active/rollback references, new objects, temporary write space and manifest metadata; either side may reject insufficient space. The device remains final authority. Hardware verification is a separate explicit status flag or recorded real-device acceptance, never inferred from target.

## 4. Proposed HTTP transaction

1. `POST /v2/asset-publish/begin` with `{job_id, bridge_id, expected_active_context_id, manifest_id, target_id, manifest_length, manifest_crc32, manifest_bytes}`. Same job/manifest is idempotent and returns `{result:"ready", offsets:{object_id:next_offset}, missing:[object_id...]}`. Same job with different manifest returns 409 `job_conflict`. A committed same job or unchanged active `target_id` returns a durable ACK. A stale context returns 409 `context_changed` before any change.
2. `PUT /v2/asset-publish/object/{object_id}` with authenticated `job_id`, `offset`, `chunk_length`, `chunk_crc32`, and raw bytes (bounded by advertised `max_chunk_bytes`). The response includes `{object_id, next_offset}`. Repeating identical bytes at the same offset returns the same position; conflicting bytes, gaps, overflow and bad CRC return errors without advancing the stored offset. Large fonts are streamed to temporary storage; RAM need only hold one chunk plus parser workspace. Final object verification checks full length, CRC, format and target before it is addressable by a manifest.
3. `POST /v2/asset-publish/commit` with `{job_id, bridge_id, manifest_id, expected_active_context_id}`. The device validates all references and available peak space, writes the manifest durably, then atomically makes it active and returns `{result:"applied", job_id, manifest_id, active_context_id, commit_seq}`. The ACK is persisted with the commit. Repeated COMMIT returns the same ACK and does not create another context or display change. Failed validation leaves the old active manifest and context usable.
4. `POST /v2/asset-publish/cancel` is optional but, if implemented, removes only an uncommitted stage. Cancel after commit returns the committed ACK; it never rolls back the active manifest.

## 5. Errors and recovery

| HTTP | `code` | Required behavior |
|---|---|---|
| 400 | `bad_manifest`, `bad_object`, `bad_offset`, `bad_crc`, `unsupported_font_format` | Reject offending input; keep active manifest. |
| 401 | `unauthorized` | Stop writes; require fresh authenticated session. |
| 409 | `owner_conflict`, `target_mismatch`, `abi_mismatch`, `context_changed`, `job_conflict`, `id_collision` | Stop transaction; reread authenticated status. |
| 413 | `object_too_large`, `manifest_too_large` | Fail preflight/job with reported limit. |
| 507 | `insufficient_space` | Preserve active and rollback manifests; expose required/free bytes. |
| 503 | `not_ready` | Wait for another formal online window; do not extend power implicitly. |

After a lost chunk ACK, reread `pending.offsets` and resend from the device position. After a lost COMMIT ACK or Bridge restart, read authenticated status: matching `active.job_id + manifest_id` means success; matching pending means resume; neither means retry BEGIN. A different active context or job requires review before retry. The Bridge persists the full manifest and all frozen object bytes before queueing, and records success only after durable ACK or the equivalent authenticated committed-status observation.

Power-loss examples: (a) power fails halfway through `font` temporary bytes: old active and rollback boot; pending offset may resume or reset to zero. (b) power fails after all objects but before durable manifest commit: old active boots; COMMIT can retry. (c) power fails after atomic activation but before HTTP ACK: new active boots, status reports the same job/manifest and stored ACK, and Bridge records success without repeating activation. Garbage collection runs only after durable commit and retains every object referenced by active or rollback manifests; it never deletes the sole valid copy to make room.

## 6. Open joint decisions

- Exact endpoint names, HTTP token/session framing, `max_chunk_bytes`, canonical JSON compatibility and ACK storage format.
- Final compiler ABI number and CTP1 asset-reference layout, including device render/clock/region lookup.
- Device filesystem block overhead and conservative peak-space formula; remove arbitrary eight-font/48 KiB product caps only after parser integer-width, RAM and streaming audit.
- Whether source JSON is stored on-device or only its ID is verified in the manifest.

The Bridge implementation must not issue the proposed requests until the device implementer confirms these fields and advertises protocol 1. The Note4 ROM OTA task remains focused on bootable A/B application images and OTA verification.
