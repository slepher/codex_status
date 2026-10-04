# Wi-Fi 变长存储与本地写入口收归（2026-10-04）

状态：源码提交f8426cb，唯一Note4 .55已按用户明确授权OTA部署，真实WFS3迁移与运行镜像已确认。起点94b7036；待办唯一来源backlog B6。

## 设计与冻结边界

- WFS3显式小端头：magic/version/总有效长度/网络数/回执数/CRC；字段为长度前缀，网络/回执为记录长度前缀。校验整个有效载荷及头元数据，拒绝未知版本、截断、尾随、数量/字段/业务上限与重复项。保留12网络、3绑定×4网络、8持久回执容量，满回执明拒且不驱逐未知ACK。
- 实际记录及字符串按需堆分配，nothrow且检查失败；网络标签/版本跟随记录，删除紧凑移动。只有一个长期仓库，事务候选在堆上；不压栈、不保留空槽。活动版本与已保存版本相同时引用网络；编辑/删除后仅保留连接所需旧SSID/凭据快照，不复制无关元数据。
- 一份NVS原子blob提交+完整读回比对；存储失败/未知ACK不得盲重写。启动读取现行WFZ2/raw-v2/raw-v1的有界堆兼容结构，验证后转实际记录；新格式提交成功才删除旧scheme键。兼容读取仍保留，删除另需证据。
- AP/USB旧无事务写入收归：停止旧sN/pN写入和GET清空，提供明确的认证本地事务入口，复用管理apply校验、修订冻结、回执与owner/合法窗口。未绑定救援不伪造bond或旁路授权，继续物理BOOT/BLE绑定流程。保留旧槽只读发现/迁移恢复。
- 不改30s会合/普通窗口权限、缓存读取、页面guard、密码选择、PowerPlan、分区或NVS配置；仅Note4目标，保留缓存，不开串口取日志。

## 步骤与验收

1. 格式/按需容器及旧格式转换，生产setup/management整合。
2. AP/USB入口经过共享事务；静态确认无旧网络写入，覆盖认证/占用/窗口/重试。
3. 生产C++格式迁移/损坏/长度/数量/掉电原子/读回未知/容量/回执/绑定版本测试；量化空、单条、典型两条、满12+8回执的存储与峰值堆成本。
4. 受影响Rust/device-sim测试；唯一zectrix-note4-b构建，匹配ELF启动/保存调用链栈门禁；核对marker、大小、SHA256。
5. 如部署，认证MAC/目标/版本与运行镜像，不改真实网络配置。更新PROGRESS/B6与本文件，区分软件故障注入和真实掉电/RF未验。

## WFS3 布局与恢复

头16B：`WFS3` 4B、版本u8=3、保留u8=0、网络数u8、回执数u8、总有效长度u32、CRC32 u32。CRC覆盖整个头+有效载荷，计算时CRC字段归零。所有整数显式小端；不保存原生struct/padding、尾部空槽或终止NUL。

载荷先存revision u64、activeVersion u64和active模式u8。模式0无活动项；模式1存bond/id并引用同版本网络；模式2另存旧SSID/password，且要求已保存版本不同或已删除。网络记录带u16有效长度，再存version u64与bond/id/host/ssid/password/label；回执记录同样带u16长度，包含revision u64、意图SHA256 32B、bridge/request/bond/error。每个文本先存u8字节长度，读取拒绝嵌入NUL、超长或截断；再核对UTF-8/SSID/密码、重复ID/映射、每bond4条/总3bond、版本与回执归属上限。新key `wifi-bonds/store3` 存在却损坏时明确失败，不回落旧key复活配置。

写入只调用NVS单blob事务，完成后逐字节读回确认；RAM候选此后才move成为当前仓库。ESP-IDF可能已提交新blob index、却在退役旧chunk时返回错误，因此**任何已尝试putBytes都按未确认处理**，后续命令先重新加载认证CRC格式，不用旧RAM修订覆盖新结果。原事务result可确认成功/拒绝；协议仍不会盲重发未知事务。

