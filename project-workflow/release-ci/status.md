# 状态

## 当前0031：http扩容候选，替代gif-http（2026-10-31）

用户要求停用gif-http，主CI默认与本地入口均已改为http扩容布局0xa067d000。新候选 `artifacts/release-ci/ap01-menu-ci/http-dist/ap01-base-0031.bin`，6827032 B，SHA256 `8ee193821256e2ff8f0f168ba7f16c65267dff8ca6d1c6c6c7d214bc62cd8fb9`。Ubuntu24.04/GCC13.2.0真实编译退出0；菜单Python/C ASan+UBSan回归、完整重建、原猫GIF描述符/完整字节一致性、恢复兼容元数据、manifest v2无密钥及四处损坏拒绝均通过。日志 `artifacts/release-ci/ap01-menu-ci/build-http.log`。

替代合集 `artifacts/release-ci/ap01-menu-ci/ap01-http-rom-candidates.zip`，ZIP CRC通过，SHA256 `0744c7e48a6c927596f0b9ff5c49c1212a32d2f88ba96046e3d4538af9f19ece`；0041复用上一轮已验证的同一文件。旧`ap01-rom-candidates.zip`和`dist/`里的gif-http候选已被替代，仅留历史证据，旁附SUPERSEDED说明。后续不再交付该布局。

仅离线验证通过，未刷机/实机启动；不以任何历史布局的启动证据为新ROM背书。云端完整AP01 jobs仍待验。

## 以下为前阶段记录

## 0031 菜单版接入完成（2026-10-31）

双 AP01 job 已纳入主 release.yml，ap01_roms 默认开启，tag 发布依赖两者成功；两者固定公开固件 commit `69015e7713394449ee40e7e6f1b17246516d82fe`，无额外 token/tag/构建密钥要求。

- 0031：匿名远端 checkout + Ubuntu 24.04 Docker 实际构建通过（GCC13.2.0）；Python 菜单补丁测试、实际C菜单ASan/UBSan、重建校验、manifest v2无密钥/菜单来源、四处损坏拒绝通过。`artifacts/release-ci/ap01-menu-ci/dist/ap01-base-0031.bin`，6,804,520 B，SHA256 `65bac6675e89bbf7fd6928b99df68faa22cab97888b18961ac172d41fc6c4a08`。gif-http同长度布局，替换猫GIF，保留原厂OTA/恢复；不含业务App。日志 `build-clean.log`，中间ELF/map在dist/objects。
- 0041：更新依赖后Windows实际重建、固件库验证器passed=true/runtime=application/device_accessed=false。`artifacts/release-ci/ap01-menu-ci/0041/ap01-codex-status-0041.bin`，7,044,408 B，SHA256 `83109dd67f41a33abb5fe704cb778f11741572f513fe31a3ea821ab5d089f486`。
- 双ROM合集 `artifacts/release-ci/ap01-menu-ci/ap01-rom-candidates.zip`；含manifest/payload/provenance/校验清单，ZIP CRC通过。0041记录本App未提交工作区，不能冒充远端HEAD构建。
- YAML与主发布依赖、AP01源码边界通过。云端完整AP01 jobs/上传与新0031同机菜单/OTA/救援尚未验。未重启Bridge、未刷机；Mac仍交Mac端。早前“菜单缺口/待决定共享密码”记录已被新实现取代。

## 前阶段记录

2026-10-31 01:41（香港）：act windows-cross 完整实际运行与真实上传通过；交叉 EXE 已在 Windows 默认实例冒烟并恢复原 EXE；双 S3 ROM 与 AP01 方案均验证完成。未提交/推送、未创建 Release、未刷 ROM。Mac 留给 Mac 端。

## 当前可交付产物与验收

