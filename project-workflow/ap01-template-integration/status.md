**2026-10-20 12:47 导航33已部署并通过用户实测**：build967049b5…/槽1，修正Pet7→菜单6及菜单→主界面误插应用、退出动画方向；用户回复“页序和动画均正常”。0031救援槽0保留且实机回切通过，原v5 Bundle保留、真实Data seq12已应用（末次读取pending）。130/133/134及12组像素、匹配宿主ASan/UBSan通过；默认Bridge仅启动既有EXE，未重建、未刷Note4、未提交。导航项结案，C9其他边界仍开放。见[最新部署记录](release-0041-20261020.md)。下方32及更早为历史。

**用户补充观察（2026-10-20 02:25 香港时间）**：顺时针旋钮实测：宠物 → 应用（从左边切入）→ 菜单（从右边切入）→ 应用（从左边切入）→ 原厂主界面（从右边切入，最上方显示日期、瓦数、温度）。 仅记录异常现象，未继续实现/刷写，详见[部署记录末节](release-0041-20261020.md)。

**2026-10-20 本轮按用户要求收尾提交**：候选32已运行槽1/build6b60914b…，0031救援槽0保留、实机回切通过；v5 Bundle b097bacb及真实Data seq9/displayed恢复。用户确认时间正确，但切页仍有偶发反向动画/页序不可靠，C9保持开放。已发现原厂key0x13→动画2、key0x14→动画1与当前应用退出相反；尚未改动或验证，停止进一步刷写。详见[部署与剩余问题](release-0041-20261020.md)。

**2026-10-20 动画与本地时钟修复已部署**：候选31已认证运行槽1/build7ac97d39…，实机先回到0031槽0再单次OTA更新槽1，救援槽未写。模板v5/完整Bundle b097bacb（CRC4b72d7d7）成功，真实Data seq7/displayed；右上device.now按分钟重绘，重置按UTC+8。111–113原生OTA/回切/同NOR恢复、115动画中途布局与移动、107旧版对照通过；实机动画/时间观感待用户确认。详见[部署与验证](release-0041-20261020.md)。

# 状态

**2026-10-19 23:37：原版0031救援完成。** 同MAC/DID认证0031且在线，用户已重新配网，确认原厂界面及旋钮正常；原厂恢复仅主动发一次、回执未知由实际运行收尾，槽0未作为写入目标。原生83/独立分区检查通过，临时服务关闭。以后带应用ROM按用户要求以移除OTA签名门禁的0041为基线，CRC/边界/身份/鉴权保留；当前0031不再刷写。详见[实机记录](flash-0041-20261019.md)。

**2026-10-19 23:29 原版0031救援进行中**：按用户授权，QEMU83原厂写入/拒签/关闭/恢复切槽通过，槽1逐字节等于原版0031、槽0全区不变；独立启动分区提交验证通过。HTTP下载及外层调度不在模拟范围，失败析构静态核对关闭且不擦除。实机认证原MAC/0041/槽0/空闲后已发唯一原厂下载请求并接收ok，等待下载完成再发一次恢复，未重派。原厂恢复可能清理配置并需重新配网；最终物理0031启动尚待核验。详见[实机记录](flash-0041-20261019.md)。

**2026-10-19 C2部署接续**：候选25旧安装经认证确认为实际运行，用户7059旋钮配对approved且确认原厂界面/切页正常。生产Bridge因C2与旧C1不匹配尚未完成登记。用户授权保留当前槽0，但旧维护巡检明确拒绝稳定确认（409 slot_image_invalid），未写APS1/新ROM。C2测试Data请求ID已修；槽巡检候选27改走原厂只读Flash接口，保留CRC/身份检查。原生76/81/82/79/80启动、完整OTA、9055ms健康锁回/切槽及中断恢复全部通过，16组画面零像素差异；候选27已严格放行但未刷机。精确产物、未知根因和部署入口阻塞见[实机记录](flash-0041-20261019.md)及[审阅](review.md)。以下为历史里程碑。

