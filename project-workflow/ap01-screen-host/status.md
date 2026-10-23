# Status

## 2026-10-22 physical entry regression and fix

The shared-host candidate01 forced a redraw and retired the current frame on every exit. After diagnostics stopped, a single knob detent still took seconds to enter App. The native red test reproduced an unnecessary generation3→4 redraw. Candidate02 retains the existing current I4 front buffer and returns READY only for the same valid generation/template, without pending activation/frame, replacement, rollback or render failure. Unbind still retires it unconditionally. No new thread or frame copy; retaining the existing buffer costs38464B while on a stock page.

Native regression passes both entry directions without redraw, latest offscreen data, Bundle invalidation and failed-frame rejection; host sanitizers and boundary/source/image audits pass. Evidence: `artifacts/ap01-entry-latency-20261022/entry-offline-acceptance.json`. Candidate02 was flashed once to application1:7043416B, builddce8d73818bb4a916dfdffaf1771fe46, SHA256470006a2fae067a1cf7b083b75c8631f9c151925786bb96aaca00a5d8b3d058d. The user confirmed normal single-detent entry from Settings clockwise and GIF anticlockwise, with normal App content. Rescue0 was never a write target. Wi-Fi RF testing and full rescue post-readback are still in progress; this does not close power-loss or resource high-water acceptance.

The offline coexistence and UI recreation continuation is complete. No flashing, device access, Bridge deployment or commit. Deployed34 remains unchanged.

- Final probe candidate08: 7,006,088 B, SHA256 91d33a5825e4ff7e89440b2e43a7fd5b6bd31549cbdf5960dbc6089c320d1b14. Native run10 passed eight codex templates plus independent two-page app, foreign Bundle updates with focus lock, real escape, and two full stock-window recreations (one during animation at x=-279).
- Final ordinary candidate03: 7,003,496 B, SHA256 8b56c53bc86f255196d2a39cd43faa7b373e82c6b86ae43449bf2ac917e4cde4. Ordinary ELF SHA256 015de1182c2a99b9a0dd40df276934aff13a693fbae066e67318f31153615ad1. No peer/probe entry. Native run03 passed navigation/Bundle/activation/draw regression.
- Runtime/service/timer reuse, old-epoch and revision rejection, stock subscription retirement/rebind, maintenance/discovery while detached and six pixel checks passed. Core ASan/UBSan passed. Sources and every input header match each candidate audit.

[Full report](../../../cuktech-ap01-firmware/reverse/reports/ap01/screen-host-lifecycle-20261020.md). Local evidence: artifacts/ap01-screen-lifecycle-20261020/ and artifacts/ap01-screen-lifecycle-production-20261020/. Exact OTA/reboot, physical LCD/SPI, power loss and true resource high-water remain separate acceptance; both manifests remain unreleased/non-flashable.
