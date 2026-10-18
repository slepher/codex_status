# 0041：包含独立 OTA 稳定槽功能的待刷 ROM

2026-10-19。本包在此前完整0041模板/发现/配对/Bundle/原厂UI基础上加入稳定槽确认、升级保护和无需上传镜像的直接回切。替代此前候选16的待刷选择；保留其产物及证据，不覆盖历史ROM。本轮仍不刷机、不部署或启动生产Bridge。

冻结候选22：6,993,160 B，SHA256 `63eb98e51c9cb1e0b2783d63b51c50da0dadb3db95b1ad107fcbb0fbb970df94`。匹配ELF 488764 B，SHA256 `d689fbfed009977496ae3631502b7c57a6fbc8fb3c9bab14e32c78c928236a32`。正式ROM命名`ap01-0041-codex-status-ota-slots-20261019.bin`；只有放行脚本核对本镜像的最终证据后才生成。

## 维护接口

接口只在独立OTA维护HTTP **18080**，使用设备绑定token认证；不依赖业务HTTP18081的owner、context、Profile或Bundle。业务从未初始化/未上线的情况有独立测试。

- `GET /api/firmware/slots`：读取运行槽、稳定槽、稳定修订、镜像CRC/长度/build。
- `POST /api/firmware/stable`：显式确认当前运行槽为稳定槽。请求包含`device_mac`、`request_id`、`slot`、`expected_stable_revision`和`confirm:true`。
- `POST /api/firmware/switch`：校验原稳定槽后直接提交启动槽，不下载或重写镜像。请求包含`device_mac`、`request_id`、`target_slot`、`expected_revision`、`expected_stable_revision`和`expected_image_crc`。
- `GET /api/firmware/<request_id>`：读取原事务结果；同ID精确重试不再次切槽。
- `POST /api/firmware/reboot`：带同一MAC/request_id，响应完成且socket关闭后重启。返回`boot_confirmed:true`需要实际从目标槽重新运行，不能用提交成功替代。

常规升级仍使用begin/chunk/commit。开始升级前必须确认当前稳定槽；当非活动槽是已确认稳定槽时，服务拒绝覆盖。用户可先回到旧稳定槽，或者在验证当前版本后明确确认它为新的稳定槽，再继续升级。该确认是运维决策，不是启动几秒后自动宣称稳定。

槽表使用0041原厂`boardctl(0xff13,显式路径)`提交；两份固件镜像全程不改字节。镜像从槽设备流式完整校验，读取启动头、型号、长度、尾部及恢复CRC；运行身份只与已映射XIP启动头及应用manifest核对，避免读取原厂映射空隙。稳定记录APS1独立原子持久化，切槽复用APF1持久事务。损坏、身份变化、过期修订、错误token/MAC及unknown均有明确反馈和重放围栏。

## 验收及边界

最终原生使用49（恢复/八模板/四画面、模板有焦点时确认稳定槽及全镜像升级）、50（新槽启动后从有焦点模板切回稳定槽）、51（原稳定槽重启及原回执恢复）。逐像素报告、两槽完整字节对比、保护区和单次提交次数必须通过后，由accept-ap01-rom.py生成正式ROM及manifest/acceptance/SHA256。候选18–21和中途调查报告均不用于最终放行。候选22修正了OTA期间根UI将绘制事件误作输入、隐藏控件导致原厂LVGL断言的问题。

**01:36最终结果：49/50/51全部通过，正式ROM已放行生成。** 两槽及保护区完整字节对比不变，原厂ff13回切返回0，重复请求只提交一次；从同NOR稳定槽重启后原switch事务`boot_confirmed=true`，绑定/Bundle/激活及四画面恢复。三轮四场景活动LVGL缓冲均零像素差异；宿主ASan/UBSan通过。正式manifest的simulator_verified和stable_slot_switch_verified均为true，hardware_verified=false。完整包包括ROM、校验、匹配ELF/map/audit/link/payload、acceptance及维护协议说明。

宿主ASan/UBSan覆盖业务未启动时认证/稳定确认/回切、目标损坏或变化、稳定槽覆盖拒绝、精确重放、unknown提交后的恢复和损坏稳定文件拒绝。资源详见[稳定槽设计](ota-stable-slots.md)；原生镜像校验栈为有界块缓冲，不整包进入RAM。

实机首启、物理LCD/SPI、真实free/高水位、Boot2电气/掉电窗口仍未验；QEMU使用已列明的合成MAC/socket/输入/fbdev/Boot2等模型。传统OTA保留且按既定人工顺序使用两通道；本服务的稳定槽保护不覆盖传统OTA旁路。未实现自动故障回滚或硬件只读槽。旧0031接收器直接跨升0041仍未验证。

AP01继续常在线、无PowerPlan；彩色圆环、缺5h自动重排、RC独立保留、全部原厂页面及五GIF保持此前行为。中文字体随用户显式发布的完整Bundle交付，不内置ROM。
