# 周期诊断激活修复计划

授权：2026-10-27用户要求修复，测试通过后重建并重启默认Windows Bridge。

正式PowerPlan只依实际Data变化、待办、显式连接/Light和物理窗口决定；HTTP需求用于选择传输，不能自行授予正式Light。删除共享计划入口中混用HTTP需求的布尔参数，诊断复用现有SyncOpen临时窗口。保持真实Wi-Fi任务、OTA及push可激活。

先补生产ble_cycle回归复现已保存网络+纯周期诊断（新旧Plan激活路径），再修共享判定、核对全部调用并运行core/app/mcp回归。通过后核对默认exe/端口/计划任务，停watchdog及任务，备份并逐文件校验data/旧exe，构建、部署并经start-bridge.ps1启动，核对hash/health/MCP/设备回执。无固件构建/刷机，不提交。
