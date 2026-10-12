# 2026-10-10 macOS Bridge 重建与启动

用户授权：构建当前工作区最新 macOS Bridge App 并启动。

## 计划

1. 核对最新 PROGRESS、源码状态、管理实例路径及本机构建依赖。
2. 使用现有缓存和固定 ArduinoJson 7.0.4 构建独立候选 App，核验签名与 SHA256。
3. 仅停止默认管理实例，私有备份运行 data 与 installation-id，保留旧 App 回滚包。
4. 替换默认 App、通过 LaunchAgent 启动，核验进程、默认 HTTP/MCP 端口及健康响应。
5. 更新本记录与 PROGRESS；不提交代码。

## 状态

- 工作区干净，HEAD `00914ab`（Plan joint PowerPlan and data delivery）。
- 最新交接为 2026-10-05；联合下发仅完成计划，尚未实现。
- 默认 LaunchAgent 指向仓库 artifacts/bridge-macos/Codex Status Bridge.app。
- 构建、部署与启动核验进行中。

## 用户补充：先同步 Windows 最新源码

- 用户指出 Mac 代码不是最新，要求连接 Windows PC 并推送最新代码；构建暂缓，先取得 Windows 源码。
- 本机 SSH 配置无 Windows 主机、已连接项目仅本机；已请求 Windows SSH 地址/用户名与仓库路径。
- 默认 Bridge 实际 supervisor 18431 / child 18433，8765/8766 监听，非受限 health HTTP 204；尚未停止、替换或重启。
- 尚未执行构建、Windows 提交/推送或本机拉取。
