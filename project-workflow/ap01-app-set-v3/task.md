# 任务

- [x] Rust v3 路由/身份围栏/错误分类及 TCP 验证
- [x] AMF3/APX2 产物校验、资源汇总和单更/批更包选择
- [x] 设备能力/集合预算匹配
- [x] 持久回执、原事务身份及进度恢复校验
- [x] core 单步异步上传/原回执恢复/显式取消协调
- [x] UI/MCP 共用有界 prepare/inspect/advance/cancel 应用入口，设备登记/操作锁/持久原事务围栏
- [x] 离线 inspect、只读 query、终态清理确认后持久归档及再次 prepare
- [x] 显式地址恢复核验、未确认事务不存在时的本地取消归档
- [x] application worker 自动调度、重连/取消请求生命周期接入
- [x] application service 与 UI 单更/批更/逐包进度/恢复（Node/WK 模拟 IPC）
- [x] core 与生产 C++ HTTP 服务模型及实际产物交叉验收（首装/单更/批更/失败整集回退、丢回复恢复）
- [x] application worker 到生产 C++ 模型的完整交叉验收
- [x] 生产页面函数→真实 application service→C++ 四链（测试 IPC 适配器）

- [ ] Native Tauri IPC validation
- [ ] Automatic network-change discovery validation

- [x] Desktop staged artifact retention/cleanup policy (128 MiB, 24-hour retention, active receipts pin cache)
