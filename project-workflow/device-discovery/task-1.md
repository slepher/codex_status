# task-1 — bridge 身份重构：MAC 为唯一键，名字为可编辑显示名

状态：done（2026-09-19）

## 范围

| 文件 | 改动 |
|---|---|
| `bridge/crates/app/src/config.rs` | `device_mac: Option<String>`（唯一键）、`device_name: String`（显示名）、`bridge_name: String`；env `CODEX_STATUS_DEVICE_MAC/DEVICE_NAME/BRIDGE_NAME` |
| `bridge/crates/app/src/discovery.rs`（新） | MAC 规范化、默认名 `CodexStatus-<MAC 后缀>`、ARP（task-2） |
| `bridge/crates/app/src/main.rs` | `AppCtx`：`device_mac`（键）/`device_name`/`device_ip`（属性）+ `save_identity()`（read-modify-write `<data>/bridge-app.json`）；UDP/HTTP/BLE 全部按 MAC 匹配（不符即拒绝）；Tauri `rename_device` / `device_discover` / `device_owner` / `claim_device` / `release_device` |
| `bridge/crates/mcp/src/lib.rs` | `McpConfig` 增 `device_name/device_mac/bridge_name/bridge_id`；`bridge_status` 输出设备 name/mac/ip/owner；`profile_push` 模板请求带 `bridge_id` |
| `bridge/crates/app/ui/index.html` | 设备页「身份」卡：名称（点击重命名）、MAC、IP、最后发现方式/时间、占用 |

## 要点

- 迁移：旧配置只有 `device_ip` → 启动照常用；首次从 UDP/HTTP/BLE 学到 MAC 时
  生成默认名并写回配置；此后 MAC 不符的通告一律拒绝（防串设备）。
- 名字仅存 `<data>/bridge-app.json`，不写设备；Unicode 允许（仅替换控制字符），
  设备侧 `/claim` 也保留 UTF-8（按字符截断，不切多字节序列）。`bridge.hostId`
  的散列输入（ASCII 化 host label）保持不变，owner id 与 envelope 一致。
- 不改变"无周期 BLE 扫描"与推送/OTA 的显式动作约束。

## 验收证据

- `git diff --check` 干净；UI JS `node --check` 通过（脚本块提取）。
- 默认名：UDP 学习（日志 `device identity learned via udp: 70041DAABBCC
  (CodexStatus-AABBCC)`）；`bridge-app.json` 持久化
  `device_mac=70041DAABBCC/device_name=CodexStatus-AABBCC`。
- 改名持久化：MCP `device_rename {"name":"书桌屏"}` → `bridge-app.json`
  内容逐字符比对一致（3 字符）；桥重启后 `device_owner` 仍为 `书桌屏`。
- 配置 IP 错误 + MAC 正确：把 `bridge-app.json` 的 `device_ip` 改为
  `192.168.1.199` 重启桥 → 日志
  `device HTTP unreachable; ARP fallback scan for 70041DAABBCC` →
  `device endpoint updated: 192.168.1.50 (via arp)`（task-2 的 ARP 恢复，
  也验证本 task 的"MAC 为键、IP 为属性"）。
- `CARGO_TARGET_DIR=artifacts/cargo-target-verify cargo test --workspace` 全绿
  （2+2+7+7 等，0 failed）。
