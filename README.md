# Codex Status

把本机 Codex 的余量放到一块 200×200 的墨水屏上。ESP32-S3 固件本地渲染；Rust 桥（Tauri v2 托盘进程）通过 `codex app-server` JSON-RPC 取数，经 Wi-Fi HTTP（主通道）或 BLE GATT（备选）下发 usage 与模板 —— 换样式只换模板 JSON，不刷固件。

A portable 200×200 e-paper companion for Codex usage. The ESP32-S3 firmware renders locally; a Rust/Tauri tray bridge pulls usage from `codex app-server` and pushes it over LAN HTTP (primary) or BLE (fallback). Templates are data, not firmware.

## 特性

- 1.54" 200×200 黑白墨水屏（SSD1681），局刷约 300 ms
- 固件内置模板引擎与三方一致的 canonical 模板哈希（固件 / Rust / Python 测试桥）
- 桥为单实例托盘进程：内建 LAN HTTP `:8765`、BLE 推送、MCP HTTP 端点 `:8766`
- 推送配置（profile）最多三个模板，推送是用户显式动作；保存模板只落盘
- 设备身份 = Wi-Fi MAC + 可编辑显示名；显式 `POST /claim` 占用与 lease 续约，多桥不互相抢占
- 低功耗：自动 light sleep（LIVE）+ 深睡窗口（DEEP），`GET /pmstats` 可观测
- 便携布局：运行数据在 `<exe>/data/`，种子在 `<exe>/seed/`，程序不写仓库

## 硬件

- Waveshare 1.54" e-Paper 200×200 B/W（SSD1681），ESP32-S3（8 MB flash）
- 桥只在 Windows 上跑（BLE/Tauri 限制）；固件构建跨平台

## 架构

```text
codex app-server ──JSON-RPC──▶ bridge-app（Tauri v2 托盘）
                                 ├─ LAN HTTP  :8765  /usage /template
                                 ├─ BLE GATT  备选通道（分片写入）
                                 └─ MCP HTTP  :8766  模板/设备/OTA 工具
ESP32-S3 固件 ◀── Wi-Fi / BLE ──┘   本地模板引擎 ──▶ SSD1681 200×200
```

## 目录

| 路径 | 内容 |
|---|---|
| `src/` | 固件（PlatformIO/Arduino）；入口 `main.cpp`，含模板引擎/存储、BLE、usage 客户端、EPD 驱动 |
| `bridge/crates/core` | app-server 客户端、usage 信封、模板库（canonical JSON + CRC32）、LAN HTTP |
| `bridge/crates/ble` | btleplug central：endpoint/usage/模板推送 |
| `bridge/crates/render` | 把固件同一份 C++ 引擎编进宿主，用于像素级预览/对拍 |
| `bridge/crates/mcp` | MCP 工具（status/get/validate/render/save/profile_push/firmware_ota/…） |
| `bridge/crates/app` | 生产形态：托盘 + 内建 HTTP/BLE/MCP；`discovery.rs` 为 ARP 发现回退 |
| `tools/test-bridge` | Python 测试桥、模板库 `templates/*.json`、`profiles.seed.json` |
| `tools/*.mjs` | Node 预览生成/场景测试 |
| `docs/` | 协议与设备文档；`docs/history/` 为历史归档 |
| `project-workflow/` | 各里程碑的计划/任务/评审记录 |

## 构建

前置：Python + PlatformIO（`pip install -U platformio`）、Rust stable（MSVC 工具链 + VS Build Tools C++）、Node 18+（仅预览脚本需要）。

### 固件（仓库根）

```powershell
pio run                    # 产物 .pio/build/esp32-s3-epaper-154g/firmware.bin
pio run -t upload          # USB 烧录
pio device monitor
```

- 仓库路径不要含空格（pioarduino `custom_sdkconfig` 会拒绝）。
- 本板闪存必须 40 MHz：`platformio.ini` 已配置，`tools/bootloader_40m_fix.py` 每次构建自动补 bootloader。
- 不要加 `-v`（GBK 控制台会挂住构建）。

### 桥（`bridge/`）

```powershell
pio run                              # 先跑一次：render crate 需要 .pio/libdeps 里的 ArduinoJson
cd bridge
cargo test --workspace
cargo build -p bridge-app --release  # 产物 target/release/bridge-app.exe
cargo run -p bridge-app
```

## 首次使用

1. 烧录固件。首次上电无已保存 Wi-Fi 时进入 AP 配网：连 `CodexStatus-<MAC后6位>`（默认密码 `codex1234`），打开 `http://192.168.4.1` 填入 Wi-Fi。
2. 运行 `bridge-app`，托盘面板里发现并占用设备（桥空闲时自动 claim，60 s 续约）。备选通道 BLE 为手动兜底：单击 BOOT 打开 BLE 会话。
3. 在面板或 MCP 里选模板组成推送配置，显式推送到设备（设备 hash 未变化的模板会跳过传输）。

常用环境变量：`CODEX_STATUS_PORT/TOKEN/TEMPLATES/INTERVAL`、`CODEX_STATUS_CODEX`（app-server 路径）、`CODEX_STATUS_DATA`、`CODEX_STATUS_DEVICE_IP/MAC/NAME`。

## 模板与 MCP

- 模板是 JSON：元素 `type/font/bind` 未识别即整份拒绝（dry-run 校验，不半渲染）。
- 改模板先 `template_render` 出图确认，再 `template_save`；推送用 `profile_push`（最多三个模板的组合）。
- MCP 端点由托盘进程提供：`http://127.0.0.1:8766/mcp`（示例见 `opencode.jsonc`）。

## 发布

推 `v*` tag 即触发 `.github/workflows/release.yml`：CI 构建固件 ROM 与 Windows 桥压缩包，并自动创建 GitHub Release。

```powershell
git tag -a v0.13.8 -m "Firmware 0.13.8-bw"
git push origin master --tags
```

## 文档

- `docs/power-state.md` — 电源状态机、身份/发现/占用协议（权威）
- `docs/device-setup-experience.md` — 设备备份、分区、显示与 BLE 调试经验
- `docs/history/` — 归档：需求（`request.md`）、早期讨论、休眠方案（`sleep-plan-v4.md`）、图标任务
- `PROGRESS.md` — 开发进度与现场交接；`AGENTS.md` — 仓库导航与约定

## License

MIT（见 `LICENSE`）。
