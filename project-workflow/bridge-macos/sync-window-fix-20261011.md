# 临时同步窗口修正（A9/B6/C8，2026-10-11）

## Plan
周期全量Data可在自然会合唤醒并临时在线，不生成正式Light；大Data通过既有sync_open的data原因绑定seq，最多90s，完成后收尾。诊断、完整读取及Data共享原有有界窗口，正式Light期限不受影响。真实push继续正式Light。

## Tasks
- 区分周期Data与push，修正共享PowerPlan判定。
- Bridge/固件合并合法临时窗口预算，完成顺序完整读取→Data→诊断complete。
- 覆盖到期边界、Sleep+Data、同ID重放、完成退出及硬超时。
- 同步协议总览/场景/权威电源文档；构建候选并记录部署边界。

## Status
源码及协议文档修正完成；后续用户授权部署，.83与配套Bridge已上线并核对镜像。完整实机矩阵尚未通过，详情见[现场调查](note4-radio-phenomena-20261011.md)。本文后续“候选/未替换”段落为构建时记录，当前部署以PROGRESS与现场调查为准。本任务不提交。

验证：core 138 passed（[日志](../../artifacts/sync-window-core-tests.log)）；最终 app 82 passed/3 ignored、device-sim 64 passed（[日志](../../artifacts/sync-window-final-tests.log)）；render全部62 passed（[日志](../../artifacts/sync-window-app-render-tests.log)）。定向覆盖周期BLE/HTTP重试保持Sleep、重启无数据源不产生Light意图、Light剩1–2s与0s合并临时预算、原预算/采样锚定不续期、模拟器跨正式到期保留原窗口、原open_id重放不续期、90s硬关闭、Sleep+HTTP Data成功关闭窗口，以及现有慢分页/丢ACK/身份/owner矩阵。最初模拟器失败揭示其到期分支重开已有窗口，已修正并全量重测。

候选：默认env `zectrix-note4-b`，ROM marker `0.18.83-note4` 已核对；[构建日志](../../artifacts/note4-83-sync-build.log)、[匹配ELF栈门禁66项](../../artifacts/note4-83-sync-stack.log)。`artifacts/note4-0.18.83-temporary-sync.bin` 1,927,024 B，SHA256 `6343FCDA4676509BEEB5025F79E991C6BDCFA399F88A4C8336902489AAD1207B`，同名ELF。配套Bridge候选 `artifacts/bridge-temporary-sync-20261011.exe`，构建见[日志](../../artifacts/sync-window-bridge-build.log)；当前实例未替换。

资源：相对.82静态RAM 119324→119332 B（+8），Flash 1873567→1874307 B（+740），BIN 1926256→1927024 B（+768）。新增固定窗口绑定seq/flag，无新任务/仓库/长期堆副本；新增持久字节在空/单条/典型/上限均为0 B，不写NVS或改持久格式。匹配ELF中`sync::dataApplied`局部栈帧272 B（entry 0x110，包含224 B既有SyncStoreBatch检查对象）；完整运行堆/任务栈峰值仍未知，66项门禁通过不代替实机峰值。临时运行状态只占本wake RAM/RTC，不扩大正式计划存储。

## Review
不得只删600s而丢失大Data HTTP通道；临时窗口不写NVS、不续owner、不改变正式计划。错误/unknown保留原Data身份；临时窗口重放不得续期。

实现复用既有sync_open/90s窗口。周期分类按已确认push指纹与冻结in_flight对照，避免把已确认周期事务当真实push；正常push仍生成正式Light。读取在Data前完成，未完成读取阻止Data-only收尾；Data先于诊断complete。认证/MAC/session/wake/owner/低电拒绝保留，模板canonical/CompiledTemplate格式未改。协议总览、48 B编码补充、场景S25–S27、实现对照、通用平台与权威电源文档已同步候选合同。

验收边界：无USB自然睡醒、真实RF下跨到期/慢传输/丢ACK和长期功耗未验证；候选必须固件与Bridge配套部署。A9关联B6/C8继续开放，不能将软件回归或候选哈希记成现场生效。

Bridge候选：37,955,072 B，SHA256 `5A205444A78893ADBBA6629195EA447A14EB5B0C9A8101DA4B668CDBAC6D18EB`。最终`git diff --check`通过；保留原有文档工作区改动。
