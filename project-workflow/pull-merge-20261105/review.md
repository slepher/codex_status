# 合并审阅

- 未整文件选ours/theirs。远端9a55586的AP01 ABI/独立App版本、OTA归属、Note4绑定重连、电池图标、Mac添加设备与P0保留。
- 本地2个提交：Wi-Fi v2无线参数匹配/紧凑控制摘要、共享固件版本与BLE claim网络准入保留。
- 原未提交改动：CI/构建入口、同Plan HTTP及重复claim抑制、ACK/在途Data持久化、BLE晚完成收尾和全部原未跟踪文件保留。
- 网络冲突：interfaces_on名称筛选叠加活动链路检查；claim_network在探测HTTP前使用远端host_address选择真实Wi-Fi接口；无IP首次Plan保持远端回退路由，同时传设备wc匹配版本。
- BLE冲突：settle_native接入远端NativeRadio冷却状态机，补充测试迁移到新类型；嵌套超时仍隔离，晚完成不伪造成功。
- 版本：沿用本地共享FW_RELEASE_VERSION机制，基号推进到远端0.18.108；不把.108源码视为已部署组合ROM。
- 交接：按每台设备的最新证据合并，Windows/书桌取本地10月末记录，Mac/Note4/AP01取远端11/05记录；两份原卡完整归档。

验证限制：Windows不能执行macOS原生Swift/WebKit检查；未构建/部署ROM，未启停Bridge或修改data。AP01 Python模板CRC旧断言在HEAD及origin/master均可复现（实际3ce26768，预期9bc56e9a），不是此次合并引入，未顺带改模板。

- 跨层回归发现Simulator紧凑wifi_status缺少transaction_high_watermark；通过现有C++ wifiManagementHighWater导出真实值，保留principal/namespace隔离，原端到端测试改验控制摘要及实际序号。固件协议无新增字段。
