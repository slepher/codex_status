# 新窗口任务：图标落地（BT/WiFi + 出厂 ROM 资产 → 模板）

先读（按序）：`PROGRESS.md` 最新一节（图标专题）→ 本文件 →
`docs/power-state.md` §7（模板协议）与 `src/template_engine.cpp` 的
`icon`/`when` 实现；出厂资产提取记录见 PROGRESS 与
`artifacts/factory-rom-recon-*.py`。

## 已完成（勿重做）

1. **出厂 ROM 资产**（备份 `...\codex_status_backup\factory-backup-154bw\factory-full-8mb.bin`，
   源码 `waveshareteam/ESP32-S3-ePaper-1.54/02_Example/ESP-IDF/V2/11_FactoryProgram`）：
   - 状态图标 20×20 RGB565A8（battery/wendu/shidu），整屏图 `_3_` 200×200；
     1bpp 规则 `rgb565 < 0x7fff → 黑`。
   - 还原图 `artifacts/factory-rom-screen-mockup.png` / `factory-rom-icons.png`
     / `factory-rom-bg-200x200.png`；20×20 bits 见
     `artifacts/factory-rom-icons-template.json`。
2. **BT/WiFi 图标 12/16/20**（icons8 bluetooth-b 结构：左缺边三角 + 右两个三角）：
   - `artifacts/icon-bluetooth-{12,16,20}.png`（+`-8x`）、
     `icon-wifi-{12,16,20}.png`（+`-8x`）、`icons-bt-wifi-sheet-10x.png`、
     `icons-bt-wifi-bits.json`（base64 bits）、
     `icons-bt-wifi-template.json`/`.png`（固件引擎已渲染验证）。
   - 生成器 `artifacts/gen-bt-wifi-icons.py`；几何参数：竖线 x=0.5（y 0.02..0.98）、
     三角顶点 x=0.75（y 0.28/0.72）、左臂自由端 x=0.25；孔洞 12/16/20 =
     2×1 / 2×4 / 2×7 px。

## 待用户决策 / 待办

1. **图标细节**：12px 的两个孔只有 1px，是否接受；是否要"反色/填充"变体
   （白字形黑底，用于深色块）或更粗/更细描边；BT 是否要官方版（两旗在左/右）
   还是当前 icons8 版。
2. **落地用途**：放进哪个模板（quad/full/mini？位置、大小 12/16/20）；
   是否需要按状态显示——当前 `when` 只支持 `{"bind":…,"exists":bool}`，
   若要 `equals/contains`（如 WIFI OFF 时隐藏 WiFi 图标）需三端同步扩展：
   `src/template_engine.cpp`（parseCondition/conditionMatches）、
   `bridge/crates/core/src/template.rs`（validate_condition）、
   Python/测试哈希（`cargo test --workspace` + `node tools/test-quad-preview.mjs`）。
3. **更多图标**：出厂 battery/温度计/水滴已可复用（20×20 bits）；其余图标按
   `gen-bt-wifi-icons.py` 的几何风格新画即可。

## 现场

- 设备 `0.13.4-bw`（ota_0）、IP `192.168.1.50`、owner `1a2b`；桥 debug 父
  PID 12000 + watchdog 19944（:8765/:8766），MCP `template_render/validate`
  可用。产物都在 `artifacts/`（gitignored，勿入库）。

## 硬性约束

- 模板三端一致（固件/Rust/Python canonical 哈希）；改 `type/when` 必须三端同改。
- 保存模板只落盘；推送到设备是用户显式动作（`profile_push`），改完先
  `template_render` 出图给用户确认。
- 不提交/显示密钥；不擅自停桥；`pio run` 勿加 `-v`；新窗口改固件需 bump
  `FW_VERSION` 并走 MCP `firmware_ota` 验证版本变化。
