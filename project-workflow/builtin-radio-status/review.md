# 验收边界

内置界面必须区分 Wi-Fi phase、实际 BLE 射频与手动授权；默认/错误文本不得覆盖状态。仅 ESP32 休眠目标使用，不向 AP01 系统菜单扩散。刷新沿用现有三次有界收敛，不建立新任务/NVS，不续期电源计划。宿主测试不代替实机无线/面板验收。

源码审阅：screen默认/错误页预留底部两行，旧usage fallback在右上空白区显示；用户CompiledTemplate无叠加。实际BLE开关只在内置页参与脏状态，不为模板引入额外刷新；休眠画面沿用已有committed-closure语义。RAM静态构建仍81240B，与`.37`相同；Flash程序1783988B，比`.37`增加244B，最终bin增加288B；无新增NVS/持久格式、无大数组/任务。宿主及匹配ELF检查已通过。Note4仅200/400布局宿主覆盖，未编译或部署，不扩大为实机验收。

2026-10-27分批提交前复核：再次运行`python tools/test-builtin-radio-status.py --out artifacts/wifi-families/other-batch-tests`，生产绘制提取及wifi-metadata回归均通过；未重新构建或刷机，既有`.38-bw`实机证据与风险边界不变。Wi-Fi族Bridge重启/部署在独立提交中记录。
