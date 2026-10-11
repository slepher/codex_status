//! One rendezvous transaction: BLE exchange, early close, authenticated HTTP
//! verification and the frozen admitted HTTP work set.
use super::*;
use bridge_core::rendezvous::{Budget, CLOSE_BUDGET, HTTP_PROOF_BUDGET};

fn report_route(target: &str) -> Result<std::net::Ipv4Addr, String> {
    let device = target.split(':').next().unwrap_or(target)
        .parse::<std::net::Ipv4Addr>().map_err(|_| "report_device_address_invalid")?;
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .map_err(|_| "report_route_unavailable")?;
    socket.connect((device, 80)).map_err(|_| "report_route_unavailable")?;
    let local = match socket.local_addr().map_err(|_| "report_route_unavailable")?.ip() {
        std::net::IpAddr::V4(ip) => ip,
        _ => return Err("report_route_unavailable".into()),
    };
    if local.is_unspecified() || local.is_multicast() ||
        (local.is_loopback() && std::env::var_os("CODEX_STATUS_SIM_BLE_ENDPOINTS").is_none()) {
        return Err("report_route_unavailable".into());
    }
    Ok(local)
}

// Explicit claim is required before BLE provisioning can create the HTTP path.
// Reuses the HTTP claim permission/decision contract on capable firmware.
pub(super) async fn claim_on_ble(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection) -> Result<(), String> {
    if link.info["ble_claim"] != 1 { return Ok(()); }
    if ctx.device_status_cache.lock().unwrap().get(mac).is_some_and(|s| s.yielded) {
        return Err("owner_yielded".into());
    }
    let owner = link.command("wifi_owner", json!({})).await.map_err(|_| "owner_check_failed")?;
    if owner["ok"] != true { return Err("owner_check_failed".into()); }
    // Established wifi_owner replies omit owner_id when free. A successful
    // authenticated reply proves that state; a wrong-typed field never does.
    let id = match owner.get("owner_id") {
        None => "", Some(Value::String(id)) => id.as_str(),
        _ => return Err("owner_check_unknown".into()),
    };
    if !id.is_empty() && id != ctx.bridge_id { return Err("occupied".into()); }
    let token = bridge_mcp::load_device_token(&crate::mcp_config(ctx), mac).ok_or("device_token_required")?;
    let name = ctx.bridge_name.lock().unwrap().clone();
    let ack = link.command("claim", json!({"operation_token":token,"name":name,
        "host":bridge_ble::lan_ip(),"port":ctx.config.port,"lease_s":300})).await
        .map_err(|_| "claim_result_unknown")?;
    if ack["result"] != "applied" || ack["owner_id"] != ctx.bridge_id {
        return Err(ack["error"].as_str().unwrap_or("claim_rejected").to_owned());
    }
    let remaining = ack["expires_in_s"].as_u64().filter(|s| *s > 0 && *s <= 300).ok_or("claim_result_unknown")?;
    crate::update_device_claim_cache(&mut ctx.device_status_cache.lock().unwrap(), mac,
        Some(json!({"id":ctx.bridge_id,"lease_s":300,"expires_in_s":remaining})), device_now(mac) as i64, false);
    tracing::info!(device_mac=%mac, transport="ble", "explicit device claim confirmed");
    Ok(())
}

