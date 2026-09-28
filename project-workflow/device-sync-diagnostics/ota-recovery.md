# OTA recovery simplification (2026-09-29)

## Goal

An authenticated emergency upload must work with the device operation token and a valid ESP image, even when sync diagnostics, claim ownership, or a prior OTA job is broken. The normal Bridge path still checks MAC, target, frozen image size and SHA256 before upload, and verifies the running image after reboot.

## Work

1. Firmware: remove the OTA arm/ticket gate and its persisted state. Keep `/doUpdate` token authentication and `Update` image validation. Publish a direct-OTA capability and provide a token-authenticated running partition prefix SHA256 endpoint independent of sync ownership.
2. Bridge: choose the old arm flow only for deployed ROMs without the direct-OTA capability. For new ROMs, upload directly and use the independent image endpoint for confirmation. Keep the old flow during migration.
3. Check the Rust tests and both firmware targets serially; flash the Note4 and 1.54 by verified MAC and ROM hash, then observe version, slot, reset and running-image hash. Preserve the Bridge runtime data and record final evidence in `PROGRESS.md` and the backlog.

Emergency direct upload accepts a valid same-chip ESP image with a correct device token; a hand-picked image can still be wrong for the board. The Bridge's normal preflight prevents that accidental mismatch. No new mandatory marker or ticket is added to the device recovery path.

## Result

- Both firmware targets built serially. Note4 `0.18.35-note4-b-ota1` and 1.54 `0.18.34-bw-ota1` ROM paths, sizes and SHA256 are recorded in `PROGRESS.md` latest section.
- Note4 crossed the old arm gate once, then used a second direct token-only OTA on the new ROM with no target, owner, ticket or arm. Both times the booted running partition prefix SHA256 matched the candidate. The second upload changed `ota_1` to `ota_0` despite keeping the same version string.
- 1.54 was USB-flashed after exact COM4 MAC check; full image readback matched the candidate byte for byte. No new OTA was attempted on this device.
- Relevant Rust tests passed, including an HTTP test for the token-authenticated image endpoint client. The updated Bridge runs from the default scheduled task with its existing data directory.
