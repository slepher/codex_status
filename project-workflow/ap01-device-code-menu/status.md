# Status

2026-10-21 navigation follow-up: candidate04 replaces03 for further simulation. Parent Settings wraps0..7; device-code actions wrap1..2, initially1(regeneration). Numeric row0 is display-only and never selected. Native-navigation01/02 passed actual original input: parent trace[6,7,0,7,6], submenu trace[1,2,1,2,1]; rotation preserves the stored value. Regenerate/Return/page recreation/same-Flash restart and prior arrow/font assertions also passed. Frames reviewed.

Candidate04: codex_status/artifacts/ap01-device-code-navigation-20261021/ap01-0041-device-code-menu-candidate04.bin,7018280 B,build b83df282fb1498198ef67514eea0e11f,SHA256 2c0e092f6e4315b235816864eb00027c41723e7aedf49c97d7a1e609755fc743. Audit/link/reconstruction passed. ROM bytes and all instance sizes equal03; no new persistence/RAM allocation. Current interactive window: http://127.0.0.1:18992/ (ROM04), default focus regeneration. Mock code99552520/resource bytes preserved from the stopped03 session. Physical f52b12/rescue0 and Bridge untouched; no deployment. Firmware source commit: 8d01ec6.

2026-10-21 style follow-up: candidate03 replaces candidate02 for further simulation. The normal stock subset lacks digits5/6/7/9, previously rendered with Noto while other digits used stock glyphs. The entire number row now uses complete original20px face0xa055fb7c in both focus states, with an error-text fallback. Code and regeneration chevrons are hidden with the original hidden flag; Return retains its arrow and row structure.

Native-style01/02 passed complete input, regeneration, Return, page recreation and same-Flash restart; actual arrow-hidden flags[true,true,false], selected/unselected framebuffer images reviewed. Candidate03: codex_status/artifacts/ap01-device-code-style-20261021/ap01-0041-device-code-menu-candidate03.bin,7018280 B,build4a747e40c92ef5292d5a98b8b4ddadce,SHA256 b77b442809421d7c8e77231ed3d62fed5bfb27e67d4881a6fefe77c2762c8dd3. Audit/link/reconstruction passed. No physical flash or Bridge access; candidate02 archived.

Previous interactive window (stopped): http://127.0.0.1:18991/ (ROM03). The18990 session was stopped after saving private NOR; mock code99552520 and resource-region bytes were preserved. Live frame: codex_status/artifacts/ap01-interactive-style-20261021/shown-fixed.png. Only simulation Flash changed; physical f52b12/rescue0 untouched.

2026-10-21: Native original0041 menu validation completed in QEMU. Firmware repository fix commit: febcd37 (initial implementation: 1e7aebe).

The original Settings menu has a stock-style device-code row and a three-row submenu: eight decimal digits, explicit regeneration, return to parent Settings. A separate checked 16-byte file persists the value; normal opening, navigation and restart do not rotate it. OTA/debug authentication and Wi-Fi remain unchanged.

Candidate01 hung in the original row constructor before supplementary font installation. Candidate02 fixes safe empty-text construction, title font switching and the actual 24-byte FontData ABI. Host ASan/UBSan storage tests, RV32 audit/link, font reproducibility and complete candidate reconstruction passed.

Actual original-ROM native-menu06 passed menu behavior; native-menu07 passed same-Flash restart. Native-menu08 passed complete press/KEY/release events and page deletion/recreation. Final native-menu09 passed restart digest comparison, complete input and actual guest framebuffer review after invoking the original default-display refresh callback. No independent SIM UI was used. This checks the added menu and original Return; it is not full business-app regression or physical LCD verification.

Candidate02: cuktech-ap01-firmware/evidence/native-settings-menu-20261021/ap01-0041-device-code-menu-candidate02.bin
Build: 1faf1ef0e52e4dfdd671db968fbaff11
Bytes: 7018168
SHA256: 286d3accf21eaeda1598c63ecf9e0d533ad77dfc711f56c3c1a2e292d7fb9a8a

No physical device accessed or flashed in this task. Physical application1 remains f52b12b55b8aaa9b8d891b7f23e98097; original0031 rescue0 remains unchanged. Hardware menu, entropy, LCD/SPI and knob acceptance remain pending. Never write rescue0.

2026-10-21 interactive viewing (session now stopped): candidate02 ran in a separate local QEMU session, with a copied NOR under codex_status/artifacts/ap01-interactive-20261021/. Browser window opened at http://127.0.0.1:18990/; arrow keys rotate and Enter sends original press/KEY/release. Original-ROM LVGL framebuffer and a right/left selection roundtrip were checked. The adapter uses the same explicit fbdev default-display refresh model as native-menu09; it does not redraw the guest UI. Stop via the viewer's close-simulation button. No real device or Bridge accessed; this session is not additional firmware acceptance.

Details: cuktech-ap01-firmware/reverse/reports/ap01/knowledge/0041-native-device-code-menu.md
