# AP01 0041 App build

本仓库提供 App 源码和 `build-0041.json` 声明式配置；兄弟仓库 `cuktech-ap01-firmware` 统一负责平台源码清单、编译参数、RV32 审计、资源预算、链接与 ROM 打包。默认使用固件仓库保存的 ArduinoJson 和 SHA256，不要求本仓库 `.pio` 依赖目录。

从本仓库根目录执行（按 AGENTS 要求提权，编译器与构建器会创建临时/输出目录）：

```sh
python -B ../cuktech-ap01-firmware/tooling/scripts/build-firmware-0041.py --app-config ports/ap01/build-0041.json --build-dir artifacts/ap01-unified-build/objects --output artifacts/ap01-unified-build/codex-status.bin
python -B ../cuktech-ap01-firmware/tooling/scripts/test-firmware-0041.py --manifest artifacts/ap01-unified-build/codex-status.manifest.json
```

`--tool-prefix` 可指定已安装的 RV32 GCC。配置只列 App 的源文件、头文件目录、宏和普通绑定源码组；平台源码、工具链 ABI、XIP 可写段约束、链接脚本及 ROM 补丁规则由固件仓库拥有。`abi_budget.cpp` 继续提供 App ABI 1 预算测量。

原有 `tools/check-ap01-native.py --out ...`、`tools/link-ap01-native.py --out ...` 只转发到固件入口的 audit/link 阶段；`tools/build-ap01-rom.py --source-dir ... --output ...` 只转发打包器并保留历史 import 接口。源码、对象、头文件或 App 配置变化后须重新审计/链接，候选文件名必须是新的。

固件仓库不传 `--app-config` 时独立构建平台 ROM，不要求本 App 或逆向仓库。构建产物是离线候选，编译成功不代表实机启动或恢复验收；本次没有部署。
