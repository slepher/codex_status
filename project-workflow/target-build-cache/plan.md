# 1.54 增量构建缓存复核（2026-09-28）

前置条件已满足：Note4 与新版 Bridge 已部署，新版 Bridge 再次 OTA 有上传 ACK 和设备换槽证据。待办唯一入口为 `docs/roadmap/backlog.md` C6；本记录只写实验步骤与证据。

## 已见现场

- 当前 `.pio/build/` 仅有 `zectrix-note4-b`；`esp32-s3-epaper-154g` 的对象文件与 `firmware.bin` 均不存在，所以该目标的下一次构建无法证明“零编译”。
- `.pio-core/sdkconfig.defaults.154g.snapshot` 存在，`platformio.ini` 已在 2026-09-28 11:24 改动，`.pio/build/project.checksum` 最后写于 20:10。不能仅凭快照判断 SCons 缓存仍在。
- 只允许 `tools/pio-target.ps1 -Target 154g`，且与 Note4 构建串行；不清理现有 Note4 产物或包目录。

## 步骤和判据

1. 记录当前 `project.checksum`、Note4 ROM 哈希、两个 sdkconfig 快照指纹和 `.pio/build` 目录；查本轮 1.54 构建输出/删除原因。若构建目录已丢失，只将首次完整应用编译记为缓存重建，不误判成目标切换必然重编。
2. 用隔离脚本单独构建 1.54 标准 B/W 目标，保存日志、耗时、编译单元数、是否出现 `*** Reinstall ***`/IDF 库重编、ROM marker/大小/SHA256；首次缺缓存时可接受全量应用编译，但不能触发无因 framework 重装。
3. 原样再跑一次 1.54，应为零编译且 ROM 哈希相同；如不满足，定位并修复脚本或输入失效源。再按 Note4→1.54 串行切换核对两侧缓存与哈希，无须为验证而清理目录。
4. 更新 `PROGRESS.md` 现场、backlog C6 与本记录结果。最终 1.54 ROM 准备妥后才按升级计划进入其 OTA 窗口；任何实机升级仍核对 MAC、token、marker、大小、哈希和设备自报身份。

## 实测结果

| 构建 | 耗时 | 对象重编 | framework 重装 / IDF 库重编 | ROM SHA256 |
|---|---:|---:|---|---|
| 154g 缺缓存首次 | 138.47s | 318 | 0 / 0 | `108E82C8…5B62FD` |
| 154g 原样复建 | 20.16s | 0 | 0 / 0 | 同上 |
| 切回 Note4 | 20.90s | 0 | 0 / 0 | `9E8A6C18…94E62EA` |
| 再切 154g | 19.94s | 0 | 0 / 0 | `108E82C8…5B62FD` |

判定：目标切换的隔离脚本和整份 sdkconfig 快照可保持缓存，未见无因全编。先前那次需要广泛重编的直接条件是 1.54 构建目录已不存在；PlatformIO 的全项目 `project.checksum` 包含源码文件清单，清单变化时 `clean_build_dir` 删除整个 `.pio/build`。这是项目结构变化后的缓存失效，不宜通过伪造 checksum 或跳过清理规避。四次日志在 ignored `artifacts/rollout-20260928/`，最终 ROM 的完整路径/大小/哈希和实机升级证据见 `PROGRESS.md` 顶节。
