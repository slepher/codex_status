# AP01 Bundle页面恢复与反馈（2026-10-20）

## Plan

用户要求：发布前处于模板页，安装完成自动回到对应模板；原厂页不抢焦点并明确等待原因；多模板按ID保留，删除时使用新Profile初始页；displayed只代表当前模板最新帧。先完成模拟，后经用户明确授权更新AP01应用槽1并实机验证，保留救援槽0；本轮不部署Bridge。

## Task

- 在单份Bundle原子提交中选择原模板ID，缺失则保留新初始页。
- UI退役保留恢复意图；替换完成恢复，用户导航/维护优先，失败留在内置界面。
- 状态和ACK携带等待原因及当前模板ID，Bridge认证缓存/详情保留反馈。
- 宿主ASan/UBSan与原生RV32/LVGL模拟覆盖原厂页、单页、多页重排/删除、实际绘制回执及失败。

## Status

**34已部署应用槽1并完成单页实机验证**。认证MAC34F01557210B、build cc18eeb5db1f0df571245c3175f35df1、6995768 B、boot_id13088846631776617966一致。先以单次page-restore34-rescue-20261020实际启动0031救援槽0，认证BootROM映射/槽号、DID2232184842及镜像头尾，再通过其独立OTA单次上传槽1；返回done/6995768 B后再次认证运行。救援槽0未写、配置未清；原事务artifacts/ap01-app-rollout-20261020/page-restore34-rollout-task.json已runtime_verified，不得重传。Bridge未部署，Git未提交。

实机验证使用完全相同的单页v9布局/字体，不改Profile。原厂页发布c24a7cb0/commit12并投递Data22后，pending/waiting_for_template_page，未抢焦点；用户进入额度页并确认保持旋钮不动，认证displayed后发布6a8d702a/CRC4831c60d/commit13，新context实际draw回调确认displayed/current_template_drawn，Data23也已绘制，boot_id保持。用户明确确认“自动返回，额度正常”。最终仍仅codex-status-a/source_crc3ce26768，所有设备Profile与刷前一致；多页实机未做，以原生/宿主8页证据为界。物理证据artifacts/ap01-app-rollout-20261020/physical-verification-page-restore34.json及artifacts/ap01-bundle-unclip-20261020/rollout34-*.json。

- 原模板ID在Bundle提交标记写入前选定；不存在则使用已验证的新initial_active_id，不增加二次持久激活事务。
- UI退役记录原模板焦点/底层页；新帧到达后自动恢复，输入会取消恢复意图，原厂页/维护/配对优先。旧页面及字体仍在替换前释放，失败保持内置界面。
- 认证状态和正常ACK增加display_reason/active_template_id。displayed只代表当前generation及当前模板；原厂页显示waiting_for_template_page，Bridge投影为“已安装，等待进入模板页”。只在UI线程访问LVGL，跨线程原因通过atomic传递。
- Bridge保留原因的认证采样；新状态没有原因字段时清除旧原因，避免旧固件/BLE回执混用。无HTML/CSS布局变更。

## Review

原生136最终passed=true：完整0041候选运行真实RV32业务/UI代码及原厂LVGL，覆盖焦点内重排保留ID并自动绘制、删除当前页使用新初始页、单页更新、原厂页保持及明确ACK/状态原因、重新进入只确认当前模板；原有正反向导航/Pet边/动画/激活重放通过。五个场景活动帧缓冲对I4渲染逐像素差异0，恢复截图已目视。模拟使用两模板旧布局夹具，网络/MAC/输入/fbdev/WEXTI仍为显式模型。135误对不含device.now的夹具启用分钟刷新检查，保留其clock timeout失败；136去除不匹配检查，未放宽页面断言。137完整OTA/138同NOR新槽恢复及无稳定记录回切/139回切后原回执恢复通过，12组活动LVGL像素零差异。正式包通过既有accept-ap01-rom.py，未放宽发布器；实机另如Status所述，物理掉电/真实栈高水位仍未验。

Bridge app设备视图4项、core认证状态8项及node tools/test-device-page.mjs通过。bash tools/test-wifi-ui.sh在WSL缺swiftc，未运行原生WKWebView/窄宽截图；未声称macOS原生UI验收。

宿主native-app-report.json通过ASan/UBSan：8页重排保留ID、全部8项激活及合同/序号重置、删除页后单页初始回退、过期绘制回执不确认新帧、渲染失败状态；完整Bundle分块/提交重放、激活丢rename ACK与重启恢复等原有检查保留。native-firmware-host-report34.json以匹配34镜像重新跑假Flash维护/OTA/恢复用例通过，放行时native-firmware-host-report.json指向同份报告。

资源：RV32 Runtime整体2088 B（33为2080 B，含内嵌Business，不重复相加）；Business1776 B（原1768）、BundleStore700/Application592 B不变。固定新增量用于有界UI恢复/原子反馈，空/1/典型2/上限8模板相同；持久格式/新增持久字节均0，帧38400 B及LVGL I4图像38464 B不变，无逐页帧缓存。bsInstall只增加可选ID指针，栈1040 B；tick160/serve416 B，Business command544/displayFeedback32 B（均静态局部帧，不是调用链高水位）。ACK增补用临时JSON文档，真实allocator峰值/实机栈高水位未测。

已部署发布包：artifacts/ap01-platform-integration/device/ap01-0041-page-restore-20261020.bin，与候选34字节一致，6995768 B，SHA256 `6ef5f3fd831e2f4faca5d50e2ea92cd620844080d33afb1873dc25a0fcfa42aa`；匹配ELF SHA256 `671adea921e1259dd87848fa901dd830489d19e6038b95512466ec794bf90426`。发布包/上传完整回执及运行build已核对，不声称全Flash实读SHA；原件0041和救援槽无改动。

证据：artifacts/ap01-platform-integration/device/native-qemu-135.json（失败保留）、native-qemu-136–139.json、对应pixels.json/PNG、发布包acceptance.json；native-core-audit.json及candidate-34.manifest.json。真实模板/Profile只经Bridge共享服务读取和发布，未直接编辑运行data；私有状态、凭据和画面留在忽略的artifacts，不入Git。
