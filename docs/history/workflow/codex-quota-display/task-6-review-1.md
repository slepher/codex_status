# task-6 Review Round 1

## Verdict

changes_required

## Findings

### Medium — WEEK detail text uses the wrong contracted y-coordinate

`task-6.md:33` requires the WEEK detail line at `y=124`, but `tools/generate-preview.mjs:181` renders `USED2 LEFT98` at `y=126`.

The PNG remains readable, non-overlapping, centered, and within the WEEK band, but does not satisfy the explicit coordinate contract. Smallest correction: change the command to `y=124`, assert that exact coordinate, regenerate, and repeat both test layers.

No other material finding remains. NEXT values are window-owned, RESET/EXP remain global-only, and the layout otherwise passes deterministic PNG, palette, frame, bar, overlap, centering and visual checks.

## Caveat

`NEXT 20:53` and `NEXT SEP 14 15:53` are fixed simulated fixtures, not live refresh times and do not establish timezone or refresh-protocol behavior.

## Continuity

- Next Task: task-6 rework
- Next Sol: task-6-review-2
- Reason: correct WEEK detail to y=124, strengthen assertion, regenerate, and repeat verification.
