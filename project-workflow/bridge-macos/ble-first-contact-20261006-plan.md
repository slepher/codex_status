# BLE-first contact and ROM rollout — 2026-10-06

User contract: normal wake starts bounded BLE contact only; absent Bridge goes back to sleep. After authentication, Bridge reads its actual host SSID, resolves its host→2.4GHz mapping, checks saved device content, saves missing/changed configuration over authorized BLE, and explicitly activates that target within the admitted work window. Unknown/ambiguous/missing configuration produces a concrete error, never an old-network fallback. Existing namespaces, claims, transaction query recovery and PowerPlan deadlines remain authoritative. USB keep-awake remains usable and periodically opens bounded BLE contact without autonomous Wi-Fi retry.

Authorized: firmware/protocol implementation, Note4-only ROM build, necessary Bridge rebuild/deployment, and USB flashing once matching hardware is connected. No other target, no partition/NVS/coredump erase, no secrets in repository/logs.

1. Add failing source guard and production protocol deadline tests.
2. Remove normal startup association, legacy scanning/retry and blocking power-save reassociation; retain explicit asynchronous provisioning.
3. Ensure cold/button/timer contact and USB offline retry all enter BLE protocol; Bridge current-SSID selection includes network-change rejection before configuration writes.
4. Run focused Bridge/Fake Wi-Fi and contact checks; build zectrix-note4-b via isolated script. Verify marker/size/hash and matching ELF stack gate.
5. Back up/deploy only default Bridge if changed. Enumerate USB and verify MAC + actual partition/active slot. Flash app only while preserving storage/partition/dump. Validate boot version, BLE responsiveness and obtainable authenticated running image.
6. Update PROGRESS and backlog with exact evidence and remaining limits. No commit.

## Execution and review

Completed steps 1–6 for Windows deployment and USB-connected Note4. Final .63 ROM 1857456 B / SHA256 130e0cf66a7389d8a293e4fd48808f2b4790464e98b70fdca6f459bfc57559c1; ELF 04e066d40237bc6997ba6b177c1bdb8f0e402b262b26849bab62d6dae92229ac. Static RAM 117324 B; flash 1810919 B; Wi-Fi local chain 2896 B. No Wi-Fi repository capacity/format changes or extra persistent records; provisioning resource evidence remains existing WFS4 budgets. Removed blocking association paths and shared startup UDP initialization, no new abstractions.

Runtime review caught .62 startup panic: UDP socket was created before TCP/IP initialization in BLE-only setup. Matching ELF resolved setup→NetworkUDP::begin→lwip_socket→sys_mutex_lock. Remove the premature begin; existing connected-only sendAnnounce uses native beginPacket lazy socket creation. Source guard now rejects UDP initialization in setup. .63 Flash hash verified; 38s raw serial proves correct version, three responsive status commands, WIFI OFF, bonded BLE connect, and deadline shutdown without recurring assert/panic. No full device dump export/erase. .62 may have overwritten a previous coredump through normal panic handling; failed candidate and raw log remain evidence.

App 67 passed/1 pre-existing ignored; Fake Wi-Fi 6 passed; render 3 passed; production contact deadline C++ and source guard passed; matching ELF stack gate and diff check passed. Firmware was written only at active ota_0 0x20000, preserving partition/NVS/otadata (byte comparison after .62 write), other flash regions never targeted. .63 boot initially rejected Windows Plan due existing owner conflict; Current Windows mapping exists: home / wd21-wo→wd21-la / revision1; live MCP automatic_state is needs_sync. Earlier empty snapshot was stale and the deployment report incorrectly reused it. This does not prove device configurations are absent. Do not force claim or synthesize credentials. Mac app deployment, actual B-network authenticated HTTP handoff, USB periodic reconnect and battery/BOOT/long-duration RF behavior remain unverified; backlog B6 remains open. No commit.
