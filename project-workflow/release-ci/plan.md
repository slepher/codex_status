# Release CI 与本地验证

2026-10-31，关联 backlog C6。复用已有 release.yml，增加仅 Windows 的手动构建选择；提取 Windows 测试/编译/打包共享入口，使用独立 target，不启动应用，不操作设备或运行 data。标签仍要求完整矩阵成功才建立 draft。

本机实跑 Windows 共享入口，验证 ZIP 内容与哈希；另以 WSL/Docker 实跑 cargo-xwin 的 Linux→Windows MSVC 路径，并加入独立手动 job。act 使用限时 dry-run，不将模拟器结果视为 GitHub 托管 runner 验收。macOS 调查 osxcross/SDK/项目原生编译器约束，原生测试留给 Mac。

## 10/31 追加：act 完整实跑

用户要求实际执行 windows-cross 全 job。使用当前源码的隔离副本、Ubuntu runner 容器和 act 本地 artifact server；执行依赖 checkout/setup-python/交叉编译/ZIP/上传及 post cleanup。遇到问题直接修复并复跑，核对服务器收到的 ZIP 与内层包/哈希，不以 dry-run 或单独脚本代替。Mac 继续留给 Mac 端；不部署、不推送。

追加用户要求：显式授权串行生成 Note4/1.54 两个 S3 ROM，加入发布产物；手动工作流允许为选定 Bridge 范围附带两份 S3。CI/本地共用 ROM 构建与打包入口，保留原 target 隔离与 sdkconfig 快照。AP01 检查兄弟固件库的真实输入/工具链/原厂基线，提供双仓库固定版本的可执行构建与 CI 接入方案，不复制维护实现。

## AP01 合并发布

按用户要求将 AP01 构建合并到本仓库 release.yml；公开固件库匿名获取固定 SHA，无须私有仓库 token 或预先打 tag。0041 带 App；0031 必须独立 OTA/诊断而非原厂 ROM，先核对其嵌入维护密码的交付方式和历史工具链，再接入输出。

## 0031 新菜单实现接入

用户确认实现完成；核实公开 commit 69015e7713394449ee40e7e6f1b17246516d82fe，更新双 AP01 依赖并增加主CI的0031 Linux job。复用固件库构建/验证器，CI仅准备历史工具链目录与GIF占位输入；新ROM不含构建密钥，验证manifest v2、菜单、损坏拒绝及双仓构建。仅候选，不刷机。

## 0031 布局纠正

用户明确停用gif-http。CI改用保留原猫GIF的http扩容方案，入口0xa067d000；删除占位GIF生成；Pillow保留供兄弟库历史格式回归，真实重编并验证GIF字节保持、原厂恢复元数据和完整重建。更新候选合集与交接哈希，原gif候选仅留历史不再交付。
