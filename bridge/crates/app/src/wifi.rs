//! Device-page Wi-Fi operations. Configurations are cached in local runtime data.
use std::collections::BTreeMap;
use std::time::Duration;
use serde_json::{json, Value};
use bridge_core::platform::model::DeviceIdentity;
use bridge_core::rendezvous::Budget;
use bridge_core::platform::wifi::{ConnectionFacts, NetworkObservation, Network, WifiState};
#[cfg(test)] use bridge_core::platform::wifi::Connection;
use sha2::{Digest, Sha256};
use crate::{AppCtx, host_wifi, platform};

fn endpoint_feedback(error: &anyhow::Error) -> Value {
    error.downcast_ref::<bridge_ble::EndpointFailure>().map(|e| e.feedback())
        .unwrap_or_else(|| json!({"state":"failed","outcome":"unknown","stage":"endpoint","error":"endpoint_failed"}))
}

fn control_projection(job: &bridge_core::platform::wifi::Job) -> Value {
    json!({"stage":if job.state=="completed" {if job.kind=="trial" {"trial_complete"}else{"saved"}}
        else if job.terminal(){"failed"}else{"connecting"},"error":job.error_code,"job_id":job.job_id,"state":job.state})
}
pub async fn action(ctx: &AppCtx, mac: &str, op: &str, body: Value) -> Result<Value, String> {
    if op == "host" || op == "host_permission" {
        let request = op == "host_permission";
        let host=tokio::task::spawn_blocking(move || host_wifi::read(request)).await.map_err(|_| "host_network_unavailable")?;
        return Ok(resolve_host(ctx,host));
    }
    if matches!(op, "permissions" | "permission_location" | "permission_bluetooth" | "permission_all") {
        let request = match op { "permission_location" => 1, "permission_bluetooth" => 2, "permission_all" => 3, _ => 0 };
        return tokio::task::spawn_blocking(move || host_wifi::permissions(request)).await.map_err(|_| "permissions_unavailable".into());
    }
    if matches!(op, "settings_location" | "settings_bluetooth" | "settings_permissions") {
        let section = match op { "settings_bluetooth" => 1, "settings_permissions" => 2, _ => 0 };
        tokio::task::spawn_blocking(move || host_wifi::open_settings(section)).await.map_err(|_| "privacy_settings_unavailable")??;
        return Ok(json!({"opened":true}));
    }
    if op == "scan" {
        host_wifi::require_bluetooth()?;
        let _delivery = ctx.device_delivery.lock().await;
        let info = bridge_ble::Pusher::read_candidate_info().await.map_err(|_| "device_not_advertising_press_boot")?;
        return Ok(json!({"mac":info["mac"], "fw":info["fw"], "wifi_setup_protocol":info["wifi_setup_protocol"]}));
    }
    let mac = DeviceIdentity::normalized_mac(mac).ok_or("invalid_device_mac")?;
    let svc = platform::service(ctx);
    let now = crate::device_now_secs(&mac).max(0) as u64;
    if op=="library_import" {
        return svc.wifi_library_import(&mac,body["library_revision"].as_u64().ok_or("wifi_library_revision_required")?,
            body["local_revision"].as_u64().ok_or("local_revision_required")?,body["snapshot_id"].as_str().ok_or("snapshot_id_required")?,
            body["network_id"].as_str().ok_or("invalid_wifi_selection")?,body["family_id"].as_str().ok_or("wifi_family_not_found")?,now)
            .and_then(|v|Ok(serde_json::to_value(v)?)).map_err(|e|e.to_string());
    }
    if matches!(op,"library_save"|"library_select"|"library_wired") {
        let expected=body["library_revision"].as_u64().ok_or("wifi_library_revision_required")?;
        let mut library=svc.wifi_library();
        if op=="library_save" {
            let mut families:Vec<bridge_core::platform::wifi_families::Family>=serde_json::from_value(body["families"].clone()).map_err(|_|"invalid_wifi_family")?;
            for (family,input) in families.iter_mut().zip(body["families"].as_array().ok_or("invalid_wifi_family")?) {
                if let Some(old)=library.families.iter().find(|f|f.id==family.id) {
                    if input.get("password_2g").is_none(){family.password_2g=old.password_2g.clone();}
                    if input.get("password_5g").is_none(){family.password_5g=old.password_5g.clone();}
                }
            }
            library.families=families;
        } else if op=="library_select" {
            let selected=serde_json::from_value(body["selected"].clone()).map_err(|_|"invalid_wifi_selection")?;
            library.devices.insert(mac.clone(),selected);
        } else {
            library.wired_family=serde_json::from_value(body["family_id"].clone()).map_err(|_|"invalid_wifi_selection")?;
        }
        return svc.wifi_library_save(expected,library,now).and_then(|v|Ok(serde_json::to_value(v)?)).map_err(|e|e.to_string());
    }
    if crate::ap01::is_ap01(ctx,&mac) && matches!(op,"get"|"sync"|"sync_cached"|"ap01_reconcile"|"ap01_accept_observed") {
        return ap01_action(ctx,&mac,op,&body).await;
    }
    if crate::ap01::is_ap01(ctx,&mac) && matches!(op,"use_current"|"trial"|"activate"|"recover_save"|"authorize") {
        return Err("ap01_manual_network_selection".into());
    }
    match op {
        "use_current" => {
            let host=current_host(ctx).await;
            let state=svc.wifi_state(&mac);
            if body["local_revision"].as_u64()!=Some(state.local_revision){return Err("local_revision_conflict".into());}
            let target=current_device_target(ctx,&mac,&state,&host).ok_or("network_not_configured")?;
            if body["network_id"]!=target.network_id {return Err("host_network_changed".into());}
            let credentials: BTreeMap<String,String>=serde_json::from_value(body.get("credentials").cloned().unwrap_or(json!({}))).map_err(|_|"invalid_credentials")?;
            if credentials.keys().any(|id|id!=&target.network_id){return Err("invalid_credentials".into());}
            // Carry user input only in RAM until the read-only BLE match decides
            // whether a stable recovery transaction is actually needed.
            let key=current_input_key(&mac,target,state.local_revision);
            {
                let mut pending=ctx.wifi_inputs.lock().unwrap();
                pending.retain(|id,_|!id.starts_with(&format!("current:{mac}:"))||id==&key);
                pending.insert(key,credentials);
            }
            let mut connection=fresh_connection(&state,target,now);
            connection.requested=true;connection.admission_error=None;
            connection.attempts=0;connection.next_attempt_at=0;connection.contact=None;
            svc.wifi_set_connection(&mac,connection).map_err(|e|e.to_string())?;
            if let Some(reply)=http_control_read(ctx,&mac,"match",target_query(target)).await {
                note_match(ctx,&mac,target,&reply);
                if reply["matches"]==true && target.credential!="replace" {
                    if reply["connected"]==true {return Ok(json!({"stage":"connected","network_id":target.network_id}));}
                    let job=svc.wifi_queue_control(&mac,&ctx.bridge_id,"activate",json!({"network_id":target.network_id,"version":reply["version"]}),now).map_err(|e|e.to_string())?;
                    immediate(ctx,&mac,&job.job_id).await;return serde_json::to_value(svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job)).map_err(|_|"wifi_job_encoding".into());
                }
                let job=queue_current_recovery(ctx,&mac,&state,target,now,false)?;
                immediate(ctx,&mac,&job.job_id).await;return serde_json::to_value(svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job)).map_err(|_|"wifi_job_encoding".into());
            }
            ctx.force_ble.notify_one();
            return Ok(json!({"state":"waiting_rendezvous","network_id":target.network_id}));
        }
        "save" => {
            let targets = serde_json::from_value(body["targets"].clone()).map_err(|_| "invalid_wifi_network")?;
            let expected = body["local_revision"].as_u64().ok_or("local_revision_required")?;
            return svc.wifi_save(&mac, expected, targets, now).and_then(|v| Ok(serde_json::to_value(v)?)).map_err(|e| e.to_string());
        }
        "get" | "sync" | "sync_cached" => {
            let selected = if op == "get" { vec![] } else {
                serde_json::from_value(body["selected"].clone()).map_err(|_| "invalid_wifi_selection")?
            };
            let credentials: BTreeMap<String, String> = if op != "get" {
                serde_json::from_value(body.get("credentials").cloned().unwrap_or(json!({}))).map_err(|_| "invalid_credentials")?
            } else { BTreeMap::new() };
            let fields: BTreeMap<String, Vec<String>> = serde_json::from_value(body.get("fields").cloned().unwrap_or(json!({}))).map_err(|_| "invalid_wifi_fields")?;
            let expected = if op == "get" { None } else { Some(body["local_revision"].as_u64().ok_or("local_revision_required")?) };
            let job = if op == "sync" {
                let host = current_host(ctx).await;
                let ssid = host_wifi::network_key(&host).ok_or("host_network_unknown")?;
                if body["host_ssid"].as_str() != Some(ssid) { return Err("host_network_changed".into()); }
                if !fields.is_empty() { return Err("whole_network_required".into()); }
                svc.wifi_queue_manual_sync(&mac, &ctx.bridge_id, selected, expected.unwrap(),
                    body["snapshot_id"].as_str().ok_or("snapshot_id_required")?, ssid, now)
            } else {
                svc.wifi_queue_fields_checked(&mac, &ctx.bridge_id, if op=="get" {"read"} else {"sync"}, selected, fields, expected, now)
            }.map_err(|e| e.to_string())?;
            if !job.sent && !credentials.is_empty() { ctx.wifi_inputs.lock().unwrap().insert(job.job_id.clone(), credentials); }
            immediate(ctx,&mac,&job.job_id).await;
            let current=svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job);
            if !current.terminal() && !current.sent {ctx.force_ble.notify_one();}
            return serde_json::to_value(current).map_err(|_| "wifi_job_encoding".into());
        }
        "import" | "read_import" => {
            let selected: Vec<String> = serde_json::from_value(body["selected"].clone()).map_err(|_| "invalid_wifi_selection")?;
            let expected = body["local_revision"].as_u64().ok_or("local_revision_required")?;
            let fields = serde_json::from_value(body.get("fields").cloned().unwrap_or(json!({}))).map_err(|_| "invalid_wifi_fields")?;
            let snapshot_id = body["snapshot_id"].as_str();
            if op == "import" && snapshot_id.is_none() { return Err("snapshot_id_required".into()); }
            // Old read_import callers also select cached data; no new read job.
            return svc.wifi_import_fields_checked(&mac, &selected, &fields, expected, snapshot_id, now)
                .and_then(|v| Ok(serde_json::to_value(v)?)).map_err(|e| e.to_string());
        }
        "recover_save" => {
            let selected: Vec<String> = serde_json::from_value(body["selected"].clone()).map_err(|_| "invalid_wifi_selection")?;
            let job = svc.wifi_queue(&mac,&ctx.bridge_id,"recovery_save",selected,now).map_err(|e|e.to_string())?;
            if !job.sent {
                let credentials = serde_json::from_value(body.get("credentials").cloned().unwrap_or(json!({}))).map_err(|_|"invalid_credentials")?;
                ctx.wifi_inputs.lock().unwrap().insert(job.job_id.clone(),credentials);
            }
            immediate(ctx,&mac,&job.job_id).await;
            let current=svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job.clone());
            if current.sent || current.terminal(){return serde_json::to_value(current).map_err(|_|"wifi_job_encoding".into());}
            host_wifi::require_device_bluetooth()?;
            // Intent exists before this optional immediate maintenance attempt.
            let lock = platform::delivery_lock(ctx, &mac);
            let Ok(_device_delivery) = lock.try_lock() else { return serde_json::to_value(job).map_err(|_|"wifi_job_encoding".into()); };
            let Ok(_delivery) = ctx.device_delivery.try_lock() else { return serde_json::to_value(job).map_err(|_|"wifi_job_encoding".into()); };
            if work_blocked(ctx, &mac) {
                return serde_json::to_value(job).map_err(|_|"wifi_job_encoding".into());
            }
            if let Ok(Some(mut link)) = bridge_ble::DeviceConnection::connect(&mac,&ctx.config.token,&ctx.bridge_id).await {
                recover_on_link(ctx,&mac,&mut link).await;
                platform::close_device_ble(link,_device_delivery,_delivery).await;
            }
            let result = svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job);
            return serde_json::to_value(result).map_err(|_|"wifi_job_encoding".into());
        }
        "migrate" => return Err("offline_migration_required".into()),
        "authorize" => {
            host_wifi::require_device_bluetooth()?;
            let lock = platform::delivery_lock(ctx, &mac);
            let (_device_delivery, _delivery) = platform::lock_device_ble(&lock, &ctx.device_delivery,
                tokio::time::Instant::now() + Duration::from_secs(10)).await?;
            let mut link = bridge_ble::DeviceConnection::connect(&mac, &ctx.config.token, &ctx.bridge_id)
                .await.map_err(|_| "ble_connection_failed")?.ok_or("device_not_advertising_press_boot")?;
            let result = match link.write_endpoint(&bridge_ble::lan_ip(),ctx.config.port,&ctx.config.token).await {
                Ok(()) => note_link(ctx,&mac,&mut link).await,
                Err(error) => endpoint_feedback(&error),
            };
            platform::close_device_ble(link,_device_delivery,_delivery).await;
            return Ok(result);
        }
        "input" => {
            let job = body["job_id"].as_str().ok_or("job_id_required")?;
            if !svc.wifi_state(&mac).jobs.iter().any(|j| j.job_id == job && !j.sent && !j.terminal()) { return Err("cannot_replace_sent_input".into()); }
            let credentials: BTreeMap<String, String> = serde_json::from_value(body["credentials"].clone()).map_err(|_| "invalid_credentials")?;
            ctx.wifi_inputs.lock().unwrap().entry(job.to_owned()).or_default().extend(credentials);
            svc.wifi_finish(&mac, job, "waiting_rendezvous", None, now).map_err(|_| "wifi_job_persistence")?;
            return Ok(json!({"state":"waiting_rendezvous","job_id":job}));
        }
        "reconcile_job" => {
            let job=body["job_id"].as_str().ok_or("job_id_required")?;
            svc.wifi_retry_settlement(&mac,job).map_err(|e|e.to_string())?;
            // Only an already reachable opportunity; do not force a BLE wake.
            let lock=platform::delivery_lock(ctx,&mac);
            if let Ok(_permit)=lock.try_lock() { acknowledge_receipts_http(ctx,&mac).await; }
            return serde_json::to_value(svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job))
                .map_err(|_|"wifi_job_encoding".into());
        }
        "cancel_job" => {
            let job = body["job_id"].as_str().ok_or("job_id_required")?;
            ctx.wifi_inputs.lock().unwrap().remove(job);
            return svc.wifi_finish(&mac, job, "cancelled", None, now).and_then(|v| Ok(serde_json::to_value(v)?)).map_err(|e| e.to_string());
        }
        _ => {}
    }
    if matches!(op,"trial"|"activate") {
        let job=svc.wifi_queue_control(&mac,&ctx.bridge_id,op,body,now).map_err(|e|e.to_string())?;
        immediate(ctx,&mac,&job.job_id).await;
        return serde_json::to_value(svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job)).map_err(|_|"wifi_job_projection".into());
    }
    if op == "status" || op == "cancel" {
        if let Some(job)=svc.wifi_state(&mac).jobs.into_iter().rev().find(|j|matches!(j.kind.as_str(),"trial"|"activate")) {
            if op=="cancel" && !job.terminal() {
                svc.wifi_finish(&mac,&job.job_id,"cancelled",None,now).map_err(|e|e.to_string())?;
            }
            opportunity(ctx,&mac).await;
            let current=svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job);
            return Ok(control_projection(&current));
        }
        return Ok(json!({"stage":"idle"}));
    }
    if op != "add" { return Err("invalid_operation".into()); }
    add_device(ctx, &mac).await
}

