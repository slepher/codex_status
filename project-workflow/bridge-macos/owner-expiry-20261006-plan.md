# Owner expiry repair and contact continuation

Authorized: fix expiry, continue Note4 ROM/USB and Bridge contact.

1. Exercise actual owner_store.cpp with simulated RTC, NVS and reset causes: same boot expiry, repeated deep wakes, asleep expiry, cold restart, stale identity/corruption/clock rollback, matching renewal and foreign refusal.
2. Replace boot-relative expiry with a 24-byte RTC deadline checkpoint. Deep wake restores only the verified original deadline; reset without proof clears stale occupancy. No new NVS format or credential storage, no automatic owner transfer.
3. Verify status expiry shares this clock; build only Note4 .64, matching ELF stack/marker/hash.
4. Flash matching COM5 device application only, run raw serial and authenticated Bridge observations. If BLE-only startup exposes the claim-before-Wi-Fi dependency, route explicit authenticated claim through the existing claim decision/authorization contract rather than bypass ownership.
5. Preserve data; update progress/backlog with final evidence and remaining limits. No commit.

## Validation and resource review

Actual owner_store.cpp host test passes same-boot expiry at the exact boundary, four deep wakes without renewal, asleep expiry, legal matching renewal, foreign refusal, cold/software reset, corrupted checksum, mismatched identity, RTC rollback, legacy record recovery, unauthorized/invalid claim and NVS write failure. Bridge app: 68 passed/1 pre-existing ignored. Production ble_cycle fake seam covers claim before Plan, foreign owner, storage rejection, local yielded state and lost ACK with one request/no force/no subsequent Plan. Render projection proves capability plus bounded occupied/storage/unauthorized replies below 512 bytes.

Final firmware .64: 1860992 B / SHA256 5e02fb5b00b225d1025ce53f0b2418c09f4e3ecc5681395000cccbf5994e7ec2; ELF a7cb6a15ae708560ec2a57ad0b220898d6076b7d9e4f89d37dfd5c133341c514. Static RAM 117324 B (unchanged), flash 1814187 B. RTC .data 4992→5016 B, exactly +24 B, one hardware retention checkpoint, not an array/repository copy. No extra NVS keys/credential duplication/serialized native struct. Existing owner limit remains one variable-length record: empty has no payload; illustrative id/name/host 4/6/13 bytes plus numeric14 =37 payload bytes; maximum UTF-8 lengths128/64/128 plus14 =334 payload bytes. NVS page/entry metadata remains pre-existing, not part of these payload counts. WFS empty/single/typical/full repository budgets unchanged.

The extra claim candidate/readback copies scale with actual owner strings, not maximum buffers. During persistence the caller arguments, current/decision/candidate/readback and global owner can hold up to six string sets (at the maximum, about2208 buffer bytes after Arduino String allocation rounding), excluding JSON transport/platform allocator overhead; Arduino String allocates by need/SSO, so these are analytical bounds, not measured RF peak heap. No complete Wi-Fi repository is placed on the claim stack. Real frames: ownerBegin80, ownerApplyClaim224, ownerClaim224, handleBleClaim480 B; HTTP claim local chain1136 B. BLE dispatcher1136 B was confirmed in .63 and does not increase; its longest local Wi-Fi chain3584 B is below4096 B, leaving platform/caller room in8192 B loop stack. Wi-Fi stack gate passes. Compiler initially inlined the helper; noinline retains the audited handler boundary. Do not attribute the pre-existing dispatcher frame to this change.

Only the default Windows Bridge was backed up/rebuilt/restarted; data-before-owner and bridge-before-owner.exe are private ignored evidence. Exe SHA256 d170041fcbdea13e1906215b542790be8e914aa9c050f5857e24fea6c5e280c7, PID66092, scheduled task Running. No Mac deployment or other device target. USB write and runtime contact validation underway; final results belong in PROGRESS/backlog, no commit.

Runtime .64 initially returned the existing free wifi_owner shape: ok=true with owner_id omitted. The first new Bridge helper incorrectly required a string field. Corrected to accept absence only in a successful authenticated reply and still reject wrong-typed identity. Added free_legacy regression through actual ble_cycle; focused matrix passes. This Bridge-only correction needs no ROM rewrite. Original .64 startup proves normal status/advertising and no panic; final claim/contact follow-up pending.

