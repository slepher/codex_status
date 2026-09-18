# task-7 — 模板推送改走 HTTP（BLE 收敛为身份/token）

Status: done 2026-09-18. Spec: `docs/power-state.md` §5/§9；交接：`PROGRESS.md`
顶部（v0.12.5 一节）。设计裁定（用户）：BLE 只负责配对/绑定、endpoint/token
下发、OTA token 取用；模板推送与日常数据一律走 HTTP。

## 目标

- 固件新增 `POST /template`（endpoint token 门控；`requestAuthorized` 不适用），
  query `id/version/hash/activate`，body 原始模板 JSON；`tplValidateForStorage`
  （CRC/min_fw/dry-run）→ `tplStoreSave` → 可选 `tplStoreSetActive` →
  `updateInfoExtra()` + 重绘；错误码 400/401/413/500。
- 桥 `push_templates_http(cfg, ids, activate)`：先 GET `/status.json` 比 hash
  跳过未变化模板，再逐一 `POST http://<device>/template`（Bearer endpoint
  token）；MCP `profile_push` 与面板 `push_profile` 都用它。
- 删除面板 `PendingPush` 与 BLE 循环中的模板消费；BLE 循环只保留 UDP
  `ble=1` 触发的 endpoint/token 交接。

## 实现

- 固件 `src/main.cpp`：版本 `0.12.6-bw`；`handleTemplatePost()` +
  `server.on("/template", HTTP_POST, ...)`；`activeTplId` 缓存失效；未变化仍
  可仅激活（hash 相同时跳过落盘）。授权沿用 `endpointTokenAuthorized`
  （与 `POST /usage` 同一 endpoint token）。
- 桥 `bridge/crates/mcp/src/lib.rs`：新增 `push_templates_http`（激活目标最后
  发送；未变化但非 active 时重发以激活）；`profile_push` 改调该函数；删除
  `BleConfig/RwLock/Arc` 模板推送代码。
- 面板 `bridge/crates/app/src/main.rs`：`push_profile` 改 HTTP 并返回摘要；
  删除 `PendingPush`、`pending` 字段与 BLE 模板消费；BLE 循环
  `template_ids: Some(vec![])`（仅 endpoint/usage 交接）；`mcp_config` 提取
  供 MCP handler 与推送共用。`ui/index.html` 推送提示改 HTTP、去 pending 轮询。
- 运行形态：现有 `bridge/target/debug` 进程按新规则重启（先停 watchdog 304
  再停父 50048），`cargo build -p bridge-app` 后用 `pwsh tools/start-bridge.ps1`
  启动（PID 41820）。

## 验证（真机）

- 固件 `pio run`；ROM `artifacts/codex-status-0.12.6-bw.bin` SHA256
  `82B0AF5D52F6A718324FCB9B1DC3E3D41732019D8C6208E3E389C45751FF9E63`；
  MCP `firmware_ota` 0.12.5→0.12.6 成功（纯 HTTP，缓存 token）。
- MCP `profile_push default`：`pushed 1 (quad); skipped 2 unchanged`；设备
  `/status.json` quad `c598adc0` active，`/log` 见 `[tpl] http saved id=quad`。
- 幂等：再次推送 `pushed 0 (no changes; skipped 3)`。
- `config-tbif`（mini 未变化但非 active）→ `pushed 1 (mini; activated)`，
  设备 active 切 mini；再推 default 恢复 quad active。
- 错误码实测：无 token 401、错 hash 400、>32KB body 413。
- 回归：`cargo test --workspace` 16 项、python 4/4、node 7 fixtures、
  `git diff --check` 0。
