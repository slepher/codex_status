# 从 clone 到 Bridge 安装、USB 刷机与配网

本页供用户把提示词交给命令行 Agent 执行。先 clone 本仓库并进入根目录，再粘贴下面的提示词；将尖括号改成自己的路径/目标。现场与候选版本以 `PROGRESS.md` 为准。Windows 和 macOS 原生构建各自 Bridge；USB 分区/刷机不是 GitHub CI 的职责。

## 一句话安装当前系统的 Bridge

```text
请遵守本仓库 AGENTS.md，检测当前 Windows/macOS 和 CPU 架构，读取 docs/build-install.md；检查并准备必要构建依赖，从当前源码构建 release Bridge，验证后安装到 <绝对安装目录>，保留已有运行数据。安装依赖或写目录需要提权时按工具权限流程执行。不要停止无关实例，不要操作设备、不提交代码。为该安装配置仓库已有的按需启动方式，核验进程路径、端口、健康接口并报告版本/架构/SHA256、数据目录与启动/停止命令。若是升级，先备份并核对该实例，再停对应 watchdog/服务，替换成功后恢复；执行不了的步骤明确列为缺口。
```

建议安装目录：Windows `D:\Apps\CodexStatusBridge`；macOS `~/Applications/Codex Status Bridge.app`。用户的指定目录优先。命令中的 `~` 应先展开为绝对路径；不得把 token、Wi-Fi 密码写进命令历史或日志。

## 依赖与原生构建

- Windows：Git、Python 3、Rust stable MSVC、Visual Studio C++ Build Tools（Windows SDK）、WebView2 runtime；PowerShell 7 用于启动与隔离构建脚本。
- macOS：Git、Python 3、Rust stable、Xcode Command Line Tools；当前应用最低 macOS 14。Apple Silicon 与 Intel 原生分别构建，不将 arm64 包装成 x64。
- 只编译 Bridge **不需要先编译固件**。取 ArduinoJson `v7.4.3` 的源码，将 `CODEX_STATUS_ARDUINOJSON` 指向含 `ArduinoJson.h` 的 `src` 目录。已有 `.pio/libdeps/<env>/ArduinoJson/src` 可复用。新依赖目录按 AGENTS 先提权创建；网络/工具缺失如实报告。
- 不打印本机运行配置；构建产物和真实 `<exe>/data` 分离。以下命令都在指定目录执行，路径含空格时正确引用。

Windows（`bridge/`）：

```powershell
$env:CODEX_STATUS_ARDUINOJSON = '<ArduinoJson绝对路径>\src'
cargo test --locked -p bridge-core -p bridge-render -p bridge-app
cargo build --locked -p bridge-app --release
```

把 `bridge/target/release/bridge-app.exe` 复制到安装目录，把 `tools/test-bridge/templates/` 复制为 `<安装目录>/seed/templates/`，同时保留 LICENSE。**不复制开发机 data，也不寻找已退役的 profiles.seed.json。** 升级保留目标 `data/`。不要在正在运行的 exe 上直接覆盖；先核对并备份该实例。

Windows 按需启动（提权工具会话，仓库根目录）：

```powershell
pwsh -NoProfile -File '<仓库绝对路径>\tools\start-bridge.ps1' -ExePath '<安装目录>\bridge-app.exe'
```

