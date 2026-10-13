# 2026-10-13 在线读取仍排队调查（B6）

范围：用户截图最后完成的 `b6345f4fbc1b14d7`；只查原因，未取消/重发任务、未改运行数据或部署代码。

## 已核实的现场

- job 实际 kind=read，request_id=w3_0000000000000039；created_at=1791881583（16:53:03），attempt_count=1，last_attempt_at=1791881593（16:53:13），completed/complete_snapshot，revision=265。不是写配置同步。
- 私有 `data/logs/bridge-app.log.2026-10-13`：16:53:03.363 发送 status，04.030 HTTP200；08.891 再发 status，09.654 HTTP200；10.684–10.748 claim；12.250–12.506 plan ACK；13.247–13.632 status；14.010–14.509 两页 snapshot 均HTTP200。
- 因此确有等待，完整读取在创建约11秒后结束；实际两页快照约500ms。不是只闪过 queued 文案；本段没有 HTTP 不可达证据。
- 持久时间使用 attempt 开始时的 now，故 updated_at=16:53:13 不能当作精确完成时间；以 HTTP 日志确定完成到16:53:14.509。

## 调度原因与证据强度

1. `app/src/wifi.rs:110–116` 创建read后调用通用 opportunity，没有传新job_id。`core/platform/wifi.rs` 统一以queued入库。这只证明持久接受，不应意味着必须等下一周期。
2. `app/src/wifi.rs:524–540` 获取设备锁失败直接返回；获取成功也只按jobs原顺序 `.find(admit_wifi)` 执行一个任务。新read追加在末尾，旧待确认事务到期可先被选中；旧任务失败后本轮不继续新read。
3. `Job::runnable` 对 confirmation_pending 独立放行，忽略执行意图期限；admit_wifi仅排除waiting_input，不排除transaction_identity_unknown。`prepare_transaction` 才检查缺失namespace，失败更新退避并返回。现场有三条这种旧任务，因而重复身份失败会消耗在线执行机会。
4. 后台cycle在status/claim/同步/PowerPlan后才调用opportunity；后台主循环sleep10秒，refresh另有节奏，不是每次新请求都有定向HTTP唤醒。现场可见读取前claim/plan的额外工作。

用临时Rust探针直接抽取生产 confirmation_pending/execution_active/terminal/runnable/admit_wifi 方法，按相同 `.find` 顺序验证：过期但结果未知的旧任务仍先于新read被选中；旧任务退避后新read才在下一机会入选。两项断言通过。探针只在内存运行，不联系设备；不冒充完整app或实机重现。

**边界**：当时日志没有opportunity触发源、选中job_id和锁竞争记录，历史next_attempt_at已被后续更新覆盖，不能断言16:53:03到底选中了哪条旧任务，或把约11秒全部归因于旧任务。已确认调度缺陷和后续后台前置工作；旧任务抢机会最符合现场，也保留锁竞争可能。

## 修复方向（本次未实施）

持久化仍保留。在线用户请求应获得针对自身job_id的有界立即执行机会；只有真实通道忙/不可用才进入有原因的等待。身份缺失等不可自动恢复事务保留结果未知的事实，但不继续自动争抢执行机会；不删除、不猜namespace、不换ID重发。验证需覆盖“在线新read+旧身份未知事务”和设备锁忙后的有界续办，记录job_id/触发源/等待原因，才能精确解释以后每次排队。

## 用户澄清：确认现有任务状态异常

用户随后澄清重点是查明队列现有任务为什么异常，而非改排队文案。已撤下本轮尚未部署的实现及测试源码增量，保留此前所有修改；未重启Bridge或修改运行数据。

17:04本机状态再读：三条任务仍awaiting_confirmation，namespace均缺失、sent/may_have_committed=true、confirmation为空。10月4日sync（3c4519756ecc9178）累计attempt_count156，10月5日sync（a7efde70624b7097）31，10月13日activate（1ef9c9c688225ab2）4。前两条execution_intent已cancelled；三条最新updated_at=17:04:09、next_attempt_at=17:05:09，error_code变为rendezvous_failed。取消来源未追溯，不推断是谁执行的。

