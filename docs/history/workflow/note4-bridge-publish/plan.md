# Note4 Bridge publish implementation plan

Source: [design.md](design.md) §6. This task owns Bridge code and this directory. Existing uncommitted work, Note4 ROM/partition files and firmware sources belong to other work and must be preserved.

1. Validate authenticated device capabilities, target geometry, ABI and reported limits; retain the old full-bundle route.
2. Bind each device Profile to an exact render target and explicit `font name -> font_id`; migrate only when authenticated capabilities resolve the old target. Import validated CSFN and freeze bytes at publish time. Show TTF/OTF conversion as unavailable until packaged.
3. Use the existing shared host engine for 400×300 and 200×200 regression. Asset-reference ABI work depends on the device implementation and protocol agreement.
4. Define a canonical full manifest, content objects, pure differential planner and peak-space preflight. Persist frozen jobs for restart recovery.
5. Finalize [protocol.md](protocol.md) with the device task before writing an HTTP client. Do not invent live endpoints or confuse ROM A/B OTA with asset installation.
6. Expose a shared read-only preview and font selection/import through UI/MCP; keep save local and publish explicit.
7. Run the host acceptance matrix, isolated Rust tests, compiled-render comparisons and `git diff --check`. Record real-device testing separately.

## Dependency checkpoint

At start, firmware reports `CT_ABI=1`; `font_asset.h` caps one font at 48 KiB, `font_store.h` caps eight fonts, and no versioned manifest endpoint is present. The new transfer client and asset-reference compilation cannot be claimed interoperable until the firmware task agrees on the protocol and updates these limits/ABI.

## Follow-up: explicit MCP light PowerPlan

1. Make `power_plan light` and the UI power action create a fresh formal light plan rather than use an ordinary rendezvous decision.
2. Persist a single pending explicit plan and its ACK state; deduplicate repeated requests while queued and preserve the same plan ID across Bridge restart. The plan goes through authenticated BLE v2 on the next deep-sleep rendezvous, or HTTP while the device is online.
3. Hold the accepted light window so routine automatic rendezvous cannot immediately undo it. Preserve ordinary no-work sleep behavior and the rule that reads do not extend light time.
4. Add host tests for queue/retry/restart/ACK/expiry and document the exact MCP response. Avoid live writes during the device task's USB 600-second observation; perform a bounded real-device test afterward.
