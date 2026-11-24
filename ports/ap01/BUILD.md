# AP01 App 构建与部署

在固件仓库运行以下命令。构建器读取 App 仓库配置中的发布版本，输出目录须为空。

```sh
mkdir -p build/codex-status-app
python3 tooling/scripts/build-app.py --app-config ../codex_status/ports/ap01/build-0041.json --out build/codex-status-app --tool-prefix riscv-none-elf-
```

使用 `tooling/scripts/app-manifest.py` 生成集合清单，使用 `tooling/scripts/app-set-client.py` 通过 `/api/app/*` 安装。包扩展名为 `.apx`，清单扩展名为 `.amf`。诊断接口 `GET /api/diagnostic/versions` 返回当前 ROM 和已加载的 App 版本。

## AP01 App 健康诊断

平台状态接口保留 App 原始健康码，并通过 `fault_health` 保留本次启动首次锁存的失败码。当前 App 码：

| 码 | 原因 |
| --- | --- |
| -101 | 启动初始化失败 |
| -102 | 后台线程退出 |
| -103 | 业务线程心跳超过 15 秒 |
| -104 | 发现线程心跳超过 5 秒 |
| -105 | TCP 监听失败 |
| -106 | UDP 监听失败 |
| -107 | 写入权限恢复失败 |
| -108 | UI 重建失败 |

App 的退出清理保留先前记录的具体原因；后续健康恢复不会覆盖平台锁存的失败码。测试：`python3 tools/test-ap01-health.py`。
