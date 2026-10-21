# AP01 shared screen host refactor

Move reusable 0041 window routing, root lifecycle and LVGL/I4 bindings into cuktech-ap01-firmware. Adapt codex_status to app-owned dynamic screen sets, independent roots, stable template IDs and actual draw receipts. Keep existing Bundle, maintenance and business protocols. Offline work only; no flashing or Bridge deployment.

## Continuation: coexistence and UI recreation

Implement retained UI lifetime anchor, preserve one service executor set across detach/reattach and reject frames from retired UI epochs. Add an opt-in native second-app fixture and execute actual LVGL coexistence/recreation probes; document stock-page fixture boundaries. Offline only, no flashing.
