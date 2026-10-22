# Review

Authentication, identity, revision/replay, current-slot and image checks remain. Manual OTA persists its transaction before resetting boot selection; unknown selection writes no image and cannot be retried by replay. Auto fallback is superseded only by an admitted explicit operation. Native selection and Flash target checks remain separate.

Menu keeps factory items0..5 (Version3), DeviceCode6, WiFi7, Back8. Stock Back is forwarded as6. Host tests exercise actual adapter with explicit fakes; native LVGL menu/scroll/return layout is now verified on james using the matching candidate and original RGB565 buffers; physical LCD remains unverified.

All App tests126 pass/3 conditional ignored. ASan/UBSan maintenance, Wi-Fi, menu, device-code and diagnostic checks pass. Complete OTA exposed pointer-arithmetic UB, fixed and rerun with halt_on_error. Test-only synthetic image is never a candidate.

Existing historical fixes were not reverted. No protocol/persistent-schema migration, no new dependencies, no firmware/Bridge deployment. Both git diff --check and source boundary check pass. PROGRESS pre-existing cleanup note is preserved in the working tree archive and excluded from the commit.

Remote follow-up: GCC8.4 audit/link/ROM and Linux sanitizers passed. QEMU full menu, actual guest encrypted PSM switch/rollback with explicit driver fixture, and saved-NOR reboot passed. Test entry now reads current UI root; restart waits up to30s for stock network cache restoration, preserving strict saved-list/current-network assertions. Synthetic OTP hardware fields and failed controls are documented in status.md. No hardware/RF acceptance inferred.
