# 2026-10-12 进度归档

## 11:46 用户手动Wi-Fi直接调查

认证MCP及登记IP恢复视图核对.83/MAC，直接保存/log、/history、/pmstats。wake675实际6.6s取得IP、7.7s HTTP，累计light_sleep137296ms；当前诊断代际与旧batch553相同，最早1962、history654–675，未因手动联网清空。明确更正：10:10取不到是HTTP/归档通道与记录缺失，不能由10:28 gap倒推即时已覆盖。新增BLE初始化过早空INFO伪错误证据（现场11条/902B），后续C8修初始化顺序。旧network_start发送阻断解释已有复现，用户约一分钟联网具体路径仍无唯一历史证据；未改计划/配置、未冻结Flash/复位/部署。见[完整调查](../../project-workflow/bridge-macos/wake-network-log-investigation-20261012.md)。

## 11:26 10:09网络启动与日志留存调查

用户要求调查退出zzz后联网延迟和日志缺失。恢复batch553明确报告当前代际404–1802共1399条overwritten；CRC验证的wake632摘要仍在，但没有IP/HTTP阶段时间。生产4KiB RTC环允许未确认记录覆盖，Flash冻结及Bridge归档依赖HTTP begin，因此网络故障现场在冻结前不保证保住。生产客户端差分回归复现Plan延长RF后旧INFO截止阻止network_start、同MAC/wake刷新后成功；既有四边界RF回归一起通过（2项测试）。旧发送前到期/超时缺失败日志，任意Err被视unknown并等待HTTP；30s等待+完成后55s冷却吻合两次联系间隔，不能倒推约一分钟关联/DHCP具体原因。完整证据与剩余B6/C8在[调查](../../project-workflow/bridge-macos/wake-network-log-investigation-20261012.md)。本次仅新增离线回归和记录，未改生产行为/部署/ROM/设备配置。

## 09:54 macOS Bridge 构建并启动

用户明确要求现在构建并启动。当前源码 `e468b3a`，本机此前运行 2026-10-05 exe；使用固定 ArduinoJson 7.0.4、既有隔离 target 构建 release（9m05s）。debug脚本在系统Bash3.2下空数组报错，改用安装指南推荐release而未修改源码。

默认App更新至 `artifacts/bridge-macos/Codex Status Bridge.app`，x86_64/20,771,296 B，SHA256 `274ef25eec0761422bde9ed91662ecb7947fa8f6a24118973964283402bb101b`，签名及候选/部署哈希一致。默认supervisor77530/child77535、health204、MCP200/43工具、bridge_status无错误；实际路径和端口归属复核通过。data/installation-id私有备份及旧App回滚包已保留，安装身份一致。

备份位置、完整计划/核验及边界见[专项记录](../../project-workflow/bridge-macos/build-start-20261012.md)。证据为 `artifacts/bridge-macos/build-start-20261012-{build.log,deployment.json}`。本轮仅App构建部署与启动冒烟核验，未做固件构建/刷写、模板发布或提交，不新增设备/UI/中文实机/长期验收结论。

## 10:44 Wi-Fi读取修复最终Bridge部署

Plan/SyncOpen改变设备RF阶段后，Bridge沿用初始INFO截止，使合法读取提前超时；首轮实机又暴露SyncOpen使用旧截止。最终提前/尾段刷新真实同MAC/wake INFO，整帧丢ACK只允许原只读工作，未知发送关闭链路。生产四边界回归及App85/BLE17通过、3条件忽略。最终默认release SHA256 `d1fd0fda57a813cd95870fa5c889138b5be8eec3b612fe3bf6c8685c87cd58b1`、20,804,352 B，supervisor1655/child1662，health204/MCP43工具，签名、私有数据备份及安装身份核验通过。

原读取被取消，另一个完整读取及后续写入在最终部署前已完成到r237，不能作为最终BLE修复实机验收；最新完整缓存失效，主动HTTP读取超时。新签名蓝牙/定位待用户重新系统授权；尚无最终BLE读取成功证据。未提交/刷ROM/发布模板。详见[专项与完整证据](../../project-workflow/bridge-macos/wifi-read-rf-20261012-plan.md)。

## 10:51 最终Bridge读取与网络实机核验通过

