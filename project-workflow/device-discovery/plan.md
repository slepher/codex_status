# 设备身份与发现（device-discovery）

用户需求（2026-09-19 晚，含修正）：

1. **设备唯一标识是 MAC**（学习并持久化，用于匹配/防串）；**名字不是唯一主键**
   （可重名），但**显示时以名字为主要对象**，MAC/IP 作为属性行；用户可在
   面板/MCP **重命名设备**。IP 运行时发现、可变、可持久化。
2. 新增 **BLE 主动协议**：经蓝牙连接读取设备 IP —— 兜底路径，需用户手动
   打开设备 BLE 会话（单击 BOOT）。
3. **ARP 按 MAC 扫描**：自动回退（同一二层网段）。
4. **关闭设备 mDNS**（`codex-status-XXXX.local`）——行为影响见下。
5. 面板/MCP 提供显式"重新发现设备"入口。

## 现状（代码事实）

- `bridge/crates/app/src/config.rs:21`：`device_ip` 同时充当身份与地址；
  `main.rs:1122-1123` `AppCtx.device_ip: Mutex<String>` + `device_mac:
  Mutex<Option<String>>`（学习到但**不持久化**）。
- UDP 通告（`main.rs:737-803`）：已知 MAC 才允许换 IP；未知 MAC 仅当广播
  IP == 配置 IP 才接受并学 MAC（`main.rs:767-782`）。**桥重启后配置 IP 过期
  则永远学不到新 IP**（新 IP 广播被丢、HTTP 也打旧 IP）。
- BLE 周期（`main.rs:989-1049`）：由设备 UDP `ble=1`（用户开 BLE 会话时发出）
  触发一次连接；`ble/src/lib.rs:374` 已读到 info JSON（固件含 `ip`、
  `http_port`，`src/main.cpp:700`），但 app 只打日志、未采用。
- 主机与设备同二层（PC 192.168.1.100 / 设备 192.168.1.50）。
- 固件 mDNS 由 `ArduinoOTA.begin()` 启动（`src/main.cpp:1923`），另
  `MDNS.addService`（:1924）；`mdns_timer` 100ms → light sleep 期间 10 次/s
  唤醒（PROGRESS/task-2 实测）。

## 目标架构

- **唯一键**：`device_mac`（学习并持久化，UDP/ARP/BLE 匹配与防串设备以此为准）。
- **显示名**：`device_name`（非唯一，可重名；bridge 本地，用户可重命名；默认
  首次识别时取 `CodexStatus-<MAC 后缀>`，改名不写设备）。
- **属性**：`device_mac`、`device_ip`（最近一次发现的地址）。旧
  `bridge-app.json` 只有 `device_ip` → 启动照常用作提示；首次学到 MAC 时生成
  默认名字并写回配置。
- **重命名**：面板设备页可改名；MCP 增 `device_rename {name}`（写
  `<data>/bridge-app.json`，与 `persist_mcp_port` 同样 read-modify-write）。
  仅校验非空/长度，**不校验重名**（名字只是显示标签）。
- **显示规则**：面板/MCP/日志以名字为主对象，MAC/IP 作为属性行展示
  （名字缺失时回退显示 `CodexStatus-<MAC 后缀>` 或 MAC）。
- **解析顺序**（HTTP 轮询/推送/OTA/MCP）：
  1. 属性 IP（在线则用）；
  2. 失败 → UDP 通告（设备主动，含 MAC/IP/时间，主路径）；
  3. 失败 → **ARP 按 MAC 扫描本机 /24**（自动，秒级，仅同二层）；
  4. 失败 → **BLE 主动读取 info `{mac, ip, http_port}`**（用户手动开 BLE
     会话；设备侧 `ble=1` 通告会自动触发桥的 BLE cycle，桥顺带采纳
     info.mac/info.ip，无需额外点击）。
- 面板设备页、MCP `bridge_status` 输出 `name`（主）+ `mac`/`ip`（属性）+
  最后发现方式/时间。
- 提供显式动作 `device_discover`（面板按钮 + MCP 工具，可选 `via:
  auto|arp|ble`），仅用户触发；保持"无周期 BLE 扫描"不变。

## 关闭 mDNS 的行为影响

