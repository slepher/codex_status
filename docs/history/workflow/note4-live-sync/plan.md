# Note4 live Codex sync — 2026-09-24

## Evidence and target

- The tray icon has a current-looking `79` value. The terminal's `codex login status` is not evidence that the running Bridge lacks a Codex snapshot.
- Persisted Note4 (`7C4FADB93408`, `192.168.3.177`) has `sync_enabled=false`; its 5h and weekly bindings use test source `static1`. Its installed `codex-status-a` Bundle already exists. The 1.54 device (`70041DD7A340`) has live Codex bindings and sync enabled.
- `platform::deliver(ctx, mac)` currently builds its HTTP link from the global selected device, even when `mac` names Note4. This can send a Note4 decision to the 1.54 address.
- The Note4 template clock uses device-local `device.now`. Current SSD2683 driver partial entry points are stubs, so `TARGET_PARTIAL=0` leaves no clock region and the deep-sleep minute wake currently returns without drawing. Live Bridge Codex source quality is good. ZECTRIX's NOTE4 B/W reference driver provides a matching SSD2683 OTP partial-refresh sequence and requires a successful full-image base.

Driver reference: https://github.com/itopinion/zectrix-note4-epd-demo/blob/main/components/zectrix_epd/zectrix_epd.cc#L628-L698 (listed by https://wiki.zectrix.com/zh/software/opensource).

## Design

1. Route every `deliver(ctx, mac)` HTTP request by the requested normalized MAC and that device's recorded IP. Refuse an unknown MAC or missing IP. Preserve the existing endpoint token, owner checks, ACK handling and serialized delivery. Keep legacy/global UI routes unchanged in this narrow fix.
2. Correct Note4's existing per-device Profile through the normal service: replace the four `static1` bindings with equivalent `codex` bindings, retain template/order/target/font settings, and set `sync_enabled=true`. Save the Profile, then verify the coordinator contract. Template publication remains a separate explicit action; use it only if the installed Bundle must change.
3. Keep one common minute-clock wake decision and sleep schedule for both ROMs. The display driver selects a partial clock-window update when it has a valid old-image base; otherwise the shared rendering path performs a local full refresh with current device time. No Bridge fetch, owner/data sequence change or PowerPlan extension is caused by a clock-only wake.
4. Port the official NOTE4 B/W SSD2683 OTP partial-refresh sequence into the NOTE4 display driver, with old/new transition encoding, BUSY timeout, a valid full-image base prerequisite, and periodic full refresh through the existing refresh policy. Do not enable the target capability until this compiles and has a safe fallback. Deep sleep drops the panel rail and volatile full-frame shadow, so a deep wake may need a full refresh to re-establish the base; light-mode minute updates can use partial.
5. Confirm the running Bridge has a good Codex source snapshot, then observe a real Note4 data ACK (`data_seq>0`, applied sequence and display state) and matching device status. If Note4 is asleep, allow its normal BLE rendezvous and report the remaining wait rather than manufacturing an ACK.

## Checks

- A focused host test proves Note4 delivery chooses its own IP when the global device is 1.54, and unknown MAC cannot borrow the global link.
- Firmware build and focused host check cover the Note4 partial transition encoder and the shared clock wake/full-base fallback. On hardware, confirm a clean full base, minute partial update in light mode, deep-wake fallback, and periodic clean full refresh before calling the driver validated.
- Run focused Rust tests, `git diff --check`, and read-only live status after the change. Preserve all unrelated dirty worktree edits; do not commit.
