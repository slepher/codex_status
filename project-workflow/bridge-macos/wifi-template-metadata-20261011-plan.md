# Wi-Fi 模板元数据修正

## Plan / 合同

用户确认：尚未联网不显示；真正关联时交替闪烁；已联网常亮；OFF只作终态。固件提供事实，模板决定图形。增加本地绑定 `device.wifi_state`（idle/connecting/connected/off）及 `device.wifi_blink`（仅connecting时存在，0/1）。不将等待BLE/等待开网当作关联，不以数据成功推断Wi-Fi成功。现有device.state保留兼容，修正其OFF兜底。阶段变化和相位变化均进入显示失效跟踪。

## Task

- [x] 共享引擎、Rust编译器、宿主预览和模板契约支持元数据。
- [x] 关联生命周期与重绘接线；等待无图标，关联闪烁，终态才OFF。
- [x] 更新库中Note4/quad模板，补状态和逐像素回归。
- [x] 定向测试、Note4候选构建和栈门禁，记录哈希及部署边界。

## Status

源码和模板已修，最终Note4 .75候选构建及52项匹配ELF栈检查通过。2026-10-11 12:26认证核验设备已运行.75/ota_1；保存模板不等于发布，活动Bundle仍为旧模板兼容路径。另两项诊断HTTP准入/ready竞态留在A9/B6原审核，不将本次显示修正冒称协议全部修复。

## Review

新增绑定枚举只追加，不重排旧CompiledTemplate编号；无新持久仓库或NVS键，TPL环境按需字符串和小量标量。模板和宿主/固件共用同一引擎。新增字段必须为设备本地字段，不能进入Bridge数据合同；验证未知bind整份拒绝。构建不清缓存，运行data不动，未请求提交。

- 共享引擎57 tests通过（包含模拟器/序列化/状态机）；core编译器7 tests、模板协议8 tests通过。生产C++元数据分类/重绘重试测试、Python固件接线守卫通过。
- quad v13 CRC `666c6404`；Note4 codex-status-a v3 CRC `ffa2bdbd`。Python沿用测试桥实际tpl_canon，Rust canonical与C++产物CRC同验；追加本地字段不影响Bridge数据合同。
- 预览：`artifacts/wifi-metadata-{idle,connecting,connected,off}.png`已目视审阅，JSON/compiled及持久记录往返均0像素差。新Note4 compiled记录11333B；没有改运行库或发布Bundle。旧模板兼容路径也不再以OFF作为连接暗相位。
- 有意保留事务失败回执；新连接阶段只消费新的失败事件，成功清当前终态，避免旧回执在恢复后再次污染显示。深睡最终帧投影为已决定关网的off，timer联系仍可进入idle；不会据数据ACK、读状态或闪烁修改PowerPlan。
- 实机未验：活动Light会合中的EPD刷新耗时、真实关联/DHCP/失败/恢复/深睡连续多轮与实体闪烁节奏。宿主像素和ELF不替代上述验收。

## 最终候选与资源

- `artifacts/note4-0.18.75-wifi-metadata.bin`，1,903,824 B，SHA256 `C0C5122FD5185B32665CA375964793FB6FCDD319576CA5AC216BA7FB1BB7E79D`；匹配ELF同名前缀，marker为`zectrix-note4-400x300|0.18.75-note4`。已按用户要求OTA部署，Bridge未重启，模板未发布，未提交git。
- 静态RAM117,964 B（相对.74 +16）；Flash1,852,479 B（+1,248）。持久/NVS新增0 B，空/单条/典型/上限配置均不变；没有新增仓库容量或凭据副本。
- 最终真实栈帧：renderActiveUsage400 B（旧384），application::tick240 B（不变），serviceNetworkGlyph32 B，wifiPhase96 B，serviceWifiMetadata96 B。新增局部模板String只存最长10字符的阶段，动态堆峰值未实测；52项既有Wi-Fi单帧/局部调用链门禁通过，不将门禁视为整机峰值证明。
- 构建/测试证据：`artifacts/wifi-metadata-075-build.log`、`wifi-metadata-075-stack.log`、`wifi-metadata-render-tests.log`。后续重编必须重新登记ROM哈希。

## OTA部署验收（2026-10-11）

- 原request `note4-wifi-metadata-75-20261011`、job `62b50171`，attempt_count=1、upload_ack=true、succeeded/image_verified，没有换ID重传。
- 12:26:29直接认证MAC `7C4FADB93408`、版本`.75-note4`、槽位`ota_1`；按1,903,824 B读取运行镜像SHA256，与上述发布ROM一致。radio.wifi_state=connected，wifi_blink=null；boot_id `116215f620e6b4d1eb142fb5a1cc316a`。
- 证据：`artifacts/ota75-queue.json`、`ota75-completed.json`、`ota75-running-image.json`，私有认证现场`ota75-before.private.json`/`ota75-after.private.json`。未操作USB，未发布新Bundle；实体闪烁与多轮状态验收仍待完成。
