# Task 13c — 模拟设备 owner 与 `/claim`

状态：宿主验证完成，已提交 `e591951`。基线 `7aa6d38`。

## 决策

先接设备 owner，再接 Data/Plan/Bundle。`POST /claim` 的参数净化、lease 归一化和动作判定必须调用已共享的 C++ `v2PrepareClaim` / `v2DecideClaim`（现有 `codex_v2_claim_decide` FFI）；Rust 只承担 HTTP、逻辑 uptime、owner 保存/恢复和回复格式，不复制一套 claim 决策。此切片不让 Bridge 登记模拟器，仍不提供 `/status.json`。

- 新增必填 `--data-dir <absolute path>`，只允许该实例目录内写文件。启动创建目录；`simulator.json` 仅含 schema 与规范化 MAC，`owner.json` 保存 owner；若目录已有不同 MAC 立即拒绝。拒绝目录等于或位于 `current_exe().parent()/data` 的祖先，避免把模拟存储直接指向本进程旁的 Bridge data；不猜测其他安装位置。测试用独立临时目录。此切片只保存 owner，Bundle/RTC/显示后续接入同目录。
- owner 保存字段与固件 `OwnerRec` 对齐：`id/name/host/port/since/last_seen/lease`，其中时间单位是该次启动 uptime 秒。正常关闭并重启进程可恢复 owner；重启 uptime 从零起算，加载时按固件 `ownerBegin` 规则把大于当前 uptime 的 `since`、`last_seen` 钳到当前值，给 Bridge 一个 lease 窗口续约。数据文件不可解析或 MAC 不匹配时启动失败，不悄悄清空。owner 清除后仍保留实例标识。文件 I/O 失败时返回 500，不能回复成功；突然断电/撕裂写入的恢复语义留到存储故障任务，不把本切片测试冒充其证据。
- 读取有效 owner 时按设备现状使用 `now_s - last_seen > lease` 判过期，`now_s` 与保存字段都是 `uint32_t` uptime 秒，减法使用 32 位回绕语义；过期则清空并保存。`since_s/last_seen_s/lease_s/expires_in_s` JSON 字段与 `ownerJson()` 对齐；owner 值的显示文本已由共享 C++ 净化，不允许凭控制面直接赋值 owner。
- `/claim` 先验 device bearer，拒绝时返回 401 `{"error":"unauthorized","owner":<当前有效 owner 或 null>}`。从 URL query 读取 id/name/host/port/lease/force/release；按固件规则，`force`/`release` 参数存在且值不为 `0` 即 true。空 id 在查当前 owner 前 400 `{"error":"args"}`。共享 C++ 决策后：空 owner release → 200 `{owner:null,released:false}`；被其他 id 占用 → 409 `{error:"occupied",owner:...}`；有效 release → 200 `{owner:null,released:true}`；首次 claim/force → 200 `{owner:...,renew:false}`；同 id 续约 → 200 `{owner:...,renew:true}` 且保持 since。只在成功 claim/renew 更新 `last_seen`，只在新 claim 更新 `since`。其他业务端点仍 501，不能触碰 owner。
- `/sim/state` 只读返回当前有效 owner 摘要（固件 owner JSON 形状），不泄 token；可以触发过期清理，但不能刷新 lease。ready 与 `/sim/state` 能力加 `claim`，并从 unsupported 移除。`/v2/status` 不因 owner 变化虚构新的字段。逻辑时钟控制仍只由 control token 使用；step 到 lease+1 秒之后 owner 过期，wall 跳变不影响租期。多个进程及目录互不共享状态。
- 给 `bridge-render` 增加窄的安全 Rust claim wrapper 调用现有 C ABI，不改共享 C++ 或固件源码。`device-sim` 需要的 URL query 解码选已在 Cargo.lock 中的现成依赖，不引入新框架或通用存储层。保持 localhost 边界，不连接主 Bridge、真实设备或生产数据。

## 验收

- 真实 loopback 集成测试覆盖：缺/错 device token 401 与 owner 回显；空 id 400；首次 claim/同 id renew/他人 409/force/release；UTF-8 与长度由共享 C++ 处理；暂停+step 后精确 lease 边界（等于 lease 仍有效，超过才过期），wall 偏移不影响；正常进程重启恢复并可续约；两实例不同 MAC/目录隔离；目录 MAC 冲突拒绝；其余写端点 501 且不改变 owner。
- `cargo test -p device-sim`、`cargo test -p bridge-render`、`git diff --check` 通过。Luna 只改 `bridge/crates/device-sim/`、`bridge/crates/render/src/lib.rs` 的窄 wrapper 与直接 Cargo 依赖；不改 Bridge app/core、`src/`、其他 worktree、硬件、生产服务或文档；不提交。主代理审查后更新进度并提交。

## 验证记录

`device-sim` 13 项真实 loopback 集成测试、`bridge-render` 全套（含 19 项 `v2_state`）和 `git diff --check` 通过。重复 URL query 键按首次出现值处理；此处采用解析器的稳定行为，常规 Bridge 请求不发送重复键。文件持久化只验证正常进程重启，不能替代断电撕裂恢复验证。
