# 状态
2026-10-31：源码及App回归完成，Windows已部署，短时实体被动观察通过。

快区间Light主导3.143h净降5个百分点（1.591/h），Sleep主导3.200h净降2（0.625/h），约2.55:1。保留短间隔和反弹；后段ADC整数读数不支持稳定比率，不当作实际电流比。

源码仅bridge/crates/app/src/main.rs、platform.rs：owner未采样/同boot租约保留；HTTP同已确认Plan抑制且回执明示未发送。132通过/3忽略；无ROM/新依赖/持久格式变更。

部署日志/备份：artifacts/note4-communication-fix-deploy-20261031/；待核对运行及实际请求。

03:28标准任务启动PID48712/8765+8766，health204；347文件data逐项SHA校验，3设备identity/Profile/sync、Wi-Fi库及4配置/凭据文件保持。旧EXE cead2571f17c3010a97d6154d2464386977f1813c62551201584705d5c5d75f1，新EXE f94cc00011708a925028d830ba53cd4be1280130bb21a6d624623381064d1e62 / 39285248 B。新版本无持久格式变更。

## 实机观察（03:28–03:30）

同MAC认证`.107-note4`/boot c74d7ad5fdc0bf012f635bef161dcf02、context 1e4157825f25db5d、Data1352、Plan9095保持；5次被动本地采样里设备认证剩余260→230→200→170秒，未续期限。新进程启动到03:30:11日志中该MAC没有HTTP PowerPlan send；现场状态持续更新，证明不是整个HTTP调度停掉。

成功claim为03:28:51、03:30:07，间隔约76秒，已不随30秒状态刷新抹掉owner重claim。两次成功前分别存在一次未完成/超时的发送及后续尝试（03:28:39→49、03:30:01→05）；此次修复不宣称消除传输错误或所有重试。成功后的下一次正常续租仍遵守60秒到期判定及调度/传输耗时。

证据：同部署目录verification.json、passive-samples.json、runtime-events.log；4个配置/凭据文件SHA与冻结备份一致。未刷ROM、未提交；Mac原生与长期功耗比未验。`git diff --check`通过。
