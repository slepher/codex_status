# AP01 Bundle刷新（2026-10-20）

## Plan

用户授权先刷新Bundle，不更新ROM。沿用当前固件I4整帧渲染，发布首栏订阅/账号/时间与底部SYNC/RC合栏布局；完整Montserrat14通过CSFN资产进入Bundle，中文整段独立字体，不切换LVGL原生直绘。

## Task

- 认证登记MAC34F01557210B及当前固件/合同；备份目标Profile、模板和字体选择。
- 生成兼容候选及字体，检查0/单周/单月/双窗口/100/缺RC、缺账号与长账号。
- 使用Bridge共享服务保存明确目标Profile并发布单份完整Bundle；未知结果只查原作业。
- 认证核对安装版本/Bundle/显示回执及ROM保持，更新交接。

## Status

后续用户已授权并部署34页面恢复ROM，使用相同v9内容重新发布，最终Bundle6a8d702a/commit13、Data23/displayed；用户确认自动返回/额度正常。此处保留字体布局阶段证据，当前部署与验收见[bundle-page-restore.md](bundle-page-restore.md)。

**v9已部署并取得显示回执**：Bundle`3f4850d5`/CRC`12ff8166`，commit_seq10，source_crc`3ce26768`；Data19 applied，19:39认证复查display_state=`displayed`，不代替用户观感验收。app_build仍`967049b5f6a589b717e03d4abe312ed1`、boot_id仍8280352869110483957、ROM6994872 B，没有刷写或重启设备。目标MAC34F01557210B；其他设备Profile前后比对不变。

最终布局：首栏订阅/账号/时间，底部SYNC/RC一栏；英文完整Montserrat14，中文微软雅黑UI20；主额度Bahnschrift64、5h数字Bahnschrift60（纯数字子集）。所有数字均保留完整字形，主额度在说明与百分号之间按实际像素居中。

| 版本 | 原事务/结果 | 视觉反馈及处理 |
|---|---|---|
| v6 | 45237758，commit7，Data15/displayed | 主数字沿用48px，用户反馈显小 |
| v7 | 9bcc6926，commit8，Data16/displayed | 放大64/60px；用户反馈主数字偏上 |
| v8 | b18d9bd4，commit9，Data17；恢复前认证displayed | 下移10px但文本框仍小于字体行高，顶部裁掉2–3px，不能仅凭留白测试判断完整 |
| v9 | 3f4850d5，commit10，Data19/applied；19:39复查displayed | 主数字区域120×76、5h数字区域110×68；同时调整字形位置，新增无裁剪像素对比 |

19:35恢复工作时默认Bridge已退出（端口无监听、计划任务Ready），按规定提权`start-bridge.ps1`启动既有EXE；未重编Bridge。所有发布为显式独立版本的新事务，每版先确认前一版已安装，不重发未知结果的旧事务。

Bridge默认选中Note4，`platform_profile_get`没有MAC参数，因此从平台状态读取AP01 Profile；所有写操作必须明确目标MAC/目标render_target，不能沿用默认设备。

19:39 pending调查：仅读认证状态，未重新发布或投递。原Bundle/Data19不变，frame_generation从415到419，display_state已为displayed且无display_error。源码表明pending表示当前帧代号尚未得到imageDrawn确认；完整Bundle替换先retire旧画面，UI清除focused，后续render只提交帧、不主动夺取页面焦点；只有应用页面获得焦点、主界面可见且非维护状态时绘制回调才确认displayed。因此发布完成不保证立即显示；这条焦点重置路径可以解释此前等待，旧采样未记录UI焦点，无法追认当时具体页面或进入时间。分钟时钟也会生成新帧，故419不能单独证明用户操作。证据：artifacts/ap01-bundle-unclip-20261020/pending-investigation-status.json；ports/ap01/business.cpp的beforeReplace/render/status及ports/ap01/native_0041.cpp的tick/imageDrawn。

## Review

同源C++/Rust编译与渲染、CSFN边界/EPD拒绝、AP01不分配PowerPlan三项回归通过，模板Python/Rust/C++规范化CRC一致。九个场景覆盖单周/单月/双窗口、0/9/99/100，另检查缺RC、缺账号及长账号不覆盖时间。主数字及5h数字+百分号的像素数量与无region的完整字形渲染完全一致，防止上沿截断；主数字上下留白差≤3px。原尺寸预览已逐项目视。未扩大到真实LCD观感或嵌入式栈高水位验收。

兼容性：本轮只使用现有CompiledTemplate ABI2、CSFN v3 alpha4、单份完整Bundle、原I4调色板路径；不是此前宿主LVGL RGB565直绘预览。Bundle路线字体没有原厂kerning，仍可能与宿主LVGL字距/边缘有所差异。未更改设备协议、owner、PowerPlan、Wi-Fi配置或NVS格式。

资源：四字体nt16/nt30/ntthin18/ntreg64分别4958/7467/10569/8214 B，共31208 B（原v5为49112 B）；0字体0、单字体按实际文件、典型四字体31208 B，8模板共享同组字体时不乘8。每份低于48KiB；CTP1记录11333 B/33 op、单帧38400 B不变，新增NVS字节0。字体payload减少17904 B，不把此数字冒充总峰值RAM或任务栈；真实峰值未测。v9认证free_bytes1150976，与v8相同；沿用既有原子提交/回执/恢复格式。

证据：[最终v9](../../artifacts/ap01-bundle-unclip-20261020/)的publish-task.json、verification.json、verification-final.json、authenticated-final.json、template-manifest.json及PNG；之前v6/v7/v8分别在ap01-bundle-refresh/fontfix/align-20261020。私有状态、字体和画面不入库。当前工作区改动尚未提交。

ROM可复现性核查：两仓库包含应用/打包源码，但`roms/0041/ap01-1.0.2_0041.bin`未入Git，构建脚本明确要求SHA972db4c1…原件；RISC-V工具链和ArduinoJson为外部依赖。现有本机材料可重建应用并打包完整ROM，不等于纯Git检出可脱离原厂二进制从零编译整个原厂系统。本轮未构建或更新ROM。
