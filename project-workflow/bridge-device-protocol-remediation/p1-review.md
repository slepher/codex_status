# P1 审阅（2026-11-06）

## 实现及设计差异

- HTTP Status、Wi-Fi完整快照及BLE摘要共享 `jsonChunk`：按UTF-8边界切片，再计量JSON转义后的实际载荷。HTTP原文上限3600B/BLE320B；BLE预留紧凑回执元数据，实际页长通常低于320B，不能承诺RTT恰好减半。分配失败不推进offset；Status同页可重放，身份/期限/总长不变。
- BLE已有rid/pid/f/ctx及s/c数组，保留紧凑字段布局、删除字符串Base64；明文控制字符拒绝（防止序列化产生非法JSON），UTF-8/字节上限继续校验。sync-v1二进制Base64保留，不属本阶段。
- 删除ReportSession、session/report路由和固件双段HTTP。单包presence只确认收到hint；来源IP必须等于上报IP，登记MAC/认证token通过后再反向Status，核对MAC、boot、owner和HTTP就绪。正常协调器继续校验业务计划和窗口，不把202当业务成功，不创建新任务。
- 固件一个按需分配的PresenceWork与6144B任务栈。工作任务仅使用冻结参数和自己的NetworkClient/HTTPClient，主循环读release/acquire完成标志后回收；网络I/O不访问主循环String/日志/设备状态。晚到结果须匹配IP、owner、原会合ID/plan和期限。普通presence30秒一次，ready最多3次，不延长电源期限。
- 删除ready state中出站session/reportBody/originSession/originRequestNo/dataResult；保留PlanActivation及有限重试状态，它们仍承担网络激活与结果收尾职责。
- HTTP只解析响应头；按地址一个带锁空闲连接，5秒后或runtime不同不复用。独占取出意味着取消即关闭；只归还完整Content-Length、非close、无多余字节的HTTP/1.1响应。拒绝重复Content-Length和Transfer-Encoding，不自动重发未知写。
- 跨Tokio runtime回归首次发现旧reactor连接失效：已增加runtime归属校验；这是实际修复，未通过改测试runtime来掩盖。旧Hex夹具已同步最终格式。

## 资源与验收边界

- 空/单条/典型/上限新增持久字节均0，NVS格式不变。
- 新常驻只持有一个工作项指针；无任务时不保留任务栈。单个/典型/最大并发均最多1项：工作项按String实际长度分配、6144B任务栈及FreeRTOS控制块、HTTPClient缓冲和响应JSON临时分配；满时合并等待，不创建空队列。任务/工作项分配失败按原有限次数或普通30秒周期退出重试，不无限阻塞主循环。
- Status仍冻结最多16KiB原文；原文chunk最多3600B，JSON载荷最多4048B。substring临时量、ArduinoJson字符串副本、序列化输出和外层信封生命周期分别计费；不以设计中的约4KiB当实际峰值测量。BLE快照和摘要仍按实际长度冻结，chunk按400B预封装预算裁切。
- 最终Note4静态RAM97800B，Flash程序1815935B；新presence任务6144B是动态额外成本。匹配ELF栈明细见status，不以局部链代替HTTP库间接调用和实机高水位。
- 实机RF/通知交付、Win/mac原生驱动、网络超时期间按键/屏幕响应、任务堆/栈高水位和功耗仍待验。没有部署或运行中Bridge重启。候选与现场版本同名，必须以SHA区分，P1 wire要配套Bridge/设备更新。

## 补充软件闭环

- presence反向回读沿用已登记端口，只从hint学习同源IPv4，不接受hint指定任意端口。原显式集成测试固定80端口在本机权限不足；改用登记的非特权临时端口后，真实处理器→认证Fake状态→排队作业→后续不重复激活闭环通过，没有放宽IP/MAC/owner校验。
- Wi-Fi多页测试原先依赖768B固定页长，P1下会一页完成；现分两次合法4条写入准备8条长UTF-8/转义/凭据配置，覆盖实际多页和修改后原snapshot失效，不放宽每批容量。

- 当前Note4所用Arduino WebServer源码明确发送 `Connection: close`，因此该目标继续正常短连接；本阶段仅交付Bridge的可复用能力，不虚报Note4握手减少。支持keep-alive的Fake/其他服务端已验证复用；修改上游WebServer不在本轮范围。