**失去**
- `codex-status-AABBCC.local` 名字解析（浏览器/`ping`/`curl` 手输名字）。
- `_http._tcp` 服务广播；Arduino IDE 的"网络端口"自动发现（ArduinoOTA 仍可
  手输 IP:3232 使用）。
- 桥、面板、MCP、device-auth、测试桥、Python 工具**均不依赖** mDNS，无影响。

**保留 / 收益**
- DHCP hostname（`WiFi.setHostname()` option 12）仍在，路由器后台仍显示设备名。
- 去掉 `mdns_timer` 10 次/s 唤醒与 mdns 任务（~1.6KB 栈、少量堆），25ms 轮询
  下约再省 5–10 次/s 唤醒 ≈ 0.3–0.5mA（估算，T10 可验）；长空闲时少切开睡眠。
- 设备 IP 发现完全走 UDP/ARP/BLE，不再依赖名字；人工需要名字时可用路由器
  后台或 ARP 结果。

**风险**
- "设备换网 + 桥重启 + 不同二层（ARP 不可达）"时，只能靠用户开 BLE 会话让
  手动路径兜底；已在流程里接受这一点（用户手动动作）。

## 设备占用（claim/lease）协议

需求：bridge 需要**主动发起通信并占用设备**的能力——**bridge 根据设备空闲
情况自行决定是否占用**（自动 claim），用户只做强制接管/释放/恢复；Wi-Fi 端口
（`/status.json`）能得知设备被哪个 bridge 占用。

### 身份

- bridge：**复用现有 envelope `bridge.hostId`**（`core/lib.rs::short_id(hostname)`，
  HTTP/BLE 两条通道都已携带）作为 `bridge_id`；`bridge_name` 为 bridge 配置项
  （默认 `bridge.label`/PC 主机名，用户可改）；claim 上报 `host`（LAN IP）、
  `port`（8765）。
- 设备：占用状态持久化 NVS（跨 OTA/重启有效）。

### 设备侧状态（`owner`）

```
owner = { id, name, host, port, since_s, last_seen_s, lease_s }
```
- **owner 只由显式 `POST /claim` 写入/清除**（claim/renew/force/release）；
  `usage`/`template` 永不创建或转移 owner。
- lease 到期 → **清空为空闲**（不转移给任何人）；是否被接管由下一次显式
  claim 决定。
- 无 owner 或已过期 → 视为空闲；`/status.json` 增 `owner`（空闲为
  `null`，含 `expires_in_s`）。

### HTTP 接口（写操作一律 token 门控，沿用现有不变量）

- `POST /claim?id=&name=&host=&port=&lease=&force=1&release=1`
  - 401 无/错 token；
  - `release=1`：owner 匹配本 id 或 `force=1` → 清空 → 200；
  - 首次/续约：设备空闲、已过期或 owner.id==id → 写入/刷新（`since` 不变）→
    200 + owner JSON；
  - 被其他 bridge 占用且 `force` 未给 → **409 + 现 owner JSON**；
  - `lease` 默认 300s（允许 60–3600）。
- `POST /usage`：归属键取 body 的 `bridge.hostId`（两条通道都已有该字段）。
  - owner 存在且未过期且 id 不符 → **409**（`{"error":"occupied","owner":…}`）；
  - owner 相符 → 正常处理并刷新 `last_seen`；
  - 无 owner（未启用占用）→ 维持现状（兼容旧行为，不影响当前单桥日常）。
- `POST /template`：`bridge_id`（= hostId）随 query/header 传入，同一 owner
  规则；非 owner 409，面板先接管再推。
- `GET /status.json`：`owner` 字段（面板/MCP 读）。

### Bridge 行为（自动占用 + 用户例外动作）

- **自动占用决策**（每个同步周期/启动时，基于设备状态）：
  - owner 为空/已过期 → **主动 claim**（幂等 `POST /claim`）→ 正常推送；
  - owner == 自己 → 推送 + 每 60s 续约；
  - owner == 他人 → **不 claim、不推送**，面板显示占用者与"强制接管"；
  - 设备离线/尚未发现 → 走发现链（task-2），恢复后重新决策。
