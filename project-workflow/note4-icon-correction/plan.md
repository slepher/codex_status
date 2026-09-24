# Note4 status icon correction — 2026-09-24

## Goal

Correct only the 400×300 template artwork and state selection. Bridge and Wi-Fi use mutually exclusive on/off icons. BLE OFF leaves its cell empty. The original sleep-20 artwork appears in the BLE cell for deep mode. Reduce the visible date/time gap by half and keep battery text at the right margin.

## Sequence

1. Restore the original 20×20 sleep bitmap in the Note4 icon source and regenerate its bitmap assets.
2. Change only the Note4 template icon elements and header spacing; document the state mapping.
3. Validate template compilation and rendered state cases with the existing host renderer; review the diff.

## Boundary

Do not edit ROM or Bridge code, deploy, publish, restart services, or touch the device. Preserve concurrent worktree changes.