// AP01 retains the original maintenance list API and physical manual selection.
fn ap01_snapshot(reply:&Value,mac:&str,library:&bridge_core::platform::wifi_families::Library,now:u64)
    -> Result<bridge_core::platform::wifi::Snapshot,String> {
    if reply["result"]!="ok" || reply["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()!=Some(mac) {
        return Err("http_identity_mismatch".into());
    }
    let revision=reply["revision"].as_u64().filter(|r|*r<=u32::MAX as u64).ok_or("invalid_wifi_snapshot")?;
    let rows=reply["profiles"].as_array().filter(|r|r.len()<=9).ok_or("invalid_wifi_snapshot")?;
    let mut networks=Vec::new();
    for row in rows {
        let ssid=row["ssid"].as_str().ok_or("invalid_wifi_snapshot")?;
        let family=library.families.iter().find(|f|f.ssid_2g==ssid);
        let id=family.map(|f|f.id.clone()).unwrap_or_else(||format!("ap01_{}",&format!("{:x}",Sha256::digest(ssid.as_bytes()))[..16]));
        let n=Network {network_id:id,host_ssid:family.map_or(ssid,|f|&f.name).into(),ssid:ssid.into(),
            label:family.map_or(ssid,|f|&f.name).into(),credential:"device".into(),password:None,origin:"device".into(),version:revision,deleted:false};
        networks.push(bridge_core::platform::wifi::ObservedNetwork{bond:"ap01".into(),writable:row["source"]=="saved",network:n});
    }
    let snapshot=bridge_core::platform::wifi::Snapshot {schema:2,device_mac:mac.into(),revision,
        snapshot_id:uuid::Uuid::new_v4().simple().to_string(),complete:true,observed_at:now,networks,active:Value::Null,capacity:json!({"total":8})};
    snapshot.validate(mac).map_err(|_|"invalid_wifi_snapshot")?;Ok(snapshot)
}
async fn ap01_action(ctx:&AppCtx,mac:&str,op:&str,body:&Value)->Result<Value,String> {
    let svc=platform::service(ctx);
    let row=svc.device_get(mac).ok_or("device_not_found")?;
    let address=row["ip"].as_str().ok_or("device_ip_required")?;
    let ip=address.split(':').next().ok_or("device_ip_required")?.parse::<std::net::Ipv4Addr>().map_err(|_|"device_ip_required")?;
    let endpoint=format!("{ip}:{}",bridge_core::ap01::MAINTENANCE_HTTP_PORT);
    ap01_action_at(ctx,mac,op,body,&endpoint).await
}
async fn ap01_action_at(ctx:&AppCtx,mac:&str,op:&str,body:&Value,endpoint:&str)->Result<Value,String> {
    let svc=platform::service(ctx);
    let code=body["device_code"].as_str().filter(|s|s.len()==8&&s.bytes().all(|b|b.is_ascii_digit())).ok_or("ap01_device_code_required")?;
    let lock=platform::delivery_lock(ctx,mac);
    let _guard=lock.try_lock().map_err(|_|"device_channel_busy")?;
    let now=crate::device_now_secs(mac).max(0) as u64;
    let read=match bridge_core::device_client::ap01_wifi_profiles(endpoint,"GET",code,None).await {
        Ok(read)=>read,
        Err(error)=>return Ok(json!({"state":"failed","stage":error.downcast_ref::<bridge_core::rendezvous::CommandFailure>().map_or("wifi_profiles_read",|e|e.stage),
            "error":"ap01_wifi_read_failed","action":"check_device_network_and_code"})),
    };
    if read.0!=200 {return Ok(json!({"state":"failed","stage":"wifi_profiles_read","error":read.1["error"],"http_status":read.0}));}
    let library=svc.wifi_library();let snapshot=ap01_snapshot(&read.1,mac,&library,now)?;
    let state=svc.wifi_state(mac);
    if op=="ap01_accept_observed" {
        let cached=state.snapshot.as_ref().filter(|s|s.complete&&s.revision==snapshot.revision).ok_or("snapshot_conflict")?;
        if serde_json::to_value(&cached.networks).ok()!=serde_json::to_value(&snapshot.networks).ok(){return Err("snapshot_conflict".into());}
        let mut pending=state.ap01_pending.clone().ok_or("ap01_no_unknown_operation")?;
        pending["state"]=json!("acknowledged_unknown");
        svc.wifi_ap01_cache(mac,Some(snapshot),Some(pending)).map_err(|_|"wifi_job_persistence")?;
        return Ok(json!({"state":"completed","stage":"observed_list_accepted","outcome":"original_result_unknown","manual_selection":true}));
    }
    let blocked=state.ap01_pending.as_ref().is_some_and(|p|p["state"]!="acknowledged_unknown");
    if op=="get" || op=="ap01_reconcile" {
        svc.wifi_ap01_cache(mac,Some(snapshot),state.ap01_pending.clone()).map_err(|_|"wifi_job_persistence")?;
        return Ok(if blocked {
            json!({"state":"needs_attention","stage":"wifi_profiles_readback","error":"ap01_wifi_result_unknown","action":"inspect_saved_list_and_credentials","pending":state.ap01_pending})
        } else {json!({"state":"completed","stage":"complete_snapshot"})});
    }
    if blocked{return Err("ap01_wifi_result_unknown".into());}
    if body["local_revision"].as_u64()!=Some(state.local_revision){return Err("local_revision_conflict".into());}
    let cached=state.snapshot.as_ref().filter(|s|s.complete&&Some(s.snapshot_id.as_str())==body["snapshot_id"].as_str()).ok_or("snapshot_conflict")?;
    if cached.revision!=snapshot.revision {return Err("revision_conflict".into());}
    let selected:Vec<String>=serde_json::from_value(body["selected"].clone()).map_err(|_|"invalid_wifi_selection")?;
    if selected.is_empty() || selected.iter().collect::<std::collections::BTreeSet<_>>().len()!=selected.len(){return Err("invalid_wifi_selection".into());}
    let credentials:BTreeMap<String,String>=serde_json::from_value(body.get("credentials").cloned().unwrap_or(json!({}))).map_err(|_|"invalid_credentials")?;
    if credentials.keys().any(|id|!selected.contains(id)){return Err("invalid_credentials".into());}
    let mut profiles:Vec<Value>=read.1["profiles"].as_array().unwrap().iter().filter(|r|r["source"]=="saved").map(|r|json!({"ssid":r["ssid"]})).collect();
    for id in &selected {
        let mut n=state.targets.iter().find(|n|&n.network_id==id).ok_or("invalid_wifi_selection")?.clone();
        if let Some(password)=credentials.get(id){n.password=Some(password.clone());n.validate().map_err(|_|"invalid_wifi_password")?;}
        if n.deleted {profiles.retain(|r|r["ssid"]!=n.ssid);continue;}
        let exists=profiles.iter().any(|r|r["ssid"]==n.ssid);
        if n.password.is_none()&&!exists{return Err("credentials_required".into());}
        profiles.retain(|r|r["ssid"]!=n.ssid);
        let mut profile=json!({"ssid":n.ssid});
        if let Some(password)=&n.password {profile["password"]=json!(password);}
        profiles.push(profile);
    }
    if profiles.len()>8{return Err("capacity".into());}
    let pending=json!({"expected_revision":snapshot.revision,"selected":selected,"state":"result_unknown","stage":"wifi_profiles_put"});
    // Persist BEFORE PUT. A lost ACK/restart may only read back, never resubmit.
    svc.wifi_ap01_cache(mac,None,Some(pending)).map_err(|_|"wifi_job_persistence")?;
    let payload=json!({"device_mac":mac,"expected_revision":snapshot.revision,"profiles":profiles});
    match bridge_core::device_client::ap01_wifi_profiles(endpoint,"PUT",code,Some(&payload)).await {
        Ok((200,reply))=>{
            let confirmed=ap01_snapshot(&reply,mac,&library,now)?;
            if confirmed.revision!=snapshot.revision+1{return Err("ap01_wifi_result_unknown".into());}
            svc.wifi_ap01_cache(mac,Some(confirmed),None).map_err(|_|"wifi_job_persistence")?;
            Ok(json!({"state":"completed","stage":"saved","manual_selection":true}))
        },
        Ok((status,reply)) if reply["result"]=="rejected" || matches!(status,401|429)=>{
            svc.wifi_ap01_cache(mac,None,None).map_err(|_|"wifi_job_persistence")?;
            Ok(json!({"state":"failed","stage":"wifi_profiles_put","error":reply["error"],"http_status":status}))
        },
        Err(error) if error.downcast_ref::<bridge_core::rendezvous::CommandFailure>().is_some_and(|e|e.outcome==bridge_core::rendezvous::DispatchOutcome::NotSent)=>{
            svc.wifi_ap01_cache(mac,None,None).map_err(|_|"wifi_job_persistence")?;
            let failure=error.downcast_ref::<bridge_core::rendezvous::CommandFailure>().unwrap();
            Ok(json!({"state":"failed","outcome":"not_sent","stage":failure.stage,"error":failure.reason,"action":"check_device_network","manual_selection":true}))
        },
        _=>Ok(json!({"state":"needs_attention","stage":"wifi_profiles_put","error":"ap01_wifi_result_unknown","action":"read_original_operation"})),
    }
}