- 用户动作：**强制接管**（force claim 成功后自动重试）、**释放**（POST
  release + 本地进入"让步/暂停"状态，不自动重新 claim，直到用户点"占用/
  恢复"）、**重新发现**。
- MCP：`device_owner`（只读）、`device_claim {force?}`、`device_release`
  （`device_claim` 无 force 时也可手动触发一次自动决策）。
- token：优先用缓存；401 时提示走 BLE 取 token（需用户开设备 BLE 会话）。
- 旧版桥（无 hostId 的写入）在 owner 有效时一律被 409 挡住（独占语义，无
  例外）。

### 边界

- 占用是**可选**能力：无人占用时一切同今天；不能因引入协议破坏单桥体验。
- 设备时钟未知用 `millis()`；`since/last_seen` 展示为设备 uptime 秒。
- BLE 会话不受占用影响（占用是 Wi-Fi 通道概念）；OTA 仍只认设备 token。

### 与既有 activeMac/超时抢占的合并（`docs/history/sleep-plan-v4.md` D5、`main.cpp:335-353`）

现状（多层已实现）：`activeMac`/`activeAt`/`activeHoldSec`（envelope
`active_hold_seconds`，默认 600s）记录"谁在显示"；`usageAccepted()`：同
mac 接受；他机在 `now-activeAt >= hold` 后抢占；active endpoint 的 BSSID
与当前不符（视为不可达）立即抢占；`activate:true` 显式覆盖最高优先。

合并原则：**claim/lease 是独立的显式占用层**（`owner.id = bridge.hostId`），
`activeMac`/`activeAt`/`activeHoldSec` 的显示接受规则原样保留，但**两者不互相
转移**：

| 场景 | 现状（activeMac 显示层） | 占用层（显式） |
|---|---|---|
| 同桥 claim 续约 | 不变 | 刷新 `last_seen` |
| 他桥，lease 未过期 | 可 `accepted:false` 等待 600s | **409，仅 force claim 可立即接管**（或等 lease 到期清空后重新 claim） |
| 他桥，lease 过期 | — | owner 自动清空；之后按旧规则同步或显式 claim |
| BSSID 变化 | 立即抢占（显示） | **不改变 owner** |
| `activate:true` | 显式覆盖（显示） | **不是 owner 旁路**；owner 存在时写仍 409 |
| 未 claim 的首个同步 | 接受（旧规则） | 接受，但**不创建 owner** |

- 未 claim 时行为与今天完全一致（占用是可选能力）；claim 后写操作才受独占
  约束。
- 显式 claim：lease 默认 300s、60s 心跳；桥若不 claim，永不成为 owner。

## 任务

- **task-1 身份重构**：Config/AppCtx/UDP/poller/BLE/MCP/面板改为 MAC 唯一键 +
  显示名 + IP 属性；学习结果持久化；改名；向后兼容旧配置；cargo 回归。
- **task-2 发现回退**：ARP 扫描（Windows `SendARP`，cfg 门控）+ BLE info 采纳
  （自动于 `ble=1`，手动 `device_discover`）+ 持久化；面板/MCP 入口。
- **task-3 固件 0.13.4-bw**：关 mDNS（`setMdnsEnabled(false)`，处理
  `MDNS.addService`）；BLE info 增 `mac`；OTA 实测（`.local` 失效、DHCP 名
  保留、HTTP/MCP 不受影响）。
- **task-4 占用协议**：固件 `POST /claim` + owner 状态 + `/status.json` +
  写路由 `X-Bridge-Id` 门控；桥侧 bridge_id/name、claim/续约/释放、面板与
  MCP 入口（依赖 task-1/2 的发现与持久化）。固件版本并入 0.13.4（与 task-3
  同一 ROM，若分两次则 0.13.5）。
- **task-5 文档/回归**：`docs/power-state.md` §9（发现/身份/占用）、PROGRESS、
  AGENTS（如需）、cargo/node/python 回归、`git diff --check`。

DoD：桥在"设备换 IP + 桥重启"两种顺序下都能自动恢复（同二层 ARP；异地需
用户开 BLE）；面板/MCP 以名字显示、MAC/IP 为属性；占用/释放/强制接管全流程
实测（409 语义正确）；mDNS 关闭后功能无回归；所有证据进 PROGRESS。不提交
（等用户同意）。
