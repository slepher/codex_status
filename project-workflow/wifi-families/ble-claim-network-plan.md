# BLE claim 网络准入修复

2026-10-30。需求：Bridge本身网络未就绪时不得通过BLE创建或续约owner，避免短暂唤醒阻塞正常Bridge。

共享claim入口在owner读取后、claim发送前验证：新鲜宿主网络及已配置目标、当前可用LAN地址、同地址Bridge HTTP health，所有检查有界。使用该地址发送claim，不使用lan_ip的127.0.0.1回退。无网络/未知权限/缺映射/无可用地址/监听不可用时明确失败、不发claim和后续Plan/Data。当前有线族仍需真实LAN接口，不能仅凭持久配置准入。保持401/409、未知提交、租约期限和设备侧owner语义。

测试沿用生产ble_cycle Fake；覆盖创建/续租网络拒绝、健康路径、已占用/未知ACK回归。不停止现有Bridge、不刷ROM、不部署Mac；本机无法执行macOS原生验收。
