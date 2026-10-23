# 四日审查修复（2026-10-22）

两仓库修复分别纳入提交，未部署、未访问设备/运行data、未启停Bridge。关联C9、A9/B6/C8；部署仍见PROGRESS。

## 已实现

- 维护：可选稳定镜像失配、健康超时与已确认旧稳定槽不再否决显式OTA/手动切槽。OTA在持久化原事务后确认下次启动为当前运行槽，再允许写非活动槽；选择结果未知保存Unknown、原ID查询/重放不重复选择。手动目标仍检查镜像及当前槽，认证/并发/修订/CRC/长度检查保留。
- 额外修复完整上传测试发现的UB：manifest与请求体指针先计算范围内偏移，不先形成越界指针再减回。
- Wi-Fi：回退凭据直接从原厂当前配置读取，不从合并展示列表提取。仅保存同名新密码不会污染回退；列表原密码保留逻辑不改。
- Bridge：JoinSet持有HTTP、源采集、缓存/状态轮询、平台同步与BLE服务；退出/panic传到既有supervisor，父取消撤销子任务。设备同步worker也纳入平台任务所有权，正常移除设备不触发故障恢复。
- 菜单：保留原厂六项逻辑索引及flex，追加设备码6/Wi-Fi7/返回8；删除手工九行定位与视觉索引转换。原厂详情回调及字体/子菜单复用保留。原生验证脚本同步新顺序。
- GET列表仅分配一个读取列表；PUT工作区/原子持久格式不变。设备码、诊断、切网日志复用IEEE CRC，菜单复用固件randomBytes。
- 固件打包器接收显式source-dir/tool-prefix/original，不再反向依赖codex_status目录。App兼容入口保留旧目录默认值与现有消费者所需的显式导出。
- PROGRESS压缩到约2KB，前卡归档至docs/history；既有清理记录保留工作区，不纳入本次提交。

## 已执行离线验证

证据目录：[artifacts/review-fixes-20261022](../../artifacts/review-fixes-20261022/)。

| 检查 | 结果 / 证据 |
| --- | --- |
| Bridge监督定向 | 5通过，rust-tests.log |
| Bridge App全套 | 126通过、3条件忽略，app-regression.log；未运行需要独立device-sim/私有网卡条件的三项 |
| Wi-Fi核心/配置列表/生产适配层 | ASan/UBSan通过，wifi-result.log、profiles-result.log、wifi-port-result.log |
| 独立维护恢复 | 旧稳定metadata、armed/超时/healthy状态、未知启动选择重放、直接手动切槽与非法镜像拒绝通过，recovery-result.log |
| 无App完整OTA | 真实生产维护HTTP/上传/鉴权/码轮换/重放/重启回执通过，maintenance-full-build.log；Flash为私有byte array |
| 菜单适配器 | 生产源码+显式ROM/LVGL替身：原厂索引、九项循环、详情回调、新增菜单返回通过，menu-result.log |
| 设备码/诊断 | CRC复用后的现有测试通过，code-result.log、diagnostic-result.log |
| 打包与边界 | synthetic fixture完整重建/篡改拒绝、边界检查通过；normal/restart QEMU拼接脚本可编译，packaging-result.log |

红测：旧Wi-Fi核心在真实运行密码与同名保存密码不同的场景命中`!strcmp(f.password,"synthetic-old")`断言；旧维护核心在稳定metadata失配时拒绝begin，命中`reply.status==200`断言。修复后同场景通过。

Mac命令：在bridge目录执行`CXXFLAGS='-isystem /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/c++/v1' cargo test -p bridge-app --offline -- --test-threads=1`。C++本机编译命令与测试程序保存在证据目录run-host.py（只消费受控夹具）；Mach-O宿主用`-Dsection(x)=unused`忽略仅RV32/ELF使用的预算节属性，不改变设备编译配置。最终维护重跑设`UBSAN_OPTIONS=halt_on_error=1`。

后续标准WSL入口`tools/test-ap01-native-app.py --image <完整候选> --out <预建输出>`已接入两个新增维护/菜单回归；仍须按agent-operations从提权命令运行，它创建私有状态目录。Wi-Fi既有入口为固件仓库`tooling/scripts/test-native-wifi-switch-0041.py`。

## 资源与验收边界

