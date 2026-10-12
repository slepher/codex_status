//! Device-page Wi-Fi operations. Configurations are cached in local runtime data.
use std::collections::{HashMap, BTreeMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use serde_json::{json, Value};
use bridge_core::platform::model::DeviceIdentity;
use bridge_core::platform::wifi::{Connection, Network, WifiState};
use sha2::{Digest, Sha256};
use crate::{AppCtx, host_wifi, platform};

#[derive(Default)]
struct Operation { cancel: AtomicBool, result: Mutex<Value> }
static OPERATIONS: OnceLock<Mutex<HashMap<String, Arc<Operation>>>> = OnceLock::new();
static INPUTS: OnceLock<Mutex<HashMap<String, BTreeMap<String, String>>>> = OnceLock::new();
fn inputs() -> &'static Mutex<HashMap<String, BTreeMap<String, String>>> { INPUTS.get_or_init(Default::default) }
static SCHEMES: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();
fn operations() -> &'static Mutex<HashMap<String, Arc<Operation>>> { OPERATIONS.get_or_init(Default::default) }

fn check_reply(reply: Value) -> Result<Value, String> {
    // Only protocol error identifiers; never expose a BLE error containing input.
    if reply["ok"] == true { Ok(reply) }
    else { Err(reply["error"].as_str().unwrap_or("wifi_failed").to_owned()) }
}
pub async fn action(ctx: &AppCtx, mac: &str, op: &str, body: Value) -> Result<Value, String> {
    if op == "host" || op == "host_permission" {
        let request = op == "host_permission";
        return tokio::task::spawn_blocking(move || host_wifi::read(request)).await.map_err(|_| "host_network_unavailable".into());
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
    match op {
        "use_current" => {
            let host=current_host(ctx).await;
            let state=svc.wifi_state(&mac);
            if body["local_revision"].as_u64()!=Some(state.local_revision){return Err("local_revision_conflict".into());}
            let target=current_target(&state,&host).ok_or("network_not_configured")?;
            if body["network_id"]!=target.network_id {return Err("host_network_changed".into());}
            let credentials: BTreeMap<String,String>=serde_json::from_value(body.get("credentials").cloned().unwrap_or(json!({}))).map_err(|_|"invalid_credentials")?;
            if credentials.keys().any(|id|id!=&target.network_id){return Err("invalid_credentials".into());}
            // Carry user input only in RAM until the read-only BLE match decides
            // whether a stable recovery transaction is actually needed.
            let key=current_input_key(&mac,target,state.local_revision);
            {
                let mut pending=inputs().lock().unwrap();
                pending.retain(|id,_|!id.starts_with(&format!("current:{mac}:"))||id==&key);
                pending.insert(key,credentials);
            }
            let mut connection=fresh_connection(&state,target,now);
            connection.state="requested".into();connection.error=None;
            connection.attempts=0;connection.next_attempt_at=0;connection.http_ready=false;
            svc.wifi_note_connection(&mac,connection).map_err(|e|e.to_string())?;
            if let Some(reply)=http_control_read(ctx,&mac,"match",target_query(target)).await {
                note_match(ctx,&mac,target,&reply);
                if reply["matches"]==true && target.credential!="replace" {
                    if reply["connected"]==true {return Ok(json!({"stage":"connected","network_id":target.network_id}));}
                    let job=svc.wifi_queue_control(&mac,&ctx.bridge_id,"activate",json!({"network_id":target.network_id,"version":reply["version"]}),now).map_err(|e|e.to_string())?;
                    opportunity(ctx,&mac).await;return serde_json::to_value(svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job)).map_err(|_|"wifi_job_encoding".into());
                }
                if let Some(job)=queue_current_recovery(ctx,&mac,&state,target,now,false) {
                    opportunity(ctx,&mac).await;return serde_json::to_value(svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job)).map_err(|_|"wifi_job_encoding".into());
                }
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
                let ssid = host_wifi::current_ssid(&host).ok_or("host_network_unknown")?;
                if body["host_ssid"].as_str() != Some(ssid) { return Err("host_network_changed".into()); }
                if !fields.is_empty() { return Err("whole_network_required".into()); }
                svc.wifi_queue_manual_sync(&mac, &ctx.bridge_id, selected, expected.unwrap(),
                    body["snapshot_id"].as_str().ok_or("snapshot_id_required")?, ssid, now)
            } else {
                svc.wifi_queue_fields_checked(&mac, &ctx.bridge_id, if op=="get" {"read"} else {"sync"}, selected, fields, expected, now)
            }.map_err(|e| e.to_string())?;
            if !job.sent && !credentials.is_empty() { inputs().lock().unwrap().insert(job.job_id.clone(), credentials); }
            opportunity(ctx,&mac).await;
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
                inputs().lock().unwrap().insert(job.job_id.clone(),credentials);
            }
            opportunity(ctx,&mac).await;
            let current=svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job.clone());
            if current.sent || current.terminal(){return serde_json::to_value(current).map_err(|_|"wifi_job_encoding".into());}
            host_wifi::require_device_bluetooth()?;
            // Intent exists before this optional immediate maintenance attempt.
            let lock = platform::delivery_lock(ctx, &mac);
            let Ok(_delivery) = ctx.device_delivery.try_lock() else { return serde_json::to_value(job).map_err(|_|"wifi_job_encoding".into()); };
            let Ok(_device_delivery) = lock.try_lock() else { return serde_json::to_value(job).map_err(|_|"wifi_job_encoding".into()); };
            if work_blocked(ctx, &mac) {
                return serde_json::to_value(job).map_err(|_|"wifi_job_encoding".into());
            }
            if let Ok(Some(mut link)) = bridge_ble::DeviceConnection::connect(&mac,&ctx.config.token,&ctx.bridge_id).await {
                recover_on_link(ctx,&mac,&mut link).await;
                link.close_bounded(bridge_core::rendezvous::CLOSE_BUDGET).await;
                drop(_device_delivery); drop(_delivery);
            }
            let result = svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job);
            return serde_json::to_value(result).map_err(|_|"wifi_job_encoding".into());
        }
        "migrate" => return Err("offline_migration_required".into()),
        "authorize" => {
            host_wifi::require_device_bluetooth()?;
            let _delivery = ctx.device_delivery.lock().await;
            let lock = platform::delivery_lock(ctx, &mac); let _device_delivery = lock.lock().await;
            let mut link = bridge_ble::DeviceConnection::connect(&mac, &ctx.config.token, &ctx.bridge_id)
                .await.map_err(|_| "ble_connection_failed")?.ok_or("device_not_advertising_press_boot")?;
            let result = if link.write_endpoint(&bridge_ble::lan_ip(),ctx.config.port,&ctx.config.token).await.is_ok() {note_link(ctx,&mac,&mut link).await}
                else {json!({"state":"waiting_authorization","error":"endpoint_failed"})};
            link.close_bounded(bridge_core::rendezvous::CLOSE_BUDGET).await;
            drop(_device_delivery); drop(_delivery);
            return Ok(result);
        }
        "input" => {
            let job = body["job_id"].as_str().ok_or("job_id_required")?;
            if !svc.wifi_state(&mac).jobs.iter().any(|j| j.job_id == job && !j.sent && !j.terminal()) { return Err("cannot_replace_sent_input".into()); }
            let credentials: BTreeMap<String, String> = serde_json::from_value(body["credentials"].clone()).map_err(|_| "invalid_credentials")?;
            inputs().lock().unwrap().entry(job.to_owned()).or_default().extend(credentials);
            svc.wifi_finish(&mac, job, "waiting_rendezvous", None, now).map_err(|_| "wifi_job_persistence")?;
            return Ok(json!({"state":"waiting_rendezvous","job_id":job}));
        }
        "cancel_job" => {
            let job = body["job_id"].as_str().ok_or("job_id_required")?;
            inputs().lock().unwrap().remove(job);
            return svc.wifi_finish(&mac, job, "cancelled", None, now).and_then(|v| Ok(serde_json::to_value(v)?)).map_err(|e| e.to_string());
        }
        _ => {}
    }
    if matches!(op,"trial"|"activate") && svc.device_get(&mac).is_some_and(|d|d["last_authenticated"]["body"]["wifi_management"]==3) {
        let job=svc.wifi_queue_control(&mac,&ctx.bridge_id,op,body,now).map_err(|e|e.to_string())?;
        opportunity(ctx,&mac).await;
        return serde_json::to_value(svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job)).map_err(|_|"wifi_job_projection".into());
    }
    if op == "status" || op == "cancel" {
        if let Some(job)=svc.wifi_state(&mac).jobs.into_iter().rev().find(|j|matches!(j.kind.as_str(),"trial"|"activate")) {
            if op=="cancel" && !job.terminal() {
                svc.wifi_finish(&mac,&job.job_id,"cancelled",None,now).map_err(|e|e.to_string())?;
            }
            opportunity(ctx,&mac).await;
            let current=svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job);
            return Ok(json!({"stage":if current.state=="completed" {if current.kind=="trial" {"trial_complete"}else{"saved"}}else if current.terminal(){"failed"}else{"connecting"},"error":current.error_code,"job_id":current.job_id,"state":current.state}));
        }
        let operation = operations().lock().unwrap().get(&mac).cloned();
        if let Some(operation) = operation {
            if op == "cancel" { operation.cancel.store(true, Ordering::Relaxed); }
            return Ok(operation.result.lock().unwrap().clone());
        }
        return Ok(json!({"stage":"idle"}));
    }
    if !matches!(op, "trial" | "activate" | "add") { return Err("invalid_operation".into()); }
    let operation = Arc::new(Operation::default());
    {
        let mut all = operations().lock().unwrap();
        if all.get(&mac).is_some_and(|old| old.result.lock().unwrap()["stage"] == "connecting") { return Err("busy".into()); }
        *operation.result.lock().unwrap() = json!({"stage":"connecting", "device_mac":mac});
        all.insert(mac.clone(), operation.clone());
    }
    let result = run(ctx, &mac, op, body).await;
    *operation.result.lock().unwrap() = match &result {
        Ok(value) => value.clone(),
        Err(error) => json!({"stage":"failed", "error":error, "device_mac":mac}),
    };
    result
}

