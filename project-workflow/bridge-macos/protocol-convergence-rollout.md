# C2协议部署与观察

2026-10-19用户授权：先提交R3–R5，再重编译/部署Windows Bridge，经受鉴权直接OTA窗口升级Note4，等待自然交汇并观察。

## Plan / Task

1. 提交已验证源码及交接，不发布1.54/AP01。
2. 核对真实运行Bridge路径、端口/任务、登记MAC及操作token可用性；核对候选版本/marker/大小/SHA256及匹配ELF。
3. 按watchdog→计划任务→已核实残留顺序停止；冻结exe/data并逐文件哈希。保留data原位，当前源码重编译Bridge；核对新exe后用受管启动脚本启动。
4. 读实时恢复视图核对Note4 MAC/target；受鉴权/update与/doUpdate直接OTA只发送一次。上传结果未知先查询运行镜像，不重发。
5. 新Bridge认证读取核对目标MAC、版本、boot、运行镜像长度/SHA256、context/合同及历史未知Job；等待自然交汇，观察至少15分钟并记录多轮Plan/HTTP/Data、复位与错误。

## Status / Review

准备中。用户已授权部署与观察；C2无旧协议回退，升级间隙失配属预期。认证/owner/原事务围栏不放宽，不改NVS/分区或发布Bundle。回滚需配套旧exe/冻结data；ROM回滚另外核对协议。最终状态更新PROGRESS及本文件，私有原文留artifacts。
