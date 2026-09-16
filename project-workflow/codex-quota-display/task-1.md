# task-1 — 确定性 Codex 余量 PNG 预览

## 目标

实现一个无第三方依赖的 Node.js 生成器，并产生第一张确定性的 200 × 200 Codex 余量显示预览 PNG。本任务只验证信息层级与陈旧状态，不实现固件、联网、认证、实时额度读取或持久化。

## 所有权

编码工作者只拥有：

- `tools/generate-preview.mjs`
- `artifacts/codex-quota-preview.png`

调度者拥有 `project-workflow/codex-quota-display/` 下的工作流文件。不得修改其他已有文件；工作者并非项目中唯一操作者，必须保留他人的改动。

## 实现方案

只使用 Node.js 内置 API：固定视图模型、所需字符的紧凑点阵字体、像素/矩形/文本/进度条绘制原语、200 × 200 RGB 光栅，以及基于内置 `zlib`、CRC-32、`IHDR`/`IDAT`/`IEND` 的最小 PNG 编码器。

模型、布局、绘制和 PNG 编码须清楚分离，便于未来移植布局概念，但不得声称主机 PNG 编码器属于固件。

固定示例数据：已用 31%、剩余 69%、重置 18:00、网络 OFF、同步 FAIL、状态 STALE、最后成功更新 14:32。

默认命令：

```powershell
node .\tools\generate-preview.mjs
```

输出：`artifacts/codex-quota-preview.png`。

## 视觉契约

阅读顺序必须清晰：`31% USED`、`69% LEFT`、`RESET 18:00`、`NET OFF`、`SYNC FAIL`、`STALE DATA`、`LAST OK 14:32`。

只使用以下 RGB 颜色：

- white `#FFFFFF`
- black `#111111`
- red `#D9362B`
- yellow `#F4C542`

不得抗锯齿、透明、渐变、阴影或使用额外颜色。黄色陈旧横幅与 `LAST OK` 必须明确说明额度为上次成功值。

## 不变量

- PNG 恰为 200 × 200，有正确签名与 IHDR。
- 输出不包含动态时间、随机数、主机路径或环境相关元数据。
- 光栅像素只含四种约定颜色。
- 固定模型的已用与剩余相加等于 100。
- 所有必需内容在原生尺寸清晰可见且不裁切、不重叠。
- 无网络、API、认证、PlatformIO、硬件、固件或显示驱动依赖。
- 自动创建 `artifacts` 目录；错误以非零退出码和简洁信息报告。

## 实现步骤

1. 检查两个拥有路径；若存在意外用户内容则停止。
2. 创建生成器并定义四种颜色与固定陈旧状态模型。
3. 只实现本预览所需字形和有边界的光栅绘制原语。
4. 按视觉契约排版，包括 31% 比例进度条。
5. 实现确定性 PNG 编码并写出目标文件。
6. 执行全部编码自测并在原生尺寸检查图像。
7. 报告路径、命令退出码、SHA-256 与目视结果，不提交。

## Coding Self-Tests

1. `node .\tools\generate-preview.mjs`：退出 0，PNG 存在。
2. 用 Node.js 读取 PNG：核对签名、IHDR、宽高均为 200。
3. 记录 SHA-256，第二次生成，再核对哈希完全相同。
4. 检查源文件仅引用 Node.js 内置模块，无网络、随机、实时钟或硬件代码。
5. 解压 IDAT 并核对所有像素只属于四种约定 RGB 值。
6. 用图像查看工具按原生尺寸确认必需内容、层级、裁切与重叠情况。

## Independent Verification

独立只读 runner 必须重新执行生成、签名/尺寸、确定性和调色板检查；独立检查源文件的范围约束；目视检查 PNG；确认没有意外路径；报告命令退出码、SHA-256 和任何额外检查。

## 路径与删除

- 允许新增实现路径仅为 `tools/generate-preview.mjs` 与 `artifacts/codex-quota-preview.png`。
- 工作流文件由调度者另行管理。
- 不允许临时文件、下载资源、包清单、锁文件、依赖目录或额外图片。
- 授权删除：无。

## 提交

建议主题：`feat(preview): add deterministic Codex quota display PNG`。

当前目录不是 Git 仓库，因此不得初始化、暂存或提交；状态记为 `inapplicable — no Git repository`。

## 完成标准与停止条件

两个实现路径存在，生成器仅用本地内置能力，全部自动和目视检查通过，独立验证通过，Sol 审阅无实质问题，无意外路径。若需要联网、依赖、Git 初始化、删除、范围外修改，或需要先决定硬件/API/认证/时区，则停止并报告。
