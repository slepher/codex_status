# Note4 与 1.54 蓝牙 Wi-Fi 初始化设计

日期：2026-09-30。状态：待实施。范围是首次无网络配网，以及用户主动重新配网；“初始密码”特指 **Wi-Fi 密码**，不新增设备管理密码。

## 现状与目标

固件无已存 Wi-Fi 槽位时进入 `startConfigMode()`：开固定密码 AP，屏幕显示 AP 名、密码与 `192.168.4.1`。BOOT 长按 15 秒也进入 AP。正常 BLE 已有按 MAC 命名的广播、120 秒配对窗口、加密且已绑定写入要求、模板控制特征的设备命令与 status ACK。Bridge 设备页已有按 MAC 的设备选择、BLE 发现、状态/连接展示，但没有配网向导。两款屏幕都必须在未安装 Bundle、未取得时间和 Bridge 状态时能显示初始化页。

## 用户流程

1. 首次启动且没有有效 Wi-Fi 槽位：设备进入 `BLE_SETUP`，显示静态 status bar 和 `SET UP WI-FI`、`Open Bridge > Devices`、`Connect Bluetooth`、设备名后缀/MAC 后六位。BLE 广播现有 `CodexStatus-<MAC后缀>` 名称。客户端扫描得到候选，连接后读取公开 INFO 中的设备 MAC/target/配网状态；广播名称只用于筛选，首次读数只用于候选识别，绑定后再次核对身份。用户按屏幕标识选中一台。已登记设备的重新配网从该 MAC 的「连接与占用」进入，屏幕由 BOOT 长按 15 秒打开配网窗口，不清除旧 Wi-Fi。
2. 如果电脑尚未与设备配对，客户端发起配对，操作系统负责交互；设备屏幕显示本次蓝牙配对码，用户在电脑上输入。校验只发生在建立新绑定时。**已有绑定直接视为可信，不要求解除配对或升级旧绑定。** 配网连接仍需已加密且已绑定，并且设备处于首次配网或物理按键打开的配网窗口。
3. 设备页在确认 MAC/型号后输入 SSID 和 Wi-Fi 密码，密码默认隐藏，可本地临时显示；提交前只做长度与必填校验。点击「连接并保存」才经现有 BLE 命令通道发送一次候选凭据。界面不把明文写入 Bridge `state.json`、日志、MCP 结果或 URL；切换设备/关闭向导即清空表单。
4. 设备收到候选后先在 RAM 中关联 Wi-Fi 并确认 DHCP/IP。屏幕依次显示 `Connecting Wi-Fi`、`Wi-Fi connected` 或简短失败原因；Bridge 接收不含密码的 BLE 进度和结果。连接成功后才以可断电恢复的提交顺序写入现有 Wi-Fi NVS 槽位（先写候选记录并校验，最后切换有效槽位；槽位已满时也须保住旧有效记录），回复成功并退出 `BLE_SETUP`，进入普通模式；Bridge 用认证通道核对同一 Wi-Fi MAC/target，再建立或更新设备登记。成功配网 **不发布 Bundle**；后续桥按既有空闲自动 claim 和正常数据/计划流程运行。
5. 密码错误、SSID 不可见、DHCP 失败或 BLE 中断时，不覆盖原凭据；保留配网页和可重试入口。Bridge 显示可理解的失败/超时，不把未收到最终 ACK 解释为成功。首次设备在电池上闲置 5 分钟后可休眠，按键重新打开；USB 供电可持续等待。串口 `wifi` 保留为本地恢复手段。

## 屏幕与 status bar

初始化页是 ROM 内置、离线可绘制的画面；不能要求先收到模板/Bundle。两 target 从各自现行 `quad` 与 `codex-status-a` 的 status bar **复用相同图标位图、尺寸、位置、字形和状态判定**，而非另造一套图标或把完整业务模板画在初始化页。页面正文使用固件已有 ASCII 字库；中文说明只在 Bridge。

| 状态 | 顶栏表达 | 正文 |
|---|---|---|
| 待配对 | BLE 图标可见；Wi-Fi OFF；Bridge 状态未知不冒充已连接；电量实时值；无可信时钟显示 `--:--` | `SET UP WI-FI` / `Open Bridge > Devices` / `Connect Bluetooth` / `CodexStatus-XXXXXX` |
| 配对中 | BLE 图标保持；屏幕显示 6 位配对码 | `Bluetooth code: 123456` / `Enter code on PC` |
| 连接 Wi-Fi | BLE 保持；Wi-Fi 格按当前目标的连接亮/灭节奏局刷；Bridge 格仍未知 | `Connecting Wi-Fi` / SSID 的安全截断显示 |
| 成功/失败 | 成功显示 Wi-Fi ON；失败显示稳定 OFF；Bridge 仍按真实认证联系更新 | `Wi-Fi connected` 或 `Check Wi-Fi password` 等不泄密原因 |

1.54 沿用 `quad` 顶部右侧图标/时钟区域；正文在其下方，电量可沿用 `quad` 的 `BATT` 表达并移入空白正文底部。Note4 沿用 `codex-status-a` 顶栏的日期/时间、BLE、Wi-Fi、Bridge、电池位置；未知日期/时间用占位，不显示假日期。两版正文根据 200×200 和 400×300 分别排版，保留长 SSID 截断余量。配网进入时全刷一次；仅状态/图标/短文案变化时局刷，局刷失败沿用既有整刷回退和显示结果诊断。不得在每次 BLE 广播时刷新墨水屏。