**2026-10-19 02:07：按用户要求停止并保存自动启动安全窗口进度。** 已实现/交叉编译，候选23尚未完成宿主及原生验收，不可替代下方已验收候选22。测试已中断、未刷机/部署Bridge/提交；精确哈希、失败/中断证据和续作命令见[自动窗口交接](boot-safety-handoff.md)。下方“最新”属于上一里程碑。

**2026-10-19 01:36 最新：独立OTA稳定槽功能和新0041 ROM已完成模拟验收，未刷机。** 正式文件`artifacts/ap01-platform-integration/device/ap01-0041-codex-status-ota-slots-20261019.bin`，6993160 B，SHA256 `63eb98e51c9cb1e0b2783d63b51c50da0dadb3db95b1ad107fcbb0fbb970df94`，匹配候选22 ELF SHA256 `d689fbfed009977496ae3631502b7c57a6fbc8fb3c9bab14e32c78c928236a32`。当前应使用此文件；下方候选16没有直接稳定槽回切功能。

维护18080新增slots/stable/switch，独立于业务18081和owner/context/Bundle。显式确认当前稳定槽、升级禁止覆盖稳定槽、不上传镜像直接回切、持久查询和响应后重启均通过。原生49/50/51在同一镜像/NOR上验证有焦点模板时升级、新槽回切、原稳定槽重启确认，真实ff13只提交一次，两槽字节及保护区不变，三轮四场景活动LVGL像素差异0。ASan/UBSan验证业务未启动仍可维护和损坏/unknown等负面路径。过程中发现并修复根UI绘制事件误入OTA输入分支的原厂LVGL断言；没有通过改模拟器绕过。见[交付说明](release-0041-ota-slots.md)和[服务/协议](ota-stable-slots.md)。

全程未刷机/访问设备/部署Bridge/提交Git；物理Boot2/LCD/实际free/高水位、0031旧接收器跨升和传统OTA旁路稳定槽保护均不在模拟证明范围，C9保持开放。

**2026-10-19 00:49 最新：本次完整0041 ROM制作及模拟验收已完成，未刷机。** 交付 `artifacts/ap01-platform-integration/device/ap01-0041-codex-status-20261019.bin`，6981976 B，SHA256 `6adb31dcaa0a35c77a38c729d298764f0c72ae7e729b6c77a21dd0b7beeb56bc`。候选16匹配ELF 473536 B，SHA256 `52909ff9eb07767e06ddf039c5c60faa47c5d26fa1a93b8f31c151b381c1a7c8`。源/头文件、审计、镜像、ELF与证据经accept-ap01-rom.py核对；正式manifest simulator_verified=true、hardware_verified=false。下方未完成描述为历史阶段。

原生31/32/33分别验证八模板完整Bundle及四场景/正反切页、完整维护升级至0x7b0000且保护区不变、同NOR新槽启动并恢复绑定/Bundle/激活/固件回执。四场景原厂活动LVGL缓冲像素差异0，真实0%与RC0不隐藏，缺5h时RC2保留。宿主ASan/UBSan、core162/render15/v2_state21、存储8组合通过；隔离Bridge重建、Note4共享兼容构建及匹配ELF Wi-Fi栈检查通过，均未部署。

维护18080/业务18081/发现8767原生接线完成，五GIF保留，AP01无PowerPlan。维护协议、资源成本和旧0031跨版本限制见[完整交付说明](release-0041.md)。QEMU使用明确的MAC/socket/输入/fbdev/Boot2等模型；物理首启、LCD/SPI、真实free/高水位及macOS WK未验，保留C9。

2026-10-18 21:39：完整0041候选01已生成（6961976 B），原生注入/线程创建已实际执行；Wi-Fi MAC全零的模型缺口正在处理。原生服务/UI及维护更新仍待验，flashable=false，未刷机。见[最新原生记录](native-0041-application.md)。

