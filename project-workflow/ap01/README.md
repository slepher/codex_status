# AP01 逆向与 Bridge 接入研究

本目录归 codex_status 管理。第三方 `cuktech-ap01-firmware-tools` 克隆仅作为只读材料来源，本项目的研究成果和后续改动在这里维护。2026-10-12 从该克隆迁入，迁移过程与验证见 [迁移记录](migration.md)。

## 阅读入口

| 内容 | 文档 |
|---|---|
| 当前结论与未完成事项 | [交接](status.md)、[任务核对](task.md)；平台待办归 [C9](../../docs/roadmap/backlog.md) |
| 0031 逆向、首次安装、0041 限制、USB 与签名 | [阶段报告](report.md) |
| Bridge 接入、资源预算、常驻 Wi-Fi、单份应用 OTA | [接入设计](design.md) |
| 分区、启动、内置应用及外部堆证据 | [分区与启动复查](knowledge/0031分区与启动入口复查.md) |
| 原厂网络监听与 HTTP 调查 | [网络复查](knowledge/0031网络监听与HTTP复查.md) |
| 扫码获取固件的安全性 | [扫码审查](qr-login-review.md) |
| 0041原厂文件、下载校验和可复用登录 | [下载验收](firmware-download-20261013.md) |
| 0041历史专项：版本/签名及资源对照 | [0041分析](knowledge/0041签名与Bridge接口分析.md)、[专项计划与验收](0041-reverse-20261013.md) |
| 0031自定义App：首次刷入、App更新与恢复 | [刷入方案](first-flash-plan-20261013.md) |
| 已有准备、历史试编译与接续缺口 | [资料核查索引](preparation-index-20261013.md) |
| 最小XIP App候选ROM、ABI与离线验收 | [实现验收](app-implementation-20261013.md)、[源码说明](../../firmware/ap01/README.md) |
| App/底座失败及不开壳恢复难度 | [恢复评估](failure-recovery-20261013.md) |
| 研究需求与工具依赖 | [需求和规格](requirements.md)、[静态分析设计](reverse-design.md) |
| 上游项目对比 | [三项目价值分析](../../docs/cuktech-ap01-repository-value-analysis.md) |

当前目标为**支持自定义App的0031变体**；设备据用户报告仍为0031。最小XIP App/ABI候选已构建并通过离线测试，未刷机，完整Bridge/独立App OTA未实现，见实现验收。0041仅作版本/签名对照。原厂代码、上游作者报告、探针和实测分别注明来源，不能互相代替。

2026-10-13已下载并独立核验0041原件，文件位于本仓库`artifacts/ap01-research/`。新版已完成选定Ghidra函数反编译、0031/GNU对照、证书/分区/堆与首次安装条件评定；没有升级设备，尚无已证实0041安装绕过。登录缓存仍位于Git忽略的artifacts目录，本轮静态逆向未读取。

## 工具、证据与来源

[分析工具](../../tools/ap01/README.md)归本仓库；读取同父目录第三方克隆的原厂镜像、校验模块和载荷源码。引用版本为 `3bd98a8ca4fb35a9f8869121b13961d405b16dff`，上游知识和源码链接固定到该版本。文中未带本仓库前缀的 `core/`、`features/agents_dashboard*`、原厂固件路径及上游 `knowledge/` 均指该只读克隆。源码不重复导入；上游许可与作者归属见 [原项目许可说明](https://github.com/andy-JustSayWhen/cuktech-ap01-firmware-tools/blob/3bd98a8ca4fb35a9f8869121b13961d405b16dff/%E5%BC%80%E6%BA%90%E9%A1%B9%E7%9B%AE%E5%BC%95%E7%94%A8%E4%B8%8E%E8%AE%B8%E5%8F%AF.md)。

原始反汇编、地址索引及预算对象位于本仓库的 `artifacts/ap01-research/raw/`；保留字节和历史来源路径，不改写成新测量结果。此前主项目生成的压缩证据仍在 `artifacts/c9-ap01-compression-evidence.json`。这些原始产物随 artifacts 规则不入 Git；文档与分析脚本可随本仓库提交。原厂 ROM、账号、设备配置未迁入。

迁移前全部研究文件及六份带追加段落的上游文档保存在本地 `artifacts/ap01-research/source-before-move.zip`，路径与哈希清单为 `migration-manifest.json`。这份备份用于核对迁移完整性，不是后续文档维护位置。
