# Note4 A/B ROM OTA bring-up plan

Date: 2026-09-23. Scope: Note4 firmware, independent PlatformIO environment,
16 MB partition layout, USB first flash, and one authenticated OTA upgrade.
Bridge source and its workflow are outside this task. No Git commit.

## Baseline and gates

1. Preserve the existing full-flash backup and verify its length/hash. Confirm
   the attached USB endpoint, chip, flash and PSRAM against the earlier read-only
   inventory; capture the visible board/panel identity where possible. Record
   the factory slot state before writes.
2. Compare the official PCB V1.0 pin map and SSD2683 reference driver with the
   actual unit. Implement the smallest Note4-specific adaptation: GPIO/power,
   display init and full refresh, buttons/wake, and an independent 16 MB build
   environment and partition CSV. Keep partial refresh disabled until measured.
3. Build ROM A and ROM B in the Note4 environment. Check image target, sizes,
   partition offsets, SHA256 and bootloader settings. Keep the two images
   distinguishable by version. Verify the factory backup is recoverable.
4. USB first-flash ROM A using the authorized port. Capture serial and device
   status evidence for image, running OTA slot, panel refresh, buttons and
   basic wake behavior. Stop if boot, BUSY, or identity cannot be verified.
5. Use the existing authenticated OTA client to install ROM B. Check its target
   validation and token path. If it requires unfinished Bridge work, stop at
   that blocker without editing Bridge. Otherwise capture upload/ACK, reboot,
   running slot switch, version and panel behavior. Record manual restoration.
6. Run focused build/checks and `git diff --check`; summarize results and any
   remaining limits in `status.md`. Add a short `PROGRESS.md` handoff at each
   real-device milestone, linking back to `status.md`.

## Stop rules

- Do not flash if the unit cannot be identified as monochrome Note4 V1.0,
  backup verification fails, or the independent image/partition layout does
  not fit the 16 MB flash.
- Stop hardware writes on an uncertain pin/waveform mapping, repeated BUSY
  timeout, unexpected partition/slot result, or failed post-flash boot.
- Do not weaken device-token checks. Stop OTA if no existing authenticated
  client can perform it without concurrent Bridge changes.
- Preserve all unrelated working-tree edits, especially Bridge files and
  `generic-display-platform-implementation/`.