WFZ2/raw-v2/raw-v1兼容布局隔离在wifi_legacy.h，只在启动迁移期间按需堆分配；不再生成压缩新写。旧scheme仅在新blob读回成功，或后续启动独立校验新blob成功后remove；remove失败保留pending并可重试。旧sN/pN仍仅有只读发现/物理迁移及显式工厂重置路径，未删除其恢复代码。兼容代码真正移除须后续独立实机验收，不混同本轮入口退役。

## AP/USB 行为

- AP `/save` 复用正常HTTP认证apply；同时提供 `/api/wifi/apply` 和 `/api/wifi/result`。原SSID/密码裸表单、自动覆盖slot0、保存即重启和GET清空均退役；`/clear`返回410。恢复页引导Bridge/BOOT绑定，并提供折叠的认证事务入口。
- USB `wifi <ssid> <pass>`只返回退役提示；`wifi-json <JSON>`验证现有endpoint token，支持apply/result，随后与HTTP共用MAC/owner检查及Wi-Fi管理模块的bond permit、合法窗口、冻结expected_revision、1–4条原子校验和持久回执。不推断或伪造bond，不清所有网络，不保存后自动试连/启用/重启。
- 串口输入上限8204B、正文8192B；超长整条拒绝，不截断执行；无凭据/token回显，输入和正文工作串使用后清零。source audit + 整板编译证明原sN/pN凭据写入者已消失；旧last提示仍只更新旧槽使用顺序。真实AP/USB写入未执行，不能将此审计称为实机入口验收。

## 资源证据

Xtensa同一14.2工具链：WifiText8B、网络56B、回执72B、仓库120B，非平凡record数组cookie为8B（allocation probe显示n×56+8）；文本分配实际字节+1，空字段不分配。唯一长期仓库，不另存WifiSchemes全量副本；事务候选只包含实际记录，活动未编辑版本不重复密码。保存/激活将已验证网络工作集move到候选，不再复制一次完整网络集。8项回执只分配实际数量；意图hash/归属/结果/修订用于重试幂等，不驱逐未确认回执，满账本明拒。

| 合成使用量（网络/回执） | 格式有效字节 | 干净NVS记录成本 | 仓库常驻RAM | 克隆+提交峰值RAM |
|---|---:|---:|---:|---:|
| 空0/0 | 33 | 128 | 120 | 306 |
| 单条1/1 | 225 | 320 | 433 | 1,316 |
| 典型两条2/2 | 387 | 480 | 670 | 2,114 |
| 满12/8，所有字段上限 | 5,036 | 5,152 | 5,986 | 22,044 |
| 满容量且活动凭据保留旧版本 | 5,134 | 5,248 | 6,084 | 22,436 |

空仓库未写key时实际持久化0B；33B是显式保存空状态的格式成本。常驻RAM含仓库120B+实际record数组+字符串+数组cookie；峰值包含旧仓库、一个堆候选、编码和读回两个恰好B字节缓冲。宿主C++new计数窗口实测克隆/提交额外峰值266/1043/1684/16970B；按已验证Xtensa record/指针尺寸换算表中目标成本，**这是存储组件请求量，非整机实测峰值**。allocator块头/对齐、现场NVS/RF驱动与JSON/String的开销另计；传输正文至多8192B，USB累积/整行/正文可各保留一份，认证请求及完整快照JSON亦为有界按需工作集，未冒充零成本。

NVS记录成本来自当前IDF源码：32B entry，单chunk最多4000B，每chunk header及blob index各一entry，数据向32B取整。表按干净布局计算；namespace还需一entry、每页管理头、碎片、旧值直到原子提交完成以及GC备用页另占空间。因此12条是协议容量，不保证同NVS其他仓库已占满时写入成功；实际NVS不足返回storage_unknown并查询确认，不裁剪条目。受限byte-limit已验证短写保留旧仓库。

旧格式压缩读取的暂态预算：旧输入≤5312B、兼容结构5248B、ROM tinfl堆工作区10992B；workspace释放后再转换实际条目，最大输入/解压阶段约21792B（含两个120B仓库对象，另计allocator/驱动）。该成本只属于迁移，不是WFS3常驻内存；全量旧结构从不压栈。整板静态RAM由.54的125412降至117876B（-7536B），不包含上述动态工作集。