async fn run(ctx: &AppCtx, mac: &str, op: &str, body: Value) -> Result<Value, String> {
    host_wifi::require_device_bluetooth()?;
    let _delivery = ctx.device_delivery.lock().await;
    let lock = platform::delivery_lock(ctx, mac);
    let _device_delivery = lock.lock().await;
    let mut link = bridge_ble::DeviceConnection::connect(mac, &ctx.config.token, &ctx.bridge_id)
        .await.map_err(|_| "ble_connection_failed")?.ok_or("device_not_advertising_press_boot")?;
    let result = async {
        if link.info["wifi_setup_protocol"] != 1 || link.info["wifi_management"] != 3 { return Err("update_required".into()); }
        link.write_endpoint(&bridge_ble::lan_ip(), ctx.config.port, &ctx.config.token).await.map_err(|_| "endpoint_failed")?;
        if matches!(op,"trial"|"activate") {
            let job=platform::service(ctx).wifi_queue_control(mac,&ctx.bridge_id,op,body.clone(),crate::device_now_secs(mac).max(0) as u64).map_err(|_|"wifi_job_persistence")?;
            unified_on_link(ctx,mac,&mut link,bridge_core::rendezvous::Budget::new(Duration::from_secs(25)),None).await;
            return serde_json::to_value(platform::service(ctx).wifi_state(mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job)).map_err(|_|"wifi_job_encoding".into());
        }
        if op == "add" {
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
        }
        let reply = check_reply(link.wifi_read("wifi_scheme_get").await.map_err(|_| "wifi_command_failed")?)?;
        SCHEMES.get_or_init(Default::default).lock().unwrap().insert(mac.to_owned(), reply.clone());
        Ok(reply)
    }.await;
    link.close_bounded(bridge_core::rendezvous::CLOSE_BUDGET).await;
    drop(_device_delivery); drop(_delivery);
    result
}

