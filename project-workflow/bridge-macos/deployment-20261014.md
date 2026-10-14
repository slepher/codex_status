# Note4 稳定 MAC 修复部署（2026-10-14）

## Plan / task

用户授权刷入修复，准备完成后通知用户按 BOOT 开启 Wi-Fi，得到就绪回复再直接原生 OTA。目标仅 Note4 `7C4FADB93408`，不经过 Bridge OTA 队列。

- [x] 构建 `.100-note4`：仅稳定 MAC 修复及版本标识，核对 marker/大小/SHA256/匹配 ELF 栈检查。
- [x] 用户 BOOT 就绪后核对实时 MAC/认证身份，原生 `/doUpdate` 上传一次；未知结果先读回，不盲目重发。
- [x] 认证核验版本、运行镜像 SHA256；按托管流程清除旧 Bridge 进程 BLE 隔离并验收恢复。
- [x] 更新交接及部署证据，保留旧 `.99` 本机归档；不提交 Git，不改 AP01 工作。

## Status / review

Note4构建42.40秒成功，RAM119948 B、Flash1788019 B，生产 endpoint 8项及 BLE-first 回归再次通过，匹配ELF栈/旧入口64项检查通过。BIN `artifacts/bridge-macos/note4-0.18.100-stable-mac.bin`，1,839,920 B，SHA256 `54137baf377e8b081750154bb27e7658d065c14d4b3c110f0a71c611244e1884`。marker为Note4/.100；匹配ELF/map及摘要在同前缀JSON，构建/栈日志在同目录。

已有 `.99` BIN/ELF 和 Bridge 私有存档可复用，无需导出设备配置或接 USB。只修 ACK MAC 来源，不变更存储/鉴权/电源协议。部署前仍运行 `.99`，Bridge17:32版本因BLE清理超时处于进程内隔离状态。

## Deployment / review

用户回复“已就绪”后直接 `/status.json` 核对MAC，再用生产客户端认证status与旧ROM镜像哈希，确认 `.99/ota_1`。核对原App路径/监听者/hash后暂停托管服务。10:38:01开始原生 `/doUpdate`，设备token仅在进程内使用，一次上传返回HTTP200/UPDATE OK；未经过Bridge队列，未写NVS或配网记录。`.100`写入ota_0，保留`.99`ota_1。

恢复原Bridge后，10:38:51 BLE INFO / endpoint成功，随后status、claim、plan有ACK，证明已跨过此前端点回执故障。sync_open等待ACK超时，network_start结果未知，disconnect又在2001ms后超时。后续HTTP成功，证明网络启动已生效；不能因会合最终complete而认为BLE清理安全。

第一次并发核验处于网络切换期未通过auth_status断言；仅重试只读核验，未重传ROM。之后认证status通过、`.100/ota_0`运行SHA256与冻结BIN完全一致。读回boot_id `a7a3e58a75178ab9a3ec0265aeb3d412`、reset_reason8；历史seq1为boot/no_connection，seq2为button/ext1，不能冒充timer验收。

10:40在已确认HTTP在线后再次按托管流程重启原App，清除此次清理引起的隔离；supervisor68397、health204。数据578已HTTP ACK applied/displayed，最终认证data_seq/applied_seq均578。原App二进制/配置保持，未额外发布模板或创建Wi-Fi任务。

证据：同目录 `note4-0.18.100-{preflight,upload,verified,live}.json`。刷入和当前连接/数据恢复完成，**Bridge清理超时→永久隔离及缺明确提示仍待修，不能承诺不再断连**。后续需修复Bridge恢复闭环并完成自然定时深睡醒次矩阵，B6/C8不结项。
