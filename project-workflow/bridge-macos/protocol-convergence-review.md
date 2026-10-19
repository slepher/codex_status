# 协议收敛审阅

2026-10-19，R1–R5源码完成。依据[原核验](protocol-review-validation-20261019.md)，验证与产物以[状态](protocol-convergence-status.md)为准。

替换路径已经删除：Wi-Fi只有core Job/Transport执行与收尾；Connection输出不再被HTTP结果反向改写；本轮会合用具名上下文，不再藏私有JSON状态。同步HTTP已移除，取消会结束TCP future并释放许可，不再需要worker存活跟踪。原生BLE/系统查询线程不属于被替换的HTTP机制。

CommandContext建于载体认证、session/replay/context准入之后，业务身份与原事务围栏保留。C2拒绝C1及旧JSON命令字段，Wi-Fi操作使用显式编号，未知名称不能回落到通配编号。删除旧串口wifi-json写入及无会话管理函数，避免设置sessionValid=true后留下非帧绕路。裸OTA仍需设备token。

持久connection schema2只恢复意图/退避；旧瞬时观察/HTTP成功不恢复，认证安全错误保留。观察和联系按boot/时间投影；较新设备失败不能被旧HTTP成功覆盖。终态未知仍阻挡不安全重发，只能查询原事务；接受不等于完成，不提前ACK。

491项完整回归通过，5条件忽略；合并后的现场失败初态与最终迁移断言另作定向复验。两款ESP的版本/产物/匹配ELF栈门禁通过；AP01真实应用宿主ASan/UBSan与RV32链接通过，完整候选不可据此直接刷写。未执行本轮长期RF、Flash掉电、原生BLE取消完整矩阵、AP01恢复ABI/屏幕验收；源码验收后用户已授权Windows/Note4部署，实际结果见[rollout](protocol-convergence-rollout.md)。

旧Bridge不能读取新持久schema作为回滚方式，须恢复冻结exe/data。代码行总数不是验收指标：删除旧执行/状态/协议路径，同时增加必要失败与迁移回归；不以删校验或压缩排版凑净减少。

后续用户授权部署已完成：Windows重编译、Note4单次直接OTA及运行哈希证明，15分钟观察/OTA后7次自然交汇、16条历史Job/Profile不变。初始升级/到期HTTP超时及Sleep阶段HTTP离线均在rollout如实分账，不宣称全程零错误。
