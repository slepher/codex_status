# C2协议部署与观察

2026-10-19用户授权：先提交R3–R5，再重编译/部署Windows Bridge，经受鉴权直接OTA窗口升级Note4，等待自然交汇并观察。

## Plan / Task

1. 提交已验证源码及交接，不发布1.54/AP01。
2. 核对真实运行Bridge路径、端口/任务、登记MAC及操作token可用性；核对候选版本/marker/大小/SHA256及匹配ELF。
3. 按watchdog→计划任务→已核实残留顺序停止；冻结exe/data并逐文件哈希。保留data原位，当前源码重编译Bridge；核对新exe后用受管启动脚本启动。
4. 读实时恢复视图核对Note4 MAC/target；受鉴权/update与/doUpdate直接OTA只发送一次。上传结果未知先查询运行镜像，不重发。
5. 新Bridge认证读取核对目标MAC、版本、boot、运行镜像长度/SHA256、context/合同及历史未知Job；等待自然交汇，观察至少15分钟并记录多轮Plan/HTTP/Data、复位与错误。

## Status / Review

源码提交9e748eb。部署及15分钟观察完成。用户已授权部署与观察；C2无旧协议回退，升级间隙失配属预期。认证/owner/原事务围栏不放宽，不改NVS/分区或发布Bundle。回滚需配套旧exe/冻结data；ROM回滚另外核对协议。最终状态更新PROGRESS及本文件，私有原文留artifacts。

## 已执行

- 冻结前核对Windows主进程36464/watchdog15164、计划任务及8765/8766；登记MAC与实时/status.json一致，/update携操作token返回200。
- 按顺序停止后，备份 `artifacts/protocol-convergence-backup-20261019-c2`：旧exe及264个data文件，逐一SHA一致。当前源码 `cargo build -p bridge-app` 成功；启动前data未变。新exe38357504 B，SHA256 `33595924e6d9791e4c9fef1405710b3ce58b9e154855dc400d5f37b807e291ed`；受管脚本启动PID17908。
- 20:23:46香港时间，经设备token鉴权的直接/doUpdate上传Note4候选一次，HTTP200 / UPDATE OK。证据 `artifacts/bridge-macos/protocol-convergence-ota-attempt.json`。未使用Bridge旧协议OTA、不改NVS/分区/Bundle。
- 20:25:05认证C2核验：MAC7C4FADB93408、0.18.105-note4、ota_1，完整1850016 B运行镜像SHA256 `5433769a050a7fe717f52d3d1b5c0c1f76e66a2ce12b5e4cce1880eee7059608`，与候选一致。reset_reason3为本次OTA软件重启。证据 `protocol-convergence-running-verified.json`。
- 20:24:13自然BLE交汇，Plan5693 ACK；network_start accepted，20:24:27认证http_ready，随后Data827 displayed。四次初始HTTP失败止于20:24:34，属于升级/建链恢复阶段；后续稳定情况按完整观察记录，不写零错误笼统结论。
- OTA冷重启触发现行unknown-retention保护（application.cpp）：context由9c3bd636edcd32e4转443f617ea34aa91a，commit802→803；committed_job_id仍cac235bb，没有发布/激活任务或Bundle请求。旧context数据不能重用，新context完整Data已显示。
- 从20:27:14开始每30s只读采样15分钟，日志 `protocol-convergence-observation.jsonl`；不延长PowerPlan、不人为开BLE/唤醒。期间同步比对冻结前Wi-Fi Job及认证缓存。

## 最终观察（20:27:14–20:42:14 香港时间）

31次/900秒只读采样完成；15次HTTP成功，Light到期后16次HTTP不可达，后者与后续Sleep计划一致，不作为连续HTTP成功。Plan5693原600秒期限递减至0，未被状态读取/重复确认续期。20:34:50尝试HTTP发送Plan5694超时；20:35:56自然BLE交汇确认Sleep Plan5695，之后5696/5697/5698/5699/5700均ACK并正常断链。

OTA后累计7次自然交汇完成/7个BLE Plan确认：20:24:13首次Light，20:35:56、20:37:03、20:38:11、20:39:19、20:40:26、20:41:32为Sleep；后六轮均判定delivery=idle、needs_http=false。没有人为BOOT/开网、强制claim或重新配网。

HTTP阶段新Data828已displayed；冻结前16条Wi-Fi Job逐字段不变，未新增Wi-Fi Job，Profile逐字段不变，committed_job_id仍cac235bb。BLE核对的wake_generation始终125178307；OTA后的日志BLE WARN=0、ERROR=0。HTTP阶段可用堆117004–119788 B，最大连续块69620 B；不把短样本当长期泄漏/全栈验收。末次BLE联系20:41:32，Bridge health204、运行exe哈希未变。

证据：`artifacts/bridge-macos/protocol-convergence-observation-summary.json`及逐样本jsonl。部署ROM/匹配ELF额外保存在 `artifacts/bridge-macos/note4-0.18.105-c2.{bin,elf}`；ELF31071844 B，SHA256 `beb72328bdebe26fd69e411ddc7e28c585d72abd0d1fc0913cd11dc1ee5d8afa`。不只依赖可重编的.pio缓存。

本轮Windows Bridge/Note4部署及短时自然交汇验收完成。1.54/AP01未部署；B6/C8长期RF/Flash掉电/原生BLE取消完整矩阵及C9继续开放。当前设备按Sleep运行，HTTP离线不代表新协议失效；后续活动再由正式Plan取得机会。本轮未恢复旧wire或更换事务ID重发旧写。