原因：
- runnable中的confirmation_pending分支不受execution_active/取消/到期约束，认为仍应查原事务。
- 缺namespace的已发送任务在wifi_bind_transaction明确禁止猜测补齐，所以不能通过正常自动查询恢复；却没有停止自动调度的独立阻塞处理。
- finish允许confirmation_pending任务更新状态，取消后再执行就重新变成awaiting_confirmation，execution_intent仍cancelled；“取消执行”与“历史结果未知”混在当前任务状态中。
- rendezvous.rs:506–517 对已准入任务批量记录会合失败，覆盖根因error_code。故后来看到rendezvous_failed不说明身份问题已经消失。
- 核对当前配置只能证明当前内容，不能冒充旧事务已成功，也不能用新身份重发旧操作。

额外复现：使用现有app Fake HTTP夹具从真实wifi::action(get)入口运行新增临时回归，在线设备+旧已发送无namespace的activate下，结果返回queued/attempt_count0，预期completed，稳定失败。日志artifacts/bridge-macos/wifi-immediate-red.log；临时回归源码仅存artifacts/bridge-macos/wifi-immediate-repro-test.rs，未把失败测试留在生产测试套件。

Review：调查完成；应保留不可确认的历史事实、停止无意义自动尝试，并明确区分取消/需处理/正常等待。实际修复尚未实施，不以改显示文本代替状态机修复。

## 再发生风险修复：Plan / Task / Status / Review

用户授权若仍可能发生则修逻辑。新事务取消/过期与重绑定身份变化同样可触发，故需修复，不能仅取消旧记录。

- [x] 准入以执行意图、期限及身份可核验为前提；历史unknown不删除、不重发，通用失败不覆盖硬阻塞或撤销状态。
- [x] 显式新请求获得自身HTTP机会；设备忙/不可达显示实际原因。
- [x] Core与app Fake回归先红后绿，UI回归和窄宽截图。
- [x] 构建/备份/部署Bridge，核验遗留任务不再调度、健康与数据采集，更新交接。ROM不变。

Status：实施中。Review：待验证。

实现证据：`wifi-state-red.log`以完整身份的新任务证明取消仍自动重试；修复后Core153+22集成通过，App94通过/3条件忽略；加强后的在线新read+旧合法待确认任务、设备锁忙/续办、离线失败回归通过（wifi-immediate-final.log）。取消/过期停止自动确认，已在途可信结果仍可确认，缺身份/身份变化转needs_attention；历史记录和原request_id保留。通用通信失败不覆盖这些状态。UI66场景、设备页JS通过，560/1000任务区域截图已审阅；历史取消仍显示“原操作结果未知”。最终候选构建中，尚未部署。

## 17:32 部署验收

最终Bridge20,739,536 B，SHA256 `df4d523d318446dfb78a7ad9a46aa8245ab35acb0efbbfde29e14782e210a7b2`；release2m51s（最终源码）、签名严格验证、托管PID1941、health204。旧237cd5应用及134个data文件停服务后私有复制并逐文件hash核对，位于`~/Library/Application Support/dev.codexstatus.bridge/backups/wifi-task-state-20261013/`。ROM未变，无Git提交。

只读MCP核验：两条旧sync恢复cancelled，activate为needs_attention/transaction_identity_unknown；三条sent/may_have_committed仍true，confirmation仍空；并未伪造完成。尝试次数156/31/4。本地3目标ID及顺序、设备快照revision265不变；Codex envelope可用、quality good/error null。认证状态刷新返回offline/http_connect，本次没有新建实机Wi-Fi任务，在线立即执行和忙后续办由app模拟设备回归验证。

证据：wifi-state-core-final.log（153+22）、wifi-state-app-final.log（94/3ignored）、wifi-immediate-final.log、wifi-state-late-receipt.log、wifi-state-ui.log（66）、wifi-state-release.log、wifi-state-deployed.json、wifi-state-live-first.json。窄宽任务区截图无溢出，取消历史保留原操作结果未知提示。17:35最终复查：跨155.6秒（超过原60秒退避），三条任务状态/尝试次数/更新时间全部不变；证据wifi-state-live-final.json。Status/Review：本次修复及部署完成，保留未知历史结果，设备在线实机即时读取未验。
