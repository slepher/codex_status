# 当前字段配套部署

## Plan / Tasks
用户授权：提交后刷 Note4、发布模板、重启配套 Bridge。提交当前同步、绿灯、BOOT 网络及字段校验修复和协议文档；保留现场 data，构建配套 Bridge；核对 MAC/ROM marker/hash 后通过持久 OTA 任务升级；保存并显式发布保留原布局的 codex-status-a v5 完整 Bundle；认证核验运行镜像、模板及服务。

## Status
部署进行中。实现提交ea69ae7；完整Bridge构建发现MCP预览Env.state遗漏，f73ee6d改为当前独立字段。首次启动退出101，源码定位为PlatformService::open在旧模板CT不支持时重编源并传播错误，main的expect终止进程；修正为保留源供编辑，发布前明确验证源/CT并拒绝。新增恢复回归覆盖保留旧源、启动成功、发布拒绝、保存有效模板后可发布；平台服务55项通过。原程序与完整data已备份至artifacts/device-fields-deploy-20261011/。

固件使用已通过构建/栈检查的 device-fields-candidate，不重编、不改变版本文本；以完整哈希区分同版本镜像。仅 Note4，Fake 与书桌屏不部署。

## Review
先备份运行数据和 exe；停止核实的 watchdog、任务和残留主进程，保留 data。OTA 未知结果查询原任务，不换 ID 重发。部署成功不代表尚未完成的跨到期/自然 sleep 实机矩阵通过。
