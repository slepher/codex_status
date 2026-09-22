# Task 0 — compatibility baseline (M0)

Goal: freeze current behaviour as regression evidence before the v2 model lands.

## Baseline inventory

| Invariant | Where it is enforced today | Test |
|---|---|---|
| 3-way canonical JSON + CRC32 hash | `bridge/crates/core/src/template.rs:319`, `tools/test-bridge/bridge.py` | `crates/core/tests/template.rs:13` |
| Unknown `type/font/bind` rejects whole template | `template.rs:294` (Rust), `src/template_engine.cpp:597` (fw) | `tests/template.rs:100` |
| Host preview == firmware pixels | `crates/render` compiles the firmware engine | `render/tests/policy.rs`, `tools/test-quad-preview.mjs` |
| ASCII sanitisation | `template_engine.cpp:56` | engine dry run |
| 5h missing → static 100 / hidden reset | `envelope.rs:28`, firmware `findWindow` | `tests/envelope.rs` |
| RC `availableCount<=0` → row hidden | `envelope.rs:174` | `tests/envelope.rs:117` |
| No username → row hidden | `envelope.rs:99` | `tests/envelope.rs:140` |
| token gates `/update /doUpdate /claim` | `main.cpp:2595` | on-device |
| claim is the only owner write | `main.cpp:2631` | on-device |
| MAC identity | `main.cpp:442`, `udp_listen` | `app/src/discovery.rs:192` |
| Save ≠ push | MCP/app commands | this task |

## Deliverable

`crates/core/tests/legacy.rs`: golden fixture test pinning the legacy envelope
shape consumed by firmware ≤0.15.10 (`schema, server_time, bridge, account,
buckets, templates, mode/usage_rev` merge) plus the legacy `/template` transfer
frame (2-byte LE offset chunks, CRC32 = hash). This is the interop contract the
v2 path must not break while legacy devices remain in the field.
