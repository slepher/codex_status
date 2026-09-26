# Fake ROM F 代表性验收证据

本表按 `docs/fake-rom-simulator-design.md` 的 M01–M18 分类。每类选择能区分正确与错误的代表性软件 case；不排列所有错误组合。硬件边界只见 `hardware-cases.md`，未实测。

| 类别 | 代表性证据 | 边界 |
|---|---|---|
| M01 | `app` 的 `online_offline_and_mismatched_updates_stay_with_their_mac`，`core` 的 `authenticated_snapshot_survives_failed_attempt_and_restart` | 旧认证快照保留，失败尝试单列 |
| M02 | `core` 的 `save_does_not_publish_and_publish_freezes`、`full_bundle_job_survives_restart_with_frozen_bytes`；隔离 Bridge 两族显式发布后认证交付 | 不把保存算作发布 |
| M03 | `app` 的 `cycle_targets_use_registered_mac_status_and_ignore_current_selection`、`claim_refuses_wrong_target_mac_without_posting`；隔离双 MAC 100 次另一目标联系 0 误传（见 PROGRESS） | MAC 是身份，IP 是属性 |
| M04 | `device-sim` 的 `writes_require_their_domain_token_then_reject_unconfigured_commands`、`claim_uses_shared_decision_and_enforces_token_owner_actions`、`plan_checks_json_session_owner_and_token_order_without_advancing_state` | 代表 401、409、nonce/session 失败；不穷举 |
| M05 | `device-sim` 的 `plan_acks_share_firmware_decisions_and_replays_keep_the_deadline`、`configured_power_sleeps_and_timer_and_button_wakes_have_distinct_windows`；`core` 的 `data_reads_and_status_polls_never_extend_light` | USB、低电软件决定各有定向 case |
| M06 | `device-sim` 的 `real_v2_client_installs_one_and_eight_template_bundles`、`bundle_hard_exit_during_slot_or_metadata_sync_restores_prior_job`；`core` 的编译/CRC/target 拒绝测试 | 1–8 项全按键循环 |
| M07 | 隔离 Bridge 在设备提交后磁盘 `sending`，Bridge 重启 `unknown`，认证状态对账 `succeeded`；`core` 的 `device_status_reconciles_lost_bundle_ack` | 证据详见 PROGRESS 最新 Fake ROM 节 |
| M08 | `core` 的 `ack_loss_retries_same_seq_and_content`、`stale_ack_never_confirms_newer_data`、`full_sync_deadline_only_moves_on_ack`；`device-sim` 的 `data_bound_frame_matches_shared_preview_after_value_change` | 成功帧逐字节对拍 |
| M09 | `core` 的 `push_change_queues_full_snapshot_with_pull_fields`、`pull_only_change_does_not_push_or_change_powerplan`、`failed_fetch_keeps_last_values` | 数据源不是设备伪造值 |
| M10 | `device-sim` 的 `committed_frame_matches_shared_preview_bits_byte_for_byte`、`device_clock_bind_uses_the_experiment_wall_time`、显示失败后全刷/预算断言、8 项循环断言；`app` wake history 测试 | 物理面板错误在 H02 |
| M11 | `device-sim` 的 `bundle_hard_exit_during_slot_or_metadata_sync_restores_prior_job`、`experiment_clock_survives_process_restart_without_reusing_boot_uptime`、`instance_identity_rejects_mac_conflict_and_corrupt_owner_file` | 文件同步切点直接退出，无伪 ACK |
| M12 | `device-sim` 的 `ota_switches_only_after_a_complete_catalogued_upload_or_explicit_override` | 错 token/target、截断/目录外代表 case |
| M13 | `device-sim` 的 `ota_upload_can_commit_after_its_ack_is_lost`、`ota_pending_survives_process_death_before_delayed_reboot`；`core` 的 `ota_freezes_per_mac_and_never_reuploads_after_restart` | 运行镜像精确 SHA 无设备证明 |
| M14 | `device-sim` 的 OTA override/上传来源断言；隔离 Bridge OTA 保持 `awaiting_confirmation` / `version_seen_unproven`（见 PROGRESS） | 版本相同不能推断 `image_verified` |
| M15 | `core` 的 `publish_freeze_and_queue_order`、`queued_ota_cancel_removes_frozen_blob`、`incremental_job_persists_frozen_bytes_across_restart_and_edit` | 同类任务冲突按产品合同 |
| M16 | `device-sim` 的 `timer_ble_rendezvous_accepts_formal_plan_before_http_opens`、`bridge_v2_connection_uses_fake_ble_without_os_radio`；隔离 Bridge 24 h 协作运行逐 BLE 窗口断言 `V2Connection` 会合成功 | 真实射频/GATT 缓存在 H01 |
| M17 | `device-sim` 的 `instances_advance_at_independent_rates`、`wall_offsets_do_not_change_monotonic_and_invalid_commands_are_atomic`；`core` 的 `device_clock::independent_views_and_wall_jump_do_not_move_monotonic`；`app` 的 `fake_wall_jump_does_not_extend_status_or_claim_freshness` | 实验单调与 wall 分开 |
| M18 | `device-sim` 的实例锁、坏 owner 文件、OTA pending/Bundle 写中硬退出；隔离 Bridge 的 M07 重启对账 | 仅隔离目录；生产数据未触碰 |

全链长跑、确定性回放及 Bridge/设备进程恢复的实际次数与结果以 `PROGRESS.md` 最新 Fake ROM 节为准；表中单测名只说明相应行为已可重复执行，不代表硬件认证。
