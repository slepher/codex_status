# Task 3 — Bundle, profile, active context, data idempotency (M3)

Self-contained `Bundle` (v2 §3/§9): Profile order + initial active + all template
sources + all CompiledTemplates + bindings + render/firmware target contract +
embedded resources + total length/CRC.

Device A/B bundle (`bundle_store.cpp`, LittleFS `/bundle/{a,b}.bin` + commit
records with double copies and a storage sequence):

1. current slot always usable; new bundle writes the inactive slot;
2. receive complete, verify length/CRC/schema, compile every template;
3. read back and verify; only then write the commit record double copy;
4. power loss yields a complete old or complete new bundle, never a mix;
5. insufficient space rejects the publish without deleting the only valid copy;
6. previous complete bundle is recovery-only, no history UI;
7. recovery import is minimal, never auto-wakes/claims/publishes/changes active,
   and the recovered profile starts with sync disabled.

`active_context_id` is the only runtime context: new value on bundle commit,
explicit activate, unrecoverable cold start, and recovery from the spare bundle;
normal deep wake keeps it. Old-context Data/Bundle/Activate are rejected.

`data_seq` within one context: monotonic, same seq+content idempotent, same seq
different content conflict, older seq rejected, gaps allowed.
