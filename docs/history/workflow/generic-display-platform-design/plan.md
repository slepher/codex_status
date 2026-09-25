# Generic display platform architecture — plan

## Objective

Produce one implementation-independent, repository-grounded architecture for
evolving Codex Status into a multi-device, multi-display, template-driven,
multi-source low-power information terminal.

## Deliverable

`docs/generic-display-platform-design-v2.md`

The independent first pass is retained only as decision history at
`docs/history/generic-display-platform-design-v1.md`; it is not an active
architecture specification.

The document must cover the product/domain model, firmware/ROM architecture,
Bridge architecture, template compilation and data-requirement flow, device
and display targets, immutable deployment, transport/power integration,
four-tab UI, MCP parity, migration from the current repository, validation,
and explicit deferred scope.

## Constraints

- Existing behavior and security invariants remain available during migration.
- The current BLE rendezvous design is reference material, not an authority for
  the broader architecture.
- The design must avoid a big-bang rewrite and distinguish current facts from
  target decisions and unverified hardware assumptions.
- This milestone changes documentation only.
