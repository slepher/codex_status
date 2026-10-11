# Codex Status 当前交接

更新时间：2026-10-11 17:55（香港时间）

## 部署与工作区


- Note4 `7C4FADB93408` 已认证运行 **0.18.82-note4 / ota_0**，登记IP `.177`。ROM `artifacts/note4-0.18.82-battery-smoothing.bin`，1,926,256 B；SHA256 `B2C9E1C1FA8E8A01FE2CB66985875155E61B6F6ADF08F046E8B98C69D6E286E7`，同名匹配ELF。OTA `46986171` 单次上传、succeeded/image_verified。
- 默认Bridge通过计划任务部署，exe SHA256 `3F53CFEB953A883C1637280E7332357CC54CC44C35E989DA7551B0160E3C759D`。exe/data备份见 `artifacts/bridge-before-contract-fix-20261011*`。书桌屏未刷机。
- 基线提交 `f822531`；用户已授权BOOT/GATT、计划恢复、OTA与本轮电量平滑统一收尾提交。实际codex-status-a v4已发布，Bundle `53cab015` succeeded，认证applied_seq453/displayed。不要用本地夹具覆盖用户实际布局。

## 活跃事项与验收边界（B6/C8，关联A9）

- **BOOT/GATT、跨重启计划、OTA统一实现已部署**：[执行/测试/资源/剩余矩阵](project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md)。用户六次实体BOOT开关符合预期；手动图标独立于自动BLE。PPL1正式计划3490经一次明确USB工具复位仍同ID/restored_light，剩417/600秒，直接HTTP且自动BLE关闭。没有一次性OTA marker，报告不删除正式计划。
- core/app等241测试、render/MCP/device-sim126测试通过，3条件忽略；固件模块及匹配ELF66项栈门禁通过。数据合同变更后旧索引失效、发布ACK后认证回读已修正并配套部署。首次模板短暂内置界面的拒绝码未保留，不将源码缺口当唯一已证根因。
- **未结问题**：USB工具复位后启动/认证HTTP延迟、间歇超时，根因未知；被动串口对照未证明CDC因果，boot保持、模板已恢复。证据 `artifacts/note4-81-cdc-auth-meta.json`、`note4-81-final-auth.private.json`。不声称历史.78停滞已根治。
- 无USB Sleep重启BLE、电池深睡/Light到期竞态、Flash掉电、长期RF/功耗、动态堆峰值及macOS/1.54未验；B6/C8不结项。电量95/96/97抖动已查本地Slate和GitHub CrossPoint：前者单次平均/充电显示策略，后者整数EMA；电量EMA已随.82落地，见下条。

- **电量平滑已部署/用户验收通过**：[执行记录](project-workflow/bridge-macos/note4-battery-smoothing-20261011.md)。5秒采样、90/10整数EMA仅用于显示，原始电压/低电保护独立；RAM+16 B、BIN+464 B，无NVS/新库/新任务。12项定向回归和66项栈检查通过；20次实机观测原始95/96/97、采样约5秒，用户按键确认没有跳变。新boot `eb3cc78d6243e38e5feb871b29efcb77`，模板ready/displayed；证据 `artifacts/note4-82-ota-status.json`、`note4-82-battery-observation.json`。

## 入口

- C9单份内容/CSZ1压缩随.80继承至.81：[交接](project-workflow/single-copy-updates/status.md)。原生双OTA分区保留；AP01实机/加载器后续调查。
- 唯一待办：[backlog](docs/roadmap/backlog.md)；[当日历史](docs/history/progress-archive-2026-10-11.md)；[操作细则](docs/agent-operations.md)。
