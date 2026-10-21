# Tasks

- [x] Allow application OTA/slot/maintenance-health access with current menu code independently of Bridge binding; preserve original bound-token access and all firmware guards.
- [x] Verify full zero-binding HTTP OTA, rotation/resume/replay/restart receipts, no-stable switch, lock independence/no implicit binding, real-ROM full inactive bytes/protected regions and bound-key compatibility; record candidate06 separately from deployed05.

- [x] Gate manual Wi-Fi configuration and diagnostics with the current menu code; keep daily Bridge and firmware/recovery authorization unchanged.
- [x] Verify parser rejection, missing/corrupt storage, bounded failures, code rotation/restart, business traffic and firmware/recovery paths in host and original-ROM simulation.

- [x] Recover exact native Settings creation/style/event ABI and prove real ROM insertion.
- [x] Implement persistent eight-digit code and explicit regeneration with error-path tests.
- [x] Integrate three-row submenu using stock rows, fonts and knob events.
- [x] Verify RV32 audit/link, real-ROM QEMU input, framebuffer style, return, page recreation and restart.
- [x] Record candidate and deployment boundaries separately.
- [x] Unify all device-code digits on one complete original ROM font; hide chevrons on code/regenerate rows; verify original-ROM input/restart pixels and refresh the interactive simulator only.
- [x] Wrap Settings and submenu selection in both directions; keep numeric readout outside selection, initially focus regeneration; verify actual ROM navigation/restart and refresh simulation only.
- [x] Deploy application1; verify native menu/screen/knob, actual code rotation and old-code rejection; full inactive rescue0 SHA remains unchanged.
- [x] Deploy06 only to application1; verify menu-code maintenance/switch/reboot, independent rescue return, original Bridge compatibility, code persistence and full inactive rescue0 hash.
- [ ] Power-cut consistency and task-stack high water.
