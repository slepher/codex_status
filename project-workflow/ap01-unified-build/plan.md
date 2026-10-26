# AP01 unified build

User scope: firmware repository owns platform compilation, audit, link and ROM packaging; codex_status supplies App source/config only. Keep existing command names as forwarding entries.

1. Centralize platform source list, compiler/link flags and audit/link validation in firmware.
2. Declare App sources and options in ports/ap01/build-0041.json; replace duplicated App tools with forwarding entries.
3. Validate standalone, App, legacy staged commands, source ownership, stale-input refusal and fresh isolated build.
4. Record candidate sizes/hashes and update handoff. No flashing or deployment. Paired commits explicitly authorized on 2026-10-27; no push.
