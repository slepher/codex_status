# 会合协议配套部署与读取缺陷恢复

2026-10-04，用户授权“启动bridge并开始部署”，随后报告e263e08c8ac205b5读取snapshot_shape终止。

1. 已核对登记Note4 7C4FADB93408 / 192.168.10.101 / zectrix-note4-400x300；备份旧app和私有运行数据，打包已测试Bridge候选并启动默认LaunchAgent；health204/MCP200。
2. 已按MAC排队OTA .46；作业1df2d89b succeeded/image_verified，设备认证镜像SHA256等于9BE3591D…A756D，ota_0。未改分区、串口或网络配置。
3. 真实完整读取暴露双legacy槽投影缺陷：projectWifiNetwork把局部字符数组借给ArduinoJson，循环末尾两个槽都变为legacy-1；严格快照验证拒绝重复身份。保存非秘密完整分页响应，复用生产Wi-Fi HAL增加两个旧槽用例取得red；改字符串复制，green后构建Note4 .47并按MAC OTA。保留.46及匹配ELF；不放宽Bridge校验/占用/认证。
4. 部署后经生产入口重新排队完整读取，核验任务completed、两旧槽身份和完整快照；验证真实会合HTTP实证/handoff/BLE关闭/PM状态，更新PROGRESS与backlog。新协议仍需连续RF/功耗与故障矩阵，不以单次部署代替长期验收。

证据目录：artifacts/bridge-macos/rendezvous-deploy（无密码/token）。私有数据备份在用户Application Support目录，未落仓库。

## 用户追加：显式迁移旧 Wi-Fi 槽（不静默变成 legacy）

5. 设备新增显式 wifi_migrate；必须当前加密绑定、endpoint 认证、匹配 owner 与未过期物理维护授权。main 仅负责读旧 wifi NVS/退役键；wifi_setup 管理验证、整批原子 v2 提交及读回。凭据仅设备 RAM/NVS，不经过 Bridge。稳定 ID wifi-slot-0/1/2，归当前 BLE identity，label 标明迁移槽；host_ssid 暂沿旧 SSID，冲突整批拒绝、不造映射。
6. 保留已有 active；没有 active 时使用当前连接 SSID（未连接才用旧 last），迁移不切网。重试仅接受内容完全一致的同 bond/ID；读回成功后逐槽移除旧键，任何失败保留可恢复记录。旧键全移除才返回 retired=true，丢 ACK 后可重复显式迁移查验。
7. Bridge 增加 MCP platform_wifi_action op=migrate；释放两锁再有界 close，错误有限化。生产 HAL 覆盖双槽/凭据/当前选择/重试/冲突/存储失败/权限/不切网，Note4 构建、配套部署；现场完成迁移后新完整读取验证正常记录无 legacy。物理 BOOT 授权仍是写入条件，准备完成后若不足须用户短按 BOOT。

## 存储失败后续

.47实机两次storage_unknown，旧键未删；.48诊断已部署并核对镜像。已有NVS页备份显示3页近满/1页擦除，未取得新维护窗口验证写入阶段。生产HAL加1KiB可写空间限制，旧固定结构写入出现red；改为现有ESP ROM miniz的zlib封装压缩，同一NVS scheme key原子提交、解压/CRC/字节读回核验，读取继续兼容原v1/v2 blob。没有删除NVS其他命名空间、没有改分区或把凭据搬到文件系统。真实物理迁移须新BOOT窗口并核验结果；AP/USB旧格式写入收归与后续兼容代码移除仍归B6，不混为迁移已完成。

.49构建63.29s成功，1,730,464B / SHA25636057EAEB427D912FCD8C69A68951B48FF2139F6F5604EF905FBA3E45B480A76，ROM/ELF已保留；OTA13c4c235 succeeded/image_verified，真实迁移等待用户再开BOOT窗口。

.49用户重开BOOT后，真实迁移storage_encode（bytes=0，未进入NVS写入），说明ROM便捷压缩入口不可用；旧键保留。改底层tdefl/tinfl显式heap workspace，大压缩字典优先PSRAM、避免loop栈；每次释放前擦除工作区。不能将此结果称为NVS空间因果已证实。准备.50再核验。

## .50迁移后重启循环：USB诊断与恢复计划

用户报告.50不停重启并接入USB。真实串口连续两次loopTask stack canary，匹配.50 ELF回溯setup→wifiSchemesBegin→readWifiStore→Preferences::getBytesLength；尚未进入解压。Xtensa entry实测wifiSchemesBegin=0x14c0(5312B)、readWifiStore=0x14e0(5344B)，两者超过8192B loopTask栈。原因是空存储/旧v1分支的WifiStoreV2{}赋值临时对象，无论分支是否执行，函数入口均预留整个栈帧。之前迁移ACK成功仅证明存储提交读回与退役，不能证明重启稳定；此前把离线归为正常休眠的判断错误。

1. 用.50实际ELF建立启动读取栈帧门禁red，各函数≤1024B。
2. 仅将大型默认对象初始化改为原地构造，保留默认字段及旧格式读取；复测生产HAL/受影响device-sim。
3. 构建Note4 .51，核验ROM大小/标识/SHA与栈帧green。
4. USB已核对MAC并只读备份16KiB NVS至私有Application Support/deployment-backups/reboot50-rescue；已停留ROM下载模式，停止panic循环。读取dump/保留证据，随后只替换活动应用，不改分区/不清NVS/不退回无法解码新配置的旧ROM。
5. 真实冷启动与重复复位，核验不panic、联网、认证镜像、完整方案读取revision1/两个正常槽/无legacy；旧失败作业保留。

恢复结果：.51构建41.79s，真实栈门禁48/80B通过，Wi-Fi HAL及device-sim5通过；USB仅写ota_0，完整ROM读回等于候选，认证镜像SHA256 A7AB14F7…C2B3D7A / 1730672B，当前发布登记已更新。两个USB复位观察均无panic并联网。真实完整读5a55a29e9a41542a一次completed，revision1/两个正常槽/无legacy/active wifi-slot-1；旧键启动NOT_FOUND。真实dump离线解析成功，调用链与串口一致；NVS/dump/完整报告仅在私有目录。

用户进一步要求仓库按实际用量存储、以头表达长度，并将嵌入式资源设计约束写入AGENTS.md；指导已写入。变长持久化/RAM按需结构改造单列B6待办，尚未实现，不把压缩落盘或本次栈修复算作该目标完成。
