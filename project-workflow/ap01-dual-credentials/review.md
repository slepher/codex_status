# 验证

定向 Rust 测试通过：设备码保存、空输入复用、错误码不覆盖、跨 MAC 不读取、token 共存及原有 Wi-Fi 丢 ACK 流程。Node 页面测试通过；WKWebView 82 场景通过，已查看 560/1000 AP01 维护输入截图，文案完整。Bridge codesign 校验通过；ROM 重建及 test-firmware-0041 通过。无新 QEMU 或实机验证。

firmware 自身不扩展 token 权限；Bridge 设备码用于维护，业务 token 继续配对通信。此轮没有实现用八位设备码代替 C2 业务 principal/claim，不改变业务所有权合同。历史鉴权清理另行进行。
