# macOS 菜单栏图标对比度（2026-11-02）

## Plan
保留系统模板模式，让 macOS 决定图标前景色；透明底数字替换实心底牌，过期/错误用形状提示。Windows保持既有彩色图标。

## Task
- 修复刷新时模板标记丢失，使用 Tauri 原子图标/模板更新接口。
- 更新 macOS alpha 蒙版，覆盖数字、缺数据、过期与错误。
- 定向测试、构建、保留data重启，核对HTTP/MCP与现场图标。

## Status
已实现并部署，未提交。源码依赖 tray-icon 0.24.2 的 set_icon 固定以 is_template=false 更新图片；初始 builder 的模板设置不能跨刷新保留。Tauri 2.11.5 已提供 set_icon_with_as_template，无需新增依赖。

## Review
- 透明底数字由系统模板着色；过期底部短横线、错误底部感叹号，无数据双横线。初始与5秒刷新都保留模板标记，刷新使用原子接口避免两次绘制闪烁。Windows保持原彩色底牌。
- `cargo test --locked -p bridge-app --target-dir ../artifacts/bridge-macos/target icon::tests`：2 passed；覆盖0/50/100/无值、三种实例shape与四种状态，验证前景alpha、透明背景及状态保留数字。
- 构建成功，保留原有5条未使用代码警告；`git diff --check`通过。新exe 81374832 B，SHA256 `c04827c134837ea20d1848558659732b7e49e111351e89a43eb23a5f41c4309d`。
- 已核对LaunchAgent路径后停止，原data备份至 `artifacts/bridge-macos/data-before-tray-20261102/`，旧exe `bridge-app-before-tray-20261102`；重启supervisor40079/child40084，8765 health204、8766 MCP ping成功。
- 构建/测试日志：`artifacts/bridge-macos/build-tray-20261102.log`、`tray-test-20261102.log`。CUA只能捕获Finder窗口，获取Bridge/SystemUIServer均超时，未取得菜单栏截图；深/浅背景、选中及跨多次刷新视觉验收待用户确认，不声称已完成。
- 验收范围为 macOS 图标对比度/状态提示与服务启动；不涉及设备、固件或 Wi-Fi UI。

## 11:42 截图与重新构建
用户截图已显示深背景白色数字50及底部过期短横线；解释该短横线为新增Stale状态标记，本轮未改其语义。fetch后HEAD较origin/master多2提交、无落后；没有新的远端源码。保留当前工作区重建并通过LaunchAgent重启，supervisor40665/child40669，health204/MCP ping通过。data再次备份至artifacts/bridge-macos/data-before-rebuild-*，构建日志rebuild-20261102.log。签名后最终exe81374832B，SHA256 `6181605b483e36a71de3ac75fbe884e9509e04f806d4e3dda2ea5a4447621f5f`，替代上轮部署哈希。

## 仅数字与迁移数据查找（执行中）
用户要求去掉底部标记，只保留自适应颜色数字。任务：移除macOS所有额外状态标记并恢复数字垂直居中；定向测试、备份后重建重启。另只读检查本机数据目录、Downloads迁移归档与备份，不覆盖当前数据；证据只记录路径和条目统计，不打印凭据。

## 仅数字与数据找回结果
已移除Stale横线/Error感叹号，数字垂直居中；保持模板原子更新。定向2测试通过，重建部署exe81374816B，SHA256 `fc1493d522e7eab7ad1234b98b272dbedce2eb00976214fae38588c575cf5f27`；supervisor41283/child41287，health204/MCP ping通过。构建/测试日志build-digits-20261102.log、tray-digits-test-20261102.log。
两个迁移归档不含原Bridge运行数据；173个历史项目会话中找回2台旧Mac设备与3组Wi-Fi及历史缓存密码，已整理私有恢复清单和报告，均在artifacts/bridge-macos/，未导入在线Bridge。报告recovery-report-20261102.md；原安装身份/完整配对凭据/事务状态未恢复。未提交本轮代码与记录。

## 导入与重新配对（执行中）
Plan/Task：备份后导入3组含历史2.4G密码的共享族库；通过真实认证/配对重新登记两台设备，不伪造历史认证。Note4用户明确暂不在身边，等待后续物理窗口。AP01同MAC已发现，但GET配对预检被Bridge统一报transport_unknown，外部相同HTTP预检200且can_pair=true；未创建pending文件/未发POST。先增加不含凭据的传输错误日志，再定位并继续。
Status：3族已导入revision1、读回密码3条，未设5G密码、未创同步Job；完整data备份data-before-import-20261102-115519。
Review：本机Bluetooth not_determined；不把设备列表恢复等同认证完成。

## 导入与配对结果
3组Wi-Fi共享族库revision1已导入，密码与私有恢复清单逐条读回一致，MCP无密码视图可见3族；缺失的5G密码保留None，未创建Wi-Fi设备同步任务。AP01经现网UDP同MAC发现、双屏码5839用户旋钮确认后，原事务查询paired，token文件已保存，认证状态device_mac=34F01557210B一致并注册当前设备列表。sync_enabled沿用新登记默认true；未发布Bundle或刷ROM。
Note4用户明确暂时不在身边，蓝牙授权仍not_determined，保留恢复清单，未伪造认证登记，待真实BOOT窗口重新配对。
AP01早前只读预检报pairing_transport_unknown且无pending；补充不含凭据的错误日志并重建/重启后成功，未捕获旧错误原因，不将重启恢复归因具体网络故障。新exe81375344B，SHA256 `8d8986960ae3d745c9dafe54fbd45675e72df20c7a73307ac36ed84f05141c3a`，日志build-pairing-diagnostic-20261102.log。最终状态与读回证据私有保存import-pairing-status-20261102.json。

