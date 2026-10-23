# 验收边界

保留旧记录及 serde 缺省语义；无批量迁移。无设备契约不得发送空字段包；保留 claim/owner 校验。构建包含工作区已有提交，保留原有文档改动。

验证完成：默认值仅改 device_upsert 的新记录分支，已有记录（包括缺字段旧记录的false缺省）继续保留；设备契约/claim/owner等准入未改。原有PROGRESS及历史清理修改保留在归档。338测试通过，git diff --check通过。
