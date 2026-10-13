# 小米扫码查询ROM的安全审查

2026-10-13补充：用户已持有并绑定AP01，报告0031可升级0041；实际云查询已确认0041/MD5。下文“无设备/现在不扫码”是2026-10-11原版审查时的条件。本仓库下载工具不调用原版ensure_login入口；用户最新明确要求保存登录凭据，现扫码成功立即保存Windows当前用户DPAPI缓存并读回，再开始查询/下载，默认复用缓存。见 [下载专项](firmware-download-20261013.md) 与 [运行方法](../../tools/ap01/README.md)。离线保存/重载/失败后缓存保留自检通过；实际扫码/文件取得状态以专项记录为准。此前仅内存的安排已被用户最新授权替代。

2026-10-11；针对克隆仓库3bd98a8原代码，未实际扫码、未读取用户凭据。仅静态审查与目录权限查看，不是小米协议或依赖供应链的完整安全认证。

## 结论

查询/下载本身不调用OTA；审查范围未发现主动向仓库作者服务器发送账号凭据的代码。但原版扫码会取得可复用passToken、读取账号设备列表并明文落盘，且要求账号里存在AP01。用户目前没有设备，直接运行原版既扩大账号授权接触面，也很可能无法完成目的。因此不建议直接用主账号扫描原版二维码。

此前“型号查询不带设备编号”的说明仅适用于firmware_info()单个接口；完整ensure_login()仍调用ap01_device()，已纠正。这里的“只读查询”是本次操作限制，不是云端颁发了只读权限令牌。

## 调用链和目的地

app/ap01_official_firmware.py仅公开login/info/download子命令；与可安装固件的app/ap01_firmware.py区分。
- login.py:19固定https://account.xiaomi.com/longPolling/loginUrl，sid=xiaomiio，callback为https://sts.api.io.mi.com/sts。
- 官方响应返回二维码qr和轮询lp地址，再由requests请求。登录返回userId/passToken；代码本地生成deviceId。
- xiaomi_cloud.py:192起用passToken访问https://account.xiaomi.com/pass/serviceLogin，再访问响应中的location取得serviceToken。
- xiaomi_cloud.py:269固定API前缀https://api.io.mi.com/app/。
- ensure_login在login.py:134/148读取设备列表，必须找到AP01后才保存env/mi-cloud.env。
- firmware_info在xiaomi_cloud.py:307提交home/latest_version和model=njcuk.enstor.ap01，无DID。
- download_latest_official_firmware只获取文件、核对同次云端MD5、计算SHA256并保存只读BIN。没有调用rpc/miIO.ota，也不执行下载的文件。

## 发现

| 发现 | 影响与证据 |
| --- | --- |
| 明文可复用凭据 | login.py:151/155保存passToken到env文件；客户端可用它续取serviceToken，不是一次性固件查询票据 |
| Windows权限假设不成立 | login.py:194/196只有chmod(0600)，不设置Windows DACL；不能据此称仅当前用户可读 |
| 登录要求已有AP01 | login.py:134/148和xiaomi_cloud.py:340，账号无AP01则异常退出；不适合当前无设备用户直接照SOP |
| 动态URL未加目的域名限制 | qr、lp、location直接采用官方响应，requests允许重定向；固件只检查初始scheme=https，未核对下载最终主机/协议 |
| 异常可能打印带票据URL | 多处拼接原始requests异常，入口又输出异常文本；不能把错误日志当作一定已脱敏 |
| 凭据清理/撤销缺位 | QR/登录态没有完成任务后自动销毁；删本地文件也不等于撤销服务端会话 |

动态URL问题依赖上游响应/重定向或环境被影响等前提，不是已发现恶意服务器外传。默认HTTPS证书校验未关闭；没有看到此流程传输明文密码、发送固件更新或关闭证书校验。正常成功输出隐藏下载URL及登录令牌；env/*.env和artifacts/official-firmware/被Git忽略，但Git忽略不能代替访问权限或令牌撤销。

Python官方说明Windows的chmod只能设置只读标志：[os.chmod](https://docs.python.org/3/library/os.html#os.chmod)。本次查看的是env父目录ACL，未验证一个尚未生成的凭据文件实际ACL；因此不宣称已经有他人读到凭据。

## 用户现在应怎么做

现在不需要扫描二维码，也不需要把密码、验证码、二维码照片或token发到聊天里。更低接触面的方法是取得别人已公开的原厂BIN/公开下载链接，由本机检查版本和哈希。

若随后决定使用自己的账号，建议先提供单次“登录后仅按型号查版本和下载”的专用流程：
1. 登录凭据只在单进程内存中使用，不写env，不列举设备；云端是否允许未绑定设备账号查询仍待验证。
2. 固定小米登录/API目的地，限制二维码/轮询/回调和下载的HTTPS重定向；错误输出去掉URL参数和凭据。
3. 只下载BIN到本机，不调用任何设备控制或OTA方法；完成/取消关闭进程并清理二维码。
4. 用户仅在官方米家App内核对授权页后确认；若手机显示非预期账号或登录请求就取消。完成后在小米账号支持的登录/授权管理中撤销此次会话；不能承诺退出脚本即云端失效。

以上是建议流程，本次未实现这个专用流程、没有生成二维码，也没有替用户同意账号登录。原版SOP现阶段不适用本用户，故不提供看似可直接成功的扫码命令。
