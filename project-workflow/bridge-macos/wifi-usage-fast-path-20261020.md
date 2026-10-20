# Wi-Fi余量事件投递修复

## Plan

- 保留app-server通知响应，源兜底采集上限60秒。
- 源成功刷新通知独立快照消费者；每台在线设备独立合并事件，使用已有HTTP Data/ACK路径，不等待安装六轮门限或其他设备。
- 用户进一步明确：所有新工作首次就投递，仅已发送待确认队列保留六轮重试；不能把首次安装也挡住。保留安装/激活/OTA顺序、owner、认证、未知事务；不因Data续Light。

## Task / status

- [x] 回归：通知即时响应/60秒兜底、Data首次立即投递、双MAC隔离、忙设备事件合并、pull-only无写入、丢ACK冻结原事务不提前重试。
- [x] Core181/App122全套通过；App3项及Core2项实机条件测试忽略，Core集成另26项通过。最终新增断言重编定向3项再次通过。
- [x] 11:23部署本机Mac Bridge，认证同MAC Data871 HTTP ACK applied/displayed；固件未变。未提交。

## Review

## 实现

- `runtime.rs`保留app-server通知等待，兜底间隔限制1–60秒；App/CLI缺省60秒。成功读源后通知独立平台消费者；不再通过server_time判定是否消费，避免同秒通知被吞。维护轮不重新喂旧快照，不伪造源新鲜度。
- 平台消费者只更新源快照和发送每MAC的watch邮箱；网络worker按设备串行、跨设备独立，邮箱仅保留最新事件，避免慢设备堵采集或无界任务堆积。原10秒维护仍用于状态/作业，不是10秒轮询Codex源。
- 新Data（dirty/full到期且无in_flight）、waiting安装和首次activate立即执行；已有in_flight/sending/重复activate按该设备上次尝试后的6个维护tick重试。source更新不能把未确认Data变成新ID/新内容。
- 在线Data复用deliver/认证/ACK路径，排在诊断同步之前；安装/OTA/activate优先级与owner/数据合同/源就绪围栏保持。Data不续电源期限；正式Plan仍由原路径决策。
- 暂停、按MAC排除、AUTODELIVER开关保留。worker邮箱随设备删除/排除关闭；任务退出可在下个机会重建。不改协议或固件资源布局。

## 验证

- 日志：`artifacts/bridge-macos/wifi-usage-fast-path-20261020-tests.log`。全套使用本轮源码重新构建；新增跨MAC/冻结重试断言在全套后定向重编通过。不是复用10月19日二进制。
- `source_notification_is_immediate_and_fallback_is_one_minute`用Tokio虚拟时钟验证等待前通知保留、等待中通知立即响应、旧180秒配置在60秒兜底。
- `source_event_dispatches_first_data_and_coalesces_while_device_busy`走真实平台消费者→设备worker→cycle→生产HTTP客户端→Fake C2 ACK；持有A设备锁时更新源两次，B仍先收到Data；释放A后收最新80，再同秒源变成70立即发送。只改resetsAt不发送。丢ACK后源再变化不提前重试/不改原冻结payload。
- `new_work_bypasses_retry_gate_but_unconfirmed_data_does_not`覆盖新Data、新waiting安装、首次activate，以及sending/重复activate/in_flight/pull-only。
- 首轮本机CLT缺cstdarg，沿既有MacOSX15.2.sdk和CXXFLAGS解决；未安装/修改工具链。`git diff --check`通过，无临时DEBUG日志。

## 本机部署

