# Pull合并状态（2026-11-05）

原HEAD：6f61814；远端：9a55586。本地2个独有提交、远端26个提交及原未提交修复已组合；没有unmerged paths，已完成冲突处理和验证；用户随后授权提交，本次以merge commit登记全部合并结果，未推送。

## 备份与保留

- `artifacts/pull-merge-20261105/local-before.zip`：29个原修改/未跟踪文件，另有完整binary patch及分叉提交清单。
- 合并备份stash `bd778d3fdbcc34aca6aaf9ec526159adaf59f72e`：2026-11-06按用户要求核验后清理；18个原未跟踪文件逐字节已提交，6个原修改文件与stash一致，其余代码/待办新增行经新版类型映射全部保留，旧交接卡完整归档。ZIP/patch继续保留，其他历史stash不变。
- 原18个未跟踪文件已逐个核验内容一致（仅Git换行规范化），已暂存以避免漏提交。
- 当前卡按设备合并时间线，两份原卡归档到docs/history；无运行现场核验，不将源码版本/历史候选视为部署。

## 验证

- Git冲突标记/未解决路径清空；diff --check通过。
- Node设备页、AP01职责边界、发布ROM规则回归通过。
- 首轮Rust工作区在模拟器紧凑Wi-Fi状态处发现缺字段；已接入真实C++事务高水位，原失败用例定向通过，最终App/BLE/Core/MCP/Render共459通过、5条件忽略；模拟器并行跑出现3个401/400认证断言不稳定（首轮通过），随后串行全套69+4全部通过。最终各包合计532通过、5忽略；不宣称默认并行整套全绿，干扰根因未定。
- Python模板哈希：quad与Note4通过；AP01预期9bc56e9a、实际3ce26768，在原HEAD和远端origin/master均复现，保留既有问题，不顺带修改模板。
- bash tools/test-wifi-ui.sh：Windows缺swiftc，macOS原生UI/窄宽截图未验。

隔离构建目录：bridge/target-note4-sync。完整日志：artifacts/pull-merge-20261105/。未构建固件、未启停Bridge、未动data、未操作设备。

最终日志：`cargo-test-final.log`保留并行3失败；`sim-targeted.log`为原接口失败用例修复通过；`sim-serial.log`为模拟器全73项通过。路径均在上述私有目录。未执行workspace doc-tests（并行命令在模拟器失败处停止）。

2026-11-06清理复核：发现UTF-8分片测试样本“界尾”在合并中发生转码，已恢复两边原文；对应bridge-ble测试1项通过。此修正不改变运行逻辑。