## 5GHz密码同步
用户明确要求5G密码与2.4G相同。停桥完整备份至artifacts/bridge-macos/data-before-5g-password-*，将3族password_5g复制为各自password_2g，库revision1→2；断言其余state字段完全保留，原子替换后重启。重启读回3/3一致，health204/MCP ping通过；不新增SSID或设备同步任务。

## 托盘左右键统一
Plan/Task：macOS与Windows统一左键打开面板、右键原生菜单；保留禁用左键菜单设置，将面板触发点改为左键按下，macOS显示面板前调用app.show()解除应用隐藏。macOS重建/重启；Windows仅源码，不部署，实机验收待设备可用。
Status/Review：已核对tray-icon0.24.2默认menu_on_right_click=true，两平台沿用原生右键菜单。只改变共享左键事件与macOS应用显示，不新增依赖。

托盘左右键修改已完成macOS构建/重启；exe 81375344B，SHA256 `2b918db7469466df899e3a0a22fdb15124b1d4bddd1e903c54896d1495637fde`，supervisor49125/child49131，health204/MCP ping通过，git diff --check通过。数据已完整备份，Windows未构建/部署；CUA此前出现严重超时，本轮未用其做点击验证，不以服务健康代替原生点击验收。

## macOS左键仍打开菜单的修正
用户实测上轮左键仍弹菜单，所以上轮未通过点击验收。依赖tray-icon0.24.2在create/set_menu直接挂载NSStatusItem.menu，即使menu_on_left_click=false；Mac改为不挂载菜单，右键Up显式调用既有window.popup_menu（Muda无position时使用屏幕鼠标位置，不依赖主窗口可见）。Windows保留原生右键菜单。左键Down继续show_panel；增加不含数据的左右键事件日志，构建后核对运行。

修复版已构建/部署：exe 81394752B，SHA256 `8c8111cdf7d26b44068a3a3042cf790ef458b051eba92c6cbc07de1a6ad20a4f`；supervisor49512/child49516，health204/MCP ping通过。用户已实测确认左右键“两者都正常”；点击日志同时捕获左键打开面板/右键打开菜单。上轮仅变更按下时机未解决问题，本轮取消macOS自动挂载后通过用户点击验收。Windows未构建部署。

## 模板信息为空的调查与恢复
程序为最新已部署构建，运行data/templates含4个种子JSON，但ensure_runtime仅复制旧模板目录，界面实际读取PlatformService持久模板/族配置；新机state两者均空，并非托盘重编未生效。本轮先备份state-before-template-import-*（0600），5个仓库模板经正式MCP同源校验后save到平台库：154g mini v2/full v3/quad v15，AP01 codex-status-a v9，Note4 codex-status-a v7（core/tests/fixtures）。读回5条。
界面按族配置显示，故创建三个repo-default本地配置（“1.54 黑白/Note4/AP01 彩色 · 仓库默认”），所有模板按顺序纳入、未创建enabled子集；sync_enabled=false。读回3配置，保存事件通知界面刷新。只保存未发布，未修改设备Profile、Bundle任务或当前显示。源模板未改，未修订启动自动导入行为。证据template-validation-20261102.json、template-import-20261102.json、template-profile-import-20261102.json。

## 读取主机网络未触发授权
Plan/Task：真实Bridge被动查询location/bluetooth均not_determined，host=permission_required，说明并非已授权后未刷新。生产“读取主机网络”绑定wifiHost(false)，明确绕过授权，而wifiHost(true)可复用既有授权/设置引导。先加生产按钮回归red，再切true，跑Node与原生WK窄宽截图审阅，重建/重启保留data；原生系统弹窗需用户确认，不以合成IPC冒称授权完成。

## 授权修复验收
真实根因包含：读取主机网络按钮wifiHost(false)只被动读；首次未选设备时clearDeviceView→wifiFamilyRender→selectedTarget抛错，中断后续wifiInitPermissions和设备列表读取。已改渲染为允许空selectedDeviceMac，按钮显式wifiHost(true)复用授权/设置链。两项生产回归均先red，Node green；原生WK80通过，560/1000权限截图人工审阅通过（合成IPC不能证明系统授权）。
新包已构建/重启，exe 81394752B SHA256 `de3e039fddefcfb8b45e37ddca86359f0f7af7f781035317eddcf767130578df`，supervisor51790/child51794，health204/MCP请求通过。用户已确认真实系统弹窗出现并允许；读回权限{"bluetooth": "allowed", "location": "allowed", "platform": "macos"}，host状态connected、网络数1。私有证据host-permission-live-20261102.json；构建/回归日志build-host-permission-20261102.log、host-permission-node-20261102.log、host-permission-wk-20261102.log；未改设备网络/ROM，data已备份。

## 2026-11-03 提交归档

本专项的托盘数字/左右键、配对诊断、空设备初始化及主机授权按钮修复已分批整理提交。上文“未提交”为当时过程记录；部署状态仍以既有用户验收和 PROGRESS 为准。本轮只提交源码与文档，没有重新构建、重启或部署服务，也没有操作设备。