2026-10-18 20:32原生续作：渲染/画布已迁为调用方持有对象，绑定与渲染核心11个RV32对象通过只读段/初始化段检查；原生String端口宿主四组像素差异0、共享回归33项通过。0041窗口/图片/旋钮事件取得专用模型依据，尚无完整ROM。见[原生里程碑](native-0041-progress.md)。下方为此前Bridge/SIM验收。

2026-10-18：Bridge接入代码与实时共享核心/NuttX模拟闭环通过，未部署。完整0041原生ROM接线未完成、没有新ROM；生产Bridge/Note4不变，本轮未访问或刷写设备。

## 当前实施结果

**最新HTTP/存储修订**：维护18080与业务18081分离，配对走维护；业务停机后维护健康/配对仍可访问。11检查/8截图及core161/render15/app AP01 4项通过。0041/resource总量1.25 MiB；完整1/2/4/8模板raw/CSZ1共8组合安装/更新及满盘保护通过；原机free仍未知，当前预算不要求删除GIF整页。完整结果见[独立HTTP与存储调查](storage-http.md)，最新实时报告run-20261018-195827；下方早先10项报告保留为历史。

用户最新纠正：新ROM必须基于0041；已把Bridge认证目标、模拟设备、完整Bundle测试与构建审计标识改为`cuktech-ap01-0041`。0031原生地址审计保留历史含义，不能视作0041 ROM验收。0041软件目标复验已通过：core模板3项、app AP01专项4项，另单独验证拒绝0031；实时SIM仍10检查/8截图。Bridge候选哈希已更新，未部署。

- 新增Bridge UI/MCP共用的AP01搜索/四位码/查询/取消服务；多接口实际掩码广播、8秒期限、3次发送、取消与32候选上限，已知设备共享近期搜索，接口变化重置退避。默认实例接收AP01通告，命名实例主动探测不依赖8767监听；已知IP失败后UDP→邻居表，认证MAC/能力后更新地址。真实多网卡、DHCP与权限矩阵未验。
- 候选token/request在POST前持久保存；结果未知查询原事务，重启不更换ID。配对确认不自动发布或启用同步；AP01业务使用每MAC绑定凭据。owner/租约与PowerPlan分开，常在线目标生成/发送Plan均拒绝。
- 独立loopback设备服务执行共享C++绑定、Bundle、激活、data_contract与渲染。NuttX UI只经私有模拟端口接收帧/描述并回传真实按键/绘制与激活回执，不是网络图像推送协议。实时集成10检查/8截图：旧按键释放不能确认、后台不抢天气焦点、缺5h居中、RC显隐及0值、双模板新context、本地持久激活/重启恢复、Weather/Pet双向切页；6组共享帧与实际LVGL像素差异0。最终证据[run-20261018-194410](../../artifacts/ap01-platform-integration/run-20261018-194410/live-report.json)，ELF14985192 B/SHA256 `21f4bd4a78716fb317eb4f2ce70b6f82c7d67696aa9b3f7d39e1e286665b52b4`；`latest.txt`同时记录入口。
- 共享core161/render15/MCP3项通过；app串行107通过/3条件忽略，新增共享发现缓存后AP01专项4项通过；Node设备页回归与Edge560/1000截图通过。`bash tools/test-wifi-ui.sh`已尝试，Windows无swiftc，未冒充WKWebView通过。
- Bridge候选`bridge/target-note4-deploy/debug/bridge-app.exe`，38741504 B，SHA256 `f55dbd432ef1de86c47191cfe1bfcc5457b6a35939469f32104e99495dc43bc4`；未启动、未替换生产EXE。最终live报告登记其余二进制/ELF哈希。
- RV32便携核心编译通过，修复ArduinoJson可写静态singleton；对象无可写静态段。绑定APB1空/典型单条/8条上限分别16/121/2136 B，提交核心峰6816 B（不含JSON/UI/Bundle）。单函数最大栈binding496 B、pairing656 B，不代表调用链总栈。原生接线与容量风险详见[原生审计](native-integration-review.md)。

