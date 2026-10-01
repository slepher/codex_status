# Codex Status

把本机 Codex 使用余量放到便携墨水屏上。支持 **Waveshare 1.54" 200×200** 和 **ZecTrix Note4 400×300** 两款 ESP32-S3 设备，通过 Rust/Tauri 桌面桥同步数据，自定义 JSON 模板即可换样式，无需重刷固件。

A portable e-paper companion for local Codex usage, supporting **Waveshare 1.54" 200×200** and **ZecTrix Note4 400×300** ESP32-S3 devices, with a Rust/Tauri desktop bridge and customizable JSON templates.

[中文](#中文) · [English](#english)

## 中文

### 支持的硬件

| 设备 | 屏幕 | 硬件 | 固件构建目标 |
|---|---|---|---|
| Waveshare 1.54" | 200×200 黑白，SSD1681 | ESP32-S3，8 MB Flash | `esp32-s3-epaper-154g` |
| ZecTrix Note4 DevKit V1.0 | 400×300 黑白 | ESP32-S3，16 MB Flash，8 MB OPI PSRAM | `zectrix-note4-b` |

两款设备共用模板引擎与显示平台，使用各自的固件和分辨率适配模板。200×200 提供 `quad`、`mini` 等布局；400×300 提供 `codex-status-a`。ROM 必须与设备 target 匹配，不能混刷。当前桌面桥面向 Windows。

### 主要功能

- **Codex 余量显示**：通过本机 `codex app-server` JSON-RPC 获取配额与重置时间，设备本地渲染。
- **模板与按键切换**：每台设备的 Profile 包含 1–8 个有序模板，全部参与按键循环；保存只落盘，显式发布才下发完整 A/B Bundle。
- **双板、多设备管理**：Wi-Fi MAC 是设备身份，显示名可编辑；各设备独立维护 Profile、状态与发布任务。
- **Wi-Fi / BLE 通信**：Wi-Fi HTTP 为主通道，BLE GATT 用于认证与备选会合；claim/lease 协议协调多个 Bridge 的占用。
- **低功耗运行**：支持 light sleep、deep sleep 与离线时钟，由 Bridge 生成正式 PowerPlan；实际续航与长期残影仍需实机测量。
- **预览与 MCP**：宿主预览复用固件 C++ 渲染引擎；内建 MCP 提供模板、Profile、发布、设备诊断与 OTA 工具。

### 架构

```text
本机 codex app-server
        │ JSON-RPC
        ▼
Rust / Tauri Bridge（Windows 托盘）◀── 桌面 UI / MCP :8766
        │ Wi-Fi HTTP / BLE GATT
        ├── ESP32-S3 → Waveshare 1.54" 200×200
        └── ESP32-S3 → ZecTrix Note4 400×300
                      本地模板渲染
```

Codex 是平台的数据源之一。Bridge 负责数据绑定、Profile、Bundle 发布与电源计划，固件负责校验、存储和渲染。默认 Bridge HTTP 端口为 `8765`，MCP 地址为 `http://127.0.0.1:8766/mcp`。

### 构建

需要 Python、PlatformIO、PowerShell 7，以及 Rust stable（Windows MSVC 工具链和 Visual Studio C++ Build Tools）。Node.js 用于预览与场景校验脚本。仓库路径不要包含空格。

**固件：在仓库根目录执行，一次只构建一个目标。** 默认开发目标为 Note4；切换双板目标使用隔离脚本，保留每个目标的 sdkconfig 与 framework 包，避免反复重装。

```powershell
# Note4 400×300
pwsh -File tools/pio-target.ps1 -Target note4

# Waveshare 1.54" 200×200（需要该板 ROM 时单独构建）
pwsh -File tools/pio-target.ps1 -Target 154g
```

产物分别位于 `.pio/build/zectrix-note4-b/firmware.bin` 和 `.pio/build/esp32-s3-epaper-154g/firmware.bin`。单独构建 Note4 也可使用 `pio run -e zectrix-note4-b`。不要省略环境名或同时运行两个 PlatformIO 构建；不要为这两块黑白板选择 `gray4` / `btpm` 实验变体。Flash 已配置为 40 MHz，并由构建脚本修正 bootloader。

**桌面桥：固件构建准备好 ArduinoJson 依赖后，在 `bridge/` 执行。** 也可将 `CODEX_STATUS_ARDUINOJSON` 指向已有的 ArduinoJson `src` 目录。

```powershell
cd bridge
cargo test --workspace
cargo build -p bridge-app --release --locked
```

产物为 `bridge/target/release/bridge-app.exe`。运行数据默认位于 EXE 旁的 `data/`，种子目录为 `seed/`；升级时保留 `data/`。运行中的 EXE 会被锁定，构建前应先停止对应实例；测试可使用独立 `--target-dir`，避免碰到现有运行目录。

开发默认实例使用 debug 产物，在仓库根目录启动：

```powershell
cargo build --manifest-path bridge/Cargo.toml -p bridge-app
pwsh -File tools/start-bridge.ps1
```

启动脚本通过按需 Windows 计划任务 `CodexStatusBridge` 启动默认实例，重复调用只报告已有进程。受限工具环境中须以非受限权限运行该脚本。退出 Codex 后的跨会话存活验证状态见 backlog。

### 使用流程

1. 为对应硬件烧录正确 ROM。现行首次 Wi-Fi 配网使用设备 AP：连接屏幕显示的 `CodexStatus-…` 热点，密码 `codex1234`，打开 `http://192.168.4.1` 填入网络信息。通过 Bridge 蓝牙输入 Wi-Fi 信息的新流程目前仅完成设计。
2. 启动 Bridge，在设备页发现并登记设备，核对 Wi-Fi MAC。设备空闲时 Bridge 自动 claim 并续约；需要 BLE 会合时通过设备按键打开会话。
3. 选择适合设备分辨率的模板，预览后保存，组成 1–8 项 Profile。
4. 显式发布到选定设备。保存模板或 Profile 不会自动发布；休眠设备的任务可等待后续会合，结果以任务状态和设备回报为准。

MCP 配置示例见 [opencode.jsonc](opencode.jsonc)。模板编辑使用 `platform_template_get`、`platform_template_validate`、`template_render`、`platform_template_save`；Profile 使用 `platform_profile_get` / `platform_profile_save`，下发使用 `platform_publish`。设备操作按 MAC 指定目标。常用环境变量包括 `CODEX_STATUS_CODEX`、`CODEX_STATUS_DATA`、`CODEX_STATUS_PORT` 与 `CODEX_STATUS_INTERVAL`。

### 项目导航

| 路径 | 内容 |
|---|---|
| `src/` | ESP32-S3 固件、模板引擎、设备协议、电源管理与屏幕驱动 |
| `bridge/crates/app` | Tauri 托盘应用、多设备管理、HTTP/BLE/MCP 服务 |
| `bridge/crates/core` / `ble` | 数据源、模板契约、HTTP 客户端与 BLE 通信 |
| `bridge/crates/render` / `mcp` | 同源宿主渲染与 MCP 工具 |
| `tools/` | 双板隔离构建、Bridge 启动、预览与测试工具 |
| `docs/` | 架构、协议、文档导航与唯一待办清单 |
| `project-workflow/` | 进行中专项的设计、计划与验证记录 |

文档入口：[文档索引](docs/README.md) · [平台架构](docs/generic-display-platform-design.md) · [电源与占用协议](docs/power-state.md) · [最新现场](PROGRESS.md) · [待办与实现状态](docs/roadmap/backlog.md) · [开发约定](AGENTS.md)。

## English

### Supported hardware

| Device | Display | Hardware | Firmware environment |
|---|---|---|---|
| Waveshare 1.54" | 200×200 monochrome, SSD1681 | ESP32-S3, 8 MB flash | `esp32-s3-epaper-154g` |
| ZecTrix Note4 DevKit V1.0 | 400×300 monochrome | ESP32-S3, 16 MB flash, 8 MB OPI PSRAM | `zectrix-note4-b` |

Both devices share the rendering engine and display platform, with separate firmware images and resolution-specific templates. The 200×200 family includes `quad` and `mini`; the 400×300 family includes `codex-status-a`. Firmware must match the device target. The desktop bridge currently targets Windows.

### Features

- **Codex usage at a glance**: fetch quotas and reset times from the local `codex app-server` through JSON-RPC, then render on the device.
- **Custom layouts and button cycling**: each device Profile holds 1–8 ordered templates, all available through button cycling. Saving stays local; explicit publication sends a complete A/B Bundle.
- **Two boards, multiple devices**: Wi-Fi MAC is the stable identity; names are editable. Each device has its own Profile, state and publication jobs.
- **Wi-Fi and BLE**: Wi-Fi HTTP is the primary channel; BLE GATT handles authentication and fallback rendezvous. Explicit claim/lease ownership coordinates multiple bridges.
- **Low-power operation**: light sleep, deep sleep and an offline clock, governed by Bridge-generated PowerPlans. Battery life and long-term ghosting still require hardware measurements.
- **Preview and MCP tools**: desktop previews use the same C++ rendering engine as firmware. Built-in MCP tools cover templates, Profiles, publication, diagnostics and OTA.

### Architecture

```text
Local codex app-server
        │ JSON-RPC
        ▼
Rust / Tauri Bridge (Windows tray) ◀── Desktop UI / MCP :8766
        │ Wi-Fi HTTP / BLE GATT
        ├── ESP32-S3 → Waveshare 1.54" 200×200
        └── ESP32-S3 → ZecTrix Note4 400×300
                      Local template rendering
```

Codex is a data source for the display platform. The Bridge manages data bindings, Profiles, Bundle publication and power plans; firmware validates, stores and renders them. Default Bridge HTTP port: `8765`. MCP endpoint: `http://127.0.0.1:8766/mcp`.

### Build

Prerequisites: Python, PlatformIO, PowerShell 7, and Rust stable with the Windows MSVC toolchain and Visual Studio C++ Build Tools. Node.js is used by preview and scenario checks. Keep the repository path free of spaces.

**Firmware: run from the repository root, one target at a time.** Note4 is the default development target. Use the isolation script when switching boards to preserve each target's sdkconfig and framework packages.

```powershell
# Note4 400×300
pwsh -File tools/pio-target.ps1 -Target note4

# Waveshare 1.54" 200×200, when building for that board
pwsh -File tools/pio-target.ps1 -Target 154g
```

Outputs: `.pio/build/zectrix-note4-b/firmware.bin` and `.pio/build/esp32-s3-epaper-154g/firmware.bin`. For a standalone Note4 build, `pio run -e zectrix-note4-b` is also supported. Always specify an environment; do not run concurrent PlatformIO builds or select the experimental `gray4` / `btpm` variants for these monochrome boards. Flash is configured at 40 MHz, with a bootloader fix supplied by the build script.

**Desktop bridge: run in `bridge/` after a firmware build has provided ArduinoJson.** Alternatively, set `CODEX_STATUS_ARDUINOJSON` to an existing ArduinoJson `src` directory.

```powershell
cd bridge
cargo test --workspace
cargo build -p bridge-app --release --locked
```

Output: `bridge/target/release/bridge-app.exe`. Runtime data defaults to `data/` beside the executable, with seeds in `seed/`. Preserve `data/` during upgrades. Stop the matching running instance before rebuilding its EXE; use a separate `--target-dir` for tests when necessary.

For the default development instance, build the debug executable and start it from the repository root:

```powershell
cargo build --manifest-path bridge/Cargo.toml -p bridge-app
pwsh -File tools/start-bridge.ps1
```

The script starts the default instance through the on-demand Windows scheduled task `CodexStatusBridge`; repeated calls report the existing process. Run it outside restricted tool sessions. Cross-session survival after Codex exits is tracked in the backlog.

### Getting started

1. Flash the firmware matching your board. The current initial Wi-Fi setup uses the device AP: connect to the `CodexStatus-…` hotspot shown on screen, use password `codex1234`, and enter network details at `http://192.168.4.1`. A new Bridge-based BLE Wi-Fi setup flow is designed but not yet implemented.
2. Start the Bridge, discover and register the device, and verify its Wi-Fi MAC. The Bridge automatically claims idle devices and renews its lease. Open a BLE session using the device buttons when needed.
3. Choose templates for the device resolution, preview and save them, then create a Profile with 1–8 entries.
4. Explicitly publish to the selected device. Saving a template or Profile does not publish it. Jobs for sleeping devices can wait for a later rendezvous; check job status and device reports for the outcome.

See [opencode.jsonc](opencode.jsonc) for MCP configuration. Template tools include `platform_template_get`, `platform_template_validate`, `template_render` and `platform_template_save`. Use `platform_profile_get` / `platform_profile_save` for Profiles and `platform_publish` to send a Bundle. Select devices by MAC. Common environment variables include `CODEX_STATUS_CODEX`, `CODEX_STATUS_DATA`, `CODEX_STATUS_PORT` and `CODEX_STATUS_INTERVAL`.

### Repository and documentation

| Path | Purpose |
|---|---|
| `src/` | ESP32-S3 firmware, rendering, device protocol, power management and display drivers |
| `bridge/crates/app` | Tauri tray app, multi-device management and HTTP/BLE/MCP services |
| `bridge/crates/core` / `ble` | Data sources, template contracts, HTTP client and BLE transport |
| `bridge/crates/render` / `mcp` | Shared-engine desktop rendering and MCP tools |
| `tools/` | Isolated board builds, Bridge startup, previews and test utilities |
| `docs/` | Architecture, protocols, documentation index and backlog |
| `project-workflow/` | Designs, plans and validation records for active work |

Most detailed documentation is currently in Chinese: [documentation index](docs/README.md), [platform architecture](docs/generic-display-platform-design.md), [power and ownership protocol](docs/power-state.md), [current handoff](PROGRESS.md), [backlog and implementation status](docs/roadmap/backlog.md), and [contributor instructions](AGENTS.md).

## License

[MIT](LICENSE).
