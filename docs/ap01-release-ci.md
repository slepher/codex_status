# AP01 双仓库 ROM 生成

构建实现继续由 `slepher/cuktech-ap01-firmware` 拥有，本仓库只提供 `ports/ap01/build-0041.json` 和 App 源码。不需要逆向仓库，也不需要复制 OTA/维护模块进本仓库。

## 已实测的本地入口

从 `codex_status` 根目录执行；相邻目录放固件库。安装 Python 3 与 `espressif/toolchain-riscv32-esp@8.4.0+2021r2-patch5`（本次验证 Windows x64，GCC 8.4.0），或者通过 `--tool-prefix` 指定支持 `rv32imafc/ilp32f` 的同版本工具链：

```powershell
python -m pip install platformio
pio pkg install --global --tool 'espressif/toolchain-riscv32-esp@8.4.0+2021r2-patch5'
python -B ../cuktech-ap01-firmware/tooling/scripts/build-firmware-0041.py --app-config ports/ap01/build-0041.json --build-dir artifacts/ap01-release/objects --output artifacts/ap01-release/ap01-codex-status-0041.bin
python -B ../cuktech-ap01-firmware/tooling/scripts/test-firmware-0041.py --manifest artifacts/ap01-release/ap01-codex-status-0041.manifest.json
```

每次使用全新输出文件名/目录；构建器拒绝覆盖候选。Agent 执行创建目录/编译时遵循提权规则。省略 `--app-config` 可生成不带业务 App 的平台 ROM，不把它误当带模板 App 的版本。

必需输入：

- 两库固定 commit 和各自 dirty 状态；本次固件库干净 HEAD `2b88cb62589ae2003170b03cdda823647727ae56`。当前 App 工作区含未提交修复，本机结果不等同于远端 HEAD。
- 固件库跟踪的原厂 `roms/0041/ap01-1.0.2_0041.bin`：6,769,704 B，SHA256 `972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9`。打包器硬校验原件，保持归档不变。
- 固件库自带 ArduinoJson 7.4.3 和 SHA256 实现，不需要本仓库 `.pio` 目录。
- GCC、objcopy、objdump、nm 等同一 RV32 工具链；编译/审计/预算/链接/ROM 补丁均经固件库统一入口。

2026-10-31 已在当前两库实际运行以上构建/验证流程：`artifacts/release-ci/ap01-proof/codex-status-0041.bin`，7,044,408 B，SHA256 `b20582ab39e01bdc9023caa4a5f72a311961207398bfc2bade447ee6a3c572cc`；验证器返回 `passed=true`、`runtime=application`、`device_accessed=false`。配套 `.manifest.json/.audit.json/.link.json/.elf/.map/.payload.bin` 留在同目录。

## 主 release CI 接入

本仓库 [release.yml](../.github/workflows/release.yml) 同时包含 `ap01-0031`（Linux）与 `ap01-0041`（Windows）。手动 `ap01_roms=true`（默认）或 `scope=all` 输出两种候选；tag push 必须等待双 AP01、双 S3 和 Bridge 全部成功才建立 draft release。没有单独的 AP01 workflow。

两个 job 均匿名获取公开固件库完整 SHA `69015e7713394449ee40e7e6f1b17246516d82fe` 并核对 HEAD；该提交已核实存在于 GitHub。不需要额外 token、构建密码 Secret，也不必先打 tag。tag 可作为兼容版本名称，构建仍锁定 SHA；依赖修复先提交推送，再更新本工作流中的两处 fetch/核对。

### 0031 基础 ROM（独立 OTA/诊断，无业务 App）

新固件通过本地 `Access / 18080` 菜单生成、持久保存与轮换八位设备码。manifest v2 的旧凭据区域全零，ROM 不包含构建密码。首用需在设备菜单生成设备码；生命周期与鉴权见[固件库说明](https://github.com/slepher/cuktech-ap01-firmware/blob/69015e7713394449ee40e7e6f1b17246516d82fe/tooling/firmware-modules/access-menu-0031.md)。

Linux/Ubuntu 24.04 依赖 `gcc-riscv64-unknown-elf`、`binutils-riscv64-unknown-elf`、`python3-pil`、`build-essential`。历史构建器硬编码工具目录，CI 仅把系统工具链接到 checkout 同父目录的 `cuktech-ap01-firmware-tools/tools/docker/toolchain/bin/`，不需要第三个仓库源码、密钥文件或逆向仓库。

```bash
sudo apt-get update
sudo apt-get install -y gcc-riscv64-unknown-elf binutils-riscv64-unknown-elf python3-pil build-essential
# 在 codex_status / cuktech-ap01-firmware 的共同父目录执行：
mkdir -p cuktech-ap01-firmware-tools/tools/docker/toolchain/bin dist
for tool in gcc as ld nm objcopy; do
  ln -s "$(command -v riscv64-unknown-elf-$tool)" "cuktech-ap01-firmware-tools/tools/docker/toolchain/bin/riscv64-unknown-elf-$tool"
done
bash codex_status/tools/build-ap01-0031.sh cuktech-ap01-firmware dist
```

Pillow 仅供兄弟库菜单回归覆盖历史打包器，不再用于当前ROM生成占位GIF。每次构建使用新的输出目录。脚本执行固件库的 Python 菜单补丁测试和实际 C 菜单 ASan/UBSan 测试，再调用固件库编译/打包/重建验证器，检查无内置密钥、菜单来源、独立 OTA 和四处损坏拒绝。输出 ROM、payload、manifest、provenance 和 SHA256SUMS；中间 ELF/map 位于 `objects/`。

采用 `http` 扩容布局，代码放在文件偏移 `0x67e000`（运行地址 `0xa067d000`），保留原猫 GIF、原厂 OTA/恢复入口；构建额外逐字节核对 GIF 与长度描述符。已按用户要求停用 `gif-http` 发布路线，不因历史版本启动记录而优先使用破坏原动画的布局。ROM 补丁与维护实现仍由兄弟库拥有；扩容候选仍须实机验收。

### 0041 带 App ROM

Windows runner 安装前述 GCC 8.4 工具链，调用固件库 `build-firmware-0041.py --app-config` 和 `test-firmware-0041.py`。输出 ROM、manifest/payload、双库 provenance 与校验清单。更新固件依赖后已重新完成本地构建和校验。

### 验证边界

0031 已从 GitHub 匿名获取固定提交，在 Ubuntu 24.04 Docker 中实际构建并校验；0041 在本机 Windows 实际重建并校验。记录见 [release CI 状态](../project-workflow/release-ci/status.md)。两种新 AP01 job 尚未在 GitHub 托管 runner 完整运行，不把脚本实跑宣称为云端上传成功。

两份产物均是未刷机候选。0031 新菜单、持久化、实体旋钮、非活动槽 OTA 与救援路径仍需同机验收；manifest 保留不可直接视为可刷/实机验证完成的标记。Mac 构建与运行继续交给 Mac 端。
