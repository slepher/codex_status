# AP01 unified build status — 2026-10-27

完成：平台源码清单、编译器发现、RV32 flags、XIP 节审计、预算提取、ET_REL 合并、最终链接和 ROM 打包均由固件仓库维护。App 的 check/link 脚本只剩薄转发；build-ap01-rom.py 保留原 import 接口并共用固件工具链发现。App 只增加 ports/ap01/build-0041.json 源码配置，不改业务实现。

验证：独立版、App 版从源码完整构建并通过 ROM 重建/CRC/篡改拒绝检查；App 旧 audit→link→package 命令及 screen-host-probe 版完整通过；源码职责边界检查通过；3 项配置/源文件/对象过期拒绝回归通过；无相邻仓库的新副本独立构建通过，OTA 8 项中通过 7 项，1 项因未安装可选 Unicorn 跳过。

App 版保留既有 offsetof/ArduinoJson deprecated 编译警告。源文件顺序统一后链接器选择的 COMDAT 内联实例变化，App ROM 大小由 7,045,480 B 变为 7,044,408 B；固件/App 业务源码未因此改动。

离线候选，目标 AP01 1.0.2_0041，flashable=false；没有刷机、改变现场部署、启动 Bridge 。后续用户已于 2026-10-27 明确授权提交两仓配套修改；未推送。

- platform: `D:\Documents\PlatformIO\Projects\cuktech-ap01-firmware\build\unified-0041\platform.bin`，6875656 B，SHA256 `d77900bf5ce60ac9d1139a37d7a7314e75f2a6ac1f021eb7f04bfd9b440e2859`。
- codex-status: `D:\Documents\PlatformIO\Projects\cuktech-ap01-firmware\build\unified-0041\codex-status-commit.bin`，7044408 B，SHA256 `7cf24650ed8b24323e42b1b4a5400f5b05aa0d0f52c81127c2271c2c7d4e4183`。

日志/证据：兄弟固件仓库 `build/unified-0041/` 的 manifest/audit/link；`legacy/0.log` 至 `3.log`；`isolated/independence-report.json`。

提交前其他任务更新了两个非 AP01 头文件，使先前全目录头文件审计过期；已对当前快照重新完整构建并通过候选检查，最新 App 产物为上述 codex-status-commit.bin。其他任务源码不在本次提交范围。
