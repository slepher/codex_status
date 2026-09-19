# task-2 — 发现回退：ARP 扫描 + BLE 主动读取 + 手动入口

状态：done（2026-09-19，含 BLE 端到端实测）

## 范围

1. **ARP by MAC（自动，同二层）** — `bridge/crates/app/src/discovery.rs`
   （`cfg(windows)`，`GetIpNetTable` 邻居表快路径 + UDP poke 唤醒 + 邻居表
   轮询 + `SendARP` 并行扫描兜底；地址范围由 `lan_ip()` 推出 /24）。
   触发：`device_cache_loop` 连续 2 次（~20 s）HTTP 失败且 MAC 已知，或显式
   `device_discover via=arp`；60 s 冷却，结果写回 AppCtx 并持久化 `force_push`。
2. **BLE 主动读取（兜底，需用户手动开 BLE）**
   - `Pusher::cycle_once` 现返回 info JSON；app 在 `ble=1` 通告触发的 cycle
     中采纳 `{mac, ip, http_port}`（`adopt_ble_info`）。
   - `Pusher::read_device_info()` + MCP/Tauri `device_discover {via: auto|arp|ble}`；
     `via=auto` 先 HTTP、再 ARP（无 MAC 时提示开 BLE 会话）。
   - 固件 info 增 `"mac"`（task-3）。
3. **持久化**：发现到的 `{mac, ip}` 写 `<data>/bridge-app.json`。
4. **面板/MCP**：设备页「最后发现」显示 via/时间；MCP `device_owner` 同字段；
   `bridge_status` 带 name/mac/ip/owner。

## 验收证据

- **模拟换 IP（同二层，ARP 恢复）**：配置 `device_ip=192.168.1.199`、桥重启
  → 日志 `device HTTP unreachable; ARP fallback scan for 70041DAABBCC`
  → `device endpoint updated: 192.168.1.50 (via arp)`；随后 `/claim` 探测与
  `usage push -> 200` 恢复正常（同秒级，命中邻居表快路径）。
- `device_discover via=arp` → 返回 `discover.via="arp"`、IP 正确；
  `via=auto`（HTTP 在线）→ `discover.via="http"`。
- `via=ble`（设备 BLE 关闭）→ 30 s 扫描后明确报错
  `device CodexStatus-* not found`（手动兜底提示路径）。
- **BLE 端到端（用户单击 BOOT 后）**：`device_discover via=ble` 2–3 s 成功，
  复用已配对 bond（`peerBonded=true, peerEncrypted=true`），返回 info JSON：
  `{"fw":"0.13.4-bw","mac":"70:04:1D:AA:BB:CC","ip":"192.168.1.50",
  "http_port":80,...}`，`discover.via="ble"`；桥日志 `device info: {...}`。
  首次连接一次 `connect` 失败（设备刚进会话，重试即成功）。
- 邻居表冷路径（UDP poke）无法在本机复现（改 IP 需要设备换网；无管理员权限
  删除邻居缓存）——留待真实换网场景；代码路径与超时已实现。
- `cargo test --workspace`（隔离 target）绿；`git diff --check` 干净。