pub fn matching_network<'a>(scheme: &'a Value, host: &Value) -> Option<&'a str> {
    let ssid = host_wifi::current_ssid(host)?;
    let mut matches = scheme["networks"].as_array()?.iter().filter(|n| n["host_ssid"].as_str() == Some(ssid));
    let id = matches.next()?["network_id"].as_str()?;
    if matches.next().is_some() { return None; }
    Some(id)
}

pub async fn view(ctx: &AppCtx, mac: &str, refresh: bool) -> Result<Value, String> {
    let mac = DeviceIdentity::normalized_mac(mac).ok_or("invalid_device_mac")?;
    let queued = if refresh { Some(action(ctx, &mac, "get", json!({})).await?) } else { None };
    let (host, permissions) = tokio::task::spawn_blocking(|| (host_wifi::read(false), host_wifi::permissions(0))).await.map_err(|_| "host_network_unavailable")?;
    let result = operations().lock().unwrap().get(&mac)
        .map(|operation| operation.result.lock().unwrap().clone())
        .unwrap_or_else(|| json!({"stage":"idle"}));
    let scheme = SCHEMES.get_or_init(Default::default).lock().unwrap().get(&mac).cloned().unwrap_or(Value::Null);
    let _ = platform::service(ctx).wifi_expire(&mac, crate::device_now_secs(&mac).max(0) as u64);
    let state = platform::service(ctx).wifi_state(&mac);
    let target=current_target(&state,&host);
    let matched=target.map(|n|n.network_id.as_str());
    let connection=host_wifi::current_ssid(&host).and_then(|ssid|state.matching_connection(ssid));
    let result_pending=state.jobs.iter().any(|j|j.confirmation_pending());
    let auto_state=if host["state"]!="connected" {host["state"].as_str().unwrap_or("unknown")}
        else if target.is_none(){"no_mapping"} else if let Some(c)=connection {
            if result_pending&&c.automatic_state()=="retry_exhausted" {"awaiting_confirmation"} else {c.automatic_state()}
        } else {"waiting_for_window"};
    for j in state.jobs.iter().filter(|j| j.terminal()) { inputs().lock().unwrap().remove(&j.job_id); }
    Ok(json!({"device_mac":mac,"online":platform::service(ctx).device_get(&mac).map(|d|json!({"presence":d["last_authenticated"]["body"]["presence"],"transports":d["last_authenticated"]["body"]["transports"],"wifi_control":d["last_authenticated"]["body"]["wifi_control"],"observed_at":d["last_authenticated"]["observed_at"],"boot_id":d["last_authenticated"]["body"]["boot_id"],"status_seq":d["last_authenticated"]["body"]["status_seq"]})),"host":host,"permissions":permissions,"scheme":scheme,"result":result,"automatic_state":auto_state,
        "matched_network_id":matched,"matched_target":target,"connection":connection,"differences":state.differences(),"wifi":state,"queued":queued}))
}

