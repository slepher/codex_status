# 任务

1. 已完成：核对既有 workflow、打包与 Astranaut 方案（用户指定 GPT-6.1 Terra 调查）。
2. 已完成：增加 Windows/Windows-cross 独立手动入口、共享打包脚本和本地说明。
3. 已完成：Windows 原生实跑、Linux→Windows交叉实跑、ZIP 校验、act容器dry-run、差异检查，记录限制和 Mac 接续命令。剩余外部验收归 backlog C6。

4. 已完成：act windows-cross 完整真实运行，修复公开依赖token与WSL artifact网络问题，接收端逐字节/哈希/CRC核验通过。
5. 已完成：用户授权的双S3 ROM串行构建/打包，marker/槽容量/ELF栈/USB成员哈希验证；Bridge+双S3合集。
6. 已完成：AP01双仓库方案、独立手动CI，本机实际候选生成/验证及边界检查；云端CI未运行。
7. 已完成：用户授权默认Bridge重启验证交叉EXE，health/MCP/C++渲染/WebView2通过；完整备份后测试，已恢复原EXE/服务，设备身份与配置保持。

8. 已完成：0041 job 合并主 release.yml，公开兄弟库匿名锁定 SHA；删除独立 AP01 workflow。云端未验。
9. 进行中：0031 独立 OTA/诊断基础 ROM；嵌入维护密码的公开交付方式待用户决定，工具链/产物待验。

10. 已完成：新0031菜单实现公开提交核实、双AP01主CI接入、Linux匿名依赖真实0031构建/校验、Windows0041重建、双ROM候选合集与文档。云端/实机剩余归C6。

11. 按用户要求停用gif-http，主CI调用入口已改http扩容，保留猫GIF并增加字节一致性检查；实际构建结果及替代产物见status最新条目。
