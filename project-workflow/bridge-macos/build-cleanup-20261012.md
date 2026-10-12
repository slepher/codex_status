# Bridge 历史构建清理（2026-10-12 10:17 +08:00）

用户授权清理历史 build，仅保留当前 build。

- 删除 31 份旧/候选 App、4 份历史独立 Bridge 二进制，以及 `bridge/target/debug`、`artifacts/bridge-macos/sim-target`、`artifacts/bridge-macos/target/debug`。按删除前 `du -sk` 统计，共 28.51 GiB。
- 保留当前 `artifacts/bridge-macos/Codex Status Bridge.app` 和 `artifacts/bridge-macos/target/release`（约 1.08 GiB，可复用构建缓存）。旧 App 回滚副本已按本次授权删除。
- 清理前后当前 App exe SHA256 为 `274ef25eec0761422bde9ed91662ecb7947fa8f6a24118973964283402bb101b`，签名核验通过；目录最终只剩当前一份 App。
- 删除目标递归检查无 `data` / `instances` 目录；未访问或删除 macOS 私有运行数据、安装身份及其备份，也未删除源码、固件产物或诊断证据。
- 曾有并发 debug 测试，等待其退出并取得 Cargo 缓存锁后才删除对应目录；清理不停止任何进程。
- 清理后磁盘可用约 58.53 GiB（之前约 29.34 GiB；系统同时有其他活动，差值不等同于精确释放量）。
- 清理后的另一任务正在重建 release（cargo90075/rustc90103），Bridge 当时已停止、health 无响应；本次未启动/重启 Bridge，保留其正在使用的 release 目录。

完整删除清单与空间核对：`artifacts/bridge-macos/build-cleanup-20261012.json`。
