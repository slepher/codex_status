# Review — 2026-09-24

- Scope check: modified only the Note4 template JSON, its icon source/assets, and project documentation. Concurrent ROM/Bridge edits were untouched.
- Correctness check: 49 template operations and 14 bitmap resources remain under ABI2 limits (64/16). The existing `bridge-render.exe --compare-compiled` succeeded for five connection/deep states with zero pixel differences.
- Visual check: the final header experiment aligns `--%` ink center at 18 px and battery outline center at 17.5 px. No text y offset was guessed; the 26 px region matches the font's line height.
- Remaining validation: save and publish the corrected template only when requested; inspect the Note4 screen afterward. The old `make-template.py` predates the current 96px layout and should not regenerate this canonical JSON.
