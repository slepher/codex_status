# AP01 shared screen host refactor

Move reusable 0041 window routing, root lifecycle and LVGL/I4 bindings into cuktech-ap01-firmware. Adapt codex_status to app-owned dynamic screen sets, independent roots, stable template IDs and actual draw receipts. Keep existing Bundle, maintenance and business protocols. Offline work only; no flashing or Bridge deployment.

## Continuation: coexistence and UI recreation

Implement retained UI lifetime anchor, preserve one service executor set across detach/reattach and reject frames from retired UI epochs. Add an opt-in native second-app fixture and execute actual LVGL coexistence/recreation probes; document stock-page fixture boundaries. Offline only, no flashing.

## Physical rollout entry-latency regression (2026-10-22)

User reports several-second App entry from both Settings clockwise and Pet anticlockwise, even after batch diagnostics stop. Compare deployed06 cache reuse against current prepare/full redraw and surface disposal. Preserve one current I4 surface for immediate unchanged-page entry; retire on unbind/replace/root deletion, reject failed/pending state and retain existing bounded async preparation for other templates. Add runnable native generation/context/updated-data/retirement checks, reproduce old behavior, audit RAM lifetime, rebuild exact candidate, then repeat authorized application1 deployment and physical entry/menu/Wi-Fi checks while preserving rescue0. No Bridge restart; do not count current rollout as UI acceptance.