最终匹配ELF栈帧：启动192B、读取240B、提交64B、poll144B、setup/management各624B；全部≤1024B。gate同时审查Wi-Fi/Preferences/NVS可见局部调用链，最高2448B≤4096B，为调用者/平台与间接ROM调用保留另4KiB；不称为所有ISR/平台栈的完整测量。原.50应继续red，候选green。不同使用量不扩大帧，条目/凭据均在堆上。

## 验收与候选

- 生产setup/management/storage三套ASan+UBSan通过；逐字节翻转、每个截断位置、有效CRC下未知版本/保留位/字段/记录长度/数量/尾随、重复项、3bond×4/总12/回执8、活动编辑/删除旧凭据恢复通过。
- 旧v1/raw-v2/WFZ2迁移、写失败/读回失败/退役失败重试、旧键验证后remove、原子旧或新提交边界、140个分配位置探针实际命中36个失败位置（其余为超出本事务分配次数的对照）、commit后返回错误且原事务result确认、NVS有限空间均通过。故障注入不等于真实切电/Flash寿命验收。
- 最终Rust受影响过滤回归：core12、app8、device-sim5通过；真实生产HAL的snapshot→apply→result、丢ACK/重启不新增完整配置读取。保留30s/合法窗口/owner与绑定/页面guard边界。旧恢复夹具另通过。
- 仅构建zectrix-note4-b，无并行pio，无clean/删缓存/改分区；最终增量构建37.56s。git diff --check通过。Bridge HTML未改，无macOS Wi-Fi UI变更或新的WK模拟验收声称。

最终候选0.18.55-note4-b-wifi-store3，target zectrix-note4-400x300，1735744B；ROM `artifacts/bridge-macos/note4-0.18.55-wifi-store3.bin`，SHA256 `3A552561A88B5572FD901FD6ACF2D65E75BD47BE9AF337262CD43065247642FF`，匹配ELF同名。marker、注册MAC及public/authenticated身份已核对。

部署状态：用户明确授权提交、OTA与部署后，OTA bfd4e115 succeeded/image_verified。设备token认证独立读取运行镜像，MAC7C4FADB93408/fw .55/target/1735744B/hash3A552561…642FF全部匹配，ota_0；已登记相同hash为该target最新release。真实旧WFZ2迁移后认证wifi_storage为WFS3/ready=true/uncertain=false/249B/2网络/0回执/old_key_present=false，业务RAM460B（另加数组cookie8B）。后续不同boot_id的深睡唤醒reset_reason=8仍加载249B/2条新仓库并联网；末次heap_free40872/heap_min35200B仅为短时现场，不代表整机峰值或长期矩阵。

Bridge健康204、exe hash23DFF946…D4D87未变，本轮生产Bridge无改动无需重建。与部署前备份比较targets/snapshot/baseline/local_revision全相同；未再读取完整配置、改真实网络、开串口、清NVS、改分区/TCC/钥匙串。私有data和installation-id备份：`/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-store3-authorized-20261004-190915`，0600/0700；旧.54 ROM保留。此前auto-review未明确授权的拒绝是历史状态，现已明确授权并完成。

证据：wifi-store3-rom-manifest.json、wifi-store3-deployment-preflight.json、wifi-store3-current-runtime.json、wifi-store3-resource-budget.json、wifi-store3-layout.s、wifi-store3-stack.log、wifi-store3-stack-regression-red.log、wifi-store3-entrypoint-audit.json、wifi-store3-cpp-final.log、wifi-storage-tests.log、wifi-store3-recovery.log、wifi-store3-rust-final.log、wifi-store3-pio-release.log，均在artifacts/bridge-macos。

认证wifi_storage的ram_requested_bytes报告业务record/字段请求量，不含数组cookie，资源表已另计cookie。

真实WFZ2→WFS3迁移、后续启动读取与运行镜像已验；真实AP/USB写入、RF/错误密码/连续睡醒/断电、系统权限与其他硬件/跨平台矩阵、兼容代码最终退役仍归B6，不结项。

部署证据另见wifi-store3-{authorized-preflight,ota-queue,ota-status,deployed-status,running-image,release-published,current-runtime}.json。
