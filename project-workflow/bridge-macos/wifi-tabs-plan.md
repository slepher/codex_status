# Wi-Fi 分 tab 落地与渲染验收（2026-10-03）

## 范围与判据

用户要求落实此前示意中的 Bridge 方案 / 设备方案 / 比较与同步三个 tab，并安排测试。
当前生产页面把三块纵向堆叠，表单没有字段布局；既有 VM 测试不能检测 CSS 与真实 DOM。

1. 三个语义 tab，默认 Bridge 方案；鼠标及方向键/Home/End 可切换，只有选中面板可见。
2. 切换只改变可见性，保留表单、草稿、比较选择、预览和持久任务，不联系设备。
3. 表单使用独立字段组，宽屏双列、默认窗口单列。网络列表、比较表、恢复操作和任务区有清晰层级。
4. 原服务与 Wi-Fi 协议保持；本次不需要更新 ROM。

## 执行与测试

- 更新生产 ui/index.html；既有 JS 回归覆盖 tab 状态与无隐式调用。
- 增加 tools/test-wifi-ui.swift + tools/wifi-ui-scenarios.js：macOS 原生 AppKit/WKWebView 加载同一生产 HTML，使用合成无密码 IPC 返回，实际点击/键盘事件、计算布局与截图。
- 检查 560×680 与 1000×800：三个 tab、空态、多绑定列表、长文本、比较/预览、离线等待与错误、编辑保留；断言只有一块可见、字段独立、无横向溢出及无 JS 异常。
- 先在原页面跑出缺 tab 的失败，再修复通过；截图存 artifacts/bridge-macos/wifi-ui，逐张查看。截图用于本次人工审阅，不自动把未经审阅的页面当作正确基准。
- 原生渲染测试不运行真实 Rust IPC、BLE或改网；既有 service/Fake ROM 测试仍负责这些行为。此处采用系统 WKWebView 避免为布局验证引入整套驱动依赖，不声称是 WebdriverIO 全应用 E2E。
- 构建/备份/替换当前 macOS Bridge，复用现有 LaunchAgent 与数据；校验产物哈希、运行路径、HTTP/MCP。最新现场写 PROGRESS，B6 保留硬件验收边界。

## 状态

已完成实现、原生渲染验收和 macOS Bridge 部署；用户随后提供的真实 Bridge 截图已显示三个 tab 与字段布局。

运行回归：

```sh
node tools/test-device-page.mjs
bash tools/test-wifi-ui.sh
git diff --check
```

原页面在真实 WKWebView 中先失败于 Missing three Wi-Fi tabs；修复后 6 个场景 × 2 个窗口尺寸共 12 项通过，输出 14 张截图，tab 切换 IPC 调用为 0。证据在 artifacts/bridge-macos/wifi-ui/{red,green,logic,build-final,package}.log、results.json 和 verification.json。原生测试使用合成 IPC，未覆盖真实 Rust/设备通信，也未建立自动像素基准。

部署应用 artifacts/bridge-macos/Codex Status Bridge.app：75,192,688 B，SHA256 B827FA658CCC1A2E3E86B44592A909550773251683A837C78E67155B7D80CF1A。打包产物与构建产物 UUID/27 个代码与数据段一致；原 LaunchAgent supervisor PID33643，父 PID1，子进程33647；health204，MCP43工具及 bridge_status 正常。旧应用 before-wifi-tabs.app 和 AppSupport/backups/wifi-tabs-20261003-233745 私有数据备份保留。安装身份/endpoint 配置未变，ROM 未改。

## 随后真实读取现场（23:44–23:45，北京时间）

用户询问是否插 USB 调查。任务 c98bf7178356cbdc 仍 waiting_rendezvous，第5次尝试，阶段 snapshot，错误 http_status_unavailable，sent=false，snapshot=null。23:44:02 BLE status/plan 获 ACK，wifi_authorize 收到回复；随后 /api/status 超时或 transport_or_protocol，独立公共 /status.json 3s 探测也不可达。完整方案需认证 HTTP，BLE 可联系不证明 Wi-Fi 可达；当前仅定位到 HTTP 状态前置失败，不能判定深睡、入网失败、IP变化或 RF 为根因。尚未进入完整快照分页。

先建议短按 BOOT 开 120s 维护窗口并观察 HTTP；暂不需要 USB。未打开串口（可能触发复位）、写真实网络或取消任务。固件 handleBootClick/enterBleOn 和 Bridge opportunity_window 为依据；USB不等于维护写授权。实机配置矩阵仍归 backlog B6。

后续协议审查及23:43/23:44时间关联更正见 [rendezvous-audit.md](rendezvous-audit.md)。

## 2026-10-04 设备 → Bridge 使用已读取快照

用户明确方案选择语义：选择上次完整读取的设备方案，将选中字段保存到 Bridge；此操作不再次读设备、不创建读取作业、不要求设备在线/BOOT。设备页“读取设备完整方案”独立更新快照。删除“刷新后导入”UI入口，旧 read_import 请求在 app 入口兼容为立即缓存导入，保留已持久化旧作业的恢复能力。预览与本地 revision、snapshot_id 绑定，提交在服务事务内检查相同快照，防止选择后被后台读取替换。验证 UI IPC 无 get/read_import/authorize 与无新增 jobs、MAC/版本隔离，原生窄宽截图；重建并部署现有默认 Bridge，保留数据与回滚包。legacy NVS 迁移仍未完成，不能用本地导入结项。

本项已实现、验证并部署；app离线且投递两锁held的生产入口导入通过，core Wi-Fi8通过，JS与原生12场景通过；560/1000设备→Bridge方向截图审阅通过。当前部署哈希见PROGRESS最新节，未执行真实用户方案导入作为测试。
