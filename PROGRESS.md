# Codex Status 当前交接

更新时间：2026-10-20 02:25 香港时间

## 部署与工作区

- AP01 `34F01557210B`/DID2232184842：候选32运行**槽1**，build `6b60914b9f64a7117a53c45b6d93501e`。两次经0031基础ROM槽0启动/核对头尾后单次OTA更新应用槽，**救援槽0保留且实机回切已验**。本轮事务 `artifacts/ap01-app-rollout-20261020/navigation-rollout-task.json`。
- 模板v5/完整Bundle `b097bacb`（CRC4b72d7d7）保留；真实Data seq9/displayed，commit_seq4。用户确认**当前时间已正确**，重置UTC+8转换沿用已确认版本。**切页仍未通过：左旋钮偶发反向动画，页序不可靠**。
- 用户要求先快速结束并提交，停止后续实现/刷写。默认Bridge既有AP01版本格式修复/core175已部署，本轮无再次重建，Note4 ROM未刷写。

## 待续与边界

- 用户补充实际顺时针序列：宠物 → 应用（左入）→ 菜单（右入）→ 应用（左入）→ 日期/瓦数/温度主界面（右入）。这是异常观察，非目标顺序；仅记录，详见部署文档末节。

- C9剩余导航问题：最新原厂反编译确认key0x13的动画参数为2、key0x14为1，当前应用退出用相反参数；页面图也非简单连续索引。尚未修改/验证这项发现，不能称导航修复完成。对照原厂事件图和方向映射后再模拟/实机验收。
- 32宿主ASan/UBSan及原生120完整OTA/RTC归一化、121回切、122同NOR恢复通过；123旧版同页退出复现。125原始固定下一页=1断言失败（实际到原厂7且未回应用），独立后置条件复核通过；该模拟未覆盖实机方向问题，不冒充完整硬件验收。物理掉电/Boot2前回退/高水位未保证。
- [部署与详细边界](project-workflow/ap01-template-integration/release-0041-20261020.md)；物理反馈 `artifacts/ap01-app-rollout-20261020/physical-verification-navigation.json`；[待办 C9](docs/roadmap/backlog.md)。

## 精确ROM

- 已部署：`artifacts/ap01-platform-integration/device/ap01-0041-app-navigation-20261020.bin`，6994856 B，SHA256 `7f3789d37ad0ca8b8c1bbd7733729ec69262f7e3248501b40e05e750cf37a33c`；匹配ELF SHA256 `1ad5735dd7a443078106859ee2a0ad07be9666952eae6de6653195c86cff4bc0`。
- 救援槽0：`artifacts/ap01-base0031-20261019/ap01-base0031-v2-rom.bin`，6822344 B，SHA256 `d843aae4e097c31ce06a315c62e1a66f4fb3bf9299db289b6f31562d0ef2231b`；未写槽0，本轮回切巡检CRC/运行/头尾通过，未声称全片实读哈希。
