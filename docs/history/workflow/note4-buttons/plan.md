# Note4 awake button behavior

## Decision

Note4 GPIO39 (PGUP) is not a deep-sleep wake source. The user chose PGUP as an awake-only template-cycle button and confirmed that GPIO0 (ENTER) alone is sufficient for deep-sleep wake. GPIO18 (PGDN/power) retains its existing behavior. A reported ENTER press before USB attachment produced no visible response; establish from reset/wake and display evidence whether the MCU woke before attributing the symptom to the panel or wake configuration.

## Task

1. Configure PGUP as active-low input on the Note4 normal runtime path.
2. A debounced PGUP press cycles one installed template through the existing `nextTemplate()` path exactly once. Do not change the 1.54 button path or v2 context semantics.
3. Build Note4 B and 1.54 firmware in this isolated worktree, inspect diff, and record the outcome in `status.md` and `PROGRESS.md`. Hardware verification remains a separate step.
