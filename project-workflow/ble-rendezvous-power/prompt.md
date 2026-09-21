# prompt.md — 新窗口交接：ble-rendezvous-power 阶段 2 收尾 → 阶段 3（v2 事务）

你是一个新窗口。按顺序读：`PROGRESS.md` 最新一节 → 本目录 `status.md` →
`plan.md` → `task-4.md`（阶段 1 已完）→ `task-5.md`（阶段 2，含照片门槛）→
`task-6.md`（下一步）→ `docs/ble-rendezvous-power-design.md`（权威，重点
§3/§5/§8/§12/§13）。未经用户要求不提交；不把任何 token/凭据写进文档或日志。

## 现在的状态（2026-09-21 下午）

- **设备**：`0.15.7-bw` @ `192.168.3.163`（电池，light，quad v11
  `93199731`，`rgn=13`、`epd_trusted=true`、`pm/rv2=0`；调试设置已复位
  `idle_deep_s=600`、`frame_capture=0`）。0.15.7 修复了离开 deep 的 Zzz
  残影（pull 前先切 light + clean 全刷）。
- **桥**：fresh debug 进程（父 PID **5216** + watchdog **14540**），
  MCP `http://127.0.0.1:8766/mcp`；运行数据
  `bridge/target/debug/data/`；tracing 日志
  `bridge/target/debug/data/logs/bridge-app.log.<date>`。
- **未提交**（用户尚未要求）：`src/refresh_policy.*`、`src/main.cpp`、
  `src/EPD_SSD1681.*`、`src/ble_bridge.*`、`tools/rtc-budget.py`、
  `bridge/crates/render/{build.rs,src/*,tests/policy.rs}` 及本目录
  task-4..10 / status / PROGRESS。

## 已完成（阶段 1/2）

- 阶段 1（task-4，0.15.3）：基线证据 `artifacts/ble-rendezvous/`；RTC 审计
  （`tools/rtc-budget.py`：7 680 B 区、余 ~6 KB）；`EPD_SSD1681_*` BUSY
  返回值传播（`epd_busy_fails`/`epd_trusted`）；`rv:2` 分流 + INFO
  `rendezvous_v`/`rv_max` + NVS `pm/rv2` 回滚开关（`/diag?rv2=`）。
- 阶段 2（task-5，0.15.5/0.15.6）：`refresh_policy` 语义区域、墨量统计、
  ghost 预算、保守判定（黑块在照片通过前一律全刷）、clean/trust/local 全刷、
  `/diag?rgn|policy|clean|busy_fail` 与 telemetry；0.15.6 把区域合并从
  “字节扩张”改为**真实像素相交**（修复 BOOT 点击全刷；图标变化恢复
  `partial/ok`）；冷启动不再滞留 `Connecting:` 页。主机测试
  `cargo test -p bridge-render --test policy`。

## 待办（阶段 2 出口，需要用户/时间）

1. **照片验收**（用户，固定机位）：全刷基准 + 黑块对比；通过前 `rgn` 黑块
   类别保持全刷，失败则永久全刷。
2. **时钟预算长测**：deep ≥1.5h 观察 `rtcClkPartials` 到 90 时本地全刷
   （`/status.json` 的 `refresh_kind/reason`）。
3. **BOOT BLE 回归**（物理按键）：legacy 模板推送仍可用、`rv:2` 控制得到
   `not_ready` NACK。
4. **task-10（用户要求仅入文档，本窗口实现）**：
   - A：冷启动有 usage 缓存时直接进模板（`WIFI OFF`、Wi-Fi 灭、桥断连），
     不等 Wi-Fi；连上后一次局刷点亮图标；失败保持模板 + 既有有界退避。
   - B：连接中 Wi-Fi 图标 ~1Hz 闪烁，成功常亮、失败/超时熄灭；模板驱动
     （可能 quad v12 + 三端哈希同步），只用局刷；**必须先实测算功耗**再定
     周期/时长。
   - 同时修 0.15.7 遗留：timer pull→light 会画两帧（两次全刷闪烁），原型
     RAM 标志 `wakeBaselineDrawn` 已回退，按 task-10 重新实现并验证。
   - 注意：A/B 都要保持“无 Connecting 页”和 deep 返回时限不变。

## 下一步：阶段 3（task-6，v2 事务，默认关闭）

1. **能力与会话**：仅当 `pm/rv2=1` 时 INFO 才报 `rendezvous_v:2`；会话
   nonce 只在 bonded+encrypted 的 status 握手返回；不放进广播/INFO。