/// One real GATT rendezvous. Small snapshots stay on BLE; larger work receives
/// a formal light plan and is then delivered by the HTTP cycle.
pub async fn ble_cycle(ctx: &AppCtx, candidates: &[String]) -> Result<BleOpportunity, String> {
    if candidates.is_empty() {
        return Ok(BleOpportunity::NoDevice);
    }
    crate::host_wifi::require_device_bluetooth()?;
    let _delivery = ctx.device_delivery.lock().await;
    let (mac, mut link) = match bridge_ble::DeviceConnection::connect_any(
        candidates,
        &ctx.config.token,
        &ctx.bridge_id,
    )
    .await
    .map_err(err_text)?
    {
        Some((mac, link)) => (mac, link),
        None => return Ok(BleOpportunity::NoDevice),
    };
    let lock = delivery_lock(ctx, &mac);
    let _device_delivery = lock.lock().await;
    let still_registered = service(ctx).devices().into_iter().any(|device| {
        device["device_mac"]
            .as_str()
            .and_then(DeviceIdentity::normalized_mac)
            == Some(mac.clone())
    });
    // Fetch the token before the exchange budget so a short BLE window can
    // close immediately after the accepted network-start command.
    if still_registered {
        cache_device_token_if_missing(ctx, &link, &mac).await;
    }
    let http_window = Budget::new(Duration::from_secs(90));
    let mut admitted_wifi: Vec<String> = service(ctx)
        .wifi_state(&mac)
        .jobs
        .iter()
        .filter(|job| bridge_core::rendezvous::admit_wifi(job, device_now(&mac)))
        .map(|job| job.job_id.clone())
        .collect();
    let mut admitted_at = std::time::Instant::now();
    let mut recovery_state = None;
    let work = async {
        if !still_registered {
            return Err(format!(
                "connected BLE device {mac} is no longer registered for device protocol"
            ));
        }
        link.write_endpoint(&bridge_ble::lan_ip(), ctx.config.port, &ctx.config.token)
            .await
            .map_err(err_text)?;
        let mut state = link.command("status", json!({})).await.map_err(err_text)?;
        if state["result"] != "applied" {
            return Err("BLE status rejected".to_owned());
        }
        let received_at = device_now(&mac);
        service(ctx)
            .note_device_status_at(&mac, &state, "ble", received_at)
            .map_err(err_text)?;
        service(ctx)
            .note_ble_contact_at(&mac, received_at)
            .map_err(err_text)?;
        // BOOT may expose GATT while all normal contact remains on HTTP.
        if state["automatic_contact"] == false {
            return Ok((state, false, false, false));
        }
        claim_on_ble(ctx, &mac, &mut link).await?;
        let sync_capable = state["sync"]["v"] == 1;
        let mut sync_enabled = state["sync"]["enabled"] == true;
        if sync_capable && !sync_enabled {
            if let Ok(reply) = link.command("sync_config", json!({"enabled":true})).await {
                sync_enabled = reply["result"] == "applied";
            }
        }
        let decision = service(ctx).next_delivery(&mac, true, device_now(&mac));
        let temporary_data_seq = (decision["decision"] == "light_data" && decision["temporary_data"] == true)
            .then(|| decision["data_seq"].as_u64()).flatten();
        state["_temporary_data_seq"] = json!(temporary_data_seq);
        let pending_data = (decision["decision"] == "ble_data")
            .then(|| service(ctx).data_message_body(&mac)).flatten();
        let remaining = state["power"]["provisional_remaining_s"]
            .as_u64().unwrap_or(0).min(300) as u32;
        let failure_report = state["network_error"].as_str().is_some();
        let needs_http = !failure_report && (remaining > 0 ||
            (state["power"]["mode"] == "light" && state["power"]["remaining_s"].as_u64().unwrap_or(0) > 0) || !admitted_wifi.is_empty()
            || service(ctx).wifi_state(&mac).connection.is_some_and(|c| c.state == "requested")
            || matches!(decision["decision"].as_str(), Some("light_data" | "bundle" | "activate" | "waiting_for_device_contract"))
            || service(ctx).ota_job(&mac).is_some_and(|job|
                matches!(job.state.as_str(), "queued" | "awaiting_confirmation"))
            || (sync_enabled && state["sync"]["retry_skip"] == 0
                && (state["sync"]["due"] == true || state["sync"]["pending"] == true)));
        if failure_report {
            tracing::info!(event="wifi_failure_report", device_mac=%mac,
                reason=state["network_error"].as_str().unwrap_or("unknown"),
                "device network attempt ended; next period may retry");
        }
        tracing::info!(event="network_admission",device_mac=%mac,needs_http,
            delivery=decision["decision"].as_str().unwrap_or("unknown"),
            wifi_jobs=admitted_wifi.len(),
            ota_state=service(ctx).ota_job(&mac).map(|job|job.state).as_deref().unwrap_or("none"),
            diagnostics_due=state["sync"]["due"]==true,
            "rendezvous network work admission");
        // A reachable HTTP service can complete admitted work directly.
        let reachable = if !needs_http { false } else if let Some(target) = device_link_for_mac(ctx, &mac) {
            let expected = mac.clone();
            let probe = blocking(move || {
                match device_client::status(&target.ip, &target.token, Duration::from_millis(400)) {
                    Ok(s) if DeviceIdentity::normalized_mac(s["device_mac"].as_str().unwrap_or("")) == Some(expected) => Ok(true),
                    Ok(_) => Err("http_identity_mismatch".into()),
                    Err(error) if bridge_core::rendezvous::wifi_error(&error).1 == "unauthorized" => Err("unauthorized".into()),
                    Err(_) => Ok(false),
                }
            }).await;
            match probe { Ok(ready) => ready, Err(error) => { crate::wifi::http_failed(ctx, &mac, &error); return Err(error); } }
        } else { false };
        state["_http_reachable"] = json!(reachable);
        state["_need_http"] = json!(needs_http);
        let requested = service(ctx).wifi_state(&mac).connection.is_some_and(|c| c.state=="requested");
        state["_wifi_repair"] = json!(needs_http && (!reachable || requested));
        if reachable && !requested { crate::wifi::http_ready(ctx, &mac); }
        else if needs_http && remaining == 0 {
            let _=tokio::time::timeout(Duration::from_millis(700),crate::wifi::prepare_connection(ctx,&mac,&mut link,remaining>0)).await;
            if !reachable { crate::wifi::http_failed(ctx,&mac,"http_unavailable"); }
        }
        admitted_wifi = service(ctx).wifi_state(&mac).jobs.iter()
            .filter(|job| bridge_core::rendezvous::admit_wifi(job, device_now(&mac))).map(|j| j.job_id.clone()).collect();
        let plan = service(ctx)
            .plan_for_rendezvous_network_work(
                &mac,
                device_now(&mac),
                if remaining > 0 {
                    "manual"
                } else {
                    "rendezvous"
                },
                remaining,
                needs_http,
            )
            .map_err(err_text)?;
        let mut plan_body = serde_json::to_value(&plan).map_err(err_text)?;
        plan_body["need_http"] = json!(needs_http && !reachable);
        if let Some(data) = &pending_data {
            plan_body["data"] = data.clone();
        }
        if !sync_enabled && wake_history_sync_due(&mac, &state) {
            plan_body["history_sync_ms"] = json!(2500);
        }
        // Timeout means unknown, not rejected. The device may already have
        // applied the persisted plan and started HTTP. Reconcile the same ID.
        state["_sent_plan"] = serde_json::to_value(&plan).map_err(err_text)?;
        state["_plan_ack_unknown"] = json!(true);
        state["_sync_enabled"] = json!(sync_enabled);
        recovery_state = Some(state.clone());
        admitted_at = std::time::Instant::now();
        let reply = link.command("plan", plan_body).await;
        let ack_unknown = reply.is_err();
        let ack = reply.unwrap_or_else(|_| json!({"result":"unknown"}));
        state["_sent_plan"] = serde_json::to_value(&plan).map_err(err_text)?;
        state["_plan_ack_unknown"] = json!(ack_unknown);
        let accepted = ack["result"] == "applied" && ack["plan_id"].as_u64() == Some(plan.plan_id);
        let ack_category = if !accepted {
            "ack_rejected"
        } else if ack["plan_id"].as_u64().is_some_and(|id| id != plan.plan_id) {
            "ack_mismatch"
        } else {
            "none"
        };
        if !accepted && !ack_unknown {
            tracing::warn!(event = "ack", device_mac = %mac, plan_id = plan.plan_id,
                result = ack["result"].as_str().unwrap_or("unknown"),
                ack_plan_id = ack["plan_id"].as_u64().unwrap_or(0),
                accepted_remaining_s = ack["accepted_remaining_s"].as_u64().unwrap_or(0),
                error_category = ack_category, transport = "ble", "device PowerPlan rejected");
            return Err("BLE PowerPlan rejected (ack_rejected)".to_owned());
        }
        let confirmation = if accepted {
            service(ctx).note_plan_ack(
                &mac,
                plan.plan_id,
                ack["accepted_remaining_s"].as_u64().unwrap_or(0) as u32,
                remaining > 0,
                device_now(&mac),
            )
        } else {
            json!({"outcome":"unknown"})
        };
        if let Some(body) = &pending_data {
            let data_ack = &ack["data"];
            let applied = accepted && data_ack["result"] == "applied"
                && data_ack["data_seq"] == body["seq"]
                && data_ack["active_context_id"] == body["active_context_id"];
            if !ack_unknown {
                let outcome = service(ctx).note_ack(&mac, DeliveryKind::BleData,
                    body["seq"].as_u64().unwrap_or(0),
                    body["crc"].as_str().unwrap_or(""), applied,
                    data_ack["display_state"].as_str().unwrap_or("unchanged"));
                tracing::info!(event="ack", device_mac=%mac,
                    seq=body["seq"].as_u64().unwrap_or(0),
                    data_seq=data_ack["data_seq"].as_u64().unwrap_or(0),
                    result=data_ack["result"].as_str().unwrap_or("unknown"),
                    outcome=outcome["outcome"].as_str().unwrap_or("unknown"),
                    transport="ble", "joint Plan/Data acknowledgement");
            }
        }
        tracing::info!(
            event = "ack",
            device_mac = mac,
            plan_id = plan.plan_id,
            result = ack["result"].as_str().unwrap_or("unknown"),
            ack_plan_id = ack["plan_id"].as_u64().unwrap_or(0),
            accepted_remaining_s = ack["accepted_remaining_s"].as_u64().unwrap_or(0),
            error_category = ack_category,
            confirmation = confirmation["outcome"].as_str().unwrap_or("unknown"),
            transport = "ble",
            "device PowerPlan acknowledgement"
        );
        let mut sync_opened = false;
        let should_open = !failure_report && (temporary_data_seq.is_some() || (sync_enabled
            && state["sync"]["retry_skip"] == 0
            && (state["sync"]["due"] == true || state["sync"]["pending"] == true)));
        if should_open && !ack_unknown {
            let reason = if temporary_data_seq.is_some() {
                "data"
            } else if state["sync"]["rounds"] == 15 && state["sync"]["pending"] != true {
                "periodic"
            } else {
                "retry"
            };
            let open_id = uuid::Uuid::new_v4().simple().to_string();
            state["_sync_open_id"] = json!(open_id);
            state["_sync_open_unknown"] = json!(true);
            recovery_state = Some(state.clone());
            match link
                .command(
                    "sync_open",
                    json!({
                        "reason":reason, "open_id":open_id, "data_seq":temporary_data_seq,
                        "wake_generation":state["wake_generation"],
                        "wake_seq":state["wake_seq"],
                    }),
                )
                .await
            {
                Ok(open) if open["result"] == "applied" => {
                    sync_opened = true;
                    state["_sync_open_unknown"] = json!(false);
                }
                Ok(_) if accepted && plan.mode == bridge_core::platform::model::PlanMode::Light => {
                    state["_sync_open_unknown"] = json!(false);
                    state["_sync_open_error"] = json!("sync_open_rejected");
                    tracing::warn!(device_mac=%mac,stage="sync_open",error_category="sync_open_rejected","sync window rejected; accepted light plan remains valid");
                }
                Ok(_) => return Err("sync_open_rejected".into()),
                // Unknown is only a candidate opportunity. Authenticated HTTP
                // must prove the same plan/wake and the temporary sync phase.
                Err(_) => sync_opened = true,
            }
        }
        if ack_unknown && plan.mode == bridge_core::platform::model::PlanMode::Sleep {
            return Err("BLE sleep plan acknowledgement unknown".into());
        }
        let light_plan = if ack_unknown {
            plan.mode == bridge_core::platform::model::PlanMode::Light
        } else {
            ack["accepted_remaining_s"].as_u64().unwrap_or(0) > 0
        };
        Ok((state, sync_enabled, sync_opened, light_plan))
    };
    let mut normal_result = match tokio::time::timeout(Duration::from_secs(10), work).await {
        Ok(result) => result,
        Err(_) => match recovery_state {
            Some(state)
                if state["_sent_plan"]["mode"] == "light"
                    || state["_sync_open_unknown"] == true =>
            {
                let enabled = state["_sync_enabled"] == true;
                let light = state["_sent_plan"]["mode"] == "light";
                let opened = state["_sync_open_unknown"] == true;
                Ok((state, enabled, opened, light))
            }
            _ => Err("BLE rendezvous timed out".to_owned()),
        },
    };
    let shared_tail = Budget::at(admitted_at, HTTP_PROOF_BUDGET).capped(http_window.remaining());
    let mut http_budget = http_window;
    let mut wifi_repair_error = None;
    if let Ok((state, _, opened, light)) = &mut normal_result {
        if *opened || (*light && state["_need_http"] == true) {
            let plan_id = state["_sent_plan"]["plan_id"].as_u64().unwrap_or(0);
            let unknown = state["_plan_ack_unknown"] == true || state["_sync_open_unknown"] == true;
            // These optional commands consume a short shared tail.
            if !admitted_wifi.is_empty() {
                // Cancelling a fragmented write leaves a partial JSON frame.
                // Use the shared deadline; on timeout never reuse this link.
                if tokio::time::timeout(shared_tail.remaining(),crate::wifi::note_link(ctx,&mac,&mut link)).await.is_err() {
                    drop(_device_delivery);drop(_delivery);
                    link.close_bounded(CLOSE_BUDGET).await;
                    return Err("wifi_window_timeout".into());
                }
            }
            // An unknown plan may query an already sent transaction, but
            // does not authorize a fresh write before HTTP proves the plan.
            let confirmation_only = service(ctx).wifi_state(&mac).jobs.iter()
                .find(|j| matches!(j.kind.as_str(),"sync"|"recovery_save") && j.runnable(device_now(&mac)))
                .is_some_and(|j| j.sent);
            if !unknown || confirmation_only {
                let _ = crate::wifi::recover_on_link_budget(ctx, &mac, &mut link, shared_tail).await;
            }
            if !unknown && state["_wifi_repair"] == true {
                if let Err(error) = crate::wifi::automatic_on_link(
                    ctx,
                    &mac,
                    &mut link,
                    shared_tail,
                    state["power"]["provisional_remaining_s"].as_u64().unwrap_or(0) > 0,
                )
                .await { wifi_repair_error = Some(error); }
            }
            if state["_need_http"] == true && state["_http_reachable"] != true &&
                wifi_repair_error.is_none() {
                // A lost SyncOpen reply does not make the already acknowledged
                // Plan uncertain or block its independently admitted HTTP work.
                let confirmed = if state["_plan_ack_unknown"] == true {
                    link.command("status", json!({})).await.ok().is_some_and(|check|
                        check["power"]["plan_id"].as_u64()==Some(plan_id))
                } else { true };
                if confirmed {
                    let attempt = device_link_for_mac(ctx,&mac)
                        .ok_or_else(||"report_device_address_missing".to_owned())
                        .and_then(|target| report_route(&target.ip));
                    if let Ok(route) = attempt {
                        let fields=json!({"plan_id":plan_id,"bridge_addr":route.to_string(),
                            "bridge_port":ctx.config.port,"origin_request_no":plan_id});
                        let accepted=tokio::time::timeout(shared_tail.remaining(),
                            link.command("network_start",fields)).await;
                        let applied=matches!(&accepted,Ok(Ok(reply)) if reply["result"]=="accepted");
                        let uncertain=!matches!(&accepted,Ok(Ok(_)));
                        state["_network_start_accepted"] = json!(applied);
                        state["_network_start_unknown"] = json!(uncertain);
                        if !applied && !uncertain {
                            wifi_repair_error=Some("network_start_rejected".into());
                        }
                    } else {
                        wifi_repair_error=Some(attempt.unwrap_err());
                    }
                } else {
                    wifi_repair_error=Some("plan_unconfirmed".into());
                }
            }
            // Authenticated HTTP status below supplies the remaining formal
            // and temporary windows; a Sleep plan's zero duration is no cap.
        }
    }
    if let Some(error) = wifi_repair_error { normal_result = Err(error); }
    if let Ok((state, enabled, _, light)) = &normal_result {
        if !enabled {
            let _ = tokio::time::timeout(
                Duration::from_millis(500),
                sync_wake_history(&mut link, &mac, state),
            )
            .await;
        }
        if !light {
            let _ = crate::wifi::recover_on_link_budget(
                ctx,
                &mac,
                &mut link,
                Budget::new(Duration::from_millis(500)),
            )
            .await;
        }
    }
    // Release both delivery locks before link cleanup. A hanging
    // `peripheral.disconnect()` (btleplug on macOS after a natural link loss)
    // must not block HTTP delivery for this MAC or the next rendezvous for any
    // device. `close_bounded` therefore always returns; the link handle is
    // abandoned on timeout. Evidence: artifacts/bridge-macos/ble-close-lock/.
    drop(_device_delivery);
    drop(_delivery);
    link.close_bounded(CLOSE_BUDGET).await;
    if let Ok((state, _, opened, light)) = &mut normal_result {
        if state["_need_http"] == true &&
            (state["_http_reachable"] == true ||
             state["_network_start_accepted"] == true || state["_network_start_unknown"] == true) {
            let plan_id=state["_sent_plan"]["plan_id"].as_u64().unwrap_or(0);
            let explicit=state["rendezvous_v"]==1;
            let proof_budget=if state["_http_reachable"]==true {
                Budget::new(Duration::from_secs(3)).capped(http_window.remaining())
            } else {shared_tail};
            match wait_for_device_http(ctx,&mac,state,plan_id,explicit,proof_budget,http_window).await {
                Ok((status,budget,sampled_at)) => {
                    http_budget=budget;
                    if let Err(error)=service(ctx).note_device_status_at(&mac,&status,"http",sampled_at) {
                        normal_result=Err(err_text(error));
                    } else {
                        *light=status["power"]["mode"]=="light" &&
                            status["power"]["remaining_s"].as_u64().unwrap_or(0)>0;
                        *opened=status["sync"]["phase"]=="WIFI_SYNC_ONCE";
                    }
                }
                Err(error) => {crate::wifi::http_failed(ctx,&mac,&error);normal_result=Err(error);}
            }
        }
    }
    let sync_opened = normal_result.as_ref()
        .is_ok_and(|(state, enabled, opened, light)|
            state["_need_http"] == true && (*opened || (*enabled && *light)));
    let may_continue = normal_result.as_ref().is_ok_and(|(state, _, _, light)|
        *light && state["_need_http"] == true);
    let temporary_data = normal_result.as_ref().is_ok_and(|(state, _, _, _)|
        state["_temporary_data_seq"].as_u64().is_some());
    let result = normal_result.map(|_| ());
    if sync_opened || (may_continue && !admitted_wifi.is_empty()) {
        let lock = delivery_lock(ctx, &mac);
        let _device_delivery = lock.lock().await;
        // Reads must finish before a Data-only completion releases the window.
        for job_id in &admitted_wifi {
            crate::wifi::opportunity_admitted(ctx, &mac, job_id, http_budget).await;
        }
    }
    if temporary_data && sync_opened {
        if service(ctx).wifi_state(&mac).jobs.iter()
            .any(|j| admitted_wifi.contains(&j.job_id) && !j.terminal()) {
            return Ok(BleOpportunity::Connected { mac, result: Err("temporary_read_incomplete".into()) });
        }
        // Complete admitted Data before diagnostic complete can close the window.
        // deliver serializes itself; do not hold the per-device lock here.
        let delivered = tokio::time::timeout(http_budget.remaining(), deliver(ctx, &mac)).await;
        match delivered {
            Ok(reply) if reply["ack"]["result"] == "applied" => {},
            Ok(reply) if reply["ack"]["result"] == "rejected" =>
                return Ok(BleOpportunity::Connected { mac, result: Err("temporary_data_rejected".into()) }),
            _ => return Ok(BleOpportunity::Connected { mac, result: Err("temporary_data_unknown".into()) }),
        }
    }
    if sync_opened || (may_continue && !admitted_wifi.is_empty()) {
        // `sync_http_inner` does not take the lock itself; re-acquire only the
        // per-MAC lock to keep admitted HTTP work serialized.
        let lock = delivery_lock(ctx, &mac);
        let _device_delivery = lock.lock().await;
        let reads_done = service(ctx)
            .wifi_state(&mac)
            .jobs
            .iter()
            .filter(|j| admitted_wifi.contains(&j.job_id))
            .all(|j| j.terminal());
        // An admitted read that is still waiting owns the current window;
        // diagnostic complete would close it before the original job finishes.
        if reads_done {
            sync_http_inner_budget(ctx, &mac, may_continue, http_budget).await;
        }
    }
    Ok(BleOpportunity::Connected { mac, result })
}

