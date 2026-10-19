# 0041 自动启动安全窗口待刷交付

2026-10-19。自动安全窗口及原生59–63模拟矩阵全部通过，严格放行后已生成完整待刷ROM。本轮未刷机、未访问AP01实机、未启停或部署生产Bridge。

## 当前候选

候选24，完整0041镜像6995352 B，SHA256 `a7d1962b5b7d3ef6f590857632f5d27ffbef5f8481039bf96b98cccdbe6b8194`。匹配ELF491388 B，SHA256 `6bd74440cc7660ca13215d16902921efe33f36b637b8bdd2ccf0129f914ca048`。完整镜像保留原厂0041启动头/入口、ROMFS、原厂页面和五个GIF，仅原UI注册hook及追加的自有XIP模块不同；不是单独模块或分区dump。

## 安全窗口

独立维护HTTP18080持有APS1稳定确认及APF1升级/切换事务；业务HTTP18081、Codex数据/owner/Bundle不参与健康判定。非稳定槽启动时完整验证已确认稳定镜像，持久记录试启动，再预置稳定槽；原生UI心跳和维护监听连续健康9秒后锁回当前工作槽。30秒未健康保持稳定槽，显式救援取消自动锁回；结果未知不重复提交。稳定身份不会被短暂健康自动替换。无已确认稳定记录报告unconfigured。

正常启动：稳定0 → 完整OTA到工作1 → 工作1启动预置稳定0 → 连续健康9秒 → 锁回工作1。窗口内中断：工作1预置稳定0 → 截取原NOR并停止 → 同一NOR从稳定0启动 → 绑定/Bundle/激活/业务恢复，原升级事务返回rejected/boot_trial_pending。详细协议和资源预算见[稳定槽设计](ota-stable-slots.md)。

## 模拟器与证据要求

旧引擎`a549e66f…da18`仅允许一张固定分区表写入，原生55第二次ff13被模型拒绝返回7，固件停在unknown，未用于健康放行。隔离复制冻结QEMU源，变体只将OTA写窗口扩为两张独立4KiB分区表；活动固件槽、Boot2及其他保护区仍拒写。没有替换ff13返回值、未改原厂切槽指令；双表age/CRC与完整镜像字节由实际NOR快照核验。冻结旁仓及旧引擎保持不变。

新引擎SHA256 `b46de0508e0f4be6c4ef5ded168360c7bf4bc5e23bf43e44e9f244a79d33b0ce`。`boot-safety-qemu-model.json`记录源header/构建器/测试/引擎指纹；三种Flash profile（OTA关闭、槽0/槽1运行）通过两张表写入、活动槽与保护区拒写、跨区拒写、原子快照及复位保持检查。宿主ASan/UBSan完整集成和八项安全窗口故障场景通过。

原生59–63验证顺序：59首次绑定/八模板/四画面/完整OTA；60同NOR工作1预置/健康锁回/手动回切；61同NOR稳定0恢复；62从59同一升级后NOR分叉，在首次成功ff13之后立即中断；63原样从62的预置快照回到稳定0并恢复业务。正常健康时长必须按guest时钟至少9000ms且小于30000ms；预置和锁回交替写表并保留上一份有效稳定表，手动切槽重放不再次提交。

`accept-ap01-rom.py`核对当前生产源/头文件、原厂镜像重建、ELF/引擎/runner/宿主测试指纹、双表CRC/age/选槽、同一NOR和四画面零像素差异。证据不全时不放行。

已核对的正常路径：原生60在guest 2507ms预置稳定0，11510ms锁回工作1，间隔**9003ms**；表由E000 age3/slot0 → F000 age4/slot1，再显式回切E000 age5/slot0。三次ff13均返回0，回切精确重放没有第四次提交。原生61同NOR稳定槽恢复通过。原生62在guest 2511ms首次提交后中断，只有一次ff13；原生63输入与62预置快照字节一致，SHA256 `76d6cb5d2233b418ca81d62b2248dbc2fa2874423b5c4d3f16516c0ab07066b9`。63稳定0恢复全部通过，原升级事务为`rejected/boot_trial_pending`、业务在线，绑定/Bundle/激活和八模板双向切页恢复，ff13调用数为0。59/60/61/63各四画面，16组活动LVGL缓冲均零像素差异。

## 最终产物

目录`artifacts/ap01-platform-integration/device/`：正式ROM `ap01-0041-codex-status-boot-safety-20261019.bin`，6995352 B，SHA256如上；同名前缀manifest、acceptance和sha256。放行命令：`python -B tools/accept-ap01-rom.py --candidate 24 --fresh 59 --update 59 --switch 60 --recovery 61 --trial-cut 62 --fallback 63 --name ap01-0041-codex-status-boot-safety-20261019.bin`。验收记录41份证据指纹，`flashable=true / simulator_verified=true / boot_safety_window_verified=true / stable_slot_switch_verified=true / hardware_verified=false`。

完整包同前缀`.zip`包含ROM/匹配ELF/map/audit/link/payload、放行清单、宿主及原生报告/像素报告、示例截图和协议说明；不包含真实凭据、设备dump或生产运行数据。旧候选22及其手动切槽交付保留，但新待刷选择为本包。Bridge本轮无修改或部署；沿用既有AP01接入能力。

## 边界

安全保护从维护服务**成功预置稳定槽之后**开始，不覆盖更早的Boot2或初始化失败。QEMU中断代表模型内复位分支，不等同物理断电电气验收；真实首启、Flash掉电时序、LCD/SPI、存储free及栈/RAM高水位仍未知。原生矩阵两槽使用相同候选24，未冒充0031跨升或旧版本实机回退验收。传统厂商OTA保留，仍按人工顺序使用两通道，其旁路不受本服务写保护约束。AP01保持常在线、无PowerPlan；字体仍随用户显式发布的完整Bundle交付。
