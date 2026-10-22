# Fast boundary-wrap animation — 2026-10-22

Scope: user requested simulator verification first, no device flashing. This follow-up made no LAN/device access, configuration changes, Bridge restart, commit or push. Full rescue readback remains stopped at the earlier explicit user request.

## Measured cause

The original RV32/LVGL selection path is correct. The shared firmware menu adapter requested animated scroll even for first/last wraps. On the four-row list, the native viewport is y60..239; reverse wrap initially selects a row at y279..346, forward wrap selects y-52..15. The selected boundary row is therefore outside the viewport until animation catches up.

Guest-timed original encoder processing with two separate samples 101–102ms apart reproduces selection paths0→3→2 and3→0→1. On the old ROM neither boundary row is fully visible in any of its three observed selected samples. Slow reverse input613ms apart gives the animation time to finish; first complete boundary visibility is357ms after input. This matches the user's slow/fast difference and retained physical per-index events, but does not prove all physical rotary reports share one cause.

## Minimal correction

Firmware repository owns `tooling/firmware-modules/native_device_code_menu_0041.cpp`: only wraps between first/last selectable rows pass animation OFF to the original scroll function. Ordinary adjacent movements retain animation. No debounce, step loss/clamping, wake bypass, index/order, checkmark or Wi-Fi worker change.

With the same original encoder processing and corrected explicit input scheduling, fixed reverse/forward input gaps are102ms and boundary rows are fully in the native viewport within1ms; slow control within2ms. All three selected samples of each fast boundary are fully visible, with identical index paths. Raw target rectangles are y167..234 (last) and y60..127 (first).

## Validation and limits

- [Guest-time red/green acceptance](../../artifacts/ap01-wifi-animation-20261022/animation-acceptance.json): both fast directions and slow control, original RV32 execution clean.
- [Host red/green](../../artifacts/ap01-wifi-animation-20261022/menu-host-report.json): old policy rejected, fixed ASan/UBSan menu routing/wrap/adjacent-animation assertions pass.
- RV32 compile/audit/link/pack passed;316 source/header hashes match the compiled candidate. No new heap, persistent state or thread; select/event stack frames stay48/128B. Image grows32B.
- Full original menu/driver regression passed with clean native execution: empty/single/typical/max lists, both wrap boundaries, checkmark/focus separation, stock brightness/version/screensaver, device-code return, Return home and delete/recreate; synthetic driver success/duplicate/failure rollback remains model evidence. Native max-top/current RGB565 previews reviewed.
- Native RGB565 samples are retained, but virtual fbdev readiness can lag current selection. The proof above concerns original LVGL viewport placement, not an immediate presented physical LCD frame. Actual LCD cadence and physical GPIO bounce remain unverified. Same-frame multiple steps may legitimately share one rendered frame; no artificial per-detent delay is introduced.

Earlier controls are not product failures: host action queries advanced hundreds of guest milliseconds; queued input behind virtual fbdev completion was delayed; natural encoder polling was not supplied by that fixture. Final baseline02/fixed02 uses explicit original encoder calls scheduled on guest UI ticks ahead of virtual completion and records actual dispatch times. Geometry is RV32 `<4i`, not the previous incorrect16-bit unpacking.

## Offline candidate, not deployed

`artifacts/ap01-wifi-animation-20261022/ap01-wifi-animation-candidate01.bin`:7045480B; build`ec0a58f71f65c2830c08c9bfd7c155db`; SHA256`5a49f78b550fabbafcc3cde75aa1f6e9986fb9dd88320cd029cd6655d61e0cee`.

Physical AP01 remains the log ROM build`f8e9001b8a004bc65b70b6f07c50525c` in application1, with rescue0 preserved. No physical result or new deployment is claimed.


## Authorized physical rollout and acceptance

User subsequently authorized this exact candidate on AP01, preserving rescue0 and verifying only authenticated version/slot. Independent0031 rescue0 was authenticated before one upload to inactive application1; new build ec0a58f71f65c2830c08c9bfd7c155db/7045480B is authenticated running on the same MAC in slot1. No rescue ROM writes, full-slot/file exports, configuration changes, Bridge restart, commit or push. User confirms native interface/knob/App contents normal and both fast wrap directions no longer skip boundary highlighting. The reported fast-wrap symptom now passes physical visual acceptance; this does not assert GPIO bounce immunity or visibility of every intermediate item when multiple steps share one frame. Retained trace is partial (52 earlier events overwritten, producer drops0); do not claim complete two-direction reconstruction. [Rollout evidence](../../artifacts/ap01-wifi-animation-rollout-20261022/physical-verification.json).