完整0041原生HTTP/OTA仲裁、VFS、共享引擎RAM context、UI回调和镜像保护区仍未接完；原机存储不能按宿主容量推导。**本轮没有待刷ROM路径或哈希。**

证据入口：主仓库 PROGRESS.md、docs/generic-display-platform-design.md；AP01 归档的 tooling/scripts/ap01-ui.h、reverse/reports/ap01/application-rehost-complete-0041-20261017.md。原厂导航是有条件的有向边，不能替换成全局过滤后的固定循环。

## 结果

- `codex-status-a` 新彩色变体：[模板](../../tools/test-bridge/templates/codex-status-ap01.json)，28操作、7远端/2本地字段。缺5h隐藏整组并居中；不加入Token统计或历史。模板CRC **fb18016d**，Python/Rust/共享C++一致。
- RC与5h独立：先前单窗口预览夹具漏填RC，现已统一展示示例RC 2，并回归两种布局中RC存在显示、缺失隐藏；模板绑定原本正确；本次字体/排版调整后CRC为fb18016d。该数字是示例，不代表当前账号实时读取。
- 活动CT仍11321 B，CTP1记录11333 B，布局/ABI2不变。单帧38400 B；LVGL包括64 B调色板后38464 B，原子换帧峰76928 B。字体资产标签5500 B＋数字30433 B＋中号13095 B，由已有完整Bundle发布。
- 原有Weather→全部模板→Pet双向导航；后台数据不抢屏；前台缺失重排、充电弹窗、屏保唤醒、8项循环/失败保持位置、原厂3秒主页均验。对应实现为 [导航端口](../../src/ap01_template_page.h)、[LVGL显示接收端](../../src/ap01_template_lvgl.h)。
- 隔离0041 NuttX SIM **21检查/9截图**通过，4次模板画面与同源引擎逐像素差异0。显示缓存按需4096 B，包含1280 B解码行；没有完整RGB565/ARGB复制帧。
- SIM ELF `/home/slepher/nuttx-ap01-template-20261018/nuttx/nuttx`，**14,965,080 B**；SHA256 **f38e2a3362dc4f09706224e64fbacab2d6e1e5850333100e312d8d2515434f5e**。这是宿主ELF，不是ROM。测试自己的SIM/Xvfb均退出。

## 验证与证据

证据全部在 [artifacts/ap01-template-integration](../../artifacts/ap01-template-integration/)，不提交字体/截图/日志。`sim-integration-report.json`、`aa-native-navigation.png`、`template-manifest.json`、`aa-compiled-parity.log`分别记录真实X11/像素、页面对照、编译合同与CTP1读回。JSON→CTP1及序列化读回差异均0。

视觉精修后完整core/render **253项通过**；app首次并行回归103通过/1个全局测试网络环境断言失败/3忽略，单独重跑通过，此前串行104通过/3忽略；本轮最终串行105通过/3忽略。新增AP01能力回归所属5项通过；最终模板/边界及跨语言CRC通过。详见review，不把首次失败抹成“始终全绿”。

## 复现

新目录先提权预建；构建/复制/WSL命令提权执行。现有目录可复用，不例行clean，不触碰生产Bridge/原SIM。

