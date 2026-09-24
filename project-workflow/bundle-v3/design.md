# Bundle v3: manifest plus raw compiled objects

Status: design decision, 2026-09-24. Implementation is separate from the current v2 Bundle OOM repair.

## Measured problem

The frozen 1.54 `mini` + `quad` Bundle is 55,347 bytes. Its two `compiled.binary` fields are 22,666 hexadecimal characters each (45,332 characters together). Each fixed-layout render plan is 11,333 raw bytes. `mini` uses 5 of 64 operations, 3 of 32 requirements and no resources; 98.6% of its bytes are zero. `quad` uses 31 operations, 14 requirements and 5 resources; 90.5% are zero. Template sources together occupy about 5.6 KB.

## Container and upload

- Advertise `bundle_format: 3` in authenticated device capabilities. The Bridge emits v3 only for a v3 device. `compiler_abi: 2` remains valid while render-plan semantics and packed element layouts remain unchanged.
- A v3 Bundle is one binary container: a fixed header (`CSB3`, format, manifest length, object length, whole-payload CRC32), canonical UTF-8 JSON manifest, then concatenated raw compiled objects. No hex, base64, multipart form or second delivery channel is needed: the existing `/v2/bundle/chunk` endpoint already streams arbitrary bytes to LittleFS.
- The manifest includes job/device/bridge IDs, firmware/render targets, ABI, Profile, template sources, bindings, resources, and for each compiled object its encoding, offset, length and CRC32. Offsets are relative to the raw-object section. Canonical manifest bytes are frozen with the job and included in the outer CRC.
- Each compiled object uses `ct-dense-v1`: `CTD3` magic (4 bytes), ABI (1), op/requirement/resource counts (1 byte each), source CRC32 (4), template ID (17), exactly the used packed `CtOp` (118 bytes each), `CtReq` (84 bytes each) and `CtResource` (66 bytes each), then CRC32 of that object (4). The container stores these bytes directly, without text encoding.

## Expected size

The measured `mini` compiled record becomes about 875 bytes, `quad` about 5,197 bytes. Replacing 45,332 hex characters with roughly 6,072 raw dense bytes should reduce this specific 55.3 KB Bundle to roughly 16.2 KB including the header and object descriptors, about 71% smaller. A canonical encoder must report the exact final length before release.

## Validation and atomic install

- Validate header and bounded lengths before allocation. Receive the container in raw chunks into LittleFS. On commit, stream the outer CRC over the staged file and parse only the manifest; never reserve the entire Bundle in a `String`.
- Check manifest device/firmware/render targets, format, ABI, profile order and offsets; reject overlapping or out-of-range objects. For each object, verify its length and CRC, decode into a zeroed static `CtTemplate`, validate count limits, source CRC and compiled-template structure, then write its compiled record into the inactive A/B slot.
- Store the exact manifest and source payload for recovery. Read back and verify the inactive slot, then atomically advance the commit record. Failed validation or interrupted writes must leave the last committed v3 slot usable.
- Return specific errors for `format`, `length`, `crc`, `abi`, `target`, `source_crc`, `oom` and `space`; preserve the failed job for retry.

## Upgrade rule

Per user decision, a v3 ROM treats pre-v3 Bundles as absent. It does not migrate or activate them. It reports no committed job/context and waits for a newly published v3 Bundle. Keep old files until a v3 commit succeeds when space permits; reclaim them afterward. The Bridge must mark frozen v2 jobs stale rather than retrying them against a v3 device, and publish a fresh v3 job from the saved Profile. The current 1.54 device follows this path after its v3 ROM update.

## Acceptance

- Host/device golden vectors for both targets round-trip the dense object and render identically to the current compiled plan.
- Reject malformed lengths/offsets, overlap, truncation, corrupt CRC, count overflow, wrong ABI and wrong target before changing the active slot.
- Simulate resets at each staging/write/commit point and confirm the previous committed v3 Bundle or the new one remains active.
- Publish `mini` + `quad` to 1.54, verify job/context/data ACKs and display, and measure wire length and peak heap; repeat on Note4.