- 默认受管实例路径核对后停止；旧app/data/installation-id备份到`artifacts/bridge-macos/wifi-usage-fast-path-20261020-backup`（0700），167文件逐文件SHA核验。替换前全部data文件仍匹配备份；不清配置、配对或回执。
- 独立候选由`tools/build-bridge-macos.sh`生成；最终构建日志`wifi-usage-fast-path-20261020-build-final.log`，签名验证通过。现用exe 79067472 B，SHA256 `eb4d56a4f436d0b90bfea8e3a5d90ec07457dfd9ee4efc29cb386b70884d3ec9`；旧exe `850f0cc38752c11dfa5fefe60714b783354fe0f3c820714a5ba81b66a050568c`。部署清单`wifi-usage-fast-path-20261020-deployment.json`。
- 11:23受管supervisor85131启动，health204/MCP可读；源quality good、weekly used40（remaining60）。11:23:22.680首次源刷新，设备状态/claim建立后11:23:34.377即发送Data871，11:23:37.287收到同MAC `7C4FADB93408`的applied/displayed ACK；首次发送未等60秒。实际Data HTTP1.087秒。此轮是启动重确认，同CRC `1b9c99be`，不是新的真实余量变化，不能冒充真实余量变化端到端时延。
- 11:24:24.168再次源刷新，间隔61.487982秒（60秒等待+RPC）；认证缓存同MAC `.105-note4` 的data/applied均871、displayed。白名单证据`wifi-usage-fast-path-20261020-live-verification.json`。

## Review / 资源与边界

### 60→59后的Light无Wi-Fi反馈（续查）

11:26:14 Data872/Plan5880 BLE联合ACK、Light600秒已确认，但同轮network_admission为needs_http=false；11:27:21下一轮diagnostics_due=true才network_start，用户随后见图标。不是仍在sleep，也不是仅屏幕旧帧。

- 根因：ble_cycle先按传输大小/已有Light/诊断计算needs_http，小push当轮仍是sleep且BLE能装下，判false；随后生成的正式Light600秒没有反向更新该网络准入。设备正确退出sleep，但这次没有network_start。
- 曾探索仅在needs_http加入小push并补发network_start。生产BLE回归确实复现旧代码“Data59/Light600成功、整轮Ok、need_http=false”；探索补丁使同轮network_start及HTTP证明通过。新增同值周期对照未通过（App122通过/1失败/3忽略），不能称全套通过。证据`small-push-wifi-20261020-{red,green,control,app-tests}.log`。
- **用户随后明确纠正：原设计由Plan本身发放激活，不应把network_start作为另一项必需业务指令。探索补丁、对应测试夹具扩展均已撤回，未部署。** 11:23快路径修复仍保留；现用exe仍`eb4d56a4…884d3ec9`。构建出的small-push候选已记录withdrawn，不得据其构建成功称问题修复。
- 历史定位：`c97fd87`（2026-10-08 19:50:58+08:00）加入need_http/network_start分拆；当前`applyV2Plan`只设置Light和awaitNetworkStart，`transport.cpp::network_start`另外绑定回报路由并设置v2RvNetworkRequested，`radio.cpp`依赖后者真正开网。这是Plan与激活效果脱节的具体位置。
- 已按最新用户语义补充权威设计/场景合同，实施表明确当前缺口。后续应将网络选择、回报路由、激活与失败反馈归入同一Plan原事务；保留按当前Host SSID选网、owner/session/MAC鉴权、有界RF/HTTP证明和原ID幂等，不直接在Plan ACK后无条件连接旧SSID，也不通过让图标假亮掩盖问题。需要固件与Bridge配套修改/验收，**尚未实施该协议修复**。

无固件构建/刷写，ROM、设备RAM/栈/NVS增量0。Bridge没有新增持久格式，运行时每个已调度MAC仅一份watch最新tick及worker状态（O设备数），没有按空容量预留或逐事件快照队列；具体allocator峰值未测。既有源/事务持久化和原子提交复用。已保留接手时的其他未提交修复。

当前完成软件修复与Mac部署、同MAC启动Data ACK。自然新余量变化的实际通知延迟、面板目视、长期RF和Windows配套部署未验；上游跨会话通知为何未到达仍未确定。兜底缩为60秒后不依赖通知保证一分钟级采集，不保证网络/面板总延迟为零。A9/B6/C8保持各自剩余验收边界。
