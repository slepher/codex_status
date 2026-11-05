# 验收边界

必须：测试/编译失败立即退出；打包只来自显式文件白名单，不复制 data；输出独立；ZIP 检查与 SHA256。手动 Windows 模式跳过固件和 Mac，不发布。标签保持完整矩阵门禁。

本地命令不证明 actions checkout/cache/upload/release、权限和云端 runner 可用；macOS 签名公证、干净主机启动和设备通信独立验收。

已核对：手动在 tag 上运行也不会发布（release 额外要求 push 事件）；Windows 原生与交叉 target 分离；ZIP 输入只列 exe/README/LICENSE/seed templates，不包括运行 data。原生 ZIP 额外独立检查 CRC、EXE 逐字节一致和 x64 PE 通过。已有业务修改未回退。

交叉编译输出只声明构建与包检查，不把 Wine 或 Linux 构建视为 Windows 原生 BLE/WebView2 验收。固定容器 digest；Mac 原生保留，未增加未经证实的 osxcross job。

## act 完整运行修复审查

- WSL artifact 地址在 runner 不可达：首轮编译/ZIP成功，upload-artifact 的 CreateArtifact 请求真实超时。修复在本地启动方式，把控制进程和 runner 放进 Docker host 网络，server 绑定127.0.0.1；runner直连返回404（服务可达），不开放防火墙，不关闭上传。
- 无个人 token 的独立控制容器中，第二个 actions/checkout 明确失败 `Input required and not supplied: token`。仅 windows-cross 的公开 ArduinoJson 改为固定 v7.4.3 的匿名 shallow fetch/checkout；当前仓库 checkout 和其他平台流程不改。fetch/checkout 失败仍由 bash -e 终止，不回退未知头文件。
- act 默认当前仓库 checkout 使用本地副本，因此验收的是当前工作区源码而不是远端 HEAD。真实 fetch、setup-python、cargo-xwin、打包与upload步骤均需通过，最终以接收端文件验证为准。

## AP01 主 CI 边界

0041 调用公开兄弟库固定 commit 的构建/验证器；无维护实现复制。主 tag 发布依赖该 job 成功。0031 原厂镜像不等于用户要求的 OTA/诊断基础 ROM，未替代交付；嵌入密码的镜像不能因为输入使用 Secret 就视为不含凭据。

## 0031 新菜单版集成审查

采用公开已推送69015e7，同一SHA用于双AP01。集成层只准备编译工具与占位GIF输入，ROM补丁/鉴权/验证器仍在兄弟库。0031构建时不接受密钥，额外验证manifest v2凭据区域全零；损坏检查覆盖头、两个菜单hook及尾CRC。主release等待双AP01成功，未把候选标记改成可刷。真实Docker构建与0041重建通过，云端upload未验。

## 停用 GIF 布局

用户指出新菜单版两种布局均未完成实机验证，历史gif-http记录不能为其背书。默认改为http扩容布局0xa067d000，保留原猫GIF；新增GIF描述符与完整GIF字节比较，以及mode/native_recovery_compatible断言。现有无内置密钥、完整重建与损坏拒绝检查保留。Pillow仅为兄弟库回归历史格式所需，当前产物不使用占位GIF。