Second real .64 contact sent claim but rejected it: source proved authValid false because the old loadOrIssueAuthToken lived only in the Wi-Fi-connected branch, while the Bridge reused its correctly cached token and did not trigger a GATT token read. Move unchanged NVS token load before all startup BLE entry and remove connected-mode reread. .65 is the final candidate; this is an initialization dependency of the BLE-first adapter, not a token rotation/bypass. Startup source guard now requires operation token initialization before contact. .64 artifacts/logs remain evidence and must not be the final published ROM.

配网续验：统一事务 recovery_save 不应依赖不存在的全量 snapshot；用已认证 active 的 revision 作原子单行 upsert 前置，保留用户编辑的 snapshot 冲突检查及原 ID query-only。BLE 共用适配器 apply 映射既有 wifi_save（固件不接收 wifi_apply）。增加共用服务无快照/丢 ACK/拒绝回归后重建 Windows 并实体验证。

用户续问未缓存密码：实机完整 HTTP snapshot revision9 中 home 已缓存，wifi-slot-0/1 是 legacy 导入记录。projectWifiNetwork 仅 writable 返回 password，导致 legacy只读记录丢失已有凭据。修复认证快照中的 legacy 凭据投影，仍只读、不放宽其他绑定；投影回归、.66 Note4 构建/栈/哈希后应用槽刷写及全量读取验收。

更正：slot记录真实bond=f0:18:98:6b:98:c6/0（Mac），不是legacy，前述分类错误。.66仅legacy投影不足，最终改.67：sessionValid+claimed的认证owner完整快照读取所有已存WFS凭据；writable仍按namespace，非owner其他绑定仍隐藏。回归通过生产分页读验证两权限，保留原密码、不迁移/删除其他绑定。

资源与闭环补充：Windows C86B2953…573222E/PID26532 已部署；无快照单条 recovery_save 先认证 active revision、持久冻结expected_revision再原子apply，BLE适配wifi_save；core15 Wi-Fi+新增无快照/冲突/失ACKquery回归，app68 passed/1ignored。实机.65保存原w3_23提交revision8，activate w3_26完成revision9，home实际wd21-la/192.168.3.177/http_ready=true；Windows实际wd21-wo。认证HTTP完整读已成功（旧Mac绑定两条密码被旧权限投影隐藏，home有密码），因此不能把提示继续归为读取失败。.67仅读投影变更无NVS布局/新仓库/复制压栈；空0额外、实际2他绑定≤156B额外序列化密码字段、最多12条时其他8条≤624B（密码≤64+字段约14）。JSON及完整字符串在冻结阶段并存，额外峰值是这些字节与节点/分配开销之和，未测实机峰值heap；无需全仓库预分配。静态RAM117324B不变，最终ELF Wi-Fi及owner栈门禁通过。

.67重启第一轮BLE读取：note_link外层250ms取消了第四条分片写，后续owner/snapshot/match无应答；appendJson未完成帧污染本链，断开恢复。取消改用原共享绝对deadline，超时先close且不复用链。sync_open明确拒绝不应撤销已确认light Plan的独立配网授权，记录阶段错误后继续light工作；sleep/未知Plan仍原拒绝行为。Bridge回归重建再验，不改GATT/不增窗口。

HTTP完整快照读曾成功但其他绑定仍未缓存：客户端分页未传已获取的session_nonce，sessionValid=false。wifi_snapshot明确收nonce参数、每页发送；统一机会传当前nonce，旧协议测试显式空nonce。core分页401/格式/过期/修订/MAC/偏移错误回归同时断言真实请求携带nonce，最终Bridge重建部署再读取确认。.67认证/api/ota/image SHA256 ADC9CF4F…F69BBC1 与ROM匹配，ota_0；分区/NVS/otadata before/after逐字节一致。

最终验收：.67 ota_0认证运行SHA与ROM一致；Windows最终4A8D2FB8…FAC58D7/PID40328。完整读w3_2b completed/revision10，旧Mac两条与home三条password_cached=true，Mac两条仍writable=false；wd21-la/192.168.3.177认证HTTP正常。app最后68passed/1ignored，coreHTTPnonce失败合同与所有已列门禁通过；证据与remaining矩阵登记PROGRESS顶节/backlog B6，无提交。