async fn add_device(ctx: &AppCtx, mac: &str) -> Result<Value, String> {
    host_wifi::require_device_bluetooth()?;
    let lock = platform::delivery_lock(ctx, mac);
    let (_device_delivery, _delivery) = platform::lock_device_ble(&lock, &ctx.device_delivery,
        tokio::time::Instant::now() + Duration::from_secs(10)).await?;
    let mut link = bridge_ble::DeviceConnection::connect(mac, &ctx.config.token, &ctx.bridge_id)
        .await.map_err(|_| "ble_connection_failed")?.ok_or("device_not_advertising_press_boot")?;
    let result = async {
        if link.info["wifi_setup_protocol"] != 1 || link.info["wifi_management"] != 3 { return Err("update_required".into()); }
        if let Err(error) = link.write_endpoint(&bridge_ble::lan_ip(), ctx.config.port, &ctx.config.token).await {
            return Ok(endpoint_feedback(&error));
        }
            let caps = platform::caps_from_status(&link.wifi_read("wifi_capabilities").await.map_err(|_| "device_capabilities_failed")?)?;
            let mut identity = DeviceIdentity::new(mac, &crate::discovery::default_device_name(mac)).map_err(|_| "device_identity")?;
            identity.discovered_via = "ble".into();
            if let Some(ip) = link.info["ip"].as_str().filter(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok()) {
                identity.ip = Some(ip.to_owned());
            }
            // Preserve an existing record and its permission/profile as-is.
            if platform::service(ctx).device_get(mac).is_none() {
                platform::service(ctx).device_upsert(identity, caps).map_err(|_| "device_save_failed")?;
            }
        Ok(json!({"device_mac":mac,"registered":true}))
    }.await;
    platform::close_device_ble(link,_device_delivery,_delivery).await;
    result
}

pub async fn view(ctx: &AppCtx, mac: &str, refresh: bool) -> Result<Value, String> {
    let mac = DeviceIdentity::normalized_mac(mac).ok_or("invalid_device_mac")?;
    let queued = if refresh { Some(action(ctx, &mac, "get", json!({})).await?) } else { None };
    let host=current_host(ctx).await;
    let permissions=host_wifi::permissions(0);
    let _ = platform::service(ctx).wifi_expire(&mac, crate::device_now_secs(&mac).max(0) as u64);
    let mut state = platform::service(ctx).wifi_state(&mac);
    let history=platform::service(ctx).wifi_archived_jobs(&mac).map_err(|_|"wifi_history_unavailable")?;
    let result=state.jobs.iter().rev().find(|j|matches!(j.kind.as_str(),"trial"|"activate"))
        .map(control_projection).unwrap_or_else(||json!({"stage":"idle"}));
    let target=current_device_target(ctx,&mac,&state,&host).cloned();
    let matched=target.as_ref().map(|n|n.network_id.as_str());
    let connection=host_wifi::network_key(&host).and_then(|ssid|state.matching_connection(ssid))
        .map(|c|c.view(crate::device_now_secs(&mac).max(0) as u64,&state.jobs));
    let result_pending=state.jobs.iter().any(|j|j.blocks_mutations());
    let auto_state=if crate::ap01::is_ap01(ctx,&mac) {"manual_selection"}
        else if host_wifi::network_key(&host).is_none() {host["state"].as_str().unwrap_or("unknown")}
        else if target.is_none(){"no_mapping"} else if let Some(c)=&connection {
            if result_pending&&c.automatic_state()=="retry_exhausted" {"awaiting_confirmation"} else {c.automatic_state()}
        } else {"waiting_for_window"};
    for j in state.jobs.iter().filter(|j| j.terminal()) { ctx.wifi_inputs.lock().unwrap().remove(&j.job_id); }
    // Terminal history is display-only; it never re-enters execution admission.
    state.jobs.splice(0..0,history.into_iter().rev());
    Ok(json!({"device_mac":mac,"online":platform::service(ctx).device_get(&mac).map(|d|json!({"presence":d["last_authenticated"]["body"]["presence"],"transports":d["last_authenticated"]["body"]["transports"],"wifi_control":d["last_authenticated"]["body"]["wifi_control"],"observed_at":d["last_authenticated"]["observed_at"],"boot_id":d["last_authenticated"]["body"]["boot_id"],"status_seq":d["last_authenticated"]["body"]["status_seq"]})),"host":host,"permissions":permissions,"result":result,"automatic_state":auto_state,
        "matched_network_id":matched,"matched_target":target,"connection":connection,"differences":state.differences(),"wifi":state,"queued":queued,"library":platform::service(ctx).wifi_library(),"manual_selection":crate::ap01::is_ap01(ctx,&mac)}))
}

