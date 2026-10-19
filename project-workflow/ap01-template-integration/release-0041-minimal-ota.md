# 0041：取消OTA版本布局绑定

2026-10-19，用户要求独立OTA仅保留请求、真实槽容量及完整性等实际刷写所需检查。先前候选24及设备0031接收器均绑定型号字符串位置；本轮统一移除，不通过原厂OTA绕过问题。

## 变更

0031与0041使用逐字节一致的镜像验证器，移除型号/固定偏移、BFNP/FCFG/PCFG内容限定、参考板型/Flash/时钟/安全模式比对、固定入口/代码跨度，以及固定原厂ROM长度下限。删除参考头参数和型号缓冲。最小296 B仅为256 B头和40 B恢复尾的完整性编码边界，最大仍为真实7 MiB槽容量。

保留请求鉴权/超时/接收长度/并发及原事务围栏、头CRC、恢复尾标记/声明总长度/全文件CRC、传输CRC与原厂写入后读回校验。0041自有App manifest只作为可选回执元数据，无manifest也可上传提交；不改变请求身份、稳定槽保护或业务协议。接受上传不代表启动兼容性已得到保证。

## 构建与验证

- 0031过渡接收器`ap01-http/tail3-minimal`，6822072 B，SHA256 `e34c7b3a4bbc6f582d31a56ae2b2d587748779868ff7df171d4f89bd9e3a634f`，build `ef8319734dcaa8e63d5151bfd512a69a`。原启动头/GIF/PEC/传统入口保持；静态启动帧80 B。
- 完整0041候选25，6994424 B，SHA256 `289571d43f74dee776fdfd6f6eec2cbff99b5f1d06e7616900a62a557a6b8edb`，App build `c8ff8f02346f47b8bbe3e1d7941f6cee`。正式文件`artifacts/ap01-platform-integration/device/ap01-0041-codex-status-minimal-ota-20261019.bin`。
- 两个验证器各29项ASan/UBSan检查：0031原件/0041原件/完整ROM、不同型号位置/配置/入口/跨度均可接受；296 B到7 MiB边界通过，损坏头/正文CRC、错误恢复尾/声明长度、截断/超量仍拒绝。维护服务宿主额外证明无App manifest及最小完整性镜像实际完成写入/提交。52项请求保护、15项旧维护诊断通过。
- 0031 guest原生socket模型启动观察通过；早先host代理不支持任务分组而连接重置/BlockingIOError的失败报告保留，不冒充HTTP通过。实机维护健康独立核验。
- 原生64–68全部通过：全包OTA、9016ms健康锁回、手动回切重放及原槽恢复、同NOR窗口中断回稳定槽恢复业务。16组活动LVGL缓冲零像素差异。预算服务112 B、单事务488 B（较候选24分别减少8/16 B），持久上限不变。

严格放行命令含`--candidate 25 --fresh 64 --update 64 --switch 65 --recovery 66 --trial-cut 67 --fallback 68 --image-admission-report artifacts/ap01-platform-integration/ota-minimal-20261019/image-tests.json`。首次扩展放行工具发生变量遮蔽，误将3371 B校验器源码拷为输出，发送前大小/哈希核对已发现并隔离为invalid-source-copy，未发送该无效文件。修正变量并增加源与目标大小/哈希双重断言后重新放行，正式ROM与候选25逐字节一致。

## 实机

用户本轮已授权刷机。精确设备DID2232184842/MAC34F01557210B，旧接收器`ap01-http/tail2`、槽1；旧验证器离线确认能接受过渡0031包。单次更新过渡接收器后，认证health/build/运行槽/双表证明已从槽2运行tail3-minimal（物理age9）。随后独立OTA单次安装0041到槽1，保留槽2；当前最终运行观察见[刷写记录](flash-0041-20261019.md)及PROGRESS。

完整证据私有保存在`artifacts/ap01-platform-integration/ota-minimal-20261019/`及`physical-0041-20261019/`，任务持久防重发，不提交凭据/ROM/dump。Bridge未部署，未操作Note4。软件安全窗口仍仅覆盖成功预置后的阶段；本轮升级前没有APS1稳定确认记录，不冒充首次0041启动已获得自动保护。物理LCD/高水位及掉电仍需分别核验。