1. 仓库根 `node tools/generate-ap01-template.mjs`；`pwsh -File tools/generate-ap01-font.ps1` 和同命令分别加`-Digits`、`-Medium`，字体仅本地生成。
2. `bridge/` 下设置`AP01_PREVIEW_DIR=<repo>/artifacts/ap01-template-integration`，`AP01_PREVIEW_FONT=<该目录>/ap01-labels.csfn`、`AP01_PREVIEW_DIGITS=<该目录>/ap01-digits.csfn`、`AP01_PREVIEW_MEDIUM=<该目录>/ap01-medium.csfn`；运行`cargo test -p bridge-core --test ap01_template --target-dir target-note4-deploy --offline`。再在根运行`python -B tools/test-ap01-template-canonical.py`。
3. `cargo run -p bridge-render --target-dir target-note4-deploy --offline -- --template ../tools/test-bridge/templates/codex-status-ap01.json --usage ../artifacts/ap01-template-integration/weekly-only.usage.json --font ../artifacts/ap01-template-integration/ap01-labels.csfn --font ../artifacts/ap01-template-integration/ap01-digits.csfn --font ../artifacts/ap01-template-integration/ap01-medium.csfn --compare-compiled`。
4. 使用**独立复制**的`/home/slepher/nuttx-ap01-template-20261018`；不要指向已有交互SIM。WSL中运行`python3 -B <repo>/tools/prepare-ap01-template-sim.py <ap01归档>/tooling/scripts <隔离SIM根>`，然后在其`nuttx/`中`make -j4`；最后`python3 -B <repo>/tools/test-ap01-template-sim.py <隔离SIM根> <ap01归档>/tooling/scripts`。准备器校验目录、重定位复制的绝对链接、清退静态库重复旧成员；不能省掉这一步直接运行复制来的旧ELF。

## 仍未验收

本轮没有生产AP01 ROM、未接真实设备binding/HTTP/Bundle接收/本地持久激活。SIM显示使用共享引擎生成的离线frame夹具，激活回调不持久化；不是认证网络端到端验收。真实BSP/LVGL版本、原ROM UI hook、堆/栈、掉电/完整Bundle安装验收仍归C9。保存/编译不等于发布；未改实际Profile或部署。

视觉精修已完成宿主和SIM复核：CSFN v3 alpha4、16色覆盖率调色板、4×4环形边缘采样，字体字号/间距与重置行已调整。RC仍独立于5h，缺桶时整体居中。原尺寸前后对照`aa-before-after.png`，原有天气页与两种布局对照`aa-native-navigation.png`；未用静态效果图替代实际渲染。不宣称与原厂资源完全一致，实际屏幕效果及用户视觉确认仍未完成。

## 后续ROM接入范围修订

AP01不接PowerPlan，常在线，屏保不停止网络服务；owner续约独立保留。原配对沿用旁仓`screen-binding-design-20261014.md`：双屏四位数字＋旋钮确认，无需输入数字；撤回上一稿六位码/PAKE提议。现已补全[自动发现方案](discovery-pairing.md)：多接口主动UDP＋地址变更通告＋邻居补查，已绑定设备认证后更新IP，正常LAN首次添加与重连不输入IP。方案已在上述Bridge/共享核心/隔离SIM范围实施；完整原ROM接线仍未完成，未改ROM/Bridge运行实例。

0041冻结原件：6769704 B，SHA256 `972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9`，已只读核对。0041已有OTA源在旁仓`tooling/firmware-modules/ota_0041.c`；旧构建脚本尚含迁移前路径，实际原生接线/构建时须修正，不能使用0031脚本代替。模拟阶段文件改用原子写，避免Windows/WSL读到截断或ENODATA；修复后复验通过。

## 2026-10-19 13:13 自动安全窗口最终交付

候选24完整0041 ROM已生成并严格放行：6995352 B，SHA256 `a7d1962b5b7d3ef6f590857632f5d27ffbef5f8481039bf96b98cccdbe6b8194`。宿主ASan/UBSan、独立双PT模型三profile防护和原生59–63矩阵全部通过；9003ms健康锁回、同NOR窗口中断后的稳定槽完整业务恢复、手动切槽精确重放及16组零像素差异通过。41份证据指纹与匹配ELF/包见[交付](release-0041-boot-safety.md)。未刷机/访问AP01/部署Bridge；只证明成功预置后的软件窗口，C9保留实机首启/掉电/LCD/free/高水位边界。旧02:07未完成状态已被本节替代。