async fn raw_host(ctx:&AppCtx) -> Value {
    #[cfg(test)]
    if let Ok(host)=std::env::var("CODEX_STATUS_TEST_HOST_WIFI") {
        return serde_json::from_str(&host).expect("synthetic test host network");
    }
    // CoreLocation needs a live GUI main loop. Headless test contexts have none.
    if ctx.app_handle.get().is_none(){return json!({"state":"unavailable","networks":[]});}
    tokio::task::spawn_blocking(||host_wifi::read(false)).await.unwrap_or(Value::Null)
}
fn resolve_host(ctx:&AppCtx, mut host:Value) -> Value {
    let library=platform::service(ctx).wifi_library();
    // Empty library preserves existing installations until explicitly configured.
    if !library.families.is_empty() {
        host["family_name"]=json!(library.current_name(&host));
    }
    host
}
async fn current_host(ctx:&AppCtx) -> Value { resolve_host(ctx,raw_host(ctx).await) }
fn current_target<'a>(state: &'a WifiState, host: &Value) -> Option<&'a Network> {
    state.matching_target(host_wifi::network_key(host)?)
}
fn current_device_target<'a>(ctx:&AppCtx,mac:&str,state:&'a WifiState,host:&Value)->Option<&'a Network> {
    let target=current_target(state,host)?;
    let library=platform::service(ctx).wifi_library();
    if !library.families.is_empty() && !library.devices.get(mac).is_some_and(|ids|ids.contains(&target.network_id)) {return None;}
    Some(target)
}
fn target_query(target: &Network) -> Value {
    let mut fingerprint=Sha256::new();
    for value in [&target.host_ssid,&target.ssid,&target.label] {fingerprint.update(value.as_bytes());fingerprint.update([0]);}
    fingerprint.update([u8::from(target.credential=="open")]);
    let mut query=json!({"network_id":target.network_id,"metadata_key":format!("{:x}",fingerprint.finalize()),"open":target.credential=="open"});
    if let Some(password)=&target.password {query["credential_key"]=json!(format!("{:x}",Sha256::digest(password.as_bytes())));}
    query
}
pub(crate) async fn plan_network(ctx: &AppCtx, mac: &str, address: std::net::Ipv4Addr, budget: Budget) -> Result<Value, String> {
    let host=tokio::time::timeout(budget.remaining(),current_host(ctx)).await.map_err(|_|"host_wifi_timeout")?;
    let state=platform::service(ctx).wifi_state(mac);
    let target=current_device_target(ctx,&mac,&state,&host).ok_or("network_mapping_required")?;
    let network=json!({"v":bridge_core::template::PLAN_ACTIVATION_VERSION,
        "bridge_addr":address.to_string(),"bridge_port":ctx.config.port,"target":target_query(target)});
    bridge_core::template::validate_plan_activation(&network).map_err(|_|"plan_network_shape")?;
    Ok(network)
}
fn current_input_key(mac:&str,target:&Network,revision:u64)->String {
    format!("current:{mac}:{}:{revision}",target.network_id)
}
fn fresh_connection(state: &WifiState, target: &Network, _now: u64) -> ConnectionFacts {
    state.matching_connection(&target.host_ssid).cloned()
        .unwrap_or_else(||ConnectionFacts::new(target,state.local_revision))
}
fn observed_boot(ctx:&AppCtx,mac:&str)->String {
    platform::service(ctx).device_get(mac).map_or_else(||"null".into(),|d|d["last_authenticated"]["body"]["boot_id"].to_string())
}
fn note_match(ctx: &AppCtx, mac: &str, target: &Network, reply: &Value) -> Option<ConnectionFacts> {
    if reply["ok"]!=true || reply["network_id"]!=target.network_id {
        let error=reply["error"].as_str().filter(|e|e.len()<=64&&e.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_')).unwrap_or("invalid_match_reply");
        tracing::warn!(device_mac=mac,error_code=error,"device Wi-Fi match rejected");
        return None;
    }
    let state=platform::service(ctx).wifi_state(mac);let now=crate::device_now_secs(mac).max(0) as u64;
    if state.matching_target(&target.host_ssid).is_none_or(|current|current!=target){return None;}
    let mut result=project_match(&state,target,reply,now)?;
    result.observation.as_mut()?.boot_id=observed_boot(ctx,mac);
    if let Some(error)=result.view(now,&state.jobs).error.as_deref() {tracing::warn!(device_mac=mac,error_code=error,next_attempt_at=result.next_attempt_at,"device Wi-Fi failure observed");}
    platform::service(ctx).wifi_set_connection(mac,result.clone()).ok()?;Some(result)
}
fn project_match(state:&WifiState,target:&Network,reply:&Value,now:u64)->Option<ConnectionFacts> {
    let mut result=fresh_connection(state,target,now);
    let previous=result.view(now,&state.jobs);
    let version=reply["version"].as_u64()?;
    if result.version!=version {result.attempts=0;result.next_attempt_at=0;result.contact=None;}
    result.version=version;
    // A fresh device observation replaces stale admission errors without
    // converting local rejection into a radio failure.
    result.admission_error=None;
    let error=if reply["stage"]=="failed" {
        let text=reply["error"].as_str().unwrap_or("connect_timeout");
        Some(if !text.is_empty() && text.len()<=64 && text.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_')
            {text}else{"invalid_device_error"}.to_owned())
    }else{None};
    let stage=if reply["matches"]!=true {"needs_sync"} else if reply["connected"]==true {"connected"}
        else if error.is_some(){"connection_failed"}
        else if matches!(reply["stage"].as_str(),Some("connecting"|"accepted")){"connecting"}
        else{"waiting_for_window"};
    if error.is_some() && (previous.state!="connection_failed" || previous.error!=error) {
        result.next_attempt_at=result.next_attempt_at.max(now+60);
    }
    if matches!(error.as_deref(),Some("authentication_failed"|"security_mismatch")) {result.admission_error=error.clone();}
    result.observation=Some(NetworkObservation{boot_id:String::new(),at:now,state:stage.into(),error,
        reason:reply["reason"].as_u64().and_then(|n|u16::try_from(n).ok()).unwrap_or(0),
        ssid:reply["actual_ssid"].as_str().filter(|s|s.len()<=32&&!s.contains('\0')).map(str::to_owned),
        ip:reply["ip"].as_str().filter(|s|s.parse::<std::net::Ipv4Addr>().is_ok()).map(str::to_owned)});
    Some(result)
}
fn queue_current_recovery(ctx:&AppCtx,mac:&str,state:&WifiState,target:&Network,now:u64,_physical_window:bool)->Result<bridge_core::platform::wifi::Job,String> {
    let svc=platform::service(ctx);
    let job=svc.wifi_queue_fields_checked(mac,&ctx.bridge_id,"recovery_save",vec![target.network_id.clone()],BTreeMap::new(),Some(state.local_revision),now).map_err(|error|error.to_string())?;
    if !job.sent {
        let mut pending=ctx.wifi_inputs.lock().unwrap();
        if let Some(credentials)=pending.remove(&current_input_key(mac,target,state.local_revision)) {
            pending.entry(job.job_id.clone()).or_default().extend(credentials);
        }
        let missing=target.credential=="replace"&&target.password.is_none()&&!pending.get(&job.job_id).is_some_and(|p|p.contains_key(&target.network_id));
        drop(pending);
        if missing {let _=svc.wifi_finish(mac,&job.job_id,"waiting_input",Some("credentials_required"),now);}

    }
    Ok(job)
}
/// A read-only single-row check before PowerPlan freezes this rendezvous's work.
pub async fn prepare_connection(ctx:&AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,physical_window:bool,budget:bridge_core::rendezvous::Budget) -> Result<(), String> {
    if link.info["wc"]!=1{return Ok(());}
    // The host lookup has no GATT side effects and can be cancelled safely.
    // Radio writes below are bounded only by the carrier, using this same phase.
    let host=tokio::time::timeout(budget.remaining(),current_host(ctx)).await
        .map_err(|_|"host_network_timeout".to_owned())?;
    let state=platform::service(ctx).wifi_state(mac);
    let Some(target)=current_device_target(ctx,&mac,&state,&host) else{let _=platform::service(ctx).wifi_clear_connection(mac);return Ok(());};
    let reply=link.command_budget("wifi_match",target_query(target),budget.remaining()).await
        .map_err(|error|bridge_core::rendezvous::wifi_error(&error).1.to_owned())?;
    if reply["ok"]!=true {return Err(reply["error"].as_str().unwrap_or("protocol_rejected").to_owned());}
    if note_match(ctx,mac,target,&reply).is_some()&&(reply["matches"]!=true||target.credential=="replace") {
        queue_current_recovery(ctx,mac,&state,target,crate::device_now_secs(mac).max(0) as u64,physical_window)?;
    }
    Ok(())
}
pub fn http_ready(ctx:&AppCtx,mac:&str,status:&Value,at:u64) {
    if !status["boot_id"].is_string() {return;}
    let state=platform::service(ctx).wifi_state(mac);
    if let Some(mut c)=state.connection_facts.filter(|c|c.local_revision==state.local_revision) {
        c.note_http(status["boot_id"].to_string(),at,None);
        let _=platform::service(ctx).wifi_set_connection(mac,c);
    }
}
pub fn network_failed(ctx:&AppCtx,mac:&str,error:&str) {
    if !matches!(error,"network_not_found"|"authentication_failed"|"security_mismatch"|
        "signal_weak"|"association_failed"|"handshake_timeout"|"dhcp_timeout"|"connect_timeout") {
        if matches!(error,"not_sent"|"result_unknown"|"http_proof_budget_exhausted") {http_failed(ctx,mac,error);}
        return;
    }
    let state=platform::service(ctx).wifi_state(mac);
    if let Some(mut c)=state.connection_facts.filter(|c|c.local_revision==state.local_revision) {
        c.observation=Some(NetworkObservation{boot_id:observed_boot(ctx,mac),at:crate::device_now_secs(mac).max(0) as u64,
            state:"connection_failed".into(),error:Some(error.into()),reason:0,ssid:None,ip:None});
        c.contact=None;
        if matches!(error,"authentication_failed"|"security_mismatch") {c.admission_error=Some(error.into());}
        let _=platform::service(ctx).wifi_set_connection(mac,c);
    }
}
pub fn http_failed(ctx:&AppCtx,mac:&str,error:&str) {
    let state=platform::service(ctx).wifi_state(mac);
    if let Some(mut c)=state.connection_facts.filter(|c|c.local_revision==state.local_revision) {
        c.note_http(observed_boot(ctx,mac),crate::device_now_secs(mac).max(0) as u64,Some(error.into()));
        let _=platform::service(ctx).wifi_set_connection(mac,c);
    }
}
fn work_blocked(ctx: &AppCtx, mac: &str) -> bool {
    let svc=platform::service(ctx);
    svc.ota_job(mac).is_some_and(|j|j.blocks_following_work()) ||
        svc.job(mac).is_some_and(|j|matches!(j["state"].as_str(),Some("waiting"|"sending"|"unknown")))
}
fn opportunity_budget(status: &Value, sent: bool, sample_start: std::time::Instant, budget: bridge_core::rendezvous::Budget) -> bridge_core::rendezvous::Budget {
    if status["wifi_management"] != 3 { return budget.capped(Duration::ZERO); }
    let budget = if sent || status["wifi_control"]["mode"] == "continuous" { budget } else {
        bridge_core::rendezvous::Budget::at(sample_start, Duration::from_millis(
            status["wifi_control"]["remaining_ms"].as_u64().unwrap_or(0).saturating_sub(1000))).capped(budget.remaining())
    };
    budget.capped(bridge_core::rendezvous::WIFI_WORK_BUDGET)
}

fn opportunity_window(status: &Value, mac: &str) -> bool {
    DeviceIdentity::normalized_mac(status["device_mac"].as_str().unwrap_or("")) == Some(mac.to_owned()) &&
        status["wifi_management"] == 3 && status["wifi_control"]["read_allowed"] == true &&
        (status["wifi_control"]["mode"] == "continuous" || status["wifi_control"]["remaining_ms"].as_u64().unwrap_or(0) > 0)
}

// Use the current host's saved target, within the existing rendezvous budget.
pub async fn automatic_on_link(ctx: &AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,budget:bridge_core::rendezvous::Budget,selection_only:bool) -> Result<(), String> {
    let contact_host=tokio::time::timeout(budget.remaining(), current_host(ctx)).await.map_err(|_| "host_network_timeout".to_owned())?;
    if let Err(error)=host_wifi::require_same_network(&contact_host,&contact_host) {
        return if selection_only { Ok(()) } else { Err(error) };
    }
    if link.info["wifi_management"] != 3 {return Err("update_required".into());}
    let svc=platform::service(ctx);
    if svc.ota_job(mac).is_some_and(|j|j.blocks_network_repair()) ||
        svc.job(mac).is_some_and(|j|matches!(j["state"].as_str(),Some("waiting"|"sending"|"unknown"))) {return Ok(());}
    // At most one confirmed save followed by one activation in this budget.
    for _ in 0..2 {
        let host=tokio::time::timeout(budget.remaining(), current_host(ctx)).await.map_err(|_| "host_network_timeout".to_owned())?;
        host_wifi::require_same_network(&contact_host,&host)?;
        let state=platform::service(ctx).wifi_state(mac);
        let Some(target)=current_device_target(ctx,&mac,&state,&host).cloned() else{
            return if selection_only { Ok(()) } else { Err("network_mapping_required".into()) };
        };
        let now=crate::device_now_secs(mac).max(0) as u64;
        if link.info["wc"]!=1 {let mut c=fresh_connection(&state,&target,now);c.admission_error=Some("firmware_upgrade_required".into());let _=platform::service(ctx).wifi_set_connection(mac,c);return Ok(());}
        let reply=link.command_budget("wifi_match",target_query(&target),budget.remaining()).await
            .map_err(|error|bridge_core::rendezvous::wifi_error(&error).1.to_owned())?;
        let mut connection=note_match(ctx,mac,&target,&reply).ok_or("invalid_match_reply")?;
        // A new replace intent always needs a real input/confirmed write. Matching
        // public metadata cannot prove a password replacement was applied.
        if reply["matches"]!=true || target.credential=="replace" {
            // Physical wake selects only a previously saved network. No password
            // write or automatic provisioning; the device owns fallback.
            if selection_only { return Ok(()); }
            if svc.ota_job(mac).is_some_and(|j|j.state=="awaiting_confirmation") {return Err("ota_confirmation_network_mismatch".into());}
            let svc=platform::service(ctx);
            let job=queue_current_recovery(ctx,mac,&state,&target,now,true)?;
            let missing=target.credential=="replace"&&target.password.is_none()&&!ctx.wifi_inputs.lock().unwrap().get(&job.job_id).is_some_and(|p|p.contains_key(&target.network_id));
            if missing {let _=svc.wifi_finish(mac,&job.job_id,"waiting_input",Some("credentials_required"),now);return Err("credentials_required".into());}
            recover_on_link_budget(ctx,mac,link,budget).await;
            let outcome=svc.wifi_state(mac).jobs.into_iter().find(|j|j.job_id==job.job_id).ok_or("wifi_job_not_found")?;
            if outcome.state=="completed" {continue;}
            return Err(outcome.error_code.unwrap_or("wifi_not_dispatched".into()));
        }
        if reply["connected"]==true || matches!(reply["stage"].as_str(),Some("connecting"|"accepted")){return Ok(());}
        let existing_active=reply["active"]==true && reply["version"].as_u64().is_some_and(|v|v>0 && v<=u32::MAX as u64);
        if let Some(error)=connection.view(now,&state.jobs).error.as_deref() {
            if !matches!(error,"window_closed"|"handshake_timeout"|"network_not_found"|"signal_weak"|"association_failed"|"dhcp_timeout"|"connect_timeout"|"disconnected"|"cancelled") &&
                !(existing_active && error=="wifi_confirmation_pending") {return Err(error.to_owned());}
        }
        if existing_active {
            let fresh=tokio::time::timeout(budget.remaining(), current_host(ctx)).await.map_err(|_| "host_network_timeout".to_owned())?;
            host_wifi::require_same_network(&contact_host,&fresh)?;
            // Matching the device's current active version needs no persistent
            // activation. The caller's confirmed Plan/network_start reconnects
            // it; unknown historical submissions still fence conflicting writes.
            connection.admission_error=None;
            connection.requested=false;
            svc.wifi_set_connection(mac,connection).map_err(|_|"persistence_failed".to_owned())?;
            tracing::info!(event="existing_network_reconnect",device_mac=mac,network_id=%target.network_id,
                version=reply["version"].as_u64(),"verified active network will reconnect without a configuration transaction");
            return Ok(());
        }
        if connection.attempts>=3 {return Err("wifi_retry_exhausted".into());}
        if now<connection.next_attempt_at {return Err("wifi_retry_backoff".into());}
        let fresh=tokio::time::timeout(budget.remaining(), current_host(ctx)).await.map_err(|_| "host_network_timeout".to_owned())?;
        host_wifi::require_same_network(&contact_host,&fresh)?;
        let owner=link.command_budget("wifi_owner",json!({}),budget.remaining()).await
            .map_err(|error|bridge_core::rendezvous::wifi_error(&error).1.to_owned())?;
        if owner["ok"]!=true {return Err("owner_check_failed".into());}
        let owner_id=match owner.get("owner_id") {None=>"",Some(Value::String(id))=>id.as_str(),_=>return Err("owner_check_unknown".into())};
        if !owner_id.is_empty() && owner_id!=ctx.bridge_id {connection.admission_error=Some("occupied".into());let _=platform::service(ctx).wifi_set_connection(mac,connection);return Err("occupied".into());}
        let job=match svc.wifi_queue_control(mac,&ctx.bridge_id,"activate",json!({"network_id":target.network_id,"version":reply["version"]}),now) {
            Ok(job)=>job,
            Err(error)=>{let code=error.to_string();connection.admission_error=Some(code.clone());
                svc.wifi_set_connection(mac,connection).map_err(|_|"persistence_failed".to_owned())?;return Err(code);}
        };
        let was_sent=job.sent;
        unified_on_link(ctx,mac,link,budget,Some(&job.job_id)).await;
        let current=svc.wifi_state(mac).jobs.into_iter().find(|j|j.job_id==job.job_id).ok_or("wifi_job_not_found")?;
        // An accepted asynchronous activation needs the caller's network_start
        // before BLE release; its result remains pending under the same ID.
        if !current.permits_network_start() {
            return Err(current.error_code.clone().unwrap_or_else(||
                if current.sent { "result_unknown" } else { "wifi_not_dispatched" }.into()));
        }
        if !was_sent {
            // The exchange can update boot/contact facts; merge policy into the latest view.
            connection=svc.wifi_state(mac).connection_facts
                .filter(|c|c.local_revision==state.local_revision && c.network_id==target.network_id)
                .ok_or("local_revision_conflict")?;
            connection.attempts+=1;connection.next_attempt_at=now+60;connection.requested=false;connection.admission_error=None;
            svc.wifi_set_connection(mac,connection).map_err(|_|"persistence_failed".to_owned())?;
        }
        return Ok(());
    }
    Err("wifi_match_unconfirmed".into())
}

// Current authenticated status; no transient BLE permit is created.
pub async fn note_link(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection) -> Value {
    note_link_budget(ctx,mac,link,bridge_core::rendezvous::Budget::new(Duration::from_secs(5))).await
}
pub async fn note_link_budget(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection, budget: bridge_core::rendezvous::Budget) -> Value {
    if link.info["wifi_management"] != 3 {
        let svc = platform::service(ctx);
        for job in svc.wifi_state(mac).jobs.iter().filter(|j| !j.terminal() && !j.sent) {
            let _ = svc.wifi_finish(mac, &job.job_id, "failed", Some("update_required"), crate::device_now_secs(mac).max(0) as u64);
        }
        return json!({"state":"failed","error":"update_required"});
    }
    match link.wifi_read_budget("wifi_status",budget).await {
        Ok(status) => {
            if platform::service(ctx).note_authenticated_status_via_at(mac, &status, "ble", crate::device_now_secs(mac).max(0) as u64).is_err() {
                return json!({"state":"failed","error":"authenticated_status_invalid"});
            }
            json!({"ok":true,"mode":"authenticated_control","wifi_control":status["wifi_control"]})
        }
        Err(error) => json!({"state":"waiting_authorization","error":bridge_core::rendezvous::wifi_error(&error).1}),
    }
}

pub async fn opportunity(ctx: &AppCtx, mac: &str) {
    let lock = platform::delivery_lock(ctx, mac);
    let Ok(_guard) = lock.try_lock() else { return; };
    opportunity_locked(ctx, mac).await;
}
async fn immediate(ctx:&AppCtx,mac:&str,job_id:&str) {
    let lock=platform::delivery_lock(ctx,mac);
    let Ok(_guard)=lock.try_lock() else {defer_job(ctx,mac,job_id,"device_channel_busy");return;};
    opportunity_admitted(ctx,mac,job_id,bridge_core::rendezvous::Budget::new(Duration::from_secs(10))).await;
}

fn defer_job(ctx:&AppCtx,mac:&str,job_id:&str,reason:&str) {
    let svc=platform::service(ctx);
    let Some(job)=svc.wifi_state(mac).jobs.into_iter().find(|j|j.job_id==job_id) else{return;};
    if job.terminal() || matches!(job.state.as_str(),"reading"|"applying"|"connecting"|"needs_attention") {return;}
    let _=svc.wifi_finish(mac,job_id,if job.sent {"awaiting_confirmation"}else{"waiting_rendezvous"},Some(reason),crate::device_now_secs(mac).max(0) as u64);
    tracing::info!(event="wifi_wait",device_mac=%mac,job_id,reason,"Wi-Fi task waiting");
}
pub async fn opportunity_locked(ctx: &AppCtx, mac: &str) {
    acknowledge_receipts_http(ctx,mac).await;
    let svc=platform::service(ctx);
    if svc.wifi_state(mac).jobs.iter().any(|j|!j.sent && !j.terminal() && matches!(j.kind.as_str(),"sync"|"recovery_save"|"trial"|"activate")) &&
        svc.device_get(mac).is_some_and(|d|d["last_authenticated"]["body"]["wifi_control"]["reason"]=="claim_required") {
        let _=crate::post_claim_for_mac(ctx,false,false,mac).await;
    }
    let now = crate::device_now_secs(mac).max(0) as u64;
    if let Some(job) = platform::service(ctx).wifi_state(mac).jobs.into_iter()
        .find(|j| bridge_core::rendezvous::admit_wifi(j, now)) {
        opportunity_admitted(ctx, mac, &job.job_id, bridge_core::rendezvous::Budget::new(Duration::from_secs(10))).await;
    }
}

async fn acknowledge_receipts_http(ctx:&AppCtx,mac:&str) {
    let svc=platform::service(ctx).clone();
    let now=crate::device_now_secs(mac).max(0) as u64;
    if !svc.wifi_state(mac).jobs.iter().any(|j|j.settlement_due(now)){return;}let Some(link)=platform::device_link_for_mac(ctx,mac) else{return;};
    let work = async {
        let budget=bridge_core::rendezvous::Budget::new(Duration::from_secs(4));
        let Ok(status)=bridge_core::device_client::status_compact(&link.ip,&link.token,budget.remaining()).await else{return;};
        if status["wifi_management"]!=3 || status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()!=Some(link.mac.as_str()){return;}
        settle_receipts_http(&svc,&link,&status,budget,now).await;
    };
    work.await;
}

pub async fn opportunity_admitted(ctx: &AppCtx, mac: &str, job_id: &str, budget: bridge_core::rendezvous::Budget) {
    let svc = platform::service(ctx).clone(); let now = crate::device_now_secs(mac).max(0) as u64;
    let Some(job) = svc.wifi_state(mac).jobs.into_iter().find(|j| j.job_id == job_id && bridge_core::rendezvous::admit_wifi(j, now)) else { return; };
    if !job.sent {
        if let Some(expected)=job.host_ssid.as_deref() {
            let host=match tokio::time::timeout(budget.remaining(),current_host(ctx)).await {
                Ok(host)=>host,Err(_)=>{defer_job(ctx,mac,job_id,"host_network_timeout");return;}
            };
            if host_wifi::network_key(&host)!=Some(expected) {
                let _=svc.wifi_finish(mac,job_id,"failed",Some("host_network_changed"),now);return;
            }
        }
    }
    if work_blocked(ctx, mac) {defer_job(ctx,mac,job_id,"device_delivery_pending");return;}
    let Some(link) = platform::device_link_for_mac(ctx, mac) else {defer_job(ctx,mac,job_id,"device_endpoint_unavailable");return;};
    tracing::info!(event="wifi_attempt",device_mac=%mac,job_id,kind=%job.kind,"Wi-Fi task checking HTTP connection");
    let credentials = ctx.wifi_inputs.lock().unwrap().get(&job.job_id).cloned().unwrap_or_default();
    let cleanup_id = job.job_id.clone();
    let job_id = job.job_id.clone();
    let work = async {
        let sample_start = std::time::Instant::now();
        let status = bridge_core::device_client::status(&link.ip, &link.token, budget.remaining()).await;
        let status = match status { Ok(status) => status, Err(error) => {
            if svc.wifi_begin(&link.mac,&job_id,now).ok().flatten().is_some() {
                let (state, code) = bridge_core::rendezvous::wifi_error(&error);
                let state = if job.sent { "awaiting_confirmation" } else { state };
                let code = if matches!(code,"transport_timeout"|"transport_unavailable"|"not_sent") { "http_status_unavailable" } else { code };
                let _ = svc.wifi_finish(&link.mac,&job_id,state,Some(code),now);
            }
            return;
        } };
        if DeviceIdentity::normalized_mac(status["device_mac"].as_str().unwrap_or("")) != Some(link.mac.clone()) {
            let _ = svc.wifi_finish(&link.mac,&job_id,if job.sent {"awaiting_confirmation"} else {"failed"},Some("http_identity_mismatch"),now); return;
        }
        if status["wifi_management"] != 3 {
            let _ = svc.wifi_finish(&link.mac,&job_id,if job.sent {"awaiting_confirmation"} else {"failed"},Some("update_required"),now); return;
        }
        let _=svc.note_authenticated_status_at(&link.mac,&status,now);
        if !prepare_transaction(&svc,&job,&link.bridge_id,&status,now) {return;}
        // Queries remain legal after the write window closes.
        if !job.sent && !opportunity_window(&status, &link.mac) {
            let _=svc.wifi_finish(&link.mac,&job_id,"waiting_rendezvous",Some("window_required"),now);return;
        }
        let budget = opportunity_budget(&status, job.sent, sample_start, budget);
        let writable=status["wifi_control"]["write_allowed"]==true;
        if !job.sent && matches!(job.kind.as_str(),"sync"|"recovery_save") &&
            status["wifi_storage"]["revision"].as_u64().zip(job.expected_revision).is_some_and(|(current,expected)|current!=expected) {
            if job.fields.is_empty() {
                let revision=status["wifi_storage"]["revision"].as_u64().unwrap();let mut versions=BTreeMap::new();
                for target in &job.frozen {
                    let mut probe=target.clone();
                    if let Some(password)=credentials.get(&target.network_id){probe.password=Some(password.clone());probe.credential=if password.is_empty(){"open".into()}else{"replace".into()};}
                    let timeout=budget.remaining();if timeout.is_zero(){break;}
                    let Ok(reply)=bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,"match",target_query(&probe),timeout).await else{break;};
                    if reply["ok"]!=true || reply["network_id"]!=target.network_id || !(if target.deleted {reply["version"]==0}else{reply["matches"]==true}){break;}
                    let Some(version)=reply["version"].as_u64() else{break;};versions.insert(target.network_id.clone(),version);
                }
                if versions.len()==job.selected.len() && !budget.remaining().is_zero() &&
                    bridge_core::device_client::status(&link.ip,&link.token,budget.remaining()).await.is_ok_and(|fresh|fresh["boot_id"]==status["boot_id"] && fresh["wifi_storage"]["revision"]==revision && fresh["device_mac"]==status["device_mac"]) {
                    let _=svc.wifi_confirm_current(&link.mac,&job_id,revision,&versions,now);return;
                }
            }
            // Authenticated revision already proves this frozen cache is stale.
            // Stop before mark_sent; never turn a lost refusal into uncertainty.
            let _=svc.wifi_finish(&link.mac,&job_id,"conflict",Some("revision_conflict"),now);return;
        }
        let _ = svc.wifi_attempt_async(&link.mac,&job_id,&link.bridge_id,
            status["wifi_control"]["namespace"].as_str().unwrap_or(""),writable,&credentials,now,
            &mut HttpTransport { link: &link, budget, now }).await;
        settle_receipts_http(&svc,&link,&status,budget,now).await;
    };
    work.await;
    if platform::service(ctx).wifi_state(mac).jobs.iter().find(|j| j.job_id == cleanup_id).is_some_and(|j| j.terminal()) {
        ctx.wifi_inputs.lock().unwrap().remove(&cleanup_id);
    }
}