- 持久格式/字节增量：空、单条、典型、上限均0；没有新增记录字段。固定格式WPL1仍16+98*n字节（n=0..8）；回滚记录WNR1仍10+SSID字节+密码字节。
- GET原Workspace为3416B（两份908B列表+两份800B缓冲），现908B，减少2508B请求堆；JSON/读取800B临时栈未计入此数。PUT不变。切网复用已有Attempt.previous，无额外持久凭据副本。
- 菜单RV32 Context字段对齐/探针offset断言仍保留；移除mainUnordered占用的是原有padding区，预计结构大小不变，须以匹配RV32构建确认。Bridge只增加实际活动任务的JoinSet句柄，无持久格式变化。
- **本机首轮限制（后由下文james续验补齐）**：当时本机未找到AP01 RV32工具链及定制原生QEMU。没有新候选ROM/ELF、没有原厂菜单像素截图、没有真实调用链栈/高水位结果。宿主菜单替身不证明原厂flex/滚动像素。打包测试的synthetic-test-image.bin是不可执行的测试夹具，严禁部署。
- 后续james已补齐RV32 audit/link/ROM重建及原生菜单/重启/driver fixture和像素回归；物理验收不在本轮范围，本轮不刷机。


## james 离线续验（进行中）

用户授权 SSH james，并明确允许复制验证所需源码、0041 ROM、工具链；独立目录 `/home/slepher/ap01-review-20261022`。复用本地 Docker 基础镜像构建专用镜像，Debian 依赖走国内镜像，GCC 8.4 从 Espressif 国内官方资产镜像取得。执行容器禁网、2 CPU、2 GiB；未影响已有服务。

- `check-ap01-native.py`、`link-ap01-native.py`新增显式`--tool-prefix`，审计新增`--json-header`；默认 Windows 行为保留。
- GCC 8.4 RV32/ilp32f 源码、零可写静态段、ABI、链接及完整 ROM 重建通过。GCC 13 初试因 freestanding C++ 头不声明 malloc/free 失败，未为此改业务源码，改用历史一致工具链。
- 候选：`artifacts/review-fixes-20261022/james-native/review-candidate.bin`，7043256 B，SHA256 `8ee11c4204c2b5df01aa42f3bf97a17eeb2230df705386dfbb0c3729a3f8a5d7`；build ID `82d294d3f52d953d57ce779de459268d`。仅候选，未部署。
- 匹配 ELF：549312 B，SHA256 `138fab2b6a537229007984418f5a8055774a220066eb6001d6249d7224085709`。
- 菜单 Context 52 B；最大菜单单函数栈帧160 B；Wi-Fi列表load 864 B、HTTP入口336 B；维护loadStable 1008 B。均为静态单帧，不等于原厂完整调用链高水位。
- Linux ASan/UBSan：Wi-Fi核心/列表/生产适配、设备码、诊断、维护恢复、菜单适配器通过；以真实候选为输入的无App完整OTA/码轮换/重放/重启/无稳定记录切槽通过。修复测试适配器对空vector调用memcpy的UB。完整OTA初次因新目录缺f21失败，预建空目录后通过。
- 原生 QEMU 菜单/重启已完成（见下文最终结果）：远端海外下载失败后按用户指示从本机下载官方发行包并同步。


### 原生环境复现补充

QEMU10.1.0官方发行归档SHA256 `e0517349b50ca73ebec2fa85b06050d5c463ca65c738833bd8fc1f15f180be51`，本机分片下载后完整tar校验并同步；未伪造Git提交。直接检查并应用仓库现有CPU patch、板级源文件，构建引擎SHA256 `25a9c3620f83411672359412d6aca73c20f0e2eb94d6a8fbae5e4c39f3be3f11`。

缺失历史合成OTP，首次全零夹具使未修改0041及候选都在`chip/bl616_start.c:282`断言后空线程指针异常（mepc a002a586、mtval8）。原厂读取OTP offset24：bits25..24芯片版本须非零，bits23..22封装类型2支持原厂使用的GPIO。现夹具仅该word设`0x01800000`，其余零；无物理芯片等价声明。版本1但类型0的中间对照停在原厂GPIO限制循环a002639c，报告保留。未修改ROM断言或初始化。

`tools/run-ap01-native-qemu.py`输入注入前现在读取运行结构中的当前UI root，避免仅依赖旧构造断点；本次首次配对捕捉到了旧入口的None.to_bytes异常。该修改仅为模拟器测试入口，不改变候选ROM哈希。原厂设置与Wi-Fi列表/连接截图已审阅；完整菜单与重启最终结果见后续记录。


### 原生最终结果

`menu-fixed.json`: passed=true；原厂LVGL输入、九项设置、空/单条/典型/上限列表、焦点与勾选独立、长名裁剪、首尾循环/滚动、当前项不重连、原厂屏保/亮度/版本及设备码返回、退出/重建全部通过。`native_wifi_switch.passed=true`：实际IP后成功、重复请求保持原事务、失败恢复旧配置，执行真实guest加密PSM，RF响应是显式fixture。