- 合集 `artifacts/release-ci/codex-status-windows-cross-s3-0.18.107.zip`，38,399,895 B，SHA256 `b87ed88ee14ab5fb30ead5cfa4cb5ce886937530c9e84bdea0ba253ad9f07266`。包含真实上传取得的 Bridge ZIP、两份 OTA ROM/manifest/USB包与 SHA256SUMS；内外包 CRC、各 USB 成员长度/哈希与总清单校验通过。
- act 0.2.88 的真实成功日志 `artifacts/release-ci/act-full/act-run-3.log`；`controller-exit.txt=0`。匿名 fetch ArduinoJson v7.4.3、setup-python、cargo-xwin（6m32s）、打包、upload-artifact@v4（2文件/9,368,362 B）、post/cleanup 全部成功。最终 windows-cross job 与实跑副本逐字段一致。
- 修复首轮真实 CreateArtifact 超时（WSL 地址对 Docker 不可达）：act 控制进程与 runner 使用 Docker host 网络，server 127.0.0.1；不跳上传。第二轮发现公开依赖 checkout 必须 token，仅该 job 改匿名固定 tag fetch/checkout。runner 镜像 `catthehacker/ubuntu:act-24.04` digest `c58e2b364da03b0c804c7d660f2ecbedf2f221a382b9baa0b344b0144780ff43`。
- 接收端 artifact：`artifacts/release-ci/act-full/uploads/1/bridge-windows-x64-cross/bridge-windows-x64-cross.zip`，外层SHA `e17a139cde08034c303f4a2b280843bcd00241f6244b69ab577996c348d7a9a5`；内层Bridge ZIP 9,418,929 B，SHA `221586d3ec620364182bb45c16fd7d53984e65961ee18be43aafdb21facb081f`；EXE SHA `5f0b1f0dc20ebfda5f8c002dcd9862a13e8f61d77f1cbb2439be3423b50aa70f`。接收文件与本轮构建逐字节相同，验证在 `artifact-verification.json`。
- Windows运行（01:37）：默认实例停 watchdog→任务→残留，346文件data及旧exe逐文件备份校验；交叉EXE PID17880，health204、MCP44、被动设备视图、C++ quad PNG、6个WebView2子进程通过，15s后PID不变。随后恢复原 `cead2571...` EXE/PID7424，health/MCP正常；3设备identity及6个根配置/凭据JSON未变。证据/回退备份 `artifacts/release-ci/runtime-smoke/`，`result.json` 与 `cross-preview.png`。仅启动/运行冒烟，不等于长期Wi-Fi/BLE/RF验收。
- S3：通过共享 `build-release-rom.ps1` 串行构建/打包，两个匹配 ELF 栈门禁通过；发现并修复 `check-wifi-stack.py` 在Windows持有临时ELF句柄导致objcopy失败。输出目录 `artifacts/release-ci/release-dist/`：Note4 `.107-note4` 1,858,240 B / SHA `060a269ff81cc1061adc9225b1e29a8e511f0c954156b8f7f25665c3b594b8a4`；1.54 `.107-bw` 1,840,176 B / SHA `72225732b045651efd991bd4a29393c5df55e871b436c565066fb1ff1f5d6705`。均通过marker、双OTA槽容量、ELF/USB成员哈希校验；没有刷机。CI默认勾选 `s3_roms`，仍是独立Windows串行job；不把本地构建说成Linux act执行过该job。
- AP01：兄弟固件库干净commit `2b88cb62589ae2003170b03cdda823647727ae56`，RV32 GCC8.4.0+2021r2-patch5；同一入口完整编译/审计/链接/打包与测试通过，候选7,044,408 B / SHA `b20582ab39e01bdc9023caa4a5f72a311961207398bfc2bade447ee6a3c572cc`。输出 `artifacts/release-ci/ap01-proof/`，独立手动双仓库 workflow 与凭据/固定commit方案见 [文档](../../docs/ap01-release-ci.md)。AP01边界检查通过，未修改兄弟库或操作设备；AP01云端CI尚未实跑。

## 初轮构建记录（先前阶段）

- Windows：字体合同、ROM staging guards 通过；Rust 27 suites，448 passed / 0 failed / 5 ignored；release 编译成功，ZIP 白名单清单校验通过。
- EXE：`artifacts/release-ci/target/release/bridge-app.exe`，27,891,712 B，SHA256 `1e50c40ca9084bb56d9fbc91cf7be166b5eccb77e3ce6b024a8f3daee087f53f`。
- ZIP：`artifacts/release-ci/dist/codex-status-bridge-windows-x64.zip`，9,657,736 B，SHA256 `e2334903569e2bfde72663f558563e6ba86465b666a239241cc41e38d5680dda`。日志 `artifacts/release-ci/windows-build.log`。
- 本机 ArduinoJson 7.4.3；交叉容器固定 `messense/cargo-xwin@sha256:9856b895265d4966f212228ba64802cf89337e2a2a537aa2533c1b8784cbc81b`。交叉日志 `artifacts/release-ci/cross-build.log`。
- act 0.2.88 能列工作流；首轮 dry-run 卡在 checkout，超过8分钟后终止。禁本地复制的限时重试明确报宿主 PATH 缺 node，未当作通过。日志 `act-cross-dryrun-bounded.log`。
- 改用 `node:20-bookworm-slim` 容器和 `--no-skip-checkout --action-offline-mode` 后，180秒有界 dry-run 完整成功、exit 0，日志 `artifacts/release-ci/act-container-dryrun.log`。仅工作流计划通过，未执行上传或发布。
- 交叉镜像工具链：rustc 1.89.0，cargo-xwin 0.23.1。
- Linux → Windows：8m17s 完成 release 构建，exit 0；x64 PE/ZIP CRC 通过。ZIP `artifacts/release-ci/cross-dist/codex-status-bridge-windows-x64-cross.zip`，9,418,930 B，SHA256 `1b2911205b1980a34758b43dac17fba7ac6601f6e6067fe8b9302e4a74bffb53`。没有运行交叉 EXE，不声明 Windows 运行验收通过。
- 交叉 EXE：`artifacts/release-ci/cross-target/x86_64-pc-windows-msvc/release/bridge-app.exe`，28,101,120 B，SHA256 `943ccf464a1314a8a795a973913e68d78fef01180061ffc39299e35ab757f8d4`。
- YAML 解析、手动发布门禁检查、`git diff --check` 通过。
- 原有 BLE/coordinator/platform 未提交修改均保留，产物包含当前工作区修复，不是仅 HEAD 的可发布源码证明。
- 剩余：GitHub云端托管runner/上传/发布、AP01云端双仓CI、Mac两架构、干净Windows安装及长期Wi-Fi/BLE；不以本机现有WebView2环境冒烟替代，归C6。

## AP01 主 CI 接入更新

GitHub 核实固件库 public、master=2b88cb62589ae2003170b03cdda823647727ae56；0041 已合并主 release.yml，匿名 fetch 固定 SHA，无额外 token/tag 要求。0031 用户明确要独立 OTA/诊断；现有 gif-http 路线嵌入维护密码、替换猫 GIF，尚待确定交付方式与验证工具链。主 CI 暂只新增 0041，不宣称双 AP01 完成。

用户后续要求核对0031本地接入菜单：源码确认没有凭据显示入口，0041菜单不能代替。已在兄弟库 firmware-modules/README.md 登记所有权、实际鉴权来源、菜单要求、设备凭据建议及救援验收，根README加导航。仅文档记录，未改0031实现或操作设备；两库diff检查和主CI YAML解析通过。
