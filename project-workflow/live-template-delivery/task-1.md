# task-1 — Reliable template and BLE transport

## Priority revision — authoritative narrowed contract

User now requests ready ROM first for USB-unplug wireless testing. Preserve current worker edits; complete ONLY firmware changes in src/main.cpp, src/usage_client.cpp/.h, src/ble_bridge.cpp/.h, src/template_xfer.cpp/.h. No new files required. Set FW_VERSION0.4.1-bw and correct BLE model1.54 B/W. Explicit byte+length info/status, local hash selection and shared validation, wire template reset callback, preserve GPIO17 latch and normal always-on WiFi/BLE behavior. No schema/layout or host sender edits. Source checks + PlatformIO compile + git diff --check are both coding and independent validation for this narrowed task. Rust/Python implementation/tests below are DEFERRED (existing workspace tests already9 passed). Firmware shared-validator behavior and BLE info/ACK are exercised on device after dispatcher OTA. No deletions/partition change/flash erase. Worker does not deploy or commit. Commit subject `Fix firmware template sync and BLE status serialization`. The original detailed transport requirements below apply only to firmware; all host sections are future work.

## Manual first pairing — subsequent user requirement overrides host exclusion

- First new bond is established manually through Windows Bluetooth Add device. Keep NoInputNoOutput JustWorks, bonding+SC, noPIN.
- BOOT2s opens120-second window; remove startup5-minute window. Outsidewindow, disconnect unknown peer using persistent NimBLE bond store, not pre-encryption sec_state.
- blePoll calledloop expires unbonded connections whenwindowcloses; onAuthenticationComplete disconnects failed/nonbonded links. Preserve existing bonds.
- Endpoint/usage/template control/data use WRITE_ENC. Only descriptor-aware onWrite handles data and checks encrypted&&bonded. Remove legacy onWrite implementations because NimBLE callsboth.
- Public info reports per-peer stored peerBonded, peerEncrypted, pairingWindow. Refresh dynamically. Status notifications only to encrypted bondedpeer.
- Additional owned paths bridge/crates/ble/src/lib.rs and tools/test-bridge/bridge.py ONLY for publicinfo gate before subscription/write: refuse missing/false peerBonded with manual Windows pairing instructions. No pair API. Do NOT require initial peerEncrypted: protectedwrite can restore encryption using an existing bond. No first-use automaticpairing fallback.
- Add focused puregate tests inexisting Rust module and one Python testfile if practical; rerun Rust workspace tests and Python syntax/test plus pio compile in both verification layers. Host fragmentation/fullACK design still deferred.
- Sol policy review accepted with parent correction to initial encrypted gate. Hardware pairing completion is user action; never infer or fake it.

## Live user interaction corrections

- OldROM pairing screen disappears when normal data redraws. Add a pairing overlay in main: explicit RELEASE BOOT text; keep normal networking/cached usage running but don't overwrite overlay until window expiry or secure pairing success; then restore latest usage. User release at2seconds, existing10-second reset must be clearly distinguished.
- For wireless acceptance, host JSON fragmentation (<=180bytes, smaller negotiated limit inPython) is restored to immediate scope: otherwise largeusage aborts beforeBLEtemplatepush. Add simple byte-reassembly/boundary tests. Full ACKqueue redesign remains deferred; hardware ACK evidence must be captured separately and unverified bridge logs are not acceptance.
- User accidentally invoked oldROM factoryreset during manualpair attempt. At this boundary COM4 log confirms missing WiFi s0/p0/s1/p1/s2/p2, AP CodexStatus-AABBCC. User instructed reprovision viaAP. Do not restore firmwarefactory image or read saved network credentials without a specific need/authorization. Wait for network restoration before OTA; source work continues.

## Evidence and objective

main.cpp:418-421 passes advertised remote hash instead of local hash, so both HTTP servers return304 even with no local template. HTTP200 is stored without validation. Both bridge senders write usage in one GATT write; firmware4096-byte assembler survives disconnect. Template ACKs exist but senders do not require positive ACKs, and Rust suppresses failures. Correct these defects without layout/schema changes.

## Ownership

Existing paths: src/main.cpp, src/usage_client.cpp/.h, src/ble_bridge.cpp/.h, src/template_xfer.cpp/.h, bridge/crates/ble/src/lib.rs, tools/test-bridge/bridge.py. New focused Python transport test under tools/test-bridge and inline Rust BLE tests permitted. A small shared firmware validation header/source is permitted only if needed, report exact paths before adding. No deletions. Build outputs .pio/, bridge/target/, Python __pycache__ allowed ignored outputs; no other files. Dispatcher owns workflow artifacts and commits.