async fn current_host(ctx:&AppCtx) -> Value {
    #[cfg(test)]
    if let Ok(host)=std::env::var("CODEX_STATUS_TEST_HOST_WIFI") {
        return serde_json::from_str(&host).expect("synthetic test host network");
    }
    // CoreLocation needs a live GUI main loop. Headless test contexts have none.
    if ctx.app_handle.get().is_none(){return json!({"state":"unavailable","networks":[]});}
    tokio::task::spawn_blocking(||host_wifi::read(false)).await.unwrap_or(Value::Null)
}
fn current_target<'a>(state: &'a WifiState, host: &Value) -> Option<&'a Network> {
    state.matching_target(host_wifi::current_ssid(host)?)
}
fn target_query(target: &Network) -> Value {
    let mut fingerprint=Sha256::new();
    for value in [&target.host_ssid,&target.ssid,&target.label] {fingerprint.update(value.as_bytes());fingerprint.update([0]);}
    fingerprint.update([u8::from(target.credential=="open")]);
    let mut query=json!({"network_id":target.network_id,"metadata_key":format!("{:x}",fingerprint.finalize()),"open":target.credential=="open"});
    if let Some(password)=&target.password {query["credential_key"]=json!(format!("{:x}",Sha256::digest(password.as_bytes())));}
    query
}
fn current_input_key(mac:&str,target:&Network,revision:u64)->String {
    format!("current:{mac}:{}:{revision}",target.network_id)
}
fn fresh_connection(state: &WifiState, target: &Network, now: u64) -> Connection {
    state.matching_connection(&target.host_ssid).cloned().unwrap_or(Connection {
        host_ssid:target.host_ssid.clone(),network_id:target.network_id.clone(),local_revision:state.local_revision,
        version:0,state:"waiting_for_window".into(),error:None,reason:0,observed_at:now,attempts:0,next_attempt_at:0,http_ready:false,bridge_error:None,actual_ssid:None,ip:None,
    })
}
fn note_match(ctx: &AppCtx, mac: &str, target: &Network, reply: &Value) -> Option<Connection> {
    if reply["ok"]!=true || reply["network_id"]!=target.network_id {
        let error=reply["error"].as_str().filter(|e|e.len()<=64&&e.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_')).unwrap_or("invalid_match_reply");
        tracing::warn!(device_mac=mac,error_code=error,"device Wi-Fi match rejected");
        return None;
    }
    let state=platform::service(ctx).wifi_state(mac);let now=crate::device_now_secs(mac).max(0) as u64;
    if state.matching_target(&target.host_ssid).is_none_or(|current|current!=target){return None;}
    let result=project_match(&state,target,reply,now)?;
    if let Some(error)=result.error.as_deref() {tracing::warn!(device_mac=mac,error_code=error,next_attempt_at=result.next_attempt_at,"device Wi-Fi failure observed");}
    platform::service(ctx).wifi_note_connection(mac,result.clone()).ok()?;Some(result)
}
fn project_match(state:&WifiState,target:&Network,reply:&Value,now:u64)->Option<Connection> {
    let mut result=fresh_connection(state,target,now);
    let version=reply["version"].as_u64()?;
    if result.version!=version {result.attempts=0;result.next_attempt_at=0;result.http_ready=false;}
    result.version=version;result.observed_at=now;
    result.actual_ssid=reply["actual_ssid"].as_str().filter(|s|s.len()<=32&&!s.contains('\0')).map(str::to_owned);
    result.ip=reply["ip"].as_str().filter(|s|s.parse::<std::net::Ipv4Addr>().is_ok()).map(str::to_owned);
    if reply["matches"]!=true {result.state="needs_sync".into();result.error=None;result.http_ready=false;}
    else if reply["connected"]==true {result.state="connected".into();result.error=None;result.http_ready=false;}
    else if reply["stage"]=="failed" {
        let error=reply["error"].as_str().unwrap_or("connect_timeout");
        let error=if !error.is_empty() && error.len()<=64 && error.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_') {error} else {"invalid_device_error"};
        let new_failure=result.state!="connection_failed" || result.error.as_deref()!=Some(error);
        result.state="connection_failed".into();result.error=Some(error.into());result.reason=reply["reason"].as_u64().and_then(|n|u16::try_from(n).ok()).unwrap_or(0);result.http_ready=false;
        if new_failure {result.next_attempt_at=result.next_attempt_at.max(now+60);}
    } else if matches!(reply["stage"].as_str(),Some("connecting"|"accepted")){result.state="connecting".into();result.http_ready=false;}
    else if result.state!="connection_failed" {result.state="waiting_for_window".into();result.http_ready=false;}
    Some(result)
}
fn queue_current_recovery(ctx:&AppCtx,mac:&str,state:&WifiState,target:&Network,now:u64,_physical_window:bool)->Option<bridge_core::platform::wifi::Job> {
    let svc=platform::service(ctx);
    let job=svc.wifi_queue_fields_checked(mac,&ctx.bridge_id,"recovery_save",vec![target.network_id.clone()],BTreeMap::new(),Some(state.local_revision),now).ok()?;
    if !job.sent {
        let mut pending=inputs().lock().unwrap();
        if let Some(credentials)=pending.remove(&current_input_key(mac,target,state.local_revision)) {
            pending.entry(job.job_id.clone()).or_default().extend(credentials);
        }
        let missing=target.credential=="replace"&&target.password.is_none()&&!pending.get(&job.job_id).is_some_and(|p|p.contains_key(&target.network_id));
        drop(pending);
        if missing {let _=svc.wifi_finish(mac,&job.job_id,"waiting_input",Some("credentials_required"),now);}

    }
    Some(job)
}
/// A read-only single-row check before PowerPlan freezes this rendezvous's work.
pub async fn prepare_connection(ctx:&AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,physical_window:bool) {
    if link.info["wc"]!=1{return;}
    let host=current_host(ctx).await;
    let state=platform::service(ctx).wifi_state(mac);
    let Some(target)=current_target(&state,&host) else{let _=platform::service(ctx).wifi_clear_connection(mac);return;};
    if let Ok(reply)=link.command("wifi_match",target_query(target)).await {
        if note_match(ctx,mac,target,&reply).is_some()&&(reply["matches"]!=true||target.credential=="replace") {
            queue_current_recovery(ctx,mac,&state,target,crate::device_now_secs(mac).max(0) as u64,physical_window);
        }
    }
}
pub fn http_ready(ctx:&AppCtx,mac:&str) {
    let state=platform::service(ctx).wifi_state(mac);
    if let Some(mut c)=state.connection.filter(|c|c.local_revision==state.local_revision && c.state!="requested") {
        // Count consecutive unconfirmed attempts, rather than every reconnect
        // since this configuration was saved. Only authenticated HTTP resets it.
        c.confirm_http();let _=platform::service(ctx).wifi_note_connection(mac,c);
    }
}
/// Preserve device failure and carrier outcome in the same MCP/UI projection.
pub fn network_failed(ctx:&AppCtx,mac:&str,error:&str) {
    let state=platform::service(ctx).wifi_state(mac);
    if let Some(mut c)=state.connection.filter(|c|c.local_revision==state.local_revision) {
        if matches!(error,"network_not_found"|"authentication_failed"|"security_mismatch"|
            "signal_weak"|"association_failed"|"handshake_timeout"|"dhcp_timeout"|"connect_timeout") {
            c.state="connection_failed".into();c.error=Some(error.into());c.http_ready=false;
        } else if matches!(error,"not_sent"|"result_unknown"|"http_proof_budget_exhausted") {
            c.fail_http(error);
        } else { return; }
        let _=platform::service(ctx).wifi_note_connection(mac,c);
    }
}
pub fn http_failed(ctx:&AppCtx,mac:&str,error:&str) {
    let state=platform::service(ctx).wifi_state(mac);
    if let Some(mut c)=state.connection.filter(|c|c.local_revision==state.local_revision) {
        if c.state=="connected" || matches!(error,"http_identity_mismatch"|"unauthorized") {
            c.fail_http(match error {"http_identity_mismatch"=>"http_identity_mismatch","unauthorized"=>"unauthorized",_=>"http_unavailable"});
            let _=platform::service(ctx).wifi_note_connection(mac,c);
        }
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
    tokio::time::timeout(budget.remaining(),async {
        let contact_host=current_host(ctx).await;
        if let Err(error)=host_wifi::require_same_network(&contact_host,&contact_host) {
            return if selection_only { Ok(()) } else { Err(error) };
        }
        if link.info["wifi_management"] != 3 {return Err("update_required".into());}
        let svc=platform::service(ctx);
        if svc.ota_job(mac).is_some_and(|j|j.blocks_network_repair()) ||
            svc.job(mac).is_some_and(|j|matches!(j["state"].as_str(),Some("waiting"|"sending"|"unknown"))) {return Ok(());}
        // At most one confirmed save followed by one activation in this budget.
        for _ in 0..2 {
        let host=current_host(ctx).await;
        host_wifi::require_same_network(&contact_host,&host)?;
        let state=platform::service(ctx).wifi_state(mac);
        let Some(target)=current_target(&state,&host).cloned() else{
            return if selection_only { Ok(()) } else { Err("network_mapping_required".into()) };
        };
        let now=crate::device_now_secs(mac).max(0) as u64;
        if link.info["wc"]!=1 {let mut c=fresh_connection(&state,&target,now);c.state="firmware_upgrade_required".into();let _=platform::service(ctx).wifi_note_connection(mac,c);return Ok(());}
        let Ok(reply)=link.command("wifi_match",target_query(&target)).await else{return Ok(());};
        let Some(mut connection)=note_match(ctx,mac,&target,&reply) else{return Ok(());};
        // A new replace intent always needs a real input/confirmed write. Matching
        // public metadata cannot prove a password replacement was applied.
        if reply["matches"]!=true || target.credential=="replace" {
            // Physical wake selects only a previously saved network. No password
            // write or automatic provisioning; the device owns fallback.
            if selection_only { return Ok(()); }
            if svc.ota_job(mac).is_some_and(|j|j.state=="awaiting_confirmation") {return Err("ota_confirmation_network_mismatch".into());}
            let svc=platform::service(ctx);
            let job=queue_current_recovery(ctx,mac,&state,&target,now,true);
            if let Some(job)=job {
                let missing=target.credential=="replace"&&target.password.is_none()&&!inputs().lock().unwrap().get(&job.job_id).is_some_and(|p|p.contains_key(&target.network_id));
                if missing {let _=svc.wifi_finish(mac,&job.job_id,"waiting_input",Some("credentials_required"),now);return Err("credentials_required".into());}
                else {
                    recover_on_link_budget(ctx,mac,link,budget).await;
                    if !ctx.device_status_cache.lock().unwrap().get(mac).is_some_and(|s|s.yielded) && svc.wifi_state(mac).jobs.iter().any(|j|j.job_id==job.job_id&&j.state=="completed") {continue;}
                }
            }
            return Ok(());
        }
        if reply["connected"]==true || matches!(reply["stage"].as_str(),Some("connecting"|"accepted")){return Ok(());}
        if let Some(error)=connection.error.as_deref() {
            if !matches!(error,"window_closed"|"handshake_timeout"|"network_not_found"|"signal_weak"|"association_failed"|"dhcp_timeout"|"connect_timeout"|"disconnected"|"cancelled") {return Err(error.to_owned());}
        }
        if connection.attempts>=3 {return Err("wifi_retry_exhausted".into());}
        if now<connection.next_attempt_at {return Err("wifi_retry_backoff".into());}
        let fresh=current_host(ctx).await;
        host_wifi::require_same_network(&contact_host,&fresh)?;
        let Ok(owner)=link.command("wifi_owner",json!({})).await else{return Ok(());};
        if owner["owner_id"].as_str().is_some_and(|id|!id.is_empty()&&id!=ctx.bridge_id){connection.state="occupied".into();let _=platform::service(ctx).wifi_note_connection(mac,connection);return Ok(());}
        let job=match svc.wifi_queue_control(mac,&ctx.bridge_id,"activate",json!({"network_id":target.network_id,"version":reply["version"]}),now) {
            Ok(job)=>job,
            Err(error)=>{let code=error.to_string();connection.error=Some(code.clone());
                svc.wifi_note_connection(mac,connection).map_err(|_|"persistence_failed".to_owned())?;return Err(code);}
        };
        let was_sent=job.sent;
        unified_on_link(ctx,mac,link,budget,Some(&job.job_id)).await;
        let current=svc.wifi_state(mac).jobs.into_iter().find(|j|j.job_id==job.job_id).ok_or("wifi_job_not_found")?;
        if current.sent && current.state=="awaiting_confirmation" && current.error_code.as_deref()==Some("result_unknown") {
            return Err("result_unknown".into());
        }
        if !was_sent {
            if !current.sent {
                let code=current.error_code.unwrap_or("wifi_not_dispatched".into());connection.error=Some(code.clone());
                svc.wifi_note_connection(mac,connection).map_err(|_|"persistence_failed".to_owned())?;return Err(code);
            }
            connection.attempts+=1;connection.next_attempt_at=now+60;connection.state="connecting".into();connection.http_ready=false;connection.error=None;
            svc.wifi_note_connection(mac,connection).map_err(|_|"persistence_failed".to_owned())?;
        }
        return Ok(());
        }
        Ok(())
    }).await.map_err(|_| "wifi_window_timeout".to_owned())?
}

// Current authenticated status; no transient BLE permit is created.
pub async fn note_link(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection) -> Value {
    if link.info["wifi_management"] != 3 {
        let svc = platform::service(ctx);
        for job in svc.wifi_state(mac).jobs.iter().filter(|j| !j.terminal() && !j.sent) {
            let _ = svc.wifi_finish(mac, &job.job_id, "failed", Some("update_required"), crate::device_now_secs(mac).max(0) as u64);
        }
        return json!({"state":"failed","error":"update_required"});
    }
    match link.wifi_read("wifi_status").await {
        Ok(status) => {
            if platform::service(ctx).note_authenticated_status_via_at(mac, &status, "ble", crate::device_now_secs(mac).max(0) as u64).is_err() {
                return json!({"state":"failed","error":"authenticated_status_invalid"});
            }
            json!({"ok":true,"mode":"authenticated_control","wifi_control":status["wifi_control"]})
        }
        Err(_) => json!({"state":"waiting_authorization","error":"status_unavailable"}),
    }
}

pub async fn opportunity(ctx: &AppCtx, mac: &str) {
    let lock = platform::delivery_lock(ctx, mac);
    let Ok(_guard) = lock.try_lock() else { return; };
    opportunity_locked(ctx, mac).await;
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
    let done:Vec<_>=svc.wifi_state(mac).jobs.into_iter().filter(|j|j.receipt_ack_pending).collect();
    if done.is_empty(){return;}let Some(link)=platform::device_link_for_mac(ctx,mac) else{return;};
    let Ok(worker)=platform::delivery_lock(ctx,mac).spawn_blocking(move|| {
        let budget=bridge_core::rendezvous::Budget::new(Duration::from_secs(4));
        let Ok(status)=bridge_core::device_client::status(&link.ip,&link.token,Duration::from_secs(2)) else{return;};
        if status["wifi_management"]!=3 || status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()!=Some(link.mac.as_str()){return;}
        let nonce=status["session_nonce"].as_str().unwrap_or("");
        for job in done {let remaining=budget.remaining();if remaining.is_zero(){break;}
            if !receipt_context_matches(&job,&link.bridge_id,&status) {continue;}
            if bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,"result_ack",nonce,json!({"query_id":job.request_id}),remaining)
                .is_ok_and(|r|r["ok"]==true && r["outcome"]=="retired") {let _=svc.wifi_receipt_acked(&link.mac,&job.job_id,&link.bridge_id,status["wifi_control"]["namespace"].as_str().unwrap_or(""));}
        }
    }) else{return;};
    let _=worker.await;
}

pub async fn opportunity_admitted(ctx: &AppCtx, mac: &str, job_id: &str, budget: bridge_core::rendezvous::Budget) {
    let svc = platform::service(ctx).clone(); let now = crate::device_now_secs(mac).max(0) as u64;
    let Some(job) = svc.wifi_state(mac).jobs.into_iter().find(|j| j.job_id == job_id && bridge_core::rendezvous::admit_wifi(j, now)) else { return; };
    if work_blocked(ctx, mac) { return; }
    let Some(link) = platform::device_link_for_mac(ctx, mac) else { return; };
    let credentials = inputs().lock().unwrap().get(&job.job_id).cloned().unwrap_or_default();
    let cleanup_id = job.job_id.clone();
    let job_id = job.job_id.clone();
    let Ok(task) = platform::delivery_lock(ctx,mac).spawn_blocking(move || {
        let sample_start = std::time::Instant::now();
        let status = bridge_core::device_client::status(&link.ip, &link.token, budget.remaining().min(Duration::from_secs(2)));
        let status = match status { Ok(status) => status, Err(error) => {
            if svc.wifi_begin(&link.mac,&job_id,now).ok().flatten().is_some() {
                let (state, code) = bridge_core::rendezvous::wifi_error(&error);
                let state = if job.sent { "awaiting_confirmation" } else { state };
                let code = if code == "transport_timeout" || code == "transport_unavailable" { "http_status_unavailable" } else { code };
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
        if !job.sent && !opportunity_window(&status, &link.mac) { return; }
        let nonce=status["session_nonce"].as_str().unwrap_or("");
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
                    let Ok(reply)=bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,"match",nonce,target_query(&probe),timeout) else{break;};
                    if reply["ok"]!=true || reply["network_id"]!=target.network_id || !(if target.deleted {reply["version"]==0}else{reply["matches"]==true}){break;}
                    let Some(version)=reply["version"].as_u64() else{break;};versions.insert(target.network_id.clone(),version);
                }
                if versions.len()==job.selected.len() && !budget.remaining().is_zero() &&
                    bridge_core::device_client::status(&link.ip,&link.token,budget.remaining()).is_ok_and(|fresh|fresh["boot_id"]==status["boot_id"] && fresh["wifi_storage"]["revision"]==revision && fresh["device_mac"]==status["device_mac"]) {
                    let _=svc.wifi_confirm_current(&link.mac,&job_id,revision,&versions,now);return;
                }
            }
            // Authenticated revision already proves this frozen cache is stale.
            // Stop before mark_sent; never turn a lost refusal into uncertainty.
            let _=svc.wifi_finish(&link.mac,&job_id,"conflict",Some("revision_conflict"),now);return;
        }
        let _ = svc.wifi_attempt(&link.mac,&job_id,&link.bridge_id,status["wifi_control"]["namespace"].as_str().unwrap_or(""),writable,&credentials,now,|op, body| {
            let timeout = budget.remaining();
            if timeout.is_zero() {return Err(bridge_core::rendezvous::NotDispatched.into());}
            if op == "snapshot" {
                let mut snapshot = bridge_core::device_client::wifi_snapshot(&link.ip,&link.token,&link.mac,&link.bridge_id,nonce,timeout)?;
                snapshot.observed_at = now; Ok(serde_json::to_value(snapshot)?)
            } else { bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,op,nonce,body,timeout) }
        });
        {
            for done in svc.wifi_state(&link.mac).jobs.into_iter().filter(|j|j.receipt_ack_pending) {
                if !receipt_context_matches(&done,&link.bridge_id,&status) {continue;}
                let remaining=budget.remaining();if remaining.is_zero(){break;}
                if bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,"result_ack",nonce,json!({"query_id":done.request_id}),remaining)
                    .is_ok_and(|r|r["ok"]==true && r["outcome"]=="retired") {let _=svc.wifi_receipt_acked(&link.mac,&done.job_id,&link.bridge_id,status["wifi_control"]["namespace"].as_str().unwrap_or(""));}
            }
        }
    }) else{return;};
    let _ = task.await;
    if platform::service(ctx).wifi_state(mac).jobs.iter().find(|j| j.job_id == cleanup_id).is_some_and(|j| j.terminal()) {
        inputs().lock().unwrap().remove(&cleanup_id);
    }
}

