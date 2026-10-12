# 2026-10-12 macOS Bridge 构建与启动

用户授权：现在构建并启动当前工作区 macOS Bridge。

## 计划

1. 核对交接卡、操作细则、源码及默认实例路径。
2. 使用固定 ArduinoJson 7.0.4 和现有隔离 target 构建候选 App，校验签名、大小和 SHA256。
3. 停止默认 LaunchAgent，确认同路径子进程退出；私有备份 data/installation-id，保留旧 App。
4. 将候选部署到原 App 路径，使用原 LaunchAgent 配置启动，核验健康响应、MCP 和进程映像。
5. 更新交接卡与证据；不提交，不执行固件或模板发布。

## 初始状态

- HEAD `e468b3a`；源码无未提交修改，只有前次未完成的 build-start-20261010.md。
- 交接卡已更新至 2026-10-12；设备现场沿用卡片，不重新声明设备验收。
- 默认 App 为 artifacts/bridge-macos/Codex Status Bridge.app，exe SHA256 `664b86af8cc2013690c78e79b2eace1e9e725afab957636bc873ecf5a27f06f5`。
- 默认 supervisor 18431 / child 79742，HTTP 8765 health 204、MCP 8766 监听；LaunchAgent 环境配置为空。
- 构建、部署与启动核验已完成。

## 结果与证据

- 首次 debug 调用被 macOS 系统 Bash 3.2 的 `set -u`/空数组兼容问题阻断（未开始编译）；改用 docs/build-install.md 推荐的 release 配置，未修改构建脚本。
- `cargo build --locked -p bridge-app --release` 成功，首次隔离 release 构建 9m05s、6 项 warning；固定 ArduinoJson 7.0.4，未清缓存。构建日志 `artifacts/bridge-macos/build-start-20261012-build.log`。
- 2026-10-12 09:54 +08:00 部署至原默认 App 路径；x86_64、20,771,296 B，SHA256 `274ef25eec0761422bde9ed91662ecb7947fa8f6a24118973964283402bb101b`。候选和部署 exe 哈希相同，ad-hoc 签名 `codesign --verify --deep --strict` 通过。
- 仅停止并替换默认实例；私有备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/build-start-20261012-095407`，文件0600/目录0700，state 备份哈希一致，启动前 data/installation-id 未变；启动后 installation-id 再核对一致。
- 旧 App 保留于 `artifacts/bridge-macos/Codex Status Bridge-before-build-20261012-095407.app`；旧 exe hash `664b86af8cc2013690c78e79b2eace1e9e725afab957636bc873ecf5a27f06f5`。
- 默认 LaunchAgent supervisor77530 / child77535，实际进程路径匹配；8765 health204、8766 MCP tools/list200/43工具，bridge_status200且无RPC/工具错误。后续状态及端口归属复核通过。Codex CLI 已安装且 login status 为 ChatGPT 登录；未把登录状态当作本轮成功取得额度证据。
- 结构化证据 `artifacts/bridge-macos/build-start-20261012-deployment.json`；未做源码改动、固件构建/刷写、模板发布或提交。菜单栏/UI人工使用、设备通信/中文实机与长期矩阵仍按 backlog 保留，本轮仅构建及启动冒烟核验。
