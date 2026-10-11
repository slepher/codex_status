# 当前字段配套部署

## Plan / Tasks
用户授权：提交后刷 Note4、发布模板、重启配套 Bridge。提交当前同步、绿灯、BOOT 网络及字段校验修复和协议文档；保留现场 data，构建配套 Bridge；核对 MAC/ROM marker/hash 后通过持久 OTA 任务升级；保存并显式发布保留原布局的 codex-status-a v5 完整 Bundle；认证核验运行镜像、模板及服务。

## Status
部署进行中。实现提交ea69ae7；完整Bridge构建发现MCP预览Env.state遗漏，f73ee6d改为当前独立字段。首次启动退出101，源码定位为PlatformService::open在旧模板CT不支持时重编源并传播错误，main的expect终止进程；修正为保留源供编辑，发布前明确验证源/CT并拒绝。新增恢复回归覆盖保留旧源、启动成功、发布拒绝、保存有效模板后可发布；平台服务55项通过。原程序与完整data已备份至artifacts/device-fields-deploy-20261011/。

固件使用已通过构建/栈检查的 device-fields-candidate，不重编、不改变版本文本；以完整哈希区分同版本镜像。仅 Note4，Fake 与书桌屏不部署。

## Review

## 最终结果（22:09）
- 固件原任务3e72f00f单次上传、succeeded/image_verified。独立认证确认ota_0、1837936 B运行镜像SHA256 `32486c0763db240777758f549bce386f8884a09547ad3bc89f4423e499664517`。
- 原v5任务b1d0c9a4收到applied/displayed/flash，Bridge历史succeeded；认证committed_job一致、合同CRC eb628a93、content_error空、Data477。用户确认画面正常，Bridge OFF随后恢复ON。
- 最终Bridge PID68572/watchdog56788，8765/8766正常，SHA256 `5A0AF5FE4EE3CC7F3535F43450DC7D8306E85BB09C0FC788E8C57D8BE143F6AE`；Fake PID34328/18783未动。最终Bridge自身认证refresh成功。
- 新Bridge拒绝旧设备合同device.state，阻止OTA准入。本次临时恢复备份原Bridge处理原事务，完成v5与OTA后恢复最终exe；不增加旧字段兼容。
- 原Bundle在BLE网络准入时标Sending，Bridge重启将其变Unknown；认证仍为旧committed_job且无本次HTTP发送日志。用原job_id和冻结wire执行幂等BEGIN查询，按设备next_offset续传、commit成功，未换ID或修改本地任务结果。首次BEGIN因非原样MAC/session明确拒绝、无传输；使用设备原样MAC/current nonce后完成。通用调度恢复缺口留A9。
- 证据artifacts/device-fields-deploy-20261011：完整data-backup、bridge-before.exe、bridge-final.exe、构建/55项回归日志、ota-status/save/publish/resume-bundle/verify/refresh私有JSON。私有数据不提交。
先备份运行数据和 exe；停止核实的 watchdog、任务和残留主进程，保留 data。OTA 未知结果查询原任务，不换 ID 重发。部署成功不代表尚未完成的跨到期/自然 sleep 实机矩阵通过。