async fn http_control_read(ctx:&AppCtx,mac:&str,op:&str,body:Value)->Option<Value> {
    let link=platform::device_link_for_mac(ctx,mac)?;let svc=platform::service(ctx).clone();let op=op.to_owned();let now=crate::device_now_secs(mac).max(0) as u64;
    tokio::task::spawn_blocking(move|| {
        let status=bridge_core::device_client::status(&link.ip,&link.token,Duration::from_secs(2)).ok()?;
        if status["wifi_management"]!=3 || status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()!=Some(link.mac.as_str()){return None;}
        svc.note_authenticated_status_at(&link.mac,&status,now).ok()?;
        bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,&op,status["session_nonce"].as_str().unwrap_or(""),body,Duration::from_secs(3)).ok()
    }).await.ok().flatten()
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
            tokio::time::timeout(self.budget.remaining(), self.link.wifi_snapshot()).await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("wifi_snapshot_timeout")))
        } else {
            let id = body["request_id"].as_str().map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
            let wire = if op == "apply" { "wifi_save".to_owned() } else { format!("wifi_{op}") };
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
            let host = current_host(ctx).await;
            if host_wifi::current_ssid(&host) != Some(expected) {
                let _ = svc.wifi_finish(mac, &job.job_id, "failed", Some("host_network_changed"), now);
                return;
            }
        }
    }
    let status = if matches!(job.kind.as_str(),"read"|"read_import") {Value::Null} else {
        match tokio::time::timeout(budget.remaining(),link.wifi_read("wifi_status")).await {
            Ok(Ok(status)) => status,
            _ => {
                let _=svc.wifi_finish(mac,&job.job_id,if job.sent {"awaiting_confirmation"} else {"waiting_rendezvous"},Some("status_unavailable"),now);
                return;
            }
        }
    };
    if !status.is_null() && svc.note_authenticated_status_via_at(mac,&status,"ble",now).is_err() {return;}
    if !prepare_transaction(&svc,&job,&ctx.bridge_id,&status,now) {return;}
    let writable = !job.sent && link.command_with_id_budget("wifi_owner", json!({}),
        &uuid::Uuid::new_v4().simple().to_string(), budget.remaining()).await.ok()
        .is_some_and(|r|r["owner_id"]==ctx.bridge_id);
    let credentials=inputs().lock().unwrap().get(&job.job_id).cloned().unwrap_or_default();
    let Ok(_permit)=platform::delivery_lock(ctx,mac).active_permit() else{return;};
    let namespace=status["wifi_control"]["namespace"].as_str().unwrap_or("");
    let _=svc.wifi_attempt_async(mac,&job.job_id,&ctx.bridge_id,namespace,writable,&credentials,now,
        &mut BleTransport {link,budget}).await;
    acknowledge_receipts_on_link_with_status(ctx,mac,link,budget,&status).await;
}

