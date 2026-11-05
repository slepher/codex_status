# Note4 电池图标修复（2026-11-05）

## Plan / task

检查实际C++落点 → 最小模板修复 → 0/50/99/100%预览及像素断言 → 同步Bridge、发布完整Bundle并核验回执。用户已明确授权发布，不刷ROM。

## Review

当前有效源为 `bridge/crates/core/tests/fixtures/codex-status-a-400x300.json`。已读取Bridge源并证明与修复前仓库v7完全一致；设备缓存合同source_crc=6d54e167同样匹配。历史concepts模板含退役字段，不修改/发布。

原bar [324,16,14,7]，CT_BAR宽度四舍五入。Paint_DrawPoint偏移(-1,-1)，Paint_DrawRectangle填充不含末行；满电实际x323..336/y15..20，右侧与x337边框相邻。此前仅按CT_BAR推导的“90%起连接、97%以上覆盖、底部无间隙”不正确；生产像素复现确认97..100%右侧相连，底部本有2px间隙。

最终仅宽度14→13，bar [324,16,13,7]；满电实际x323..335/y15..20，左右1px、上下2px留白，含角点的完整白色隔离圈。模板v7→v8；source_crc 5ad7964c，Bridge compiled_crc 77e9cf2c。

## 验证

`python3 tools/test-note4-battery-icon.py artifacts/bridge-macos/target/debug/bridge-render artifacts/bridge-macos`通过：0/50/99/100%生产PNG隔离圈、0%空白、100%完整填充断言；原100%连接重现；全部JSON/CT/序列化往返零像素差。已审阅整屏与8倍近邻放大图。预览：`artifacts/bridge-macos/note4-battery-comparison.png`，左旧100%、中新99%、右新100%。

canonical回归中Note4新hash和quad通过；完整脚本因既有AP01期望9bc56e9a与实际3ce26768不符失败，已确认AP01源码与HEAD完全一致，本任务不改此无关期望。Bridge实际validate/save已验证Note4编译与hash。git diff --check通过。

## 发布状态

Bridge原Note4无本机Profile；按已安装单模板及现有Note4仓库默认族建立同序Profile，初始codex-status-a，保持sync_enabled=true/full_sync_s=3600，应用服务自动补现行数据绑定。无其他模板/ROM变化。

已同步v8，完整Bundle CRC34329d8f，唯一发布事务24725a35；自然会合后事务succeeded、无错误；独立platform_status_refresh认证同MAC/0.18.108-note4/ota_0，committed_job_id=24725a35、commit_seq=1149、context=bab703de7633667a、合同source_crc=5ad7964c、content_state=ready/content_error空。Bridge同HTTP观察displayed，data/applied_seq=9。设备当时93%，99/100%为宿主预览验收，非实机满电目视。原源和MCP回执留存artifacts/bridge-macos/battery-*及note4-battery-*。

## 用户验收

2026-11-05 用户确认“效果正常”，并要求提交此次修复及相关记录。实机显示效果已获用户认可；不将该确认扩大为实机100%电量测试。
