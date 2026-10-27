# Codex Status 当前交接

更新时间：2026-10-27 20:24，Asia/Hong_Kong。

## 当前部署

- Windows Bridge：`bridge/target/debug/bridge-app.exe`，39039488 B，SHA256 `0b8b843414d97904fb21dac3f341bfc269edcc147d42e759e6e4039fd4899a47`。20:24部署，health204/MCP44；旧exe/308文件data备份校验通过。[状态](project-workflow/wifi-families/status.md)
- Note4 `7C4FADB93408`：20:01认证`.107-note4`/ota_1，原ota_0未作为写入目标。ROM `artifacts/wifi-families/note4-0.18.107-connection-fix.bin`，1858240 B，SHA256 `973c4d6c9377747dc4fafb7e1bd72bf54fd84482a16c5dea26d9d359f8df81d7`；原OTA succeeded/image_verified，20:07同boot HTTP ready/bridge_reachable/error空。
- 书桌屏 `70041DD7A340`：20:14认证`.107-bw`/ota_1，原`.38-bw`/ota_0保留；原非活动.37槽1已替换。ROM `artifacts/wifi-families/desk-0.18.107-connection-fix.bin`，1840176 B，SHA256 `72225732b045651efd991bd4a29393c5df55e871b436c565066fb1ff1f5d6705`。原OTA succeeded/image_verified，20:15同boot HTTP/网络/内容正常，Profile/Bundle/网络目标保留；context按正常重启轮换并已对账。[验收](project-workflow/wifi-families/status.md)
- AP01 `34F01557210B`：上次验`1.0.2_0041`应用1/build`ec0a58f71f65c2830c08c9bfd7c155db`；救援0031/槽0保留。本次未操作。Mac Bridge及其他候选/ROM指纹见[前卡](docs/history/progress-archive-2026-10-27-before-wifi-control-fix.md)。

## 活跃事项与下一步

- Wi-Fi：共享族工作台已部署；连接v2只匹配真实无线参数，显示名不触发重新配网；控制摘要单帧481 B，去除重复读取，旧分页按实际RF截止与共享deadline保留命令预算。4项最终定向回归、442回归+1真实Fake专项、Note4/BW构建及各90栈检查通过；同MAC运行hash与网络已验，未用实机改名/切网作测试。完整RF/macOS余项归B6。[验收](project-workflow/wifi-families/status.md)
- 版本：Note4/BW共用`src/main.cpp`的`FW_RELEASE_VERSION=0.18.107`，后缀分别-note4/-bw；用户明确不要求同时打包/升级。后续按源码快照共用序列，打包校验marker基号/target/双槽容量并登记commit/dirty/ROM及ELF hash；旧书桌.38缓存明确拒绝冒充.107。源码号、产物、实机分别追踪；用户随后单独授权书桌屏部署，双板现同0.18.107基号。[规则](docs/build-install.md)
- 修复、版本规则和双板部署证据随本次提交登记；`test-firmware-modules.py`有HEAD同样存在的display旧断言失败，记录在专项；不扩大为全套检查通过。A9/B6/C5/C8等剩余只看[唯一待办](docs/roadmap/backlog.md)。
