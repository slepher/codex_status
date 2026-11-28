# AP01 unified build

User scope: firmware repository owns platform compilation, audit, link and ROM packaging; codex_status supplies App source/config only. Keep existing command names as forwarding entries.

1. Centralize platform source list, compiler/link flags and audit/link validation in firmware.
2. Declare App sources and options in ports/ap01/app-build.json; replace duplicated App tools with forwarding entries.
3. Validate standalone, App, legacy staged commands, source ownership, stale-input refusal and fresh isolated build.
4. Record candidate sizes/hashes and update handoff. No flashing or deployment. Paired commits explicitly authorized on 2026-10-27; no push.

## 2026-11-28 App 功能命名

按功能重命名本仓 App 文件，版本取自运行中平台的认证维护接口。
