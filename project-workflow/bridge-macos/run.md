# macOS 本地测试包

2026-10-03：当前是 P0 阶段 Intel/macOS 14 测试包，不代表首版对等或正式发行已验收。

工具链使用本机 `~/.cargo/bin` 的 Rustup stable；本次升级到 Rust/Cargo 1.99.0。构建只把产物放入独立 target，不建立独立 Rust 安装，也不运行 PlatformIO。渲染需要固定版本 ArduinoJson 头文件，本次使用 7.0.4 `src`。

```sh
CODEX_STATUS_ARDUINOJSON="$PWD/artifacts/bridge-macos/deps/ArduinoJson-7.0.4/src" \
  bash tools/build-bridge-macos.sh
open 'artifacts/bridge-macos/Codex Status Bridge.app'
```

若从受限工具执行构建/启动，目录创建与运行数据初始化按 AGENTS.md 使用非受限权限；在自己的终端正常执行即可。构建脚本使用本地临时签名，未做 Developer ID 签名、公证或 DMG 分发。

默认数据：`~/Library/Application Support/dev.codexstatus.bridge/data`，安装身份在同根 `installation-id`；命名实例使用同根 `instances/<name>/data`，显式 `CODEX_STATUS_DATA` 优先。升级测试包须保留该目录；种子在 `.app/Contents/Resources/seed/templates`，运行时只补缺失文件。

菜单栏打开现有面板，关窗隐藏，退出结束应用。默认不启用登录自启动；可选登录 plist 的真实登录/取消流程与按需 LaunchAgent 监督仍需后续验收。当前没有 macOS watchdog 崩溃恢复保证。

设备配对是 Mac 与设备的绑定；系统蓝牙权限另按应用身份管理。当前正式测试包已完成 Note4 已绑定链路读取及设备重启后恢复，无需为了正常重连再次配对。未绑定的首次接入仍需设备的物理配对窗口，密码和设备 token 不写日志。CoreBluetooth UUID 只用于本机扫描去重，业务身份仍核对完整 Wi-Fi MAC。

当前 Note4 经已有 USB 配网接入新环境。新的按 bond 网络方案/5GHz→2.4GHz 动态选择和 BOOT 离线恢复入口仍在 backlog B6；不能以 USB 配网替代其验收，也不能以这份测试包宣称自然短窗口、Windows、Apple Silicon、macOS 15+ 或 24 小时常驻已验证。