async fn settle_receipts_http(svc:&bridge_core::platform::service::PlatformService,link:&platform::DeviceLink,
    status:&Value,budget:bridge_core::rendezvous::Budget,now:u64) {
    for job in svc.wifi_state(&link.mac).jobs.into_iter().filter(|j|j.settlement_due(now)) {
        if budget.remaining().is_zero(){break;}
        if !receipt_context_matches(&job,&link.bridge_id,status){continue;}
        let _=svc.wifi_settle_async(&link.mac,&job.job_id,&link.bridge_id,
            status["wifi_control"]["namespace"].as_str().unwrap_or(""),now,
            &mut HttpTransport { link, budget, now }).await;
    }
}

async fn http_control_read(ctx:&AppCtx,mac:&str,op:&str,body:Value)->Option<Value> {
    let link=platform::device_link_for_mac(ctx,mac)?;let svc=platform::service(ctx).clone();let op=op.to_owned();let now=crate::device_now_secs(mac).max(0) as u64;
    let budget=bridge_core::rendezvous::Budget::new(Duration::from_secs(5));
    (async move {
        let status=bridge_core::device_client::status_compact(&link.ip,&link.token,budget.remaining()).await.ok()?;
        if status["wifi_management"]!=3 || status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()!=Some(link.mac.as_str()){return None;}
        svc.note_authenticated_status_at(&link.mac,&status,now).ok()?;
        bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,&op,body,budget.remaining()).await.ok()
    }).await
}

