# 只使用当前设备信息字段

## Plan
按用户要求删除device.state模板兼容，从固件/Bridge校验/宿主渲染/预览/模板移除，不自动迁移或解释旧字段。保留独立wifi_state、wifi_blink、ble_manual_active、mode等当前字段。

## Tasks
- 删除旧绑定和渲染环境参数，迁移仓库模板与测试。
- 加强已编译模板加载校验：未知/退役字段、路径与编号不符、未终止字符串均拒绝；保留有效字段的固定编号，禁止旧编号误解为另一字段。
- 已安装模板失效时进入内置恢复界面并报告错误，保留文件供显式替换；新增加载/重启拒绝回归。
- Luna执行回归与Note4候选构建，主agent审查。生产Bridge/Fake不停止，暂不刷固件或发布模板。

## Status
实现、Luna回归及主agent审查完成。固件、Rust校验/编译、宿主FFI/CLI及预览均移除device.state；codex-status-a v5、quad v14使用独立字段。实际设备仍为原.83+v4，未刷机或发布模板。

最终Note4候选 `artifacts/note4-0.18.83-device-fields-candidate.bin`，1,837,936 B，SHA256 `32486C0763DB240777758F549BCE386F8884A09547AD3BC89F4423E499664517`；同名ELF 30,484,548 B，SHA256 `BCFAC9210A8D4FE19E5E442F3D0DA5CB8CA345DB318A2C8D226FD882745CD3DD`。版本仍0.18.83-note4；RAM118916 B与前候选相同，Flash1786151 B（+544 B）；最终构建与匹配ELF64项栈检查通过。git diff --check通过。仅保留本地候选，不代表部署。

Luna回归：core模板8/8、render64/64、Fake bootstrap64/64、预览7场景、模板CRC两项及固件模块检查通过。CRC有效的持久旧绑定在模拟重启后明确拒绝，读取失败不改变文件；显式重新发布有效Bundle后可恢复。未知字段在直接bind与when exists:false中均拒绝。

证据：[Luna结果](../../artifacts/device-fields-only-20261011/results.md)。首次core/Fake运行暴露旧测试版本号和已删除Env.state初始化，修正测试后通过，原失败日志保留。

## Review
旧字段模板应明确失败，不得静默渲染缺项、改写含义或崩溃。功能不支持与已支持字段的数据缺失分开：后者继续由模板when决定显示。

- CT加载原来仅检查path非空，现补字段支持性、路径/编号一致性与NUL边界检查，绘制前同样检查。退役编号14不重用，其他有效编号不移动，不做旧字段解释。
- 加载失败设置v2CtValid=false及content_error，Data和诊断不再访问无效CT；显示转内置恢复界面。真实v4包含退役字段，将来升级必须配套有效v5 Bundle；本轮不改现场。
- 主agent发现JSON缓存校验额外链接约11KB静态scratch，改为复用无活动Bundle时已有的state.v2Ct，不新增大栈对象或长期缓冲。持久格式/条目大小不变，没有新的持久写入；空/单/典型/上限存储字节均不变。动态分配仍为原解析器的有界临时String，实机峰值与EPD恢复尚未验收。
- 读取旧模板失败时保留文件的保证不扩展为新Bundle安装失败时保留旧内容；单份Bundle覆盖仍遵守已有维护界面流程。
