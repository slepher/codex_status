# Status

2026-10-21: list-only API integrated into OTA application source; not flashed. Current device remains diagnostic0041 build8083f67405c4c7fe9122a826f9a69e7e in slot1; rescue slot0 remains original0031.

Evidence: ../cuktech-ap01-firmware/evidence/wifi-profiles-20261021/{module-validation.json,native-port-validation.json,host/native-app-report.json,native/native-core-audit.json}. Final full SIM run04 passed558steps/108screenshots, including passive edits/current SSID uniq/offline menu recovery; evidence under sim-wifi-menu-20261021/run04.

The UI model is not real PSM/RF/cloud proof. Physical switching is separate from list editing and is not exposed through this API.

Candidate packaged: 7006536 bytes, SHA256 1ce657386b8deff11887428a30340bdaf91701981c2f82d9bd1d56aeb9df33bf, build f52b12b55b8aaa9b8d891b7f23e98097. Native QEMU list-only run04/run05 passed, including original mutex/encrypted PSM/VFS and restart revision3 recovery. Initialized synthetic NOR comparison preserved9 other data files, both firmware regions and partition tables. Evidence: native-wifi-qemu04.json, native-wifi-qemu05.json, native-wifi-nor-comparison.json under the firmware repository evidence/wifi-profiles-20261021. No hardware access or deployment.