struct HttpTransport<'a> {
    link: &'a platform::DeviceLink,
    budget: bridge_core::rendezvous::Budget,
    now: u64,
}
impl bridge_core::platform::wifi::Transport for HttpTransport<'_> {
    async fn exchange(&mut self, op: &str, body: Value) -> anyhow::Result<Value> {
        let timeout=self.budget.remaining();
        if timeout.is_zero() { return Err(bridge_core::rendezvous::NotDispatched.into()); }
        let link=self.link;
        if op == "snapshot" {
            let mut snapshot=bridge_core::device_client::wifi_snapshot(&link.ip,&link.token,&link.mac,&link.bridge_id,timeout).await?;
            snapshot.observed_at=self.now;
            Ok(serde_json::to_value(snapshot)?)
        } else {
            bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,op,body,timeout).await
        }
    }
}

struct BleTransport<'a> {
    link: &'a mut bridge_ble::DeviceConnection,
    budget: bridge_core::rendezvous::Budget,
}

impl bridge_core::platform::wifi::Transport for BleTransport<'_> {
    async fn exchange(&mut self, op: &str, body: Value) -> anyhow::Result<Value> {
        if self.budget.remaining().is_zero() {
            return Err(bridge_core::rendezvous::NotDispatched.into());
        }
        if op == "snapshot" {
            self.link.wifi_snapshot_budget(self.budget).await
        } else {
            let id = body["request_id"].as_str().map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
            let wire = format!("wifi_{op}");
            self.link.command_with_id_budget(&wire, body, &id, self.budget.remaining()).await
        }
    }
}

/// Both carriers drive the same durable transitions; BLE awaits its own I/O.
async fn unified_on_link(ctx:&AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,budget:bridge_core::rendezvous::Budget,job_id:Option<&str>) {
    let svc=platform::service(ctx).clone();let now=crate::device_now_secs(mac).max(0) as u64;
    let Some(job)=svc.wifi_state(mac).jobs.into_iter().find(|j|j.runnable(now) && job_id.is_none_or(|id|j.job_id==id)) else{return;};
    if !job.sent {
        if let Some(expected) = job.host_ssid.as_deref() {
            let host = match tokio::time::timeout(budget.remaining(),current_host(ctx)).await {
                Ok(host)=>host,
                Err(_)=>{defer_job(ctx,mac,&job.job_id,"host_network_timeout");return;}
            };
            if host_wifi::network_key(&host) != Some(expected) {
                let _ = svc.wifi_finish(mac, &job.job_id, "failed", Some("host_network_changed"), now);
                return;
            }
        }
    }
    let status = if matches!(job.kind.as_str(),"read"|"read_import") {Value::Null} else {
        match link.wifi_read_budget("wifi_status",budget).await {
            Ok(status) => status,
            Err(error) => {
                let _=svc.wifi_finish(mac,&job.job_id,if job.sent {"awaiting_confirmation"} else {"waiting_rendezvous"},Some(bridge_core::rendezvous::wifi_error(&error).1),now);
                return;
            }
        }
    };
    if !status.is_null() && svc.note_authenticated_status_via_at(mac,&status,"ble",now).is_err() {return;}
    if !prepare_transaction(&svc,&job,&ctx.bridge_id,&status,now) {return;}
    let writable = if job.sent {false} else {
        match link.command_budget("wifi_owner",json!({}),budget.remaining()).await {
            Ok(reply)=>reply["ok"]==true && reply["owner_id"]==ctx.bridge_id,
            Err(error)=>{
                let _=svc.wifi_finish(mac,&job.job_id,"waiting_rendezvous",Some(bridge_core::rendezvous::wifi_error(&error).1),now);
                return;
            }
        }
    };
    let credentials=ctx.wifi_inputs.lock().unwrap().get(&job.job_id).cloned().unwrap_or_default();
    let namespace=status["wifi_control"]["namespace"].as_str().unwrap_or("");
    let _=svc.wifi_attempt_async(mac,&job.job_id,&ctx.bridge_id,namespace,writable,&credentials,now,
        &mut BleTransport {link,budget}).await;
    acknowledge_receipts_on_link_with_status(ctx,mac,link,budget,&status).await;
}

