# AP01 Wi-Fi profiles interface

2026-10-21. Fixed rescue slot0 stays original0031; no device flashing or network changes in this task.

Implement authenticated OTA-application GET/PUT /api/wifi/profiles for whole saved-list management only. Read current credentials under original network mutex; merge by SSID, saved candidates first. Current absent from saved list is a virtual read-only recovery row. No activate, stop/start, factory reset, network PSM write or identity field handling in this endpoint.

Persist private list through independent encrypted PSM namespace and temporary-file verification + atomic rename. Preserve deployed read-only diagnostics. Native validation uses a fresh exact diagnostic34 source tree; current unshipped screen changes stay outside candidate. Validate module failures under ASan/UBSan, real authentication/router behavior, RV32 XIP audit, and full SIM menu offline recovery and passive list edits. No simulated result is physical Wi-Fi/cloud proof.

2026-10-21 authorized physical deployment: pin candidate/hash/MAC, back up original network/identity files and inactive rescue; switch temporarily to unchanged rescue0, upload only application1, verify authenticated build/slot and unchanged rescue0. Exercise list-only save/uniq/revision without activation, reboot via retained maintenance/shell path, read back persistence, clean test-only candidates and compare original network/identity bytes. Never write rescue ROM or use reset_wifi/config_router.

2026-10-22 native-menu follow-up: use deployed06 exact isolated baseline; integrate Settings Wi-Fi after screensaver, listing saved SSIDs plus current recovery row, with stock option checkmark for current configuration and independent wrap/focus. Selecting another saved network must queue bounded original network-only activation off the UI thread, keep rollback/restart recovery, and update the check only after verified network result. Use existing encrypted list; preserve identity/Bridge/menu code/rescue. Validate empty/single/typical/max, native original input/pixels, errors/restart and protected areas; no physical flash or new commit without request.
