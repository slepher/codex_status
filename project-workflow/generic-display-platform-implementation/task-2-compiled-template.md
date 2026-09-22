# Task 2 — CompiledTemplate (M2)

One artifact carries field requirements and the render plan.

- Bridge `compile.rs` compiles template JSON at save/install time into a bounded,
  pointer-free, fixed-width record: `template_id`, `render_target`, `compiler_abi`,
  `requirements[]` (field index, kind, missing policy), `render_ops[]`,
  `local_dependencies[]`, `resources[]`, source CRC.
- Firmware `compiled_template.cpp` re-validates ABI, indices, lengths and resource
  bounds before use; corrupt/ABI-mismatch enters controlled recovery (recompile
  from the retained source in the Bundle, else previous complete Bundle).
- Normal wake/data/switch/render paths never parse template JSON; at most the
  active template's compiled record is loaded.
- Host parity: `crates/render` gains `codex_compile`/`codex_render_compiled`;
  a test asserts compiled rendering is pixel-identical to the JSON engine for
  full/mini/quad across normal/missing/stale/edge states.
- 8 templates must not be 8 live JSON DOMs: bridge compile peak test keeps only
  the active compiled record plus per-template fixed records.