/// Retire already durable results even when no business job remains runnable.
pub async fn acknowledge_receipts_on_link_budget(ctx:&AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,budget:bridge_core::rendezvous::Budget) {
    let svc=platform::service(ctx);
    if budget.remaining().is_zero() || link.info["wifi_management"]!=3 ||
        !svc.wifi_state(mac).jobs.iter().any(|j|j.settlement_due(crate::device_now_secs(mac).max(0) as u64)) {return;}
    let Ok(status)=link.wifi_read_budget("wifi_status",budget).await else{return;};
    if svc.note_authenticated_status_via_at(mac,&status,"ble",crate::device_now_secs(mac).max(0) as u64).is_err() {return;}
    acknowledge_receipts_on_link_with_status(ctx,mac,link,budget,&status).await;
}

async fn acknowledge_receipts_on_link_with_status(ctx:&AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,
    budget:bridge_core::rendezvous::Budget,status:&Value) {
    let now=crate::device_now_secs(mac).max(0) as u64;
    for done in platform::service(ctx).wifi_state(mac).jobs.into_iter().filter(|j|j.settlement_due(now)) {
        if !receipt_context_matches(&done,&ctx.bridge_id,status) {continue;}
        if budget.remaining().is_zero(){break;}
        let _=platform::service(ctx).wifi_settle_async(mac,&done.job_id,&ctx.bridge_id,
            status["wifi_control"]["namespace"].as_str().unwrap_or(""),now,
            &mut BleTransport {link,budget}).await;
    }
}

fn receipt_context_matches(job:&bridge_core::platform::wifi::Job,principal:&str,status:&Value)->bool {
    status["wifi_control"]["principal"].as_str()==Some(principal) &&
        job.check_transaction_context(principal,status["wifi_control"]["namespace"].as_str().unwrap_or("")).is_ok()
}

fn prepare_transaction(svc:&bridge_core::platform::service::PlatformService,job:&bridge_core::platform::wifi::Job,
    principal:&str,status:&Value,now:u64)->bool {
    if matches!(job.kind.as_str(),"read"|"read_import") {return true;}
    let result=if status["wifi_control"]["principal"].as_str()!=Some(principal) {
        Err(anyhow::anyhow!("transaction_principal_changed"))
    } else {
        svc.wifi_bind_transaction(&job.device_mac,&job.job_id,principal,status["wifi_control"]["namespace"].as_str().unwrap_or(""))
    };
    if let Err(error)=result {
        let _=svc.wifi_finish(&job.device_mac,&job.job_id,if job.sent {"awaiting_confirmation"} else {"waiting_rendezvous"},Some(&error.to_string()),now);
        return false;
    }
    true
}

/// Read admitted by the rendezvous RF window, using the same durable service.
pub async fn read_on_link_budget(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection, budget: bridge_core::rendezvous::Budget) {
    let now = crate::device_now_secs(mac).max(0) as u64;
    if let Some(job) = platform::service(ctx).wifi_state(mac).jobs.into_iter()
        .find(|j| j.kind == "read" && j.runnable(now)) {
        unified_on_link(ctx, mac, link, budget, Some(&job.job_id)).await;
    }
}

