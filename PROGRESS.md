# Codex Status 当前交接

更新时间：2026-10-20 13:25 香港时间

## 部署与工作区

- AP01 `34F01557210B`/DID2232184842：**33运行槽1**，build `967049b5f6a589b717e03d4abe312ed1`。本轮先认证回切0031槽0及头尾，再单次OTA槽1；**救援槽0保留，实机回切已验**。事务 `artifacts/ap01-app-rollout-20261020/navigation33-rollout-task.json`。
- **导航问题已修复并实机通过**：原厂Pet7→菜单6及菜单→主界面被页号递减误判为环回、重复插入应用；现仅在进入Pet的边/反向边插入全部模板。退出动画key0x13→2、0x14→1。用户本轮明确回复“页序和动画均正常”。
- v5完整Bundle `b097bacb`保留，commit_seq5；真实Data seq12已应用，最终读取display_state=pending，不冒充本次Data已取得显示回执。原先已确认的时间修复沿用。
- 发现默认Bridge未监听，已按指定计划任务脚本启动既有EXE；force=false恢复既有占用并投递快照。该次启动未重建Bridge或刷Note4。

## 下一步与验收边界

- **AP01字体核查已确认恢复遗漏**：原厂ROM有完整ASCII **Montserrat14**（`0xa0262f60`，含Y）；67套fmt_txt字库中原清单漏12套。全部字形/映射/字距与LVGL标准资源一致。页面子集确有裁剪；不再把“55套缺Y”推广为ROM缺Y。[核查及像素证据](project-workflow/ap01-template-integration/font-rom-audit.md)。核查仅读归档，未访问设备。
- AP01布局完成**Montserrat14离线预览，未部署**：首栏订阅/账号/时间，底部SYNC/RC合栏；英文/账号/时间统一原ROM完整Montserrat14原尺寸（行高16），中文混排整段微软雅黑UI20px，大数字保留。四场景覆盖/边界检查及目视通过；LVGL9宿主RGB565，非实机验收。[预览与复现](project-workflow/ap01-template-integration/layout-preview.md)，生产Bundle/ROM未改。
- 本次导航项可结案；C9整体仍开放，物理掉电/Boot2前回退、真实调用链高水位等既有边界不变。反向完整导航为原生模拟证据，用户本轮确认来自顺时针实测请求。
- 130/133/134原生完整OTA、同NOR升级/回切/原事务回执恢复通过；12组原厂活动LVGL像素与共享引擎差异0，匹配33宿主ASan/UBSan通过。128旧32对照失败；126/127/129及131/132的测试路径/离线引导门控/漏传回执参数记录保留，未放宽放行。
- [部署与边界](project-workflow/ap01-template-integration/release-0041-20261020.md)；实机证据 `artifacts/ap01-app-rollout-20261020/physical-verification-navigation33.json`；[待办C9](docs/roadmap/backlog.md)。

## 精确ROM

- **已部署33**：`artifacts/ap01-platform-integration/device/ap01-0041-pet-navigation-20261020.bin`，6994872 B，SHA256 `a4a34b872815711094c68f4a5c6c1ded8c2316475520bff95e1d8ec25011c535`；匹配ELF SHA256 `5696ba3e8193d9b2458936edbc2394cf2372930ff1b3dd41d52bc4d4a149ecbd`。
- 救援槽0：`artifacts/ap01-base0031-20261019/ap01-base0031-v2-rom.bin`，6822344 B，SHA256 `d843aae4e097c31ce06a315c62e1a66f4fb3bf9299db289b6f31562d0ef2231b`。本轮未写槽0，认证运行/头尾及回切前原生CRC检查通过；未声称全片实读哈希。
