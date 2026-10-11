# BOOT 关闭 BLE 后 Wi-Fi blink：Fake ROM 测试计划

## Plan / 验收目的

用户明确：浅睡、Wi-Fi 已连接时，按 BOOT 关闭手动 BLE 本身正常，随后 Wi-Fi 图标闪烁异常。BOOT 保持窗口内 Bridge 不可达应保留 Wi-Fi ON、显示 Bridge OFF。先调查，不修改网络行为、不刷 ROM。另已授权将绿灯改成收发数据指示，实施尚未开始，必须与当前 ROM 基线区分。

使用隔离 Fake ROM 执行已支持场景和既有回归；主 agent 同时审查真实固件/模板，列明模拟器缺失的状态与无法排除的边界。不能以模拟测试通过声称排除所有实机 RF、EPD 或时序问题。

## 隔离与执行

- 保持真实 Bridge PID75952/watchdog44608 与8765/8766运行；不调用实机、真实MCP、生产Wi-Fi修改或发布。
- Fake MAC `02:00:00:00:83:11`，监听仅127.0.0.1:18783（占用则选空闲高端口并记录），合成独立 endpoint/device/control token，不输出明文，不复制生产data。
- 目录 `artifacts/fake-boot-blink-20261011/sim-data`，已有 `bridge/target-note4-deploy` 用作编译缓存；所有新目录及可能创建目录的构建使用提权命令。
- 在bridge运行 `cargo build -p device-sim --target-dir target-note4-deploy`；`cargo test -p device-sim --test bootstrap --target-dir target-note4-deploy -- --test-threads=1`。
- 后台Fake ROM通过隐藏窗口启动，stdout/stderr重定向，立即返回；保留供用户使用。记录PID、URL、MAC、exe SHA256、启动参数（隐藏tokens）。测试使用控制API/真实HTTP和Fake BLE通路；不伪造生产固件未具备的行为。

## Tasks / 边界矩阵

| ID | 场景 | 预期与必须检查的证据 |
|---|---|---|
| B01 | Light、已连Wi-Fi，手动BLE ON→OFF | Wi-Fi/HTTP保持；无新activate/关联；手动与实际BLE分别记录 |
| B02 | 手动关闭恰逢自动BLE未完成/ACK收尾 | 自动事务有界完成；Wi-Fi不因BLE状态隐藏；若Fake不模拟则标未覆盖 |
| B03 | 关闭发生在已保存网络的延迟activate前后 | 同SSID有IP复用；无IP/真断网才进入connecting；不把连接图标问题当RF证据 |
| B04 | Wi-Fi connected、Bridge HTTP不可达/超时 | 分离Wi-Fi与Bridge状态；与真实networkFailed路径对照，差异单独报告 |
| B05 | BLE120s自然到期、BOOT300s期限、正式Light到期 | 各期限独立；OFF不续期；timer无BOOT授权；临界前/后采样 |
| B06 | 正式Light即将到期 + Data临时窗口 | 正式到期不截传输；同open重放不续期；完成/90s截止结束 |
| B07 | 快速连续短按、去抖/排队、主循环延迟 | 最终开关状态正确；不凭屏幕单帧判漏键；Fake不含GPIO队列则明确 |
| B08 | Wi-Fi断链、DHCP延迟、连接失败与恢复 | connecting仅真实过程；错误阶段明确；恢复不重复activate |
| B09 | ACK丢失、session变化、重复请求/请求冲突 | 查询原事务、不重复副作用；正确拒绝/unknown；剩余预算不重置 |
| B10 | 图标隐藏/连接blink/EPD部分失败全刷 | 区分三个机制；实际模板when条件由主agent审查；Fake无EPD BUSY则未覆盖 |
| B11 | LED当前基线与新需求 | 当前ROM以BLE射频为灯态；模拟器没有GPIO3不能证明新传输闪烁，暂不报通过 |

每行记录：已运行的测试名/API步骤、初末状态、断言、通过/失败/不支持、证据路径。失败不得为通过而改测试或生产源；新发现缺口交主agent判断。

## Status

Luna已执行，最终有效bootstrap回归64项通过、0失败，54.14s。初次运行受目录ACL阻断、Cargo重跑受正在运行的exe文件锁阻断；提权直接运行匹配测试exe完成，环境失败不算行为失败。详见`artifacts/fake-boot-blink-20261011/results.md`及`bootstrap-direct-escalated.log`。Fake保留运行于127.0.0.1:18783、PID34328；21:06复核真实Bridge PID75952和watchdog44608仍运行，8765/8766监听正常。

B03/B05/B06/B08/B09有模型回归；B01/B04/B10仅部分覆盖，B02/B07/B11缺失实体路径。模拟器button是模板循环，不能替代BOOT；不能据此排除所有边缘场景。主agent代码审查见`boot-blink-code-review-20261011.md`，绿灯独立候选见`data-led-20261011.md`。

之前只读实机对照已捕获20:42:59 BLE ON→20:43:00 OFF，Wi-Fi保持connected、wifi_conn=false、blink_ticks0，用户反馈未复现。证据 `artifacts/note4-83-boot-observation-summary.json`。旧现场约20:21:09开启手动窗口可由300s期限反推，关闭/图标逐次事件不在网络日志过滤保留范围，不能精确重建。

## Review

主agent负责真实源码审查、假设与证据区分、结果验收。当前已知候选：模板在manual=false但BLE仍on时隐藏Wi-Fi图标；Bridge不可达分支主动disconnect且忽略BOOT保持期；Bridge图标仅按联系新鲜度。候选不能直接当成当时blink已证根因。