2. **复用现有 GATT**（不改表）：`...a005` 控制 JSON（`rv>=2` 走
   `dispatchTemplateCtrl` 的 v2 分支，≤512B）；`...a006` 分片
   `magic 0xB2 + request_id u32LE + offset u32LE + data`；`...a004` 排空
   ACK `magic 0xB3 + message_id u16LE + index u8 + count u8`；`...a001`
   能力。`type=usage` 首版（512–4096B），`template|firmware` 回
   `needs_wifi`。
3. **语义**：`NOOP`（可带 `server_time`/`tz_offset_min`，不刷 usage、不续
   lease）、`UPDATE_BEGIN/CHUNK/COMMIT`；CRC32 IEEE 覆盖完整 payload；严格
   连续 offset、重复相同分片幂等、冲突/越界 NACK 取消；一个 revision 一次
   提交；`stale_revision`/`revision_conflict`；`applied`/`displayed`
   revision 分离；新 epoch 只在阶段 4 的认证 `POST /power` 登记。
4. **期限**：单事务 8s、无进展 2s、radio 总 15s、ACK 排空 500ms；回调只入
   有界队列，不碰 Wi-Fi/面板。
5. **测试**：`pm/rv2=1` 下用显式会话（扩 `bridge-ble --rv2` 或脚本）做
   NOOP、512/2048B、重复 revision 幂等、CRC/offset 错误、MTU23 分片；同时
   回归 `rv2=0` 的 legacy 路径。遥测字段按设计 §11。
6. **模板一致性红线**：本阶段不改模板协议；若改，必须同步固件/Rust
   canonical/Python 并跑 `node tools/test-quad-preview.mjs` +
   `cargo test -p bridge-core --test template`。

## OTA 事故与运维注意

- 2026-09-21：deep 期排队的 OTA 唤醒后每次被客户端侧 abort，桥按
  60s→2m→4m 退避（屏上反复闪升级页），直接 MCP 调用会卡到 120s 客户端
  超时。设备与 token 正常；curl 直传 26s `UPDATE OK`，重启 `bridge-app`
  后桥上传恢复。详见 `task-8.md` 现场笔记。
- 阶段 5 强化清单：`bridge_status` 暴露 pending OTA/ROM；preflight
  `/diag?ota_abort=1` 后加短 settle；`post_firmware` 有界超时并报已发字节；
  OTA 与 queued 推送串行；**日志 URL 脱敏 `token=`**。
- 运维：OTA 优先在设备在线时直传；深睡期排队后若见重复 abort，先重启桥
  （先停 watchdog 子进程再停父进程）再传；同版本重传桥会报“版本未变”，
  但设备可能已刷成功（看 `/status.json.fw`）。
- **安全**：`project-workflow/sleep-modes/prompt.md`（已入库，e22d952）含
  明文设备操作 token，当前仍可用；本文件刻意不含。建议下次 BOOT 会话经
  绑定 BLE 轮换 token（`tools/device-auth`/BLE auth 特征），并注意桥日志
  里 URL 明文 token 尚未脱敏。

## 现场、命令与硬约束

- 设备 token 在 `bridge/target/debug/data/device-token.json`：**只读入内存
  使用，严禁打印/入库**；token 规则（`/update`、`/doUpdate`、ArduinoOTA、
  `POST /claim` 必带）不放宽。
- 固件：`pio run`（**勿加 `-v`**，GBK 会 UnicodeEncodeError）；闪存必须
  40 MHz；USB 刷写用 `pio run -t upload`，网页 OTA 后先擦 otadata
  (`erase_region 0xD000 0x2000`)。
- 测试：`node tools/test-quad-preview.mjs`；
  `$env:CARGO_TARGET_DIR="D:\...\artifacts\cargo-target-rv2"; cargo test -p
  bridge-core --test template` 与 `cargo test -p bridge-render --test policy`
  （运行中的桥会锁 `target/debug`，必须隔离 target）。
- 设备 `/diag`（token 门控）：`rgn=1` 区域 dump、`policy=off|on`、`clean=1`、
  `busy_fail=1`、`rv2=1|0`、`render_mode=deep|light`、`deep_now=1`、
  `deep_usb=1`；只读 `GET /status.json`、`/log`、`/history`、`/frame`。
- 每里程碑更新 `PROGRESS.md` + 本目录 task/status；`git diff --check`；
  后台进程分离启动、日志进 `artifacts/`；未经用户要求不提交。
