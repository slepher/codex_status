# Codex Status 当前交接

更新时间：2026-10-19 14:48 香港时间

## 部署与工作区

- macOS 托管Bridge **14:38已部署模板页刷新修复**：exe80,984,128 B，SHA256 `6ea8904e315257784ba511034b8b90cc484298a0150c28cdc494ff073e8395b2`，health204。托管停止后冻结App/data备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/template-profile-refresh-20261019-143741`，159文件哈希一致、签名/部署包一致、启动前data未变；v7合同与族配置重启后仍在。原事务终点修复已包含。本轮未刷机。
- Note4 `7C4FADB93408` 未刷机/复位，新认证运行 `.104-note4 / ota_0`，boot_id未变。10:37自然BLE激活→network_start accepted→认证HTTP200，10:37:48原ID003f确认为completed/operation_result，完整快照revision341、active=office/v235，SSID/hostSSID均Xiaomi_1EBB，IP192.168.31.90/HTTP ready。
- 本轮修复、回归与部署记录随本次提交保存。Windows Bridge及AP01实机本轮未访问，不沿用旧exe哈希冒充当前macOS部署。
- **Note4 模板 v7 已部署（14:27）**：从历史v5恢复仅WEEK通栏，叠加v6 MONTH，5h原双栏优先；WEEK/MONTH/双栏与各历史布局逐像素一致。运行Bridge保存source_crc `6d54e167`，发布job `cac235bb` succeeded、Bundle begin/chunk/commit全200；认证新context `9c3bd636edcd32e4`、commit_seq802、data_seq=applied_seq802、displayed。真实prolite源weekly剩余98%；屏幕实物未目视。模板页族配置已补齐 `codex-status / Note4 Codex 额度`，引用同一v7并保留全部bindings/sync。AP01 v3仅项目候选。[工作流](project-workflow/free-month-metering/status.md)

## 本轮修复与验证

- weekly-only回归旧v6先红、新v7绿；compiled12/AP01 3/template8/core note4 1通过，JSON/compiled往返及历史PNG对照一致，git diff --check通过。项目模板/测试/文档已改，**本轮未提交git**，模板部署无需重编；随后因用户两次截图仍空，已修复菜单旧缓存及MCP族配置通知并重建/重启Bridge。Node先红后绿、WK70窄宽截图已审、app114通过/3条件忽略。发布与族配置真实接口读回成功；**用户已确认实际窗口显示「Note4 Codex 额度」及codex-status-a模板，UI验收完成**。[根因](project-workflow/free-month-metering/investigation-prolite-20261019.md)。

## B6/C8：Wi-Fi事务执行终点

- 用户纠正根因：执行终结与设备结果未知混淆，历史取消/过期记录永久阻塞其他事务。共享层改为终态释放准入；缺失/变化身份及三次原ID确认查询耗尽结束执行，结果证据仍未知，不删记录/补造成功/重发旧写。独立持久计数默认0不写旧条目；新事务仍认证/owner/版本/修订校验，缓存准入错误每次匹配重算。实机首版定位pending误拦network_start导致window_closed，二版已修复并部署。
- 先红→绿：Rust333通过/5条件忽略，另显式device-sim保护用例通过（默认忽略中的1项），Node通过/WK68及窄宽截图已审阅；补齐模拟器只读wifi_owner与固件一致。生产ble_cycle异步激活后network_start/原ID恢复查询、跨网络、跨重启预算、迟到结果/取消不复活、活动未知冲突保护覆盖。
- **本次修复与自然恢复验收完成：三条旧未知事务逐字段及SHA保持不变；全部新激活均有明确结果，最终003f完成。屏幕图标未直接目视，不冒充显示验收。** [计划/审查/证据](project-workflow/bridge-macos/wifi-transaction-finality-20261019.md)；原始证据在artifacts/bridge-macos/wifi-finality-*。

## C9：AP01自动启动安全窗口继续暂存

- c11e860保存当前未完成验收成果；启动稳定槽/9秒健康确认/30秒超时、双表交替及旧ROM兼容交叉编译通过，候选23未验收/未发布。下一步从54干净配置重跑，不宣称自动窗口已通过。[精确续作](project-workflow/ap01-template-integration/boot-safety-handoff.md)。
- 待刷候选22 `ap01-0041-codex-status-ota-slots-20261019.bin`，6993160 B，SHA256 `63eb98e51c9cb1e0b2783d63b51c50da0dadb3db95b1ad107fcbb0fbb970df94`；仅此前手动切槽模拟证明，物理Boot2/free/LCD仍未知。[交付边界](project-workflow/ap01-template-integration/release-0041-ota-slots.md)。本轮不部署AP01。

## 风险与边界

- 未修改固件/NVS/配网凭据，未按BOOT/连接USB。新非零确认计数写入后降级旧Bridge须配套冻结data备份；历史未知事务不清理。长期RF矩阵及其他硬件验收仍开放。
- 本轮模板/测试/文档尚未提交；AP01 v3候选未部署；Note4 v7仅协议ACK和宿主目视通过，实物屏幕未目视。
- [前卡全文与旧候选/哈希](docs/history/progress-archive-2026-10-19-before-wifi-finality.md)；待办唯一来源[backlog](docs/roadmap/backlog.md)。