用户恢复系统权限后MCP双allowed。最终App BLE Plan3663/SyncOpen/15页snapshot全部ACK，新只读e0c0b00d10f96924完整r237成功；后台自动activate到r238后，10:51认证HTTP `.31.90` 正常，HTTP只读4fe306d4227b0a3f一次完整r238/无错误，active office/Xiaomi_1EBB。本次读取故障验收完成，其他B6/C8长期边界保留。脱敏证据 artifacts/bridge-macos/wifi-read-20261012-final-{mcp-safe.json,http-safe.json}，详情见[专项](../../project-workflow/bridge-macos/wifi-read-rf-20261012-plan.md)。未显式提交Wi-Fi配置、未刷ROM/发布Bundle/提交代码。

## 11:08 统一权限按钮与首次启动请求部署

用户要求合并定位/蓝牙入口并首次启动弹出授权。UI共用permission_all，startup仅自动请求未决定权限；原生定位delegate续请求蓝牙，长时间系统选择不因UI轮询耗尽丢失第二项。拒绝/禁用提供统一隐私设置入口；已决定状态不自动重弹。Node/App85 passed（3条件忽略）/WK52通过、窄宽截图审阅通过。两处既有WK旧文案断言更新为当前产品语义，不修改业务逻辑。

release 3m18s，20,804,352 B，SHA256 `53a5fab4b26ea31cee2ac283f6cf4308d761b8d2ef6a066d1f30b36f2c0a9b63`，supervisor11717/child11724、health204/MCP43工具、签名/私有data备份/安装身份通过。11:08:42被动MCP双allowed、Xiaomi_1EBB/connected；未主动reset权限，未决系统两弹窗实际次序仍无单独人为重现证据。未提交/刷ROM/发布Bundle/改钥匙串。详细证据与边界见[专项](../../project-workflow/bridge-macos/permissions-unified-20261012.md)。
## 14:33 超时分类与关键日志修复候选

用户要求修复已确认缺口，分别报告未发送/结果未知/找不到网络/预算耗尽并补日志，不延长超时。实现已完成：共享载体错误保留发送阶段和有限原因、未发立即结束、分片写入未知禁止复用、整帧丢ACK可只读确认、持久事务保留原ID及查询围栏；AP原因不被普通断开覆盖；记录启动/关联/IP/监听/ready ACK/终止原因，区分同步总预算/无进展，提前安装INFO provider避免伪错误。详见[专项](../../project-workflow/bridge-macos/timeout-reporting-fix-20261012.md)。

Rust268通过/3条件忽略，Node、WK60（560/1000截图审阅）、生产事件回归及Note4.85/64栈门禁通过。旧宿主管理active_protected断言在HEAD原实现也失败，不把全套记通过。ROM1,851,104 B/SHA8c4126cf…ba8fecad；独立Bridge候选20,817,136 B/SHA58b4a78d…226cdb06b，签名通过。完整哈希、匹配ELF/map和成本在专项及PROGRESS。未替换运行App、未刷ROM/改网络配置/提交；实体矩阵和离线留存继续开放。


## 15:59 Bridge建链失败清理修复部署与恢复核验

用户授权先修复Bridge后重启。生产两次connect失败+永不返回的disconnect回归先失败，统一全部裸清理为原2s预算后约2.13s返回原错误、释放锁并执行下一连接。最终Rust269/3条件忽略、Node/WK60/窄宽审阅通过；首轮并发120ms旧恢复用例时序失败保留，单独及串行通过，未改生产超时。

15:51停原默认supervisor/child并私有备份data/安装身份/plist/旧App；release3m19s，20,850,560B，SHA256 fc792b2b2016181550b7629b71846c66eba0aa8f69acd0378bbc1f85ef62acf7，15:55:56部署原路径重启，supervisor15210/child15214、签名/身份/health204/MCP43通过、双权限allowed。15:56首轮Plan写入未知但清理返回扫描继续；15:58 wake908自然会合Plan3726 ACK、整轮complete，15:59认证HTTP同MAC/0.18.83正常。Bridge分类部分随本次部署，固件0.18.85仍候选，未刷ROM/发布模板/提交。总体P2和离线日志不结项。见[修复与证据](../../project-workflow/bridge-macos/ble-connect-cleanup-fix-20261012.md)。
