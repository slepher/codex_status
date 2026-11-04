# 审核边界

传输层不得重发未知修改，错误须保留 NotSent/WriteUnknown/AckUnknown。成功必须核对 MAC/协议/事务 ID；最终运行集合核验属于后续事务协调器。保持旧 OTA/配对/Wi-Fi 入口，不创建业务 owner。

回执文件由单一 application worker 拥有；首次 create 必须完成后才允许修改设备。任何落盘错误均停止修改，重新加载文件并查询原事务，不能凭内存状态重发。32 KiB 读取上限，Manifest base64 最大 4780 字符；初始失败可留下损坏文件，后续必须明确报错，不覆盖重新生成事务。
