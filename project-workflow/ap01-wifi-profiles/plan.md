# AP01 Wi-Fi profiles interface

2026-10-21. Fixed rescue slot0 stays original0031; no device flashing or network changes in this task.

Implement authenticated OTA-application GET/PUT /api/wifi/profiles for whole saved-list management only. Read current credentials under original network mutex; merge by SSID, saved candidates first. Current absent from saved list is a virtual read-only recovery row. No activate, stop/start, factory reset, network PSM write or identity field handling in this endpoint.

Persist private list through independent encrypted PSM namespace and temporary-file verification + atomic rename. Preserve deployed read-only diagnostics. Native validation uses a fresh exact diagnostic34 source tree; current unshipped screen changes stay outside candidate. Validate module failures under ASan/UBSan, real authentication/router behavior, RV32 XIP audit, and full SIM menu offline recovery and passive list edits. No simulated result is physical Wi-Fi/cloud proof.
