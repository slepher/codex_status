# Clock window retention across v2 rendezvous

## Observation and cause

On the 1.54-inch device, minute refreshes have been observed alternating between partial and full refresh. A thin timer wake uses the retained `clkPixels`, but a rendezvous boot calls `v2ActiveLoad()`, whose `clkComputeRectCt()` currently invalidates those pixels even when the context and clock region are unchanged. Rendezvous and minute-boundary wakes can alternate because the next rendezvous is scheduled 60 seconds after a completed window.

## Change

Retain the RTC clock-window pixels only when the loaded v2 context ID and every clock-region field match the pixels' recorded source. Invalidate them on cold boot, context/template changes, or a changed region. Keep the existing full-refresh fallback when the baseline is absent or a panel operation fails.

## Check

Build the 1.54 firmware and run `git diff --check`. Do not flash the device in this task. The user's observed alternation requires a later hardware check to confirm the visual result.
