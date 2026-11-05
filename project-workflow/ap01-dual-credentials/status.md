# 状态

2026-11-05 构建完成，未部署、未改运行数据，0031 未动。完整文件身份见兄弟 firmware `build/dual-credentials-20261105/release.json`。

Bridge：`artifacts/bridge-macos/Codex Status Bridge Dual Credentials.app`，exe 81457936 B，SHA256 `fc93068b4c8629e88bd984cadaf77703ec485678f6ba1ba07d32f3721eb9814d`。以当前工作区构建，包含已有其他业务改动；未提交或撤销其他改动。

设备码保存在运行 data 的 `device-code-<MAC>.json`，复用原子写/0600；token 仍在 `device-token-<MAC>.json`。没有把码输出到 MCP、页面、日志。首次需输入有效码并完成读取；现有 token 不会自动转换为设备码，当前真实设备码仍未获取。

平台 ROM 6933864 B，SHA256 `1d80fe046f479705333c6f1070829da78da712ceb00570869fc905089eb726eb`。执行代码与上一候选相同，沿用 firmware 设备码鉴权；新构建仅审计路径摘要导致包哈希变化。App 包与单 App 清单沿用上一验证包，未修改业务协议。
