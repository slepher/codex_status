# 状态

2026-10-23 完成，改动纳入本次提交；已部署 macOS Bridge。

共享注册首次默认 true，已有设备保留持久值；serde 缺省未改，无旧记录迁移。AP01 配对回执读取实际同步值，MCP说明同步更新，不发布Bundle。

验证：core/app/mcp 合计338通过、0失败、5忽略。新增首次默认/关闭后重注册/进程重开保留/再次开启保留回归；既有合同测试验证默认同步使数据进入待投递。首轮缺cstdarg，按已记录CXXFLAGS重跑通过。日志 `artifacts/default-data-sync-20261023/tests-retry.log`。

已停原托管实例，备份完整运行根和.app到私有 `artifacts/default-data-sync-20261023/backup/`；新建目录均提权。原配置保留，构建期间运行data逐文件哈希一致。构建/签名成功：79336800 B，SHA256 `d0f0948cafbe3c7a5fd70b9d49b3b0e4809115098e972bed38266119681d35b9`。原SHA `7ad7ee3049e26cc0ed209f8240cc8eed945a4bc68c4d3b0666dceeedf70d0b40`；部署详情见 `deployment.json`、`build.log`。

新服务supervisor25293/child25360，8765/8766监听。首次MCP连接被关闭，随后恢复；未证实最初中断原因。AP01认证成功且seq8已应用显示；Note4首次状态查询超时，随后自动会合恢复、seq1011已应用显示。两台sync_enabled均true，成功ACK和认证状态见 `delivery-acks.json`、`verified-runtime.json`。不以第一次超时判断永久离线。

本次未改ROM；AP01实际build ec0a58f71f65c2830c08c9bfd7c155db与前交接不同，来源/运行槽未核验。长期RF/睡眠恢复、Windows不在本次验收内。