`menu-restart3.json`: passed=true；使用`menu-fixed.flash.bin`精确哈希匹配NOR，保存中文名称/当前勾选/绑定/身份/菜单码恢复，cache_not_seeded=true。首轮重启命令误用初始Flash已纠正；随后一次真实快照重启的列表数量检查早于原厂当前网络缓存恢复，另一次同输入通过。测试现增加30s上限等待，不降低最终状态要求，最终复跑记录另列。

原厂设置页、Wi-Fi典型列表/连接勾选/最大列表返回行的原始RGB565缓冲已转PNG并审阅。证据在`artifacts/review-fixes-20261022/james-native/`；模拟LCD的PPM不是验收截图。设备码值及模拟绑定token只留私有artifact，不写交接卡或仓库。

最终`menu-restart4.json`在加入有界等待后passed=true，native_execution_clean=true、服务与恢复检查通过；本次ready_polls=1。最终判定读取报告JSON，runner stdout早先的passed/full_application是通用业务阶段输出，不能代替专项最终结果。

取回后重新核对315项当前源文件/头文件哈希、候选SHA及两份原生报告的runner SHA均匹配；最终归档`james-native/native-evidence.tar.gz`，结构化结论`james-native/final-verification.json`。重启后的中文SSID原厂RGB565截图亦已审阅。两仓库diff检查和AP01边界检查通过。本次修复分别提交两仓库，测试产物不入库。


## 2026-10-22 授权实机续验（当前）

用户授权更新AP01并保留救援0；候选01暴露共享宿主每次切入强制重绘回归，App侧保留当前有效帧后，候选02已部署应用1。用户确认设置/GIF两侧单格切入和App内容正常；原生红/绿测与匹配构建见[共享宿主状态](../ap01-screen-host/status.md)。首次测试目标SSID大小写不匹配，失败后认证核对原网；用户纠正大小写后，单次列表修正，实机连接目标/勾号及回切原网均获确认。临时列表已清理，11身份/绑定/设备码/系统文件比对通过。间歇跳过SSID仍开放；字体按用户要求暂缓，原厂运行日志缺此次明确失败原因。救援0完整6822344B读回SHA256与保留镜像匹配，从未为写目标。用户后续要求的异地网络已保存并读回确认，当前网络不变；Bridge未重启。本节覆盖历史“未部署”描述，未追加提交/推送。

## 2026-10-22 香港本机同步续验（完成，未部署）

主仓库快进至f8342fb、固件至da7a5b5；本机Windows GCC8.4重新审计/链接/打包，所有编译源/头哈希和完整ROM重建通过。候选`artifacts/ap01-synced-20261022/ap01-synced-candidate01.bin`，7043256B，build`82d294d3f52d953d57ce779de459268d`，SHA256`eaabe3681861ff5a08828dcd0ba1168a80e56adaedc1f7fd201c801944c46155`；ELF SHA256`3e33681d7db828820fb11dcd3872afbd8eb6d141af1697eb9607c259f43e562e`。本机完整镜像哈希与james产物分别登记，不以相同build ID冒充同一文件。

本机Wi-Fi核心/生产适配、App/维护/无App OTA切槽、旧metadata恢复、菜单适配器ASan/UBSan均通过。原生menu01完整菜单/切网fixture/退出重建通过；精确快照restart02恢复中文SSID/当前勾号/菜单码/绑定及身份，未注入缓存，首轮GET即200。原厂RGB565已审阅，两个虚拟固件槽、boot/partition/reserved区域字节不变。实际菜单顺序是六项原厂项、设备码6、Wi-Fi7、返回8，保留原厂flex；旧“屏保后Wi-Fi/绝对位置映射”说明不再描述当前源码。

restart01 guest无异常，但测试在GET503时直接断言，未保留响应体，不能据此断言503的深层原因。现将既有30秒轮询扩展为仅重试结构化`wifi_profiles/read_unavailable`503，记录status/stage/error；其他错误立即失败。提取实际轮询代码的HTTP/时钟夹具四项通过：已知503→200、其他503拒绝、401拒绝、持续503期限退出。restart02本身未重现503，不能记为真实503恢复验收。该改动只在离线验证脚本，候选ROM未重编。

[本机结构化验收](../../artifacts/ap01-synced-20261022/local-acceptance.json)。无实机访问/刷写、无Bridge启停或运行data变更；设备仍应用06/槽1、救援0031/槽0。真实RF/LCD/掉电/完整调用链高水位仍为C9。此处及对应报告/脚本为同步后的本机续验改动，尚未提交。