fn matching_http(
    status: &Value,
    mac: &str,
    ble: &Value,
    plan_id: u64,
    explicit: bool,
) -> Result<(), String> {
    if status["device_mac"]
        .as_str()
        .and_then(DeviceIdentity::normalized_mac)
        .as_deref()
        != Some(mac)
    {
        return Err("http_identity_mismatch".into());
    }
    if status["session_nonce"] != ble["session_nonce"]
        || status["session_nonce"].as_str().is_none_or(str::is_empty)
    {
        return Err("http_session_mismatch".into());
    }
    if status["power"]["plan_id"].as_u64() != Some(plan_id) {
        return Err("plan_unconfirmed".into());
    }
    if let Some(mode) = ble["_sent_plan"]["mode"].as_str() {
        let expired_in_sync = mode == "light"
            && status["power"]["mode"] == "sleep"
            && status["power"]["remaining_s"] == 0
            && status["sync"]["phase"] == "WIFI_SYNC_ONCE";
        // SyncOpen can keep the radio awake independently of a zero-duration
        // Sleep Plan (including a USB-powered device). It does not grant Light.
        let sleep_in_sync = mode == "sleep"
            && status["power"]["mode"] == "light"
            && status["power"]["remaining_s"] == 0
            && status["sync"]["phase"] == "WIFI_SYNC_ONCE"
            && ble["_sync_open_id"].as_str().is_some_and(|id| !id.is_empty());
        if status["power"]["mode"].as_str() != Some(mode) && !expired_in_sync && !sleep_in_sync {
            return Err("plan_unconfirmed".into());
        }
        if mode == "light"
            && status["power"]["remaining_s"].as_u64().unwrap_or(0) == 0
            && status["sync"]["phase"] != "WIFI_SYNC_ONCE"
        {
            return Err("plan_expired".into());
        }
    }
    if ble["_sync_open_unknown"] == true
        && ble["_sent_plan"]["mode"] == "sleep"
        && status["sync"]["phase"] != "WIFI_SYNC_ONCE"
    {
        return Err("sync_open_unconfirmed".into());
    }
    if explicit {
        for key in ["wake_generation", "wake_seq"] {
            if status[key] != ble[key] || status[key].as_u64().is_none() {
                return Err("http_wake_mismatch".into());
            }
        }
        let rv = &status["rendezvous"];
        // The report ACK moves rendezvous to AckGrace/Done; it does not revoke
        // HTTP. The authenticated transport fact belongs to the same boot,
        // wake and Plan checked above. Ownership is checked before any work.
        if rv["v"] != 1
            || status["transports"]["http"]["ready"] != true
            || rv["id"].as_str().is_none_or(str::is_empty)
        {
            return Err("http_not_ready".into());
        }
    }
    Ok(())
}

