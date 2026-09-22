# Prompt — 单独编译 btpm 实验 env（独立窗口，可无人值守）

你是本仓库（Codex Status）的编译代理。本次**只做一件事**：新增并构建 BT modem sleep 实验
env，收集 sdkconfig 差异并归档"预演"ROM。用户会离开做别的事：不要等待确认，按步骤执行到
结束；只有遇到阻塞才停下，并在报告里写清原因。

先快速读（只读不引大段）：

- `AGENTS.md`（构建命令与坑）
- `project-workflow/power-plan-c/plan.md`、`task-4-modem-sleep.md`、`status.md`

## 背景

- 目的：验证 `CONFIG_BT_CTRL_MODEM_SLEEP=y` 在当前 pioarduino + `custom_sdkconfig` 下能生成并
  编译，并确认它连带改变了哪些 Kconfig 项。
- 当前源码 = **未提交**的 v2 收敛工作树（`FW_VERSION` 预计 `0.16.9-bw`），**不含** Plan C
  行为；这次 ROM 只是"配置预演"，task-1 完成后要重编。
- 基础 env `esp32-s3-epaper-154g` 的既有构建产物必须保持不动：本次**只用 `-e` 构建新 env**，
  绝不构建/清理/改动 base env。

## 步骤

1. 前置检查：`git status --short`（记录即可，不改动）；确认没有其它 `pio` 构建在跑；
   不要停止运行中的 `bridge-app`（它锁 `bridge/target/debug`，与固件构建无关）。
2. 在 `platformio.ini` 末尾新增（缩进与现有 env 一致）：

   ```ini
   [env:esp32-s3-epaper-154g-btpm]
   extends = env:esp32-s3-epaper-154g
   custom_sdkconfig =
     ${env:esp32-s3-epaper-154g.custom_sdkconfig}
     CONFIG_BT_CTRL_MODEM_SLEEP=y
   ```

   注意：`custom_sdkconfig` 是列表，必须用 `${env:...}` 继承再追加；不要重写会丢基础项，
   不要改其它 env。
3. 构建（只这一个 env；该 env 首次构建为全量编译，可能 5–15 分钟）：

   ```powershell
   pio run -e esp32-s3-epaper-154g-btpm
   ```

   不加 `-v`（GBK 控制台会报错挂住）。建议把完整输出同时写入
   `artifacts/build-btpm-<date>.log`，至少保存末尾摘要。
4. 核对生成物：
   - `.pio/build/esp32-s3-epaper-154g-btpm/firmware.bin` 的大小；
   - `sdkconfig.esp32-s3-epaper-154g-btpm` 含 `CONFIG_BT_CTRL_MODEM_SLEEP=y`；
   - 与既有基线 `sdkconfig.esp32-s3-epaper-154g` 做 diff，列出**所有**变化项
     （重点：BT/PM/coexist/RTC clock/Wi-Fi 省电相关）；
   - 记录 `FW_VERSION`（`src/main.cpp`；预计 0.16.9-bw，**不要改**）。
5. 归档（预演用，命名带标记）：
   - 复制 `firmware.bin` → `artifacts/codex-status-<ver>-bw-btpm-dryrun.bin`；
   - 计算字节数与 SHA256（`Get-FileHash`）。
6. 落记录：在 `project-workflow/power-plan-c/status.md` 追加"构建记录（预演）"一节：
   时间、env 名、FW_VERSION、ROM 路径/大小/SHA256、sdkconfig 差异摘要、构建耗时、日志路径；
   并把文件顶部状态补一句"btpm env 已可构建，待 task-1 代码落地后重编正式版"。
7. 禁止：不改业务源码与 `FW_VERSION`；不构建 base/gray4 env；不 OTA；不提交；不停止或重启
   运行中的服务；不清理 `.pio/` 或 `artifacts/cargo-target-*`。

## 交付（本窗口最终报告）

- 新增 env 配置片段 + 构建结果（成功/失败、耗时、日志路径）；
- ROM 路径 / 大小 / SHA256；
- 与基础 sdkconfig 的差异清单（逐项，注明是否由 modem sleep 连带引入）；
- 若失败：完整错误摘要、已尝试的处置、建议下一步。