/// Bounded physical recovery only; shared stable transaction ID and receipt.
pub async fn recover_on_link(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection) {
    recover_on_link_budget(ctx, mac, link, bridge_core::rendezvous::Budget::new(Duration::from_secs(8))).await;
}
pub async fn recover_on_link_budget(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection, budget: bridge_core::rendezvous::Budget) {
    if work_blocked(ctx, mac) { return; }
    if link.info["wifi_management"] != 3 {
        note_link(ctx, mac, link).await;
        return;
    }
    let now = crate::device_now_secs(mac).max(0) as u64;
    if let Some(job) = platform::service(ctx).wifi_state(mac).jobs.into_iter()
        .find(|j| (j.sent || matches!(j.kind.as_str(), "sync" | "recovery_save")) && j.runnable(now)) {
        unified_on_link(ctx, mac, link, budget, Some(&job.job_id)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn wifi_family_application_paths_and_mcp_secret_boundary() {
        let root=tempfile::tempdir().unwrap();let mac="AABBCCDDEEFF";let other="AABBCCDDEE01";
        let ctx=platform::ble_close_lock_tests::test_ctx(root.path(),&[mac,other]);
        action(&ctx,mac,"library_save",json!({"library_revision":0,"families":[{"id":"home","ssid_2g":"House-2","ssid_5g":"House-5","password_2g":"synthetic-password","password_5g":"synthetic-password"}]})).await.unwrap();
        action(&ctx,mac,"library_select",json!({"library_revision":1,"selected":["home"]})).await.unwrap();
        action(&ctx,mac,"library_wired",json!({"library_revision":2,"family_id":"home"})).await.unwrap();
        let svc=platform::service(&ctx);let state=svc.wifi_state(mac);
        for raw in [json!({"state":"connected","networks":[{"ssid":"House-2"}]}),json!({"state":"connected","networks":[{"ssid":"House-5"}]}),json!({"state":"no_wifi"})] {
            let host=resolve_host(&ctx,raw);let target=current_device_target(&ctx,mac,&state,&host).unwrap();
            assert_eq!(target.host_ssid,"House-5");assert_eq!(target.ssid,"House-2");
            assert!(current_device_target(&ctx,other,&state,&host).is_none());
        }
        let unknown=resolve_host(&ctx,json!({"state":"connected","networks":[{"ssid":"Unknown"}]}));
        assert!(current_device_target(&ctx,mac,&state,&unknown).is_none());
        let view=platform::tool(&ctx,"platform_wifi_view",&json!({"mac":mac})).await.unwrap();
        assert!(!view.contains("synthetic-password"));assert!(!view.contains("password_2g"));
        assert!(platform::tool(&ctx,"platform_wifi_action",&json!({"mac":mac,"op":"library_save","body":{
            "library_revision":3,"families":[{"id":"home","ssid_2g":"House-2","password_2g":"forbidden"}]}})).await.is_err());
        // Non-secret MCP edits preserve passwords omitted by its redacted view.
        platform::tool(&ctx,"platform_wifi_action",&json!({"mac":mac,"op":"library_save","body":{
            "library_revision":3,"families":[{"id":"home","name":"Renamed","ssid_2g":"House-2","ssid_5g":"House-5"}]}})).await.unwrap();
        assert_eq!(svc.wifi_library().families[0].password_2g.as_deref(),Some("synthetic-password"));
        assert_eq!(svc.wifi_state(mac).targets[0].host_ssid,"Renamed");
    }

    #[tokio::test]
    async fn wifi_ap01_list_roundtrip_manual_auth_and_lost_ack_never_reposts() {
        use axum::{extract::State,http::{Method,HeaderMap,StatusCode},body::Bytes,response::IntoResponse,routing::any,Json,Router};
        use std::sync::{Arc,Mutex};
        #[derive(Default)] struct Fake {revision:u64,puts:usize,rows:Vec<Value>,lose_ack:bool,wrong_mac:bool}
        async fn route(State(fake):State<Arc<Mutex<Fake>>>,method:Method,headers:HeaderMap,body:Bytes)->axum::response::Response {
            if headers.get("authorization").and_then(|h|h.to_str().ok())!=Some("Bearer 01234567") {
                return (StatusCode::UNAUTHORIZED,Json(json!({"error":"manual_auth","result":"rejected"}))).into_response();
            }
            let mut f=fake.lock().unwrap();
            if method==Method::PUT {
                let request:Value=serde_json::from_slice(&body).unwrap();
                assert_eq!(request["device_mac"],"AABBCCDDEEFF");assert_eq!(request["expected_revision"],f.revision);
                f.puts+=1;f.revision+=1;
                f.rows=request["profiles"].as_array().unwrap().iter().map(|r|json!({"ssid":r["ssid"],"has_password":true,"source":"saved","current":false})).collect();
                if f.lose_ack {return (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"result":"unknown","error":"readback_unavailable"}))).into_response();}
            }
            Json(json!({"result":"ok","device_mac":if f.wrong_mac {"AABBCCDDEE00"}else{"AABBCCDDEEFF"},"revision":f.revision,"profiles":f.rows})).into_response()
        }
        let fake=Arc::new(Mutex::new(Fake{rows:vec![json!({"ssid":"Keep","has_password":true,"source":"saved","current":true})],..Default::default()}));
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let endpoint=listener.local_addr().unwrap().to_string();
        let router=Router::new().route("/api/wifi/profiles",any(route)).with_state(fake.clone());
        let server=tokio::spawn(async move {axum::serve(listener,router).await.unwrap();});
        let root=tempfile::tempdir().unwrap();let mac="AABBCCDDEEFF";let ctx=platform::ble_close_lock_tests::test_ctx(root.path(),&[mac]);
        let svc=platform::service(&ctx);
        let state=svc.wifi_save(mac,0,serde_json::from_value(json!([{"network_id":"home","host_ssid":"House-5","ssid":"House-2","password":"synthetic-password","credential":"replace"}])).unwrap(),1).unwrap();
        let code=json!({"device_code":"01234567"});
        assert_eq!(ap01_action_at(&ctx,mac,"get",&json!({}),&endpoint).await.unwrap_err(),"ap01_device_code_required");
        assert_eq!(ap01_action_at(&ctx,mac,"get",&json!({"device_code":"00000000"}),&endpoint).await.unwrap()["error"],"manual_auth");
        ap01_action_at(&ctx,mac,"get",&code,&endpoint).await.unwrap();
        assert!(svc.wifi_state(mac).snapshot.unwrap().networks[0].network.password.is_none());
        fake.lock().unwrap().wrong_mac=true;
        assert_eq!(ap01_action_at(&ctx,mac,"get",&code,&endpoint).await.unwrap_err(),"http_identity_mismatch");
        fake.lock().unwrap().wrong_mac=false;
        let snapshot=svc.wifi_state(mac).snapshot.unwrap();
        let mut request=json!({"device_code":"01234567","local_revision":state.local_revision,"snapshot_id":snapshot.snapshot_id,"selected":["home"]});
        let reply=ap01_action_at(&ctx,mac,"sync",&request,&endpoint).await.unwrap();
        assert_eq!(reply["state"],"completed");assert_eq!(reply["manual_selection"],true);
        assert_eq!(fake.lock().unwrap().rows.len(),2);assert_eq!(fake.lock().unwrap().puts,1);
        assert!(svc.wifi_state(mac).ap01_pending.is_none());
        // A committed write whose response was lost stays unknown; query only.
        request["snapshot_id"]=json!(svc.wifi_state(mac).snapshot.unwrap().snapshot_id);
        fake.lock().unwrap().lose_ack=true;
        assert_eq!(ap01_action_at(&ctx,mac,"sync",&request,&endpoint).await.unwrap()["state"],"needs_attention");
        assert_eq!(ap01_action_at(&ctx,mac,"sync",&request,&endpoint).await.unwrap_err(),"ap01_wifi_result_unknown");
        assert_eq!(ap01_action_at(&ctx,mac,"ap01_reconcile",&code,&endpoint).await.unwrap()["state"],"needs_attention");
        assert_eq!(fake.lock().unwrap().puts,2);
        let persisted=std::fs::read_to_string(root.path().join("platform/state.json")).unwrap();assert!(!persisted.contains("01234567"));
        let accepted=ap01_action_at(&ctx,mac,"ap01_accept_observed",&code,&endpoint).await.unwrap();
        assert_eq!(accepted["outcome"],"original_result_unknown");assert_eq!(fake.lock().unwrap().puts,2);
        server.abort();
        // Drop the listening socket before replying to GET: PUT is definitively
        // not sent, so it must clear the uncertainty fence and permit retry.
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let offline_endpoint=listener.local_addr().unwrap().to_string();
        let reply={let f=fake.lock().unwrap();json!({"result":"ok","device_mac":mac,"revision":f.revision,"profiles":f.rows})};
        let server=tokio::spawn(async move {
            use tokio::io::{AsyncReadExt,AsyncWriteExt};
            let (mut connection,_)=listener.accept().await.unwrap();drop(listener);
            let mut buffer=[0u8;4096];let _=connection.read(&mut buffer).await.unwrap();
            let body=serde_json::to_string(&reply).unwrap();
            connection.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
        });
        request["snapshot_id"]=json!(svc.wifi_state(mac).snapshot.unwrap().snapshot_id);
        let failed=ap01_action_at(&ctx,mac,"sync",&request,&offline_endpoint).await.unwrap();
        assert_eq!(failed["outcome"],"not_sent");assert_eq!(failed["stage"],"http_connect");
        assert!(svc.wifi_state(mac).ap01_pending.is_none());server.await.unwrap();
    }

    #[test]
    fn pending_credentials_are_isolated_by_application_instance() {
        let first=tempfile::tempdir().unwrap();let second=tempfile::tempdir().unwrap();
        let first=platform::ble_close_lock_tests::test_ctx(first.path(), &[]);
        let second=platform::ble_close_lock_tests::test_ctx(second.path(), &[]);
        first.wifi_inputs.lock().unwrap().insert("same-job".into(),
            BTreeMap::from([("home".into(), "test-only-password".into())]));
        assert!(second.wifi_inputs.lock().unwrap().is_empty());
        drop(first);
        assert!(second.wifi_inputs.lock().unwrap().is_empty());
    }
    #[test]
    fn receipt_ack_requires_frozen_authenticated_transaction_context() {
        let mut state=WifiState::default();
        let mut job=state.queue("AABBCCDDEEFF","original","activate",vec!["home".into()],100,"job".into(),BTreeMap::new()).unwrap();
        let status=json!({"wifi_control":{"principal":"original","namespace":"bond-a"}});
        assert!(!receipt_context_matches(&job,"original",&status));
        job.transaction_namespace=Some("bond-a".into());
        assert!(receipt_context_matches(&job,"original",&status));
        assert!(!receipt_context_matches(&job,"other",&status));
        for control in [json!({"principal":"other","namespace":"bond-a"}),
            json!({"principal":"original","namespace":"bond-b"}),
            json!({"principal":"original"}),json!({"namespace":"bond-a"})] {
            assert!(!receipt_context_matches(&job,"original",&json!({"wifi_control":control})));
        }
    }
    #[test]
    fn window_closed_updates_connecting_state_and_keeps_retry_backoff() {
        let target:Network=serde_json::from_value(json!({"network_id":"home","host_ssid":"Host","ssid":"Device","version":1})).unwrap();
        let mut state=WifiState::default();state.targets.push(target.clone());
        let mut initial=fresh_connection(&state,&target,100);initial.version=1;initial.attempts=1;state.connection_facts=Some(initial);
        let mut reply=json!({"ok":true,"network_id":"home","version":1,"matches":true,"connected":false,"stage":"failed","error":"window_closed"});
        let failed=project_match(&state,&target,&reply,120).unwrap();
        assert_eq!(failed.view(120,&[]).state,"connection_failed");assert_eq!(failed.view(120,&[]).error.as_deref(),Some("window_closed"));assert_eq!(failed.next_attempt_at,180);assert!(!failed.view(120,&[]).http_ready);
        state.connection_facts=Some(failed);
        let again=project_match(&state,&target,&reply,150).unwrap();assert_eq!(again.next_attempt_at,180);assert_eq!(again.attempts,1);
        reply["error"]=json!("future_device_error");assert_eq!(project_match(&state,&target,&reply,150).unwrap().view(150,&[]).error.as_deref(),Some("future_device_error"));
        reply["error"]=json!("bad error text");assert_eq!(project_match(&state,&target,&reply,150).unwrap().view(150,&[]).error.as_deref(),Some("invalid_device_error"));
    }
    #[test]
    fn retry_exhaustion_preserves_actionable_failures_and_unknown_results() {
        let mut c:Connection=serde_json::from_value(json!({"host_ssid":"Host","network_id":"home","local_revision":1,"version":8,
            "state":"waiting_for_window","error":null,"reason":0,"observed_at":100,"attempts":3,"next_attempt_at":0,"http_ready":false})).unwrap();
        for state in ["waiting_for_window","connection_failed"] {c.state=state.into();assert_eq!(c.automatic_state(),"retry_exhausted");}
        for state in ["connecting","awaiting_confirmation","connected","requested","bridge_unreachable"] {c.state=state.into();assert_eq!(c.automatic_state(),state);}
        c.state="connection_failed".into();
        for error in ["authentication_failed","security_mismatch"] {c.error=Some(error.into());assert_eq!(c.automatic_state(),"connection_failed");}
        c.error=Some("connect_timeout".into());c.attempts=2;assert_eq!(c.automatic_state(),"connection_failed");
        c.attempts=3;c.http_ready=true;assert_eq!(c.automatic_state(),"connection_failed");
    }
    #[test]
    fn usb_and_confirmation_queries_ignore_closed_legacy_windows() {
        let start=std::time::Instant::now();
        let budget=bridge_core::rendezvous::Budget::new(Duration::from_secs(30));
        let mut status=json!({"wifi_management":3,"wifi_control":{"mode":"continuous","remaining_ms":null},"power":{"remaining_s":0},"sync":{"remaining_ms":0}});
        assert!(opportunity_budget(&status,false,start,budget).remaining()>Duration::from_secs(25));
        status["wifi_control"]=json!({"mode":"window","remaining_ms":0});
        assert!(opportunity_budget(&status,true,start,budget).remaining()>Duration::from_secs(25));
        assert!(opportunity_budget(&status,false,start,budget).remaining().is_zero());
        status["wifi_control"]["remaining_ms"]=json!(3000);
        let remaining=opportunity_budget(&status,false,start-Duration::from_secs(3),budget).remaining();
        assert!(remaining.is_zero());
    }
    #[test]
    fn automatic_selection_uses_saved_bridge_targets_without_a_device_snapshot() {
        let mut state: WifiState=Default::default();
        let target:Network=serde_json::from_value(json!({"network_id":"home","host_ssid":"Current host","ssid":"Device","label":"Desk"})).unwrap();
        state.save(0,vec![target.clone()],100).unwrap();
        let host=json!({"state":"connected","networks":[{"ssid":"Current host"}]});
        assert_eq!(current_target(&state,&host).unwrap().network_id,"home");assert!(state.snapshot.is_none());
        assert!(current_target(&state,&json!({"state":"permission_required","networks":[{"ssid":"Current host"}]})).is_none());
        assert!(current_target(&state,&json!({"state":"connected","networks":[{"ssid":"Old host"}]})).is_none());
        let c=fresh_connection(&state,&target,100);state.connection_facts=Some(c);
        state.local_revision+=1;assert!(state.matching_connection("Current host").is_none());
        let mut renamed=target.clone();renamed.label="Different".into();assert_ne!(target_query(&target)["metadata_key"],target_query(&renamed)["metadata_key"]);
    }
    #[test]
    fn fresh_match_recomputes_admission_errors_without_hiding_device_failure() {
        let target:Network=serde_json::from_value(json!({"network_id":"office","host_ssid":"Office","ssid":"Office","version":7})).unwrap();
        let mut state=WifiState::default();state.save(0,vec![target.clone()],100).unwrap();
        for error in ["wifi_confirmation_pending","wifi_control_busy","wifi_sync_busy"] {
            let mut c=fresh_connection(&state,&target,100);c.version=7;c.admission_error=Some(error.into());state.connection_facts=Some(c);
            let reply=json!({"version":7,"matches":true,"active":false,"connected":false,"stage":"saved"});
            assert!(project_match(&state,&target,&reply,101).unwrap().view(150,&[]).error.is_none());
            let failed=json!({"version":7,"matches":true,"active":false,"connected":false,"stage":"failed","error":"authentication_failed"});
            assert_eq!(project_match(&state,&target,&failed,101).unwrap().view(150,&[]).error.as_deref(),Some("authentication_failed"));
        }
    }
    #[test]
    fn wifi_opportunity_never_promotes_deep_sleep_wrong_identity_or_old_capability() {
        let mac="020000000001";
        let mut status=json!({"device_mac":mac,"wifi_management":2,"power":{"mode":"light","remaining_s":60},
            "sync":{"phase":"WIFI_SYNC_ONCE"},"wifi_control":{"mode":"continuous","read_allowed":true}});
        assert!(!opportunity_window(&status,mac));
        status["wifi_management"]=json!(3);assert!(opportunity_window(&status,mac));
        status["device_mac"]=json!("020000000002");assert!(!opportunity_window(&status,mac));
        status["device_mac"]=json!(mac);
        status["wifi_control"]["mode"]=json!("window");assert!(!opportunity_window(&status,mac));
        status["wifi_control"]["remaining_ms"]=json!(1000);assert!(opportunity_window(&status,mac));
        status["wifi_control"]["read_allowed"]=json!(false);assert!(!opportunity_window(&status,mac));
    }
}
