# Note4 Wi-Fi diagnosis — 2026-10-06

Scope: explain Wi-Fi icon after A→B network change and diagnose Bridge reachability. User authorized rebuilding and temporarily starting the Windows default Bridge. Do not flash firmware or manually alter network credentials.

1. Trace template status, network selection, power windows and Bridge recovery.
2. Back up default exe/data; rebuild latest checkout and address necessary Windows build compatibility failures.
3. Start via the default Scheduled Task; collect MAC-scoped discovery/connection evidence.
4. Report confirmed source findings separately from live observations; record remaining limits in PROGRESS/backlog.

Initial evidence: default task Ready, no bridge-app process or listeners; local WLAN 192.168.3.111. Mac latest changes present through 00914ab. Original exe/data backed up under artifacts/note4-wifi-20261006. First build failed at WakeTraceRec 48-byte assertion under MSVC.

Outcome: Windows default rebuilt and running via Scheduled Task. Live host SSID wd21-wo; targets empty/no_mapping. Old registered Note4 IP times out; repeated BLE scans have no target candidate; no exact-MAC neighbor or host serial port. Device current network/power/icon phase remain unproven; BOOT window requested from user.

Necessary Windows fixes: portable 48-byte packed wake trace; existing Rust miniz_oxide/sha2 for Fake HAL instead of unavailable system zlib/OpenSSL; atomic fs::rename replacement for simulated NVS. Render tests 3/3; Fake Wi-Fi 4/6 before rename correction, 6/6 after. No firmware build/flash, network credentials change or commit. Original exe/data retained in private ignored artifacts.

## Source-only follow-up: Wi-Fi versus BLE scheduling

Confirmed production paths (no firmware edits/build/deployment in this follow-up):

- Ordinary startup runs setup→startNormalMode→connectBest before loop. Active WFS network waits up to 30s, legacy scan up to 15s, selected legacy connection up to another 30s. These waits yield RTOS time but do not poll BOOT or serviceV2Ble; normal BLE activation is in the Wi-Fi-success branch. The initial WFS wait also does not execute the icon blink callback. Nominal waits can total 75s if both stored formats participate; display/driver overhead is additional.
- Physical GPIO0 wake opens bleRecovery first and bypasses connectBest while recovery is active. Deep timer network wakes run v2RendezvousBegin first; BLE/buttons are serviced each loop, and rendezvous Wi-Fi is asynchronous. Those paths do not fit a blanket claim that Wi-Fi always starves BLE.
- Established link loss: pollWifi marks loss, after WIFI_LOST_MS=30s (subject to boot/manual grace) calls bleOff("wifi lost"). On USB keep-awake it schedules retryWifi every 60s and returns; retryWifi simply WiFi.begin on existing wifiSsid/pass. No BLE recovery or WFS rescan is opened by these retries. Battery path instead deep-sleeps and obtains a future BLE-first contact wake.
- v2RvBleReleased also blocks enterBleOn(false) after a completed/expired rendezvous for that boot, until user recovery or a new rendezvous resets it.
- WFS credentials choose only active; fallback scan reads legacy sN/pN, not the WFS collection. Thus a new saved B row does not itself become a failed-A fallback. Independent selection must respect namespace/owner and bounded radio/power windows.

Conclusion: the user hypothesis is partly correct, with a stronger failure chain of active A unavailable→startup wait (where applicable)→no automatic BLE/explicit BLE shutdown→same credentials retried while USB keeps the device awake. This is a source-supported reachable path, not evidence that the current physical device took it. Fix direction: asynchronous normal association, bounded BLE recovery independent of Wi-Fi success, and a deliberate current-store multi-network selection policy without extending formal PowerPlan deadlines.

## User correction: BLE-only ordinary contact is authoritative

The user clarified ordinary contact must only open the bounded BLE rendezvous; without Bridge it returns to sleep. Old autonomous Wi-Fi association/scan/retry timing is obsolete for this path. Supersedes the previous proposal to add independent WFS multi-network selection or merely make legacy autonomous attempts asynchronous. Wi-Fi only follows an authorized Bridge work/PowerPlan transition (explicit maintenance/USB keep-awake policies require separate handling).

Source audit: deep timer network wake already uses v2RendezvousBegin; absent plan and sync work, v2RendezvousTick does not call rendezvousWifiBegin and returns to sleep. Ordinary non-timer setup instead calls startNormalMode→connectBest, bypassing that gate. pollWifi still carries autonomous 60s retry and legacy credentials; fallback scan after admitted HTTP also still reads old slots. These reachable old branches must be retired/reconciled with the current contract, not optimized into another autonomous network policy. No firmware implementation/build/deployment performed in this correction.
