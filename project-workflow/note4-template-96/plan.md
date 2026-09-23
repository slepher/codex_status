# Note4 96px quota layout — 2026-09-24

## Requested outcome

- `codex-status-a` on the 400×300 Note4 uses 96px digits for every usage value (both dual-bucket and weekly-only branches).
- When only weekly exists, center its value block horizontally. Keep the weekly reset time, but remove the visible `RESET` word.
- Bluetooth, Wi-Fi and Bridge connection icons react to device state with the same user-visible on/off meaning as the old `quad`, instead of remaining fixed.

## Measured constraints

- Generated Noto Sans Regular 96px digits: `100` advance 165px, ink box 151×71px; `100%` at the same font has 245px advance. The 400px canvas has 376px inside 12px margins. Current weekly-only number region is only 132px, so it must widen and center. Dual ~176px columns can fit the digits plus a separate small percent only by positioning to the actual ink/advance bounds.
- 96px font line box is 132px. WEEK label and reset-time line boxes are 26px each: 184px total inside the 184px main band, before spacing. Use actual rendered ink and a composed preview to place without collision; do not claim fit from advance alone.
- No built-in 96px font exists. The current asset-font transfer protocol is not deployable. Append a target-gated built-in `ntreg96` face; preserve all existing font indices. Host renderer must include this face for 400×300 preview and Rust template validation must reject that face on 1.54 targets.
- User approved raising the compiled bitmap-resource cap from 8 to **16** to include Bluetooth-off, Wi-Fi-off, Bridge-off and `zzz` deep-sleep icons (two half-images each). The required state predicates use 57 operations, so the operation cap rises from 48 to **64** in the same ABI2 change. This is a local development device; old compiled Bundle compatibility is not required, and the full Bundle will be explicitly republished after the new Bridge and ROM are installed.

## Implementation sequence

1. Generate/register the Note4-only built-in font from the existing NotoSans-Regular source and native-hinted digits recipe. Update host renderer and target-aware validation. Verify existing 200×200 compiled rendering and 400×300 font metrics.
2. Edit only the 400×300 `codex-status-a` variant: layout both quota branches with 96px digits; center weekly-only; remove visible RESET labels but retain reset dates; bind icon variants to device state and show `zzz` in deep mode. Keep the 200×200 `quad` source unchanged. Stay within 64 compiled operations.
3. Render 5h+weekly (including `100`), weekly-only (`100`), missing quota, and connection-state cases. Inspect 400×300 PNGs for overlaps/cropping. Save template through the normal v2 library, then explicitly publish a complete Bundle only after a compatible Note4 ROM is installed and the user's requested visual is verified in host preview.

## Boundaries

- No unverified glyph, silent clipping, or automatic publication. A firmware release with the built-in font precedes installing the template that references it. Preserve other worktree changes and do not commit.
