# Note4 Profile 字体资产发布与显示结果修正

## Plan

1. Keep the approved 400×300 layout and its `ntthin18` font slot. Bind that slot in the Note4 Profile to a CSFN asset made from the official hinted Latin Noto Serif ExtraLight 200, 18 px. Preserve the large quota digits.
2. Complete the existing full A/B Bundle route: freeze selected Profile font bytes, transfer them inside the Bundle, validate before commit, and resolve them by the compiled font slot when rendering. Preserve prior Bundle behavior and A/B rollback.
3. Verify host JSON/compiled preview parity with the selected asset and run the relevant Bridge/device tests. Build only `zectrix-note4-b`; verify target marker, image size, and SHA256.
4. Verify the registered Note4 MAC and current image. Queue one authenticated Note4 OTA, confirm the new image identity, then install the matching Bridge build and publish the complete Note4 Bundle.
5. Confirm the job and device active template/commit state. Record exact evidence in `PROGRESS.md` and update the backlog only for genuine remaining work.

## Status

The user first selected Light 300. Bundle job `39e4084f` succeeded and the device reported commit sequence 354. The user then chose ExtraLight 200; asset `81d583bc` was selected by the device and family Profiles. Bundle job `5d798bd9` succeeded, with device commit sequence 355. A `waiting` response at publication time was transient, not a delivery failure.

The live device also reported `display_state=failed` after successful installation. Its old firmware counted a partial refresh failure as a final failure even when the fallback full refresh wrote the frame. The display-result fix now prioritizes a successful final write; host tests cover fallback success and actual failure. Corrected ROM `0.18.40` is `image_verified`; the new authenticated status reports `display_state=displayed`. The physical panel's exact font face still needs visual confirmation.

The device page previously refreshed the Bridge device cache summary only every 15 seconds through the 3-second `get_status` polling path, while open upgrade history remained static. It now reads the local cache every 3 seconds when the device tab is visible. MCP queries already read the current Bridge service state directly. Final default Bridge SHA256: `BA066990D60CCF2E8AAA86BC23D8B1D888C66433249893109DD052012C60F163`.