## Steps and invariants

1. Conditional request uses local hash, empty when absent; matching valid local template skips fetch. Consider unreadable local bytes as missing, not valid cached state.
2. Shared HTTP/BLE acceptance verifies expected CRC32 hash, structural tplValidate and min_fw before store. Invalid response preserves prior template. Avoid broad storage redesign.
3. Clear partial endpoint/usage/control JSON on connect/disconnect and overflow/malformed completion. Preserve exact bytes and current bounded sizes, valid JSON required before handlers.
4. Rust and Python send JSON writes sequentially with response in <=180-byte fragments (use smaller negotiated capacity when exposed). Endpoint/control may reuse helper to avoid analogous limits. Template binary offset protocol remains unchanged.
5. Both senders wait for positive matching begin/end/activate ACKs; reject negative/malformed expected ACK or timeout, propagate selected-template failures. Never log unverified activation success. Do not confuse unrelated usage/status notifications with expected template ACKs.
6. Python status output must work under GBK; BLE failure must not terminate running HTTP mock service. Keep HTTP mock clearly nonproduction.
7. Meaningful tests cover exact byte reassembly at boundaries/UTF8, bounds, positive/negative/mismatched/malformed/timeout ACKs, failure propagation. Local-hash and validation source inspection required where native device unit test unavailable.

### Added live evidence (2026-09-16)

PC connected CodexStatus-AABBCC (70:04:1D:AA:BB:CC), MTU255. info/status read returned exactly4 bytes (`dc8bcb3f`, `0ccecb3f`). NimBLECharacteristic::setValue(const T&) forwards explicitly typed T, so setValue(info.c_str())/setValue(json.c_str()) serialize pointer bytes, not text. Fix both using explicit byte pointer + length overload; this is required for reliable ACKs and belongs to owned ble_bridge.cpp. User prioritizes ready ROM for USB-unplug WiFi/BLE acceptance. BLE firmwareOTA remains unsupported; BLE updates usage/templates, WiFi updates firmware as well.

## Stop conditions

No child delegation, dependencies, protocol UUID changes, hardware writes, services or out-of-scope edits. Ask dispatcher if scope expansion required. Do not read credentials. Real compact usage fits4096; stop rather than silently raising limit. Existing unrelated edits must be preserved. User authorized implementation; dispatcher specifically authorizes normal-user PlatformIO compilation solely for existing cache/lock access (no installation). Runtime bridge-core36828 must not be stopped by worker.

## Coding Self-Tests

At repository root:
- `& 'C:\Users\user\.platformio\penv\Scripts\platformio.exe' run` (normal-user elevation permitted for cache lock only)
- `C:\Python314\python.exe -m py_compile tools/test-bridge/bridge.py`
- `C:\Python314\python.exe <actual-new-test-path>` (record exact command)
- `git diff --check`
At bridge/: `& 'C:\Users\user\.cargo\bin\cargo.exe' test --workspace --offline`.
Worker runs all commands directly after implementation/rework; reports exact exits/counts/errors. No flash.

## Independent Verification

Separate luna_runner reruns the same firmware, Python, Rust commands directly. Inspects diff: local-only conditional hash, validation before storage, fragmented writes, reset buffers, matching positive ACKs, failure propagation, safe output, owned paths only. No reliance on coding summary. Hardware gates reserved task3.

## Completion and commit

Both test layers pass; Sol review passed; scope/deletions checked; dispatcher commit `Fix template sync and BLE transfer reliability`. Next task2 only after commit. Task1 firmware need not be installed separately.

## Hardware rework: 0.4.2-bw (authoritative)
Manual Windows pairing and bonded encrypted reconnect passed. HTTP template install and usage passed. BLE END twice overflowed default 4096-byte nimble_host stack: tplStoreSetActive -> persist -> LittleFS -> partition read. Sol accepts bounded increase to8192 via platformio.ini build flag, CRC parsing fallback0U, firmware version0.4.2-bw and successful END task-stack high-water diagnostic. Own only platformio.ini, src/main.cpp, src/template_xfer.cpp. Preserve bonds/storage/protocol; no dependency cache edits, hardware writes or commits by worker. Coding and independent checks: PlatformIO run, git diff --check; inspect unsigned c1a2faaf preservation and active task diagnostic. No other source changes. Stop if more scope required. Commit subject: Fix BLE template commit stack overflow. Review2 required, review1 immutable. Live acceptance afterward requires repeated full/mini positive END/activate ACKs, no resets, stack headroom, free heap; USB-unplug remains separate physical gate. Rust sender success logs/exit0 alone are not proof because errors currently suppressed.
