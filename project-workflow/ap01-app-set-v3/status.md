# 状态

2026-11-04：core 复用 bounded transport 新增 AP01 v3 请求适配，校验菜单码、MAC/事务、块摘要/边界和成功响应身份；未知保留 dispatch 分类，不重发。隔离 target 下 cargo test --offline -p bridge-core device_client:: --lib，12 测试通过（含路由与真实回环 TCP 的身份错误/截断响应）。application service、AMF3/APX2 校验、持久回执协调和 UI 未接。未启停/部署 Bridge，未连接实机。

2026-11-04 产物校验：`core::ap01_app_set` 解析 AMF3 与资源汇总、规范 ABI 3 APX2 和 BASE32，按变更位图选择已校验包；使用真实业务包/peer 执行 2 项测试（含全部 AMF3 截断、缺包/重复、重算摘要后的非法结构）通过。命令：设置 AP01_MANIFEST/AP01_PACKAGE/AP01_PEER 到固件仓库实际产物后，cargo test --offline -p bridge-core ap01_app_set:: --lib -- --include-ignored。未接持久协调/application service/UI；总资源能力匹配仍待实现。

2026-11-04 能力匹配：Manifest::check_capabilities 核对设备 app_max、包/对齐镜像/堆/栈/任务/页面、服务位图与 RAM 扣驻留预留。真实双包恰好边界、缺字段、超限、预留反转测试通过（同上隔离 Rust 命令）。固件已发布对应 limits，Python CLI/C++ TCP 模型交叉通过；不是硬件容量实测。持久回执协调与 UI 仍待实现。

2026-11-04 持久回执：新增 Receipt 类型，首次独占创建/fsync，后续复用平台 atomic_write；保存原设备/端点/事务 ID、Manifest 原字节和旧集合围栏，不保存设备码或任意响应。恢复读限 32 KiB，严格字段；进度核对身份、单调序号/偏移/Ready、终态与文件一致性，写入失败不推进内存。3 项定向测试（含真实集合）通过；尚未接异步上传/重连协调器和 UI，不宣称 Bridge 端到端安装完成。

2026-11-04 单步异步协调：新增 advance，先重载回执并查询原事务，每次最多发送一次修改；按已确认偏移选择变更包、Ready、commit 或显式 cancel。落盘失败保留 AckUnknown，Succeeded 还需新 boot 和完整健康运行集合一致。7 项集合定向测试（含真实双包、commit 回复丢失后恢复且不重复 commit 的 TCP 测试）通过。尚未接 application worker/UI，也未完成 Rust 与 C++ 服务交叉验收。
