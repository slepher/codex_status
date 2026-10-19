# 自动启动安全窗口交接

2026-10-19 13:13 香港时间：本轮续作完成。候选24已严格放行生成完整ROM，6995352 B，SHA256 `a7d1962b5b7d3ef6f590857632f5d27ffbef5f8481039bf96b98cccdbe6b8194`。宿主ASan/UBSan与原生59–63通过，健康锁回间隔9003ms、双表交替、同NOR窗口中断→稳定槽业务恢复、手动回切及16组零像素差异均已验证；完整交付与41份证据见[最终交付](release-0041-boot-safety.md)。未刷机/访问实机/部署Bridge，本轮未提交Git。下面为02:07旧交接和中途调查历史，不再是待执行清单。

中途调查：原生54首次绑定/八模板/四画面/全包OTA通过；55预置稳定0成功，但旧QEMU只允许写固定一张PT，9秒后另一张表被模型拒绝（ff13=7），固件按unknown停止，不能作为健康验收。随后隔离构建双PT模型并通过三profile Flash保护回归，原引擎/旁仓源码不改。额外宿主短测首次目录过长超出端口128字节路径上限；改短目录t24f后通过，原日志保留。

2026-10-19 02:07 香港时间：用户要求快速结束并保存进度，已停止本轮隔离测试。未刷机、未访问实机、未部署/启停Bridge、未提交Git。

## 已保存

- `ports/ap01/firmware.{h,cpp}`：已确认稳定槽完整镜像身份核验、启动预置、原生UI/维护监听连续健康9秒锁回工作槽、30秒无健康保留稳定槽；无稳定记录不猜测。窗口内禁止新升级/修改稳定记录，显式稳定槽切换可取消自动锁回，选槽unknown不重试。
- APF1新增可选`boot_trial_pending`，预置前写旧ROM可理解的Failed回执；健康确认只更新当前启动内存态，每次启动重新验健康，避免稳定槽旧ROM读取Applied工作槽回执后阻塞业务。APS1稳定槽仍为显式确认。
- `application.{h,cpp}`传递启动时间；`native_0041.cpp`接原生UI心跳及维护监听健康条件，不依赖Codex/Bundle/owner；`slot_0041.cpp`成功提交后推进RAM表副本索引，让预置/锁回交替写表。已有绘制事件筛选保留。
- 宿主测试新增健康/超时/心跳中断/unknown/损坏槽/显式救援/无稳定记录；模拟器脚本新增`--boot-trial-cut`、`--fallback-receipt`、双表CRC/age/slot及字节保护核验。代码已保存，新增模拟分支尚未验证。

## 产物与证据

- 候选23：`artifacts/ap01-platform-integration/device/ap01-native-0041-candidate-23.bin`，6995352 B，SHA256 `4571154b90b2d8e7cddc564fec068501d148a33672ad806fe4295b7296f504f0`。
- 匹配ELF同名前缀`.elf`，491388 B，SHA256 `6bd74440cc7660ca13215d16902921efe33f36b637b8bdd2ccf0129f914ca048`；同目录保留map/audit/link/payload/manifest。
- RV32交叉编译31对象、只读段/未解析导入审计、链接与完整0041镜像重建通过。维护service静态对象120 B（前版84 B），事务504 B，持久上限1036 B不变；单帧/字体/Bundle未改。实际峰值栈/RAM仍未知。
- `boot-safety-native-build.log`、`boot-safety-link.log`、`boot-safety-rom-build.log`为构建依据。
- 首轮宿主测试在新增显式救援用例中复用了已有事务ID而触发预期冲突；用例已改独立ID。`boot-safety-host-build.log`保留失败；第二轮`boot-safety-host-build-2.log`按用户要求在宿主native-app-smoke阶段停止，**没有最终ASan/UBSan通过结论**。旧`native-firmware-host-report.json`对应候选22，不能用于候选23验收。
- 原生52使用旧51的NOR但直接替换工作镜像，旧升级回执身份不匹配导致business_offline；这是不合法升级夹具，不作新窗口失败/成功证据。已改为原生53干净配置、八模板完整Bundle和OTA；用户收尾时中断，日志`native-qemu-53.runner.log`与`.json.uart.log`保留，**无完整验收报告**。不可复用编号53覆盖旧日志；下一次使用54及以后。

## 续作顺序

提交前格式检查移除了`src/sha256.cpp`末尾多余空行及模板模拟准备脚本行尾空格；前者改变候选23审计中的源码指纹（无逻辑变化）。因此续作应重新审计/链接并使用候选24或更高编号，不能直接将候选23放行。

1. 先复核安全状态/回执兼容和表索引提交顺序；检查当前源/头文件与候选23审计一致，若改生产源码则使用新候选编号。
2. 提权运行`python tools/test-ap01-native-app.py --image artifacts/ap01-platform-integration/device/ap01-native-0041-candidate-23.bin`，脚本会新建私有目录；不要复用已跑过的测试状态。修正任何实际失败后再重建候选。
3. 用`prepare-ap01-native-qemu.py --candidate 23 --run 54`干净模拟首次绑定/八模板/四场景/完整OTA（`--firmware-probe`）；不要直接在旧Applied回执上替换镜像。原生脚本/参数可参考53日志命令。
4. 新槽启动：从完整OTA后的NOR继续，`--active-slot 1`、恢复绑定与事务回执；验证预置稳定0、9秒健康、两张有效表交替（age+1、保留稳定0表）、工作槽1及两槽字节未变，再重跑原有手动回切/恢复与四场景像素核对。
5. 同一升级后NOR分叉运行`--boot-trial-cut`，在首次ff13成功后截取`.trial-armed.flash.bin`并停机；原样从稳定0启动，`--fallback-receipt`核对待确认回执和完整业务恢复。条件允许再直接运行旧候选22稳定ROM做跨版恢复，必须先建立合法混合镜像/APS1身份，不能事后替换稳定镜像冒充证明。
6. `accept-ap01-rom.py`尚未扩展自动窗口验收参数。新增正常/断电分支匹配验证后才生成正式boot-safety ROM/ZIP，更新任务勾选、review/status/PROGRESS与C9。

## 验收边界

候选23仍`flashable=false`，未发布/未验收；上一份已完成模拟交付仍为候选22（仅手动稳定槽功能）。软件保护只覆盖OTA服务成功预置后的窗口，不覆盖更早Boot2/启动故障；物理掉电/首启、传统OTA旁路和实际资源高水位仍未验。AP01无PowerPlan。
