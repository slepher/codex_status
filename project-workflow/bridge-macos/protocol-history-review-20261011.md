# 协议提交历史审核

日期：2026-10-11。用户要求先由 Luna 检查提交历史，再据报告规划。关联 backlog A9/B6/C8；本文为证据报告，不建立第二份待办。

## 范围与方法

Luna runner 只读检查，主代理复核关键源码、提交及测试入口。HEAD `02f0965`，仓库 HEAD 可达提交129个；`git log --since=2026-09-23 HEAD` 共85个，关键词初筛45个，重点检查下表10个提交。不是全部协议端点的穷尽审计，也不是安全审计。未运行测试、未访问设备、未构建/部署、未修改生产代码。

筛选命令（PowerShell，仓库根目录）：

```powershell
git log --format='%h|%ad|%s' --date=short --since='2026-09-23' HEAD |
  Select-String -Pattern 'protocol|BLE|Wi-Fi|WiFi|HTTP|status|OTA|Bundle|claim|owner|lease|ACK|rendezvous|sync|data|diagnostic|activation|idle|compact|bounded|outcome|cache'
```

主代理独立运行 `git rev-list --count HEAD`、`git rev-list --count --since=2026-09-23 HEAD` 返回129/85，核对当前源码和 `e0e3084` diff。Git/定向读取成功；一次搜索误用Windows通配路径失败，改用 `rg -g` 后成功。宽泛搜索被中止，后续限定 `bridge/crates`。这些搜索不构成测试执行。

## 历史修复链

| 提交 / 日期 | 行为与主要证据 | 本轮判断 |
|---|---|---|
| c1ab267 / 09-28 | 统一协议与诊断流；app/sync_diagnostics.rs、core/device_client.rs、device-sim/tests/bootstrap.rs | 奠定主链路与Fake覆盖；不证明所有收尾分支遵守同一准入 |
| 94b7036 / 10-04 | Wi-Fi会合、缓存同步与恢复；app/rendezvous.rs、wifi.rs、sync_diagnostics.rs | 会合与诊断尾部需联合审核，不能分模块各自通过即结案 |
| f8426cb / 10-04 | 变长Wi-Fi存储及认证恢复写入；src/wifi_management.inc、wifi_storage.inc、wifi_setup.cpp及test-wifi系列 | 属于存储/恢复修复，不能用来证明RF与画面状态一致 |
| 1b8737a / 10-05 | 统一Wi-Fi控制、过期缓存冲突；app/wifi.rs、core/platform/service.rs、wifi_management.inc | 后续应保留缓存冲突及编辑内容语义；本轮未证实新缺陷 |
| 64c3514 / 10-06 | BLE-first、owner原租约截止；owner_store.cpp、app/rendezvous.rs、test-owner-lease.cpp | 修复身份/租约层；跨通道换接仍须保持这些约束 |
| 102b3c6 / 10-07 | BLE INFO并发及有界Wi-Fi恢复；ble_bridge.cpp、wifi_setup.cpp、app/wifi.rs | 并发与恢复预算已有专项修复；不能覆盖所有上层调度组合 |
| c97fd87 / 10-08 | compact frame及有界HTTP会合；device_client.rs、rendezvous.rs、rendezvous_protocol.* | 当前ready生命周期问题位于这条协议链，不能用“有界”证明语义正确 |
| e0e3084 / 10-08 | 空闲联系不激活Wi-Fi；rendezvous.rs增加实际工作准入、ready报告owner验证 | 入口准入已加强，当前诊断尾部仍以另一条件进入HTTP，修复范围存在缺口 |
| a3c1bdf / 10-09 | 大Status分页、整体期限、会话锁、Wi-Fi独立显示；device_client.rs、diagnostics.cpp、network_display_state.h | 已有大Status生产client回归；无证据宣布分页当前失效，组合故障仍应纳入验收 |
| cf1cfa6 / 10-09 | 激活恢复、过期缓存自动刷新、自动Data投递、延迟OTA确认；platform.rs、service.rs、bootstrap.rs及固件模块 | 已补恢复路径；本轮两项会合缺陷仍在，不能据一次成功同步结整个协议任务 |

以上路径以 `bridge/crates/` 下所属crate为准；提交可用 `git show <hash> -- <path>` 重查。历史测试存在不等于本轮已重跑或实机已闭环。

## 当前确认的两项P1

1. **BLE-only成功后诊断尾部绕过HTTP准入。** `bridge/crates/app/src/rendezvous.rs:447–452` 使用 `opened || (enabled && light)`，未继承本轮 `_need_http`。因此即便入口决定不需要HTTP，sync enabled且Light仍能进入诊断。`sync_diagnostics.rs:426–433` 有最多3次、每次15秒Status尝试及1/3秒间隔（受剩余预算限制）。10月11日旧现场约49秒尾部与源码吻合，不能把它算作已成功Data的失败。
2. **瞬时会合ready被当成HTTP持续可用证明。** `rendezvous.rs:533–540` 显式会合要求ready=true；`src/rendezvous_protocol.h:23` 仅HttpReady返回true，`.cpp:25–28` finishExchange立即转AckGrace，`src/firmware/radio.cpp:688–689` 在报告ACK后触发。合法HTTP服务仍存在时，迟到验证可能循环到预算耗尽。应保存/验证原事务与身份关联，不能直接删ready检查而放宽身份、owner或Plan期限。

现场引用：[10月11日调查](wifi-terminal-review-20261011.md)。其中没有逐条matching_http拒绝码，不能断言每一次HTTP200均因同一条件被拒。本轮没有重新采集现场。

## 测试缺口与已纠正的报告结论

- `bridge/crates/app/src/platform.rs:3294` 的 `wifi_idle_repeated_contacts_do_not_probe_http_or_activate_network` 确实调用生产 `ble_cycle` 四次并检查零HTTP；这是app全链测试。但场景为idle/sleep，不覆盖BLE-only + sync enabled + Light的尾部分支。
- `bridge/crates/device-sim/tests/bootstrap.rs:1601` 已有完整大Status分页生产client回归；`:3582` 已有延迟OTA重启保留原job并验证镜像的回归。后续补组合与失败边界，不重写已有同义测试。
- `.75` **已OTA部署并核验**；当前未提交源码已新增WifiPhase及blink，`NetworkDisplayState`已观察阶段/相位。删除Luna初稿“OFF源码仍未修”的判断，准确状态是实机闪烁/连续周期未验、模板Bundle未发布。当前旧模板兼容路径与新模板验收需分别记账。
- Luna初稿的工作区文件计数不准确，不将其作为证据；本报告不以文件数判断功能完成。

## 归因与边界

历史与当前源码支持的工程判断：问题反复出现在跨阶段交接——入口准入与尾部准入不一致、瞬时阶段与持续能力混用、业务ACK与后续诊断结果混用、分支测试通过却遗漏状态组合。不是“没有测试”，也不能仅凭这10个提交推导所有协议都存在上述问题。

owner/session、ACK丢失、缓存冲突、OTA/Bundle恢复属于后续审核范围；本轮没有证实新的鉴权绕过、重复提交或持久损坏。执行计划见[协议闭环计划](protocol-followup-plan-20261011.md)。
