# BOOT 与联网生命周期实施

## Plan：最终用户合同

本条覆盖早先“Light 断网也不开 BLE”“所有 BOOT 不延长窗口”及直接关闭 BLE 无线电的描述。

- 首次物理 BOOT 从 Deep 唤醒：建立 300 秒 Light 和 120 秒蓝牙开放窗口，通过 BLE 向 Bridge 获取设备已保存网络的激活选择；选择不携密码。无有效选择则尝试上次成功连接的配置。
- 清醒时短按交替切换蓝牙开放配置。关闭只撤销常开要求，不强行终止协议必需 BLE；不续 Light。再次打开从本次按键重新建立 300 秒 Light、120 秒 BLE，保持当前 Wi-Fi 激活配置，不重新选网。普通通信/重复消息不得延期。
- Wi-Fi 与 Bridge HTTP 通信正常：设备主动 HTTP 联系，不进行自动 BLE 交汇。用户开放 BLE 时仅供 Bridge 主动 GATT 操作，不据此调度普通交汇。USB 不自动常开 BLE。
- 无法关联/取得 IP 与已联网但 Bridge HTTP 不通分别记原因，均进入联网失败恢复。失败后以正常有界 BLE 机会汇报，再结束本轮并等下一周期；不递归增加特殊重试。首次尚未联系 Bridge 失败沿原周期。
- 已有鉴权、owner、原事务查询、有限预算、低电保护保留。新选择不携密码；显式配置管理仍走原授权事务，不借 BOOT 自动覆盖凭据。

## Task

1. 统一权威场景/电源文档，修正旧审核结论。
2. BOOT 物理窗口独立于正式 Plan，开/关/再开正确延期，状态可报告；关闭开放配置不终止必要交汇。
3. Bridge 区分普通交汇与仅供主动 GATT 的开放；人工联网准入，选择既存网络；设备无选择时回退已成功活动配置。
4. 有界 HTTP 主动联系及失联分类；正常联网禁自动 BLE，失败正常周期恢复。
5. 宿主生产状态机/Bridge 回归，Note4 构建及匹配 ELF 栈门禁；候选与现场部署分开记录。

## Status

后续已完成配套Bridge部署及Note4 .78烧录，运行镜像已认证；.77作为历史候选保留。最终部署与短时唤醒修复见[修复记录](boot-five-second-investigation-20261011.md)及 PROGRESS；未提交。

- 人工 Light 截止时间与 BLE 开放偏好分开；关闭开放不取消 Light，不打断必要交汇。普通通信和 USB 不续 BLE；实际关闭 BLE 后恢复 Wi-Fi modem 省电。
- Bridge 人工窗口无待办也准入 HTTP，仅选择设备已有配置；设备回退上次成功活动配置。健康 HTTP 缓存 45 秒内不调度自动 BLE，过期重新发现。
- 设备每 30 秒有界 POST presence，单次连接/响应分别 750 ms；响应必须含匹配 bridge_id 和 device_http_verified=true，连续三次失败进入恢复。无联系端点直接进入失败；关联/DHCP 回退单次最多 15 秒。失败在有界 BLE 状态报告，报告轮不重新开网，随后恢复常规周期。
- Rust app/core/render/ble 回归 **311 通过、3 条件忽略**（`artifacts/boot-network-regression.log`）；原生 BOOT 窗口测试通过。生产编解码覆盖人工无待办、健康 GATT 不交汇、失败报告不重开 HTTP 三种准入。
- Note4 候选 **0.18.77-note4**：`artifacts/note4-0.18.77-boot-network.bin`，1,905,872 B，SHA256 `DE09BF5A50EADFD1417F04494B30E7D31F2D78B788C2E05EC5E64D371CC7B34A`，同名 ELF；版本与 `zectrix-note4-400x300` marker 通过。构建与匹配 ELF 栈门禁通过（`artifacts/boot-network-077-final-build.log`、`boot-network-077-stack.log`）。配套 Bridge 在隔离 `bridge/target-note4-sync` 编译成功。
- 资源：最终静态 RAM 117,972 B、Flash 1,854,383 B；新增人工截止时间与有界联系状态，不新增凭据副本或 NVS 格式。空/单条/典型/上限配置持久字节增量均为 0；动态 HTTP 峰值及整条任务栈尚待实机测量，局部 ELF 栈门禁不替代该验收。

## Review

后续[短时入睡调查](boot-five-second-investigation-20261011.md)恢复设备持久记录：PWR 唤醒在6.977秒后正常深睡，交汇收尾遗漏 provisional；用户确认也按下BOOT，初始化采样/bootArmed存在漏按路径，具体按键时序未记录。这两处已在后续 .78 修复并烧录；实体按键重叠仍待复按验收。 真实首按/关/再开、断网和 Bridge 失联、屏幕模板、低功耗及长时间自然交汇未实机验收，不据此结项 A9/B6/C8。

必须覆盖首按/关/再开、正式 Plan 后延期、timer 不授人工窗、无待办人工联网、无映射回退、网络失败与 HTTP 失败、GATT 常开不触发交汇、USB 不重开 BLE。新持久格式优先避免；任何新增 RAM、栈或 NVS 成本记录实测依据。软件与构建通过不替代 RF/屏幕/低功耗实机验收。
