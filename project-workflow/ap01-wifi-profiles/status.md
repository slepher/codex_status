# Status

2026-10-21: list-only API integrated into OTA application source; not flashed. Current device remains diagnostic0041 build8083f67405c4c7fe9122a826f9a69e7e in slot1; rescue slot0 remains original0031.

Evidence: ../cuktech-ap01-firmware/evidence/wifi-profiles-20261021/{module-validation.json,native-port-validation.json,host/native-app-report.json,native/native-core-audit.json}. Final full SIM run04 passed558steps/108screenshots, including passive edits/current SSID uniq/offline menu recovery; evidence under sim-wifi-menu-20261021/run04.

The UI model is not real PSM/RF/cloud proof. Physical switching is separate from list editing and is not exposed through this API.
