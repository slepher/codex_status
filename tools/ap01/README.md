# AP01 只读研究工具

本目录保存 codex_status 的分析脚本；结论与依赖见 [研究入口](../../project-workflow/ap01/README.md)。第三方克隆保持只读，不需要在其中安装或放入本项目脚本。

## 静态分析器

`__main__.py` 使用现有 Capstone 5.0.7 及同父目录 `cuktech-ap01-firmware-tools/core/firmware_image` 校验模块，读取该克隆的 `artifacts/firmware/original/ap01-1.0.2_0031.bin` 和原始载荷源码。输入镜像需匹配工具内固定的 SHA256、长度与校验；不会联网或访问设备。

从 codex_status 根目录执行：

```powershell
$env:PYTHONDONTWRITEBYTECODE = '1'
$env:PYTHONIOENCODING = 'utf-8'
python -B tools/ap01/__main__.py --self-test
```

输出分析结果时加 `--output <本仓库中已存在的空目录>`。按仓库规则先提权创建目录；工具拒绝覆盖已有产物。目录应位于 `artifacts/ap01-research/`，不要写回第三方克隆。禁用字节码缓存可避免导入校验模块时向上游写入缓存。

## 历史资源探针

`resource_budget.py` 保留用于解释 2026-10-11 的尺寸报告，仅调整迁移后的路径。它依赖旧版 Bundle 存储结构和第三方工具链，不适配当前单份格式，不能用作当前源码的容量验证；本次不运行其编译流程。更新资源测量前须先调整探针并记录新源哈希。旧报告和对象在 `artifacts/ap01-research/raw/bridge-budget-20261011/`。