## BLE 与状态边界

通用链路顺序是 **广播 → 扫描发现 → 连接并读取 INFO → 未绑定时配对/绑定 → 加密 GATT 写入 Wi-Fi → 结果 ACK**。广播是单向发现信号，可以单独持续一段有界窗口供扫描，但不能凭广播本身完成配对或收取 SSID/密码。已绑定设备跳过配对步骤。设备侧只实现这一份 BLE/GATT 协议，不区分 Windows/macOS。

- 优先复用现有 service、模板控制命令特征及分片写入；增加明确的 `wifi_setup` 命令与不含 secret 的 ACK/阶段，避免新特征表造成 Windows GATT 缓存问题。命令须携带目标 MAC、会话 nonce 和候选 SSID/密码；设备只在本机配网窗口、目标 MAC 一致、**已加密且已绑定**时受理。固件当前 `NO_INPUT_OUTPUT` 只走无屏幕码配对；要让新绑定由设备显示码并在 Windows 输入，须改为显示能力并处理配对码回调，先在两款 ROM/Windows 11 实测。配对成功后，后续配网复用该绑定，不再逐次要求验证码。现有绑定同样直接可信。
- 配对/配网窗口与普通 4 秒/6 秒 BLE 会合分开；它不是业务数据、claim、owner 转移、模板发布或 PowerPlan。配网请求和 Wi-Fi 探测不创建/延长正式 light deadline，也不把 timer wake 算作 BOOT provisional。完成或超时关闭窗口；重新配置必须有物理按键，已绑定电脑不能静默改网。
- Wi-Fi 密码仅在设备 RAM 候选和现有 NVS Wi-Fi 槽位中出现；通过 BLE 加密链路传输，不写屏幕、串口、诊断流、Bridge 持久状态或测试夹具输出。验证仅回报 `connected`、IP 与分级错误。设备身份仍以自报 Wi-Fi MAC 为准；广播名称后缀、连接前 INFO、绑定后 INFO 与后续 Wi-Fi 认证回报必须指向同一设备，否则拒绝。
- 写 NVS 只在连接成功后执行，失败不改原槽位及 `last`；写入错误需回报失败并保留配网模式。成功 ACK 丢失时，Bridge 应按相同 MAC 的后续认证状态恢复结论，不能盲目重写。旧 Wi-Fi 仍有效的重新配网失败应继续正常运行旧网络。

## Bridge 设备页

在「当前屏幕」附近增加「添加/配置 Wi-Fi」入口。向导只在显式点击时启动 BLE 扫描；选择候选后锁定 MAC/target，依次显示「确认屏幕标识 → 点击配对并处理 Windows 系统弹窗（仅未绑定时）→ 输入 Wi-Fi → 连接结果」。已登记设备的入口带入选中 MAC，换设备立即取消原会话并清空密码。设备无法扫描、Windows 未配对、配对窗口关闭、连接超时、密码错误分别给出动作明确的提示。设备页被动刷新仍只读本地缓存，不扫描 BLE、不打开配网、不写凭据。成功后可选重命名，但不自动发布模板/Profile。设备广播本身不保证 Windows 弹窗；不依赖 Swift Pair 广告提示。

设备侧 BLE 配对方式与 GATT `wifi_setup` 命令遵循蓝牙标准，不绑定 Windows。Windows `PairAsync` 是当前 Bridge 的平台入口；若以后支持 macOS，客户端改用 Core Bluetooth 连接并访问要求加密的特征，由系统处理首次配对。Core Bluetooth 给外设的是本机 UUID，不能把它当项目的 Wi-Fi MAC；连接后仍从设备 INFO 读取并核对自报 MAC。Windows 与 Mac 各自保存绑定，换电脑需在该电脑首次配对。Swift Pair 自动发现通知是 Windows 专有扩展，不进入本方案的设备协议。当前仓库 Bridge 的 BLE/Tauri 运行与验收仍以 Windows 为准；macOS 是未来客户端适配，不在本次验收范围。

## 验收

- 双尺寸屏幕：无 Bundle、无时钟、低电量、BLE 断开、关联中和失败均可读；status bar 图标条件与目标现行模板逐状态对照，局刷失败能整刷恢复。
- 安全：未绑定或未加密、窗口外、错 MAC、过期 nonce、重复提交均拒绝；已有绑定无须重配。串口、设备日志、Bridge 日志、状态/MCP、磁盘均搜不到测试密码。现有 `/update`、`/doUpdate`、ArduinoOTA、`POST /claim` token 门槛原样通过。
- 可靠性：错误密码不覆盖旧网；成功关联/DHCP 后才持久化；断电分别注入在发送、关联、NVS 写入、ACK 前后，重启结果可判且可重试；两台 MAC 互不串扰。
- 真实设备：两 target 分别验证 Windows 配对码交互、Wi-Fi 入网、认证 MAC/target、设备登记、再次配网失败后的旧网恢复；ROM marker、大小、SHA256 与设备运行身份逐台记录。
