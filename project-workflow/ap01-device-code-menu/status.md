# Status

2026-10-21: Native original0041 menu validation completed in QEMU. Firmware repository fix commit: febcd37 (initial implementation: 1e7aebe).

The original Settings menu has a stock-style device-code row and a three-row submenu: eight decimal digits, explicit regeneration, return to parent Settings. A separate checked 16-byte file persists the value; normal opening, navigation and restart do not rotate it. OTA/debug authentication and Wi-Fi remain unchanged.

Candidate01 hung in the original row constructor before supplementary font installation. Candidate02 fixes safe empty-text construction, title font switching and the actual 24-byte FontData ABI. Host ASan/UBSan storage tests, RV32 audit/link, font reproducibility and complete candidate reconstruction passed.

Actual original-ROM native-menu06 passed menu behavior; native-menu07 passed same-Flash restart. Native-menu08 passed complete press/KEY/release events and page deletion/recreation. Final native-menu09 passed restart digest comparison, complete input and actual guest framebuffer review after invoking the original default-display refresh callback. No independent SIM UI was used. This checks the added menu and original Return; it is not full business-app regression or physical LCD verification.

Candidate02: cuktech-ap01-firmware/evidence/native-settings-menu-20261021/ap01-0041-device-code-menu-candidate02.bin
Build: 1faf1ef0e52e4dfdd671db968fbaff11
Bytes: 7018168
SHA256: 286d3accf21eaeda1598c63ecf9e0d533ad77dfc711f56c3c1a2e292d7fb9a8a

No physical device accessed or flashed in this task. Physical application1 remains f52b12b55b8aaa9b8d891b7f23e98097; original0031 rescue0 remains unchanged. Hardware menu, entropy, LCD/SPI and knob acceptance remain pending. Never write rescue0.

Details: cuktech-ap01-firmware/reverse/reports/ap01/knowledge/0041-native-device-code-menu.md
