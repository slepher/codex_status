# Firmware entry-point modularization

Date: 2026-10-08. User authorized implementation: `main.cpp` should be glue only.

## Baseline and preserved semantics

Start from the current 6,421-line firmware, including the compact-protocol candidate recorded in PROGRESS. Preserve request/authentication/owner/session/replay semantics, A/B transactions, eight-template profiles, render outcomes, callback thread boundaries, RTC retention, exact setup/loop ordering, deadlines, OTA guards and GPIO/PM cleanup. No deployment, device commands, Bridge restart or commit is part of this refactor.

## Intended boundaries

- `main.cpp`: build version and OTA identity, Arduino stack hook, setup/loop forwarding.
- `application`: boot classification, initialization order and ordered cooperative loop.
- `display`: framebuffer, panel refresh, clock window and rendering.
- `platform`: active template/context, Data/Plan, activation and Bundle execution; reuse existing decision/storage modules.
- `transport`: HTTP/BLE authentication, compact framing, request/reply state and dispatch.
- `radio`: BLE/Wi-Fi lifecycle, discovery and ready-report adapter.
- `power`: PM locks, physical recovery, sleep/wake/GPIO and battery policy.
- `clock`: timezone, system/external RTC and SNTP handoff.
- `diagnostics`: wake trace, status/PM/frame projections and diagnostic controls.
- `sync`: frozen diagnostic synchronization and its bounded window.

State has one owning module. Module-local state remains private; state required by firmware integration is exposed only through internal typed per-module state declarations, not a single runtime bag or a list of hundreds of global externs. Ordinary and RTC state remain separate, are never copied onto the loop stack, and do not become persistence formats. dev_log now calls the owning sync module's log sink instead of importing its three storage globals.

## Steps and validation

1. Record baseline source/ELF and resources; inventory declarations/call dependencies before moving code.
2. Split complete functions and state by ownership, qualify cross-module access, retain local helpers and defaults. Keep the application sequence explicit.
3. Update source-based checks to inspect the new owning files; check startup/loop ordering and moved clock helpers, shared protocol/render tests and failure behavior.
4. Build only `zectrix-note4-b` through the target isolation script; inspect marker, version, size/hash, RAM/Flash, RTC sections and matching Xtensa Wi-Fi stack gate. No upload.
5. Review diff and responsibility/dependency boundaries; update PROGRESS with candidate evidence and remaining hardware validation. Backlog is the only pending-work ledger.

## Status

Implementation and authorized offline validation complete. main.cpp is 21 lines; nine modules own execution and state. Review and evidence: [review.md](review.md). No deployment or production runtime change; existing compact-protocol/hardware validation remains in the authoritative backlog.