/// Retire already durable results even when no business job remains runnable.
pub async fn acknowledge_receipts_on_link_budget(ctx:&AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,budget:bridge_core::rendezvous::Budget) {
    let svc=platform::service(ctx);
    if budget.remaining().is_zero() || link.info["wifi_management"]!=3 ||
        !svc.wifi_state(mac).jobs.iter().any(|j|j.receipt_ack_pending) {return;}
    let Ok(Ok(status))=tokio::time::timeout(budget.remaining(),link.wifi_read("wifi_status")).await else{return;};
    if svc.note_authenticated_status_via_at(mac,&status,"ble",crate::device_now_secs(mac).max(0) as u64).is_err() {return;}
    acknowledge_receipts_on_link_with_status(ctx,mac,link,budget,&status).await;
}

async fn acknowledge_receipts_on_link_with_status(ctx:&AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,
    budget:bridge_core::rendezvous::Budget,status:&Value) {
    for done in platform::service(ctx).wifi_state(mac).jobs.into_iter().filter(|j|j.receipt_ack_pending) {
        if !receipt_context_matches(&done,&ctx.bridge_id,status) {continue;}
        if budget.remaining().is_zero(){break;}
        if link.command_with_id_budget("wifi_result_ack",json!({"query_id":done.request_id}), &uuid::Uuid::new_v4().simple().to_string(), budget.remaining()).await.is_ok_and(|r|r["ok"]==true && r["outcome"]=="retired") {let _=platform::service(ctx).wifi_receipt_acked(mac,&done.job_id,&ctx.bridge_id,status["wifi_control"]["namespace"].as_str().unwrap_or(""));}
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
        .find(|j| matches!(j.kind.as_str(), "sync" | "recovery_save") && j.runnable(now)) {
        unified_on_link(ctx, mac, link, budget, Some(&job.job_id)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut initial=fresh_connection(&state,&target,100);initial.version=1;initial.state="connecting".into();initial.attempts=1;state.connection=Some(initial);
        let mut reply=json!({"ok":true,"network_id":"home","version":1,"matches":true,"connected":false,"stage":"failed","error":"window_closed"});
        let failed=project_match(&state,&target,&reply,120).unwrap();
        assert_eq!(failed.state,"connection_failed");assert_eq!(failed.error.as_deref(),Some("window_closed"));assert_eq!(failed.next_attempt_at,180);assert!(!failed.http_ready);
        state.connection=Some(failed);
        let again=project_match(&state,&target,&reply,150).unwrap();assert_eq!(again.next_attempt_at,180);assert_eq!(again.attempts,1);
        reply["error"]=json!("future_device_error");assert_eq!(project_match(&state,&target,&reply,150).unwrap().error.as_deref(),Some("future_device_error"));
        reply["error"]=json!("bad error text");assert_eq!(project_match(&state,&target,&reply,150).unwrap().error.as_deref(),Some("invalid_device_error"));
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
        let mut c=fresh_connection(&state,&target,100);c.state="connection_failed".into();state.connection=Some(c);
        state.local_revision+=1;assert!(state.matching_connection("Current host").is_none());
        let mut renamed=target.clone();renamed.label="Different".into();assert_ne!(target_query(&target)["metadata_key"],target_query(&renamed)["metadata_key"]);
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
    #[test]
    fn automatic_selection_requires_one_exact_mapping_and_known_host() {
        let scheme = json!({"networks":[{"network_id":"home", "host_ssid":"Host 5G", "ssid":"Device 2G"}]});
        let host = json!({"state":"connected", "networks":[{"ssid":"Host 5G"}]});
        assert_eq!(matching_network(&scheme, &host), Some("home"));
        assert_eq!(matching_network(&scheme, &json!({"state":"permission_denied"})), None);
        assert_eq!(matching_network(&scheme, &json!({"state":"connected", "networks":[{"ssid":"Other"}]})), None);
        let duplicate = json!({"networks":[scheme["networks"][0].clone(), scheme["networks"][0].clone()]});
        assert_eq!(matching_network(&duplicate, &host), None);
    }
}
