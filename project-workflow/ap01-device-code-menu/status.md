# Status

2026-10-21: User requested immediate wrap-up and commit. Firmware repository commit: 1e7aebe.

Implemented the original0041 Settings device-code entry and three-row submenu using stock row/container controls and matching focus/arrow/scroll styles. Eight decimal digits persist in a separate 16-byte checked file; only explicit regeneration changes a valid saved value. No OTA/debug authentication changes and no network changes.

Host ASan/UBSan storage tests, RV32 audit/link, font reproducibility and candidate reconstruction passed. Actual native QEMU menu execution, screenshot/layout review, return/lifecycle and reboot persistence acceptance remain pending. The native QEMU verifier is committed but has only syntax validation. Candidate remains flashable=false and hardware_verified=false.

Candidate: cuktech-ap01-firmware/evidence/native-settings-menu-20261021/ap01-0041-device-code-menu-candidate01.bin
Build: c029d5107c7927b27fd1d979da203c70
Bytes: 7018104
SHA256: cc0e417b8e75c2f20461c26c0c6eccd227dedefbd3c0ceb2a873ea31d49dfe0b

No device accessed or flashed in this task. Physical application1 remains f52b12b55b8aaa9b8d891b7f23e98097; original0031 rescue0 remains unchanged. Do not deploy until native acceptance is complete; never write rescue0.

Details: cuktech-ap01-firmware/reverse/reports/ap01/knowledge/0041-native-device-code-menu.md
