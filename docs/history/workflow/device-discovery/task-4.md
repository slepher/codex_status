# task-4 — 设备占用（claim/lease）协议

状态：done（2026-09-19，固件与桥均已实测）

## 固件（并入 0.13.4-bw，`src/owner_store.*` + `src/main.cpp`）

- NVS `owner = {id,name,host,port,since,seen,lease}`（namespace `owner`）；
  过期按 `millis()` 判定，过期即清空（不转移）；重启后 `since/seen` 按当前
  uptime 收敛（避免时钟基准变化导致假过期）。`activeMac/activeAt` 保留不变。
- `POST /claim`（`requestAuthorized()`，即设备 token 门控；`id` 必填）：
  `release=1` 匹配或 `force=1` → 清空；空闲/过期/同 id/force → 写入，
  同 id 续约保留 `since`；他人且未过期且无 force → **409 + owner**；
  `lease` 默认 300、范围 60–3600；名称/主机支持 UTF-8，按字符截断。
- `usage`（`bridge.hostId`）/`template`（`bridge_id` query 或
  `X-Bridge-Id` 头）owner 校验：无 owner 完全走旧规则且不建 owner；
  owner 有效且 id 不符 → 409 `{"error":"occupied","owner":…}`，`activate`
  不是旁路；BLE usage 维持忽略（无 409 通道）。
- `GET /status.json.owner`（空闲 `null`，含 `expires_in_s`）；HTML 状态页
  同步显示 Owner。

## 桥（`bridge/crates/app`）

- `bridge_id` 复用 envelope `bridge.hostId`；`bridge_name` 默认主机名
  （Unicode）；`POST /claim` 用缓存 `data/device-token.json`（设备 token），
  401/缺失只提示开 BLE 会话；显式动作（面板/MCP）允许一次 BLE 取新 token 重试。
- 自动决策（push loop 内，写前执行）：空闲/过期→claim；己方→60 s 幂等续约
  （与推送解耦，无数据变化也续约）；他人→不 claim、不推送并显示占用者；
  离线→走 task-2 发现链。收到 409 显示不静默重试。
- 用户动作（面板设备页 / MCP `device_claim{force?}`、`device_release`）：
  强制接管、释放（POST release + 本地 yield，不自动重占）、占用/恢复；
  旧固件（无 /claim，404）自动回退旧推送行为。

## 验收证据（实测）

1. 单桥空闲：设备重启后桥自动 claim，owner `id=1a2b host=192.168.1.100:8765
   lease=300`；`/status.json.owner` 可见；续约实测 `last_seen 159→221`（62 s）
   且无数据推送。
2. B（`testbridge`，同 token）claim 被拒：`409 owner=1a2b`；B usage
   `activate:true` → 409；**无 hostId 的 usage → 409**；B template
   `activate=1` → 409；B release → 409。A usage → `200 {"accepted":true}`。
3. B `force=1` 接管 → owner 变 B；A 侧 `note="被 TestBridge@…:9999（剩余 Ns）占用"`
   且不再推送；A `device_claim force` 夺回 → note 清空、推送恢复
   （日志 `usage push -> 200`）。
4. release/让步：A `device_release` → owner null、`yielded=true`；B claim 成功；
   A 非 force claim 报 occupied（设备裁决）；B release 后 A claim 成功。
5. lease 到期语义：B `lease=60` 后停止续约 → 70 s 后 `/status.json.owner=null`
   （不转移），随后 B 重新 claim `renew=false`。
6. 无旁路：`activate:true`、BSSID 变化均不改变 owner（BSSID 路径未单独复测，
   代码未触碰）；旧版无 hostId 写入被 409。
7. OTA/重启后 owner 仍在 NVS：同版重传 0.13.4 触发重启 → `uptime=70s` 时
   owner 仍为 1a2b（`since=4` 收敛、`expires=234`），桥随后自动续约；
   `/claim` 缺失时旧固件回退也已覆盖（0.13.3 阶段实测 404 → legacy push）。
8. `cargo test --workspace`（隔离 target）全绿；`git diff --check` 干净。