fn http_transfer_budget(status: &Value, sampled_at: std::time::Instant, work_budget: Budget) -> Budget {
    let light = Duration::from_secs(status["power"]["remaining_s"].as_u64().unwrap_or(0));
    let sync = if status["sync"]["phase"] == "WIFI_SYNC_ONCE" {
        Duration::from_millis(status["sync"]["remaining_ms"].as_u64().unwrap_or(0))
    } else { Duration::ZERO };
    Budget::at(sampled_at, light.max(sync).saturating_sub(Duration::from_secs(1)))
        .capped(work_budget.remaining())
}

async fn wait_for_device_http(
    ctx: &AppCtx,
    mac: &str,
    ble: &Value,
    plan_id: u64,
    explicit: bool,
    budget: Budget,
    work_budget: Budget,
) -> Result<(Value, Budget, u64), String> {
    let mut last = "http_unavailable".to_owned();
    loop {
        if budget.remaining().is_zero() {
            return Err(last);
        }
        // The ready report may have supplied a new IP after BLE closed.
        let Some(target) = device_link_for_mac(ctx, mac) else {
            tokio::time::sleep(budget.remaining().min(Duration::from_millis(100))).await;
            continue;
        };
        let ip = target.ip.clone();
        let token = target.token.clone();
        let timeout = budget.remaining().min(Duration::from_secs(3));
        let sample_start = std::time::Instant::now();
        let sampled_at = device_now(mac);
        let observed = blocking(move || {
            device_client::status(&ip, &token, timeout).map_err(|e| if bridge_core::rendezvous::wifi_error(&e).1=="unauthorized" {"unauthorized".into()} else {"http_unavailable".into()})
        })
        .await;
        if observed.as_ref().is_err_and(|e| e=="unauthorized") { return Err("unauthorized".into()); }
        if let Ok(status) = observed {
            match matching_http(&status, mac, ble, plan_id, explicit) {
                Ok(()) => {
                    let observed_budget = http_transfer_budget(&status, sample_start, work_budget);
                    if status["owner_id"] != ctx.bridge_id {
                        // Occupancy is renewed only by an explicit claim after
                        // the authenticated device response.
                        let limit=budget.remaining().min(observed_budget.remaining())
                            .min(Duration::from_secs(2));
                        service(ctx).note_device_endpoint(mac,&target.ip).map_err(err_text)?;
                        let claim=tokio::time::timeout(limit,
                            crate::post_claim_for_mac(ctx,false,false,mac)).await;
                        if !matches!(claim,Ok(Ok(Some(ref owner))) if owner["id"]==ctx.bridge_id) {
                            return Err("claim_unconfirmed".into());
                        }
                    }
                    service(ctx)
                        .note_device_endpoint(mac, &target.ip)
                        .map_err(err_text)?;
                    crate::wifi::http_ready(ctx,mac);
                    return Ok((status, observed_budget, sampled_at));
                }
                Err(error)
                    if matches!(
                        error.as_str(),
                        "http_identity_mismatch" | "http_session_mismatch" | "http_wake_mismatch" | "plan_expired"
                    ) =>
                {
                    return Err(error)
                }
                Err(error) => {
                    if error != last {
                        tracing::info!(event="http_proof",device_mac=%mac,plan_id,
                            stage="matching_status",error_category=%error,"HTTP proof not yet confirmed");
                    }
                    last = error;
                },
            }
        }
        tokio::time::sleep(budget.remaining().min(Duration::from_millis(100))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporary_sync_budget_survives_formal_light_expiry() {
        let start = std::time::Instant::now();
        for remaining in [2, 1, 0] {
            let status = json!({"power":{"remaining_s":remaining},
                "sync":{"phase":"WIFI_SYNC_ONCE","remaining_ms":80000}});
            let budget = http_transfer_budget(&status, start, Budget::new(Duration::from_secs(90)));
            assert!(budget.remaining() > Duration::from_secs(78));
        }
        let status = json!({"power":{"remaining_s":2},
            "sync":{"phase":"DEEP","remaining_ms":80000}});
        assert!(http_transfer_budget(&status, start, Budget::new(Duration::from_secs(90))).remaining() <= Duration::from_secs(1));
        let active = json!({"power":{"remaining_s":0},"sync":{"phase":"WIFI_SYNC_ONCE","remaining_ms":80000}});
        assert!(http_transfer_budget(&active, start, Budget::new(Duration::from_secs(3))).remaining() <= Duration::from_secs(3));
        assert!(http_transfer_budget(&active, start - Duration::from_secs(80), Budget::new(Duration::from_secs(90))).remaining().is_zero());
    }

    #[test]
    fn protocol_http_remains_usable_after_ready_report_ack() {
        let ble=json!({"session_nonce":"boot","wake_generation":2,"wake_seq":9,
            "_sent_plan":{"mode":"light"}});
        let mut status=json!({"device_mac":"7C4FADB93408","session_nonce":"boot",
            "wake_generation":2,"wake_seq":9,"power":{"plan_id":101,"mode":"light","remaining_s":100},
            "transports":{"http":{"ready":true}},"rendezvous":{"v":1,"ready":false,"id":"rv","remaining_ms":0}});
        assert!(matching_http(&status,"7C4FADB93408",&ble,101,true).is_ok());
        status["transports"]["http"]["ready"]=json!(false);
        assert_eq!(matching_http(&status,"7C4FADB93408",&ble,101,true),Err("http_not_ready".into()));
    }
    #[test]
    fn device_http_status_rejects_wrong_session_wake_and_plan() {
        let ble = json!({"session_nonce":"boot","wake_generation":2,"wake_seq":9,"_sent_plan":{"mode":"light"}});
        let status = json!({"device_mac":"7C4FADB93408","session_nonce":"boot","wake_generation":2,"wake_seq":9,
            "power":{"plan_id":101,"mode":"light","remaining_s":100},"transports":{"http":{"ready":true}},"rendezvous":{"v":1,"ready":true,"id":"rv"}});
        assert!(matching_http(&status, "7C4FADB93408", &ble, 101, true).is_ok());
        for (key, value) in [
            ("device_mac", json!("70041DD7A340")),
            ("session_nonce", json!("old")),
            ("wake_seq", json!(8)),
        ] {
            let mut wrong = status.clone();
            wrong[key] = value;
            assert!(matching_http(&wrong, "7C4FADB93408", &ble, 101, true).is_err());
        }
        assert!(matching_http(&status, "7C4FADB93408", &ble, 102, true).is_err());
        let mut expired = status.clone();
        expired["power"]["remaining_s"] = json!(0);
        assert_eq!(
            matching_http(&expired, "7C4FADB93408", &ble, 101, true),
            Err("plan_expired".into())
        );
        expired["sync"]["phase"] = json!("WIFI_SYNC_ONCE");
        assert!(matching_http(&expired, "7C4FADB93408", &ble, 101, true).is_ok());
        expired["power"]["mode"] = json!("deep");
        assert_eq!(
            matching_http(&expired, "7C4FADB93408", &ble, 101, true),
            Err("plan_unconfirmed".into())
        );
        let mut sleep_ble = ble.clone();
        sleep_ble["_sent_plan"]["mode"] = json!("sleep");
        sleep_ble["_sync_open_unknown"] = json!(true);
        expired["power"]["mode"] = json!("sleep");
        assert!(matching_http(&expired, "7C4FADB93408", &sleep_ble, 101, true).is_ok());
        sleep_ble["_sync_open_id"]=json!("original-sync-open");
        expired["power"]["mode"]=json!("light");
        assert!(matching_http(&expired, "7C4FADB93408", &sleep_ble, 101, true).is_ok());
        let mut unbound=sleep_ble.clone();unbound.as_object_mut().unwrap().remove("_sync_open_id");
        assert_eq!(matching_http(&expired,"7C4FADB93408",&unbound,101,true),Err("plan_unconfirmed".into()));
        expired["power"]["mode"]=json!("sleep");
        expired["sync"]["phase"] = json!("DEEP");
        assert_eq!(
            matching_http(&expired, "7C4FADB93408", &sleep_ble, 101, true),
            Err("sync_open_unconfirmed".into())
        );
    }
}