默认服务名 `CodexStatusBridge`，HTTP 8765、MCP 8766；若已有别的实例占用，先报告冲突，不强杀占用者。停止/替换顺序与任务身份核对见 [操作细则](agent-operations.md#bridge-启停与实例)。脚本仍依赖 clone 的仓库位置，移动/删除仓库前需重新配置启动入口，安装不等于安装系统级服务。

macOS（仓库根目录）：

```bash
export CODEX_STATUS_ARDUINOJSON='<ArduinoJson绝对路径>/src'
export CODEX_STATUS_BUILD_PROFILE=release
export CODEX_STATUS_APP_OUTPUT='<完整绝对路径/Codex Status Bridge.app>'
bash tools/build-bridge-macos.sh
python3 tools/bridge-macos-service.py start --app "$CODEX_STATUS_APP_OUTPUT"
```

先在独立输出目录构建、验证，再替换既有安装；上例适用于空目标目录。服务脚本接受的参数以 `--help` 为准。当前构建使用 ad-hoc 签名，**不是 Apple Developer ID 签名/公证**；下载分发的 Gatekeeper 体验仍需验收，不关闭系统安全机制。macOS 运行数据默认在 `~/Library/Application Support/dev.codexstatus.bridge/data`；不得写入 `.app`。`CODEX_STATUS_DATA` 显式设置时优先。

安装验收：二进制架构和 SHA256、启动后实际 exe 路径、HTTP `/health`、MCP 8766、托盘/菜单栏窗口与原有数据；成功编译不能代替 UI、BLE 或设备通信验证。Agent 应先检查本机 `codex` CLI 是否安装/登录，未配置时明确提示，不能将 Bridge 健康冒充已取得 Codex 数据。

## 一句话完成 USB 初装与配网

```text
请遵守 AGENTS.md 和 docs/build-install.md，完成我通过 USB 连接的 <Note4 或 1.54> 的首次分区、刷机与配网，并接入当前 Bridge。我的授权仅针对核对过身份的这台设备：先枚举 VID/PID/serial、核对真实 Wi-Fi MAC/芯片/Flash 与固件 target；备份并校验现有分区表、NVS、运行内容及必要恢复镜像，私有保存，不输出凭据。准备对应完整构建产物和明确的写入地址清单，再按此授权写分区/刷机。保留原生 OTA 双槽；不要使用另一目标镜像或顺带擦除整片 Flash。若目标不唯一、备份失败、分区重叠/容量不符或必须破坏现存数据，暂停相关写入并说明具体问题。启动后走当前绑定加密 BLE 的配网入口，在安全界面让我选择网络/输入密码；核对认证 MAC、完成显式 claim 和我选择的完整 Profile/Bundle 发布，验证断开 USB 后的重连与显示。报告每阶段成功/失败/结果未知、ROM SHA256及尚未完成的实机验收，不提交代码。
```

用户只要求升级 ROM 时，用“升级固件、保持现有分区/NVS/内容”替换“首次分区”；**不要因提示词包含刷机就默认重新分区**。本提示词属于用户实际发起后的操作授权，本页存在本身不授权 Agent 操作任何设备。

## Agent 执行顺序与验收

1. **发现与身份**：先读 `PROGRESS.md` 和操作细则硬件/串口节。枚举 USB VID/PID/serial；Note4 使用 USB-Serial-JTAG。核对芯片 ESP32-S3、Flash 容量、真实 Wi-Fi MAC，结合 Bridge 登记识别目标；旧 IP 不作为现场。串口监听遵守无 DTR/RTS ioctl 规则，记录工具造成的复位。
2. **获取匹配产物**：Note4 为 `zectrix-note4-b` / 16 MB；1.54 为 `esp32-s3-epaper-154g` / 8 MB。Windows 新 clone 先 `pio pkg install -e <确切env>` 准备包，再经 `tools/pio-target.ps1 -Target note4|154g` 构建；同一时刻仅一个 pio，禁止 gray4/btpm。隔离脚本依赖 Windows junction/robocopy，macOS 不可假装能运行：可使用已核验的 CI 对应目标完整 USB 产物包；缺包时列为缺口，不能用 OTA `.bin` 猜 bootloader/分区内容。若需在 macOS 本机构建固件，先实现并验证等价隔离流程。
3. **备份与分区计划**：通过 esptool 读取真实分区表，确定已有 NVS、OTA、LittleFS、dump 边界。私有保存备份/匹配 ELF/哈希并读回校验。比较目标 CSV、生成的 `partitions.bin` 和现存布局；生成具体地址、文件、大小与影响范围。不得照抄旧历史地址；不使用无差别 `erase-flash`。串口/下载模式工具的进出复位单独记账。
4. **写入**：仅对已核对设备执行清单内操作。ESP32-S3 bootloader、分区表、应用与 OTA 选择状态必须来自同一目标完整构建和生成的上传参数；检查每个写区不重叠、不超真实 Flash。若需要重设 OTA 选择，应明确其范围与原因，不能顺带清 NVS/dump。1.54 当前发布/实机缺口见 C6，失败不得改刷 Note4 ROM。
5. **启动核验**：记录 ROM 文本、版本、target、boot_id/uptime/reset_reason、运行分区；`/status.json` 仅恢复视图。BLE 绑定后取得 token，再通过认证 `/api/status` 核对 MAC、运行镜像与分区。写入成功/重启日志不等于认证运行验收通过。
6. **配网与绑定**：物理 BOOT 打开允许的 BLE 窗口，系统配对/加密后走现行 Wi-Fi 管理流程。新绑定可能需要用户在系统 UI 确认；不要自动绕过。Wi-Fi 密码仅由用户在安全界面输入并按当前协议存设备/本机受控数据，token 只经绑定链路取得。HTTP/UDP/BLE MAC 不一致就停止；显式 claim 不等于发布。已有 owner 不属于本桥时明确报告占用。
7. **发布与验收**：选择对应 render_target 的 Profile，用户显式发布完整 Bundle；字体随 Bundle，中文 ROM 要求 `bundle_font_protocol=2`。分别核验事务 committed_job、active context、data_contract、Data ACK、实际屏幕。拔 USB 后验证网络恢复/一次自然休眠唤醒；真实 RF、中文字形/缺字/布局及长期功耗未完成则留在 backlog，不冒充通过。

## GitHub 构建与发布

`.github/workflows/release.yml`：推送 `v*` 标签构建所有目标；手动 `workflow_dispatch` 只上传可下载产物，`scope=windows`（默认）只测 Windows，`bridge` 测三个原生 Bridge 平台，`all` 包含固件，`windows-cross` 单独尝试 Linux → Windows。普通提交/PR 不触发。Windows x64、macOS arm64/x64 使用对应原生 runner；两个固件目标串行隔离构建。全部成功才生成 **draft Release**，由维护者检查后公开；交叉编译实验不替代标签的原生门禁。

手动运行新增 `s3_roms=true`（默认）：为选定 Bridge 范围同时生成 Note4/1.54 两份 S3 ROM。仅测试 Bridge 时取消该选项；`scope=all` 和 tag 发布始终包含两份 ROM。固件使用 Windows runner、matrix `max-parallel: 1`，与 Linux windows-cross 是独立 job；`act -j windows-cross` 不冒充已执行固件 job。两份 ROM 的本机构建通过同一 `tools/build-release-rom.ps1 -Target note4|154g` 入口，只有编译、Wi-Fi ELF 栈门禁、ROM marker/源码版本/双槽容量校验通过才打包。每目标上传 OTA `.bin`、manifest `.json` 和含 bootloader/分区/boot_app0/匹配 ELF 的 USB `.zip`。

AP01 独立双仓库工作流、凭据与已验证生成命令见 [AP01 release CI](ap01-release-ci.md)；其维护实现仍归兄弟固件库。

### 本地复现 release CI

Windows PowerShell 7（VS C++ Build Tools、Rust、Python 已安装）：

```powershell
$env:CODEX_STATUS_ARDUINOJSON = '<ArduinoJson 7.4.3 的绝对路径>/src'
pwsh -File tools/build-bridge-windows.ps1
```

此入口与 Windows job 相同：字体/ROM staging 校验、core/BLE/render/app 测试、release 编译、便携 ZIP 文件列表校验和 SHA256。默认写 `artifacts/release-ci/target` 与 `artifacts/release-ci/dist`，不启动程序、不访问设备、不使用开发实例 data。Agent 执行时按 AGENTS 要求提权创建构建子目录。首次全量编译较慢；重跑复用 target。测试失败立即退出，旧 ZIP 不代表本轮成功，须看命令退出状态。

Linux/WSL + Docker 可运行同一交叉构建入口：

```bash
export CODEX_STATUS_ARDUINOJSON=/absolute/path/ArduinoJson/src
bash tools/build-bridge-windows-cross.sh
```

脚本用 cargo-xwin 容器构建 `x86_64-pc-windows-msvc`，编译本项目 C++ 引擎，检查 x64 PE 头、ZIP CRC 并计算哈希。输出在 `artifacts/release-ci/cross-dist`，target 与原生构建隔离。容器需要下载 Rust/MSVC SDK/CRT 与依赖；使用 cargo-xwin 包含接受其文档所列 Microsoft SDK/CRT 许可。`CODEX_STATUS_XWIN_IMAGE` 可固定镜像 digest。交叉编译不能证明 WebView2、Wi-Fi、BLE 或托盘在 Windows 正常，运行验收仍在 Windows。

本机已有 act 可先验证工作流计划（不执行构建/上传）：

```bash
act workflow_dispatch -W .github/workflows/release.yml --input scope=windows-cross -j windows-cross -P ubuntu-24.04=node:20-bookworm-slim --dryrun --no-skip-checkout
```

完整 act 实跑（Linux/WSL + Docker，在包含待测修改的独立 clone/源码副本根目录执行）：

```bash
mkdir -p ../act-uploads
timeout 5400s act workflow_dispatch \
  -W .github/workflows/release.yml --input scope=windows-cross -j windows-cross \
  -P ubuntu-24.04=catthehacker/ubuntu:act-24.04 \
  --bind --action-offline-mode \
  --artifact-server-path "$(cd ../act-uploads && pwd)" --artifact-server-port 34569 \
  --env-file /dev/null --secret-file /dev/null
```

这里没有 `--dryrun`。`--bind` 让 act runner 与 Docker daemon 使用相同绝对源码路径，内层 cargo-xwin 容器才能读到 checkout 后的文件；不要改为 act 默认复制目录后仍假设宿主存在同一路径。Ubuntu runner 包含 Docker CLI；轻量 Node 镜像只适合前面的 dry-run。首次需下载 Python/SDK/依赖。本地 server 接收真正执行的 `upload-artifact@v4` 上传；端口须可供 runner 访问且未占用。

Windows Docker Desktop + WSL 若出现 runner 无法访问 WSL artifact server，使用下面的同网络版本；不需要开放防火墙。在专用 `act-run/source` 副本内执行，父目录只放本次测试文件：

```bash
run_root=$(cd .. && pwd)
cp "$(command -v act)" "$run_root/act"
mkdir -p "$run_root/uploads"
timeout 5400s docker run --rm --network host \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v "$run_root:$run_root" -w "$PWD" \
  --entrypoint "$run_root/act" catthehacker/ubuntu:act-24.04 \
  workflow_dispatch -W .github/workflows/release.yml \
  --input scope=windows-cross -j windows-cross \
  -P ubuntu-24.04=catthehacker/ubuntu:act-24.04 --bind --action-offline-mode \
  --artifact-server-addr 127.0.0.1 --artifact-server-port 34569 \
  --artifact-server-path "$run_root/uploads" \
  --env-file /dev/null --secret-file /dev/null
```

act 控制进程和 runner 都使用 Docker host 网络；上传走同一网络命名空间的 loopback，绕开 WSL→Docker Desktop 的不可达宿主地址。此处是实际完整 job，未关闭 artifact 上传。

act 默认用本地源码替代当前仓库 checkout，保留待测修改；windows-cross 的 ArduinoJson 步骤通过匿名 Git fetch/checkout 实际从 GitHub 获取 v7.4.3，无需个人 token（第二个 actions/checkout 在无 token 环境会直接失败）。不要加 `--no-skip-checkout` 把待测源码覆盖为远端旧版。测试源码仅复制 tracked/非忽略源码和必要 Git 元数据，不复制 `.pio`、运行 data、旧 target 或原 artifacts。Agent 创建目录/运行构建仍按 AGENTS 提权规则。验证服务器收到的 artifact 外层 ZIP 中确有 release ZIP 与 `.sha256`，检查内层包哈希、CRC、x64 PE，并与本轮输出逐字节比较；不能只看 job success。

这验证本地真实执行及上传，GitHub 云端 cache/权限/draft 行为仍须单独验收。Windows 宿主映射 `-P windows-2022=-self-hosted` 只能在 Windows 上执行，不能让 WSL 模拟 Windows。act 不完整模拟 permissions/timeout/environment。

Mac 端：按前文安装依赖，先在 `bridge/` 执行 `cargo test --locked --target-dir ../artifacts/bridge-macos/target -p bridge-core -p bridge-ble -p bridge-render -p bridge-app`，再用 `CODEX_STATUS_BUILD_PROFILE=release bash tools/build-bridge-macos.sh` 与 `codesign --verify --deep --strict` 验证包。Linux → macOS 可以研究 osxcross，但当前 `app/build.rs` 写死原生 clang/ar，且依赖 Apple SDK/CoreWLAN 等框架；现成脚本不能直接交叉构建。Mach-O 构建、.app 打包、签名、公证与 Mac 实机测试须分别验收，本任务保留 Mac 原生路径。

方案依据：[cargo-xwin](https://github.com/rust-cross/cargo-xwin)、[Tauri Windows 交叉编译](https://v2.tauri.app/distribute/windows-installer/)、[osxcross](https://github.com/tpoechtrager/osxcross)、[act runner](https://nektosact.com/usage/runners.html)。Astranaut 使用的是 Erlang `rebar3_docker_ci` 容器矩阵，不是通用 Actions/跨系统打包模拟器。

发布标签代表源码快照。Note4与1.54共用`src/main.cpp`的`FW_RELEASE_VERSION`，只在这一处递增；完整版本分别为`<基号>-note4`与`<基号>-bw`，target/env保持不变。两台不必同时构建、打包或升级；各自在某一源码快照打包时使用该快照的共同基号，不另开设备专属序号。AP01沿用兄弟仓库版本，不加入此序列。

打包使用`tools/stage-release-rom.py`：核对ROM marker基号与当前源码，拒绝拿旧目标缓存冒充新版；manifest同时登记`release_version`、完整`version`、target/env、source_commit/source_dirty、ROM大小/SHA256及匹配ELF哈希。源码版本、已打包产物、实机运行版本分开记录：仅改共同基号不将未升级设备登记为新版本；现场确认仍以同MAC认证及运行镜像证明为准。当前部署见PROGRESS，历史差异从统一序列开始时保留，不改写旧版本。产物须有大小、SHA256、匹配 ELF 哈希与原生 OTA 双槽容量检查。macOS 签名/公证、CI 首次实跑、1.54 最新 ROM 与真实 USB 全流程的证据尚未取得时，应保留为缺口。
