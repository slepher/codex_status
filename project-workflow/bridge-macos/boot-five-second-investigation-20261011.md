# 短时唤醒后入睡：设备端证据复核

## 修复计划与状态（用户已授权烧录）

计划：PWR/BOOT 统一独立300秒下界；初始化期间 GPIO0 边沿锁存并去重，timer 时钟快速路径遇到人工事件转正常初始化；底层深睡入口保护有效人工窗。补生产锁存状态回归、Note4 .78构建/栈门禁，再通过原OTA事务发布，认证身份及运行镜像核验。配套部署上一轮已编译通过的 Bridge HTTP联系协议。源码与定向回归完成；.78构建及52项匹配ELF栈门禁、固件模块检查通过。GPIO0采集仅初始化期间启用，ISR仅更新有界状态，主线程接纳人工事件；不在ISR中操作无线/存储。静态RAM118,580 B（相对.77增加608 B，包括GPIO中断支持）、Flash1,858,379 B；无新增NVS格式或持久字节，配置空/单条/典型/上限均不变，动态峰值仍待实机。配套Bridge已部署；OTA事务3c5e3ac9已 succeeded/image_verified，单次上传。14:52:36认证运行 .78/ota_1，完整1,909,920字节哈希 `6F3CFF3C4C77375D6BF14E44F3B8A65909AD7E66DD7FF52EFA6654600F02006B`；后续同boot_id持续运行，HTTP正常、bridge_reachable=true、automatic_ble=false、BLE关闭。证据 `artifacts/note4-fix78-verify-20261011.json`、`boot-fix78-running.json`、`boot-fix78-ota-status.json`。实体PWR→BOOT初始化重叠及低功耗矩阵仍待复按验收；没有把正常OTA启动替代按键验收。

2026-10-11；只读调查，未改固件、未部署、未打开串口、未复位设备。现场运行 .76，.77 仍为候选。关联 A9/B6/C8、G2。

## 已确认的事件

从 Bridge 后续收到的持久诊断批次恢复到此前遗漏的记录；设备 MAC 为 7C4FADB93408，wake_generation 为 2048336034。逐记录校验外层 CRC32、内层有效标记及 FNV 校验通过，未凭解码出的相似文本关联设备。

| 唤醒序号 | 设备原因 | 设备记录 | Bridge 对照 |
|---|---|---|---|
| 7 | TIMER，cause=4，ext1=0 | 活跃 3,329 ms 后正常进入 sleep，BLE 已应答 | 13:31:04.646 认证 info 关联 seq7；13:31:05.224 接受 Plan3436，剩余 0s |
| 8 | EXT1，cause=3，ext1=2，即 GPIO18/PWR | 活跃 6,977 ms 后正常进入 sleep；BLE 广播但未连接；error=0 | 没有该轮认证 BLE 联系，故不能用 13:31 的 Plan3436 解释这一轮 |
| 9 | EXT1，cause=3，ext1=1，即 GPIO0/BOOT | 正常长时间保持醒着，最终记录 awake_ms=1,009,765 | 13:32:01.112 认证 info 关联 seq9；Plan3437 首次剩余295s，后续239s |

seq8 无墙钟字段，只能由同 generation 的连续序号确定在 seq7、seq9 之间，不能给出精确按键时间。seq9 最终约16.8分钟是整轮累计，不等于初始窗口设置成16.8分钟。

原始批次：`bridge/target/debug/data/platform/diagnostics/7C4FADB93408/493-371c4540548e8a7f2c91783fdfabb3d2.json`（seq7、8）、`495-3e425d9e4d66c3eb695fc55751856b09.json`（seq9）。Bridge 日志为 `bridge/target/debug/data/logs/bridge-app.log.2026-10-11`。提取结果 `artifacts/boot-five-second-wake-evidence.json`；复核脚本 `artifacts/decode-boot-five-second.py`，不输出凭据或诊断文本原文。

## 根因路径

1. `application.cpp` 对所有 EXT1（BOOT/PWR）设置 `v2Provisional=true`，声明300秒临时 Light。
2. 只有 EXT1 掩码含 GPIO0 才调用 `bleRecovery.open()`；GPIO18 没有独立恢复窗口。
3. `radio.cpp::v2RendezvousTick()` 收尾的 `stayAwake` 检查正式 Light、同步、BLE恢复及USB，遗漏 `v2Provisional`。PWR 这轮没有 Bridge 连接，所以没有正式 Plan；BLE等待结束后 `stayAwake=false`。
4. `enterDeep()` 同样没有 provisional 检查，直接走休眠全帧与 `sleepToNextEvent()`。因此尚有约293秒临时窗口也会主动深睡。seq8 的 `no_connection`、stage=sleep、6.977秒与此路径一致；不是证据中的 panic/WDT。

结论：已定位并由设备日志支持的实际缺陷是 **PWR 唤醒的临时 Light 窗口被交汇收尾绕过**。此前只看 Bridge 记录，把第一次短时入睡与13:31的零秒 Plan直接对应，证据不足；中间另有一次未连上Bridge的PWR唤醒。

用户补充：可能误碰 PWR，但确认 BOOT 按下。这不与 ext1=2 冲突：EXT1 掩码记录深睡退出来源，不是整个清醒期间的按键事件日志，不能据此断言没有按 BOOT。

代码存在配套 G2 缺口：初始化期间没有采集 BOOT，`tick()` 的 `bootArmed` 初始为 false，必须先读到 HIGH 才接纳 LOW。若 PWR 先唤醒、BOOT 在初始化中按下并松开，整个按键没有进入循环；若 BOOT 按住跨到第一次循环，初始 LOW 同样不被接纳，松开时 `bootDownAt` 仍为0，不触发短按。该逻辑原意避免深睡 BOOT 唤醒被重复识别，但没有区分“GPIO0已作为唤醒事件消费”和“由PWR/timer唤醒后新发生的GPIO0按键”。

因此与用户描述一致的完整路径是：PWR先唤醒 → 初始化期间按BOOT未被接纳 → 只有 provisional 而无 BOOT 恢复窗口 → 交汇超时收尾绕过 provisional → 约7秒深睡。前后两端已有设备记录支持，中间具体GPIO0按下/释放时间无记录，不能把这条完整时序标记为已实机证实。单独定时唤醒期间漏按也仍需覆盖。

## 候选及修复边界

.77 加入 BOOT 的独立300秒窗口，但 PWR 仍只设置 provisional，交汇收尾仍不检查它，故 **.77 尚未修复上述 PWR 路径**。后续修复应让各休眠入口服从同一个有效窗口判定，并验证 PWR 无Bridge、BOOT无Bridge、timer无人操作三组；不应仅延长 BLE 等待时间。另补 timer/初始化期间物理按键接纳与去重的专项回归。
