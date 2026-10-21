# Status

2026-10-21: list-only backend deployed to AP01 34F01557210B application slot1, build f52b12b55b8aaa9b8d891b7f23e98097. Image7006536B, SHA2561ce657386b8deff11887428a30340bdaf91701981c2f82d9bd1d56aeb9df33bf. One upload through unchanged original0031 rescue0; current network unchanged. User confirmed original display/knob normal.

Physical checks passed: authenticated GET/PUT, current SSID uniq/duplicate collapse, stale revision rejection, no activation route, original network/identity files unchanged. Original rescue NSH swtichboot /dev/fw2 plus separate reboot returned to slot1 without a second flash; revision1 and encrypted list bytes survived. Test candidates removed, revision2, current network remains one recovery row. Inactive application1 full readback including PEC exactly matches candidate. Rescue0 full before/after6822344B including PEC both match original SHA256d843aae4e097c31ce06a315c62e1a66f4fb3bf9299db289b6f31562d0ef2231b. Final authenticated health/build/slot1/business online and cleanup list verified.

11 original network/identity/binding files match before deployment and after save/restart/cleanup.22 of23 preexisting data files unchanged; only original OTA log changed. A list save only adds the independent encrypted e#ap01_wifi.profiles file.

Important missing scope: the added Wi-Fi GUI exists only in the separately rebuilt NuttX SIM. It is not original0041 ROM execution. Native QEMU exercised the real candidate API/storage, not a new system-menu entry. Hardware menu integration and safe writes/rollback of original network.ssid/password are NOT implemented or physically verified. RF switching/cloud binding acceptance remains open.

Evidence: ../cuktech-ap01-firmware/evidence/wifi-profiles-20261021/physical-validation.json; original dumps/SSID responses remain private. Detailed report: reverse/reports/ap01/wifi-profiles-0041-20261021.md in firmware repository.
