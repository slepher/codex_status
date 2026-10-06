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
#[derive(Clone)]
struct Permit { id: String, write_until: std::time::Instant }
static PERMITS: OnceLock<Mutex<HashMap<String, Permit>>> = OnceLock::new();
static INPUTS: OnceLock<Mutex<HashMap<String, BTreeMap<String, String>>>> = OnceLock::new();
fn permits() -> &'static Mutex<HashMap<String, Permit>> { PERMITS.get_or_init(Default::default) }
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
    if matches!(op, "permissions" | "permission_location" | "permission_bluetooth") {
        let request = match op { "permission_location" => 1, "permission_bluetooth" => 2, _ => 0 };
        return tokio::task::spawn_blocking(move || host_wifi::permissions(request)).await.map_err(|_| "permissions_unavailable".into());
    }
    if matches!(op, "settings_location" | "settings_bluetooth") {
        let section = i32::from(op == "settings_bluetooth");
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
                drop(_device_delivery); drop(_delivery);
                link.close_bounded(bridge_core::rendezvous::CLOSE_BUDGET).await;
            }
            let result = svc.wifi_state(&mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap_or(job);
            return serde_json::to_value(result).map_err(|_|"wifi_job_encoding".into());
        }
        "migrate" => {
            host_wifi::require_device_bluetooth()?;
            let _delivery = ctx.device_delivery.lock().await;
            let lock = platform::delivery_lock(ctx, &mac);
            let _device_delivery = lock.lock().await;
            let mut link = bridge_ble::DeviceConnection::connect(&mac, &ctx.config.token, &ctx.bridge_id)
                .await.map_err(|_| "ble_connection_failed")?.ok_or("device_not_advertising_press_boot")?;
            let result = match link.command("wifi_migrate", json!({})).await {
                // Keep non-secret storage diagnostics on an explicit migration.
                Ok(reply) => Ok(reply),
                Err(_) => Err("migration_result_unknown_retry_same_device".into()),
            };
            drop(_device_delivery); drop(_delivery);
            link.close_bounded(bridge_core::rendezvous::CLOSE_BUDGET).await;
            return result;
        }
        "authorize" => {
            host_wifi::require_device_bluetooth()?;
            let _delivery = ctx.device_delivery.lock().await;
            let lock = platform::delivery_lock(ctx, &mac); let _device_delivery = lock.lock().await;
            let mut link = bridge_ble::DeviceConnection::connect(&mac, &ctx.config.token, &ctx.bridge_id)
                .await.map_err(|_| "ble_connection_failed")?.ok_or("device_not_advertising_press_boot")?;
            let result = if link.write_endpoint(&bridge_ble::lan_ip(),ctx.config.port,&ctx.config.token).await.is_ok() {note_link(ctx,&mac,&mut link).await}
                else {json!({"state":"waiting_authorization","error":"endpoint_failed"})};
            drop(_device_delivery); drop(_delivery);
            link.close_bounded(bridge_core::rendezvous::CLOSE_BUDGET).await;
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
                if !job.sent {let _=svc.wifi_finish(&mac,&job.job_id,"cancelled",None,now);}
                else {let _=http_control_read(ctx,&mac,"setup_cancel",json!({"query_id":job.request_id})).await;}
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
    let result = run(ctx, &mac, op, body, &operation).await;
    *operation.result.lock().unwrap() = match &result {
        Ok(value) => value.clone(),
        Err(error) => json!({"stage":"failed", "error":error, "device_mac":mac}),
    };
    result
}

async fn run(ctx: &AppCtx, mac: &str, op: &str, body: Value, operation: &Operation) -> Result<Value, String> {
    host_wifi::require_device_bluetooth()?;
    let _delivery = ctx.device_delivery.lock().await;
    let lock = platform::delivery_lock(ctx, mac);
    let _device_delivery = lock.lock().await;
    let mut link = bridge_ble::DeviceConnection::connect(mac, &ctx.config.token, &ctx.bridge_id)
        .await.map_err(|_| "ble_connection_failed")?.ok_or("device_not_advertising_press_boot")?;
    let result = async {
        if link.info["wifi_setup_protocol"] != 1 { return Err("firmware_requires_upgrade_use_usb_or_ap".into()); }
        link.write_endpoint(&bridge_ble::lan_ip(), ctx.config.port, &ctx.config.token).await.map_err(|_| "endpoint_failed")?;
        if matches!(op,"trial"|"activate") && link.info["wifi_management"]==3 {
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
        let wire_op = match op { "get" | "add" => "wifi_scheme_get", "trial" => "wifi_trial", "activate" => "wifi_activate", _ => unreachable!() };
        if matches!(op, "trial" | "activate") && !matches!(link.info["wifi_management"].as_u64(),Some(2|3)) { return Err("unsupported".into()); }
        if op == "activate" {
            let status = link.command("wifi_owner", json!({})).await.map_err(|_| "owner_check_failed")?;
            if status["owner_id"].as_str().is_some_and(|id| id != ctx.bridge_id) { return Err("occupied".into()); }
            link.write_endpoint(&bridge_ble::lan_ip(), ctx.config.port, &ctx.config.token).await.map_err(|_| "endpoint_failed")?;
        }
        let mut reply = if op == "get" || op == "add" {
            link.wifi_read(wire_op).await.map_err(|_| "wifi_command_failed")?
        } else { check_reply(link.command(wire_op, body).await.map_err(|_| "wifi_command_failed")?)? };
        let query = reply["query_id"].as_str().unwrap_or("").to_owned();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(28);
        while reply["stage"] == "connecting" {
            *operation.result.lock().unwrap() = reply.clone();
            tokio::time::sleep(Duration::from_millis(300)).await;
            let cancel = operation.cancel.load(Ordering::Relaxed) || tokio::time::Instant::now() >= deadline;
            let query_op = if cancel { "wifi_setup_cancel" } else { "wifi_setup_status" };
            reply = link.command(query_op, json!({"query_id":query})).await.map_err(|_| "wifi_result_unavailable")?;
            if cancel { return check_reply(reply); }
        }
        let reply = check_reply(reply)?;
        if op == "get" || op == "add" {
            SCHEMES.get_or_init(Default::default).lock().unwrap().insert(mac.to_owned(), reply.clone());
        }
        if reply["persisted"] == true {
            if let Some(ip) = reply["ip"].as_str().filter(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok()) {
                crate::devices(ctx).observe(mac, Some(ip), None, "ble", crate::device_now_secs(mac));
                link.write_endpoint(&bridge_ble::lan_ip(), ctx.config.port, &ctx.config.token).await.map_err(|_| "endpoint_failed")?;
                // Revalidate HTTP separately; association alone is not reachability.
                let _ = platform::revalidate_registered_device_endpoint(ctx, mac, ip).await;
                // An unchanged known IP is still a real HTTP reachability check.
                let endpoint = ip.to_owned();
                let token = ctx.config.token.clone();
                let expected = mac.to_owned();
                let reachable = tokio::task::spawn_blocking(move || {
                    bridge_core::device_client::status(&endpoint, &token, Duration::from_secs(3)).ok()
                        .is_some_and(|status| DeviceIdentity::normalized_mac(status["device_mac"].as_str().unwrap_or("")) == Some(expected))
                }).await.unwrap_or(false);
                let mut result = reply;
                result["http_reachable"] = json!(reachable);
                return Ok(result);
            }
        }
        Ok(reply)
    }.await;
    drop(_device_delivery); drop(_delivery);
    link.close_bounded(bridge_core::rendezvous::CLOSE_BUDGET).await;
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
    let result_pending=state.jobs.iter().any(|j|j.sent&&!j.terminal());
    let auto_state=if host["state"]!="connected" {host["state"].as_str().unwrap_or("unknown")}
        else if target.is_none(){"no_mapping"} else if let Some(c)=connection {
            if result_pending&&c.automatic_state()=="retry_exhausted" {"awaiting_confirmation"} else {c.automatic_state()}
        } else {"waiting_for_window"};
    for j in state.jobs.iter().filter(|j| j.terminal()) { inputs().lock().unwrap().remove(&j.job_id); }
    Ok(json!({"device_mac":mac,"online":platform::service(ctx).device_get(&mac).map(|d|json!({"presence":d["last_authenticated"]["body"]["presence"],"transports":d["last_authenticated"]["body"]["transports"],"wifi_control":d["last_authenticated"]["body"]["wifi_control"],"observed_at":d["last_authenticated"]["observed_at"],"boot_id":d["last_authenticated"]["body"]["boot_id"],"status_seq":d["last_authenticated"]["body"]["status_seq"]})),"host":host,"permissions":permissions,"scheme":scheme,"result":result,"automatic_state":auto_state,
        "matched_network_id":matched,"matched_target":target,"connection":connection,"differences":state.differences(),"wifi":state,"queued":queued}))
}

async fn current_host(ctx:&AppCtx) -> Value {
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
        version:0,state:"waiting_for_window".into(),error:None,reason:0,observed_at:now,attempts:0,next_attempt_at:0,http_ready:false,actual_ssid:None,ip:None,
    })
}
fn note_match(ctx: &AppCtx, mac: &str, target: &Network, reply: &Value) -> Option<Connection> {
    if reply["ok"]!=true || reply["network_id"]!=target.network_id {return None;}
    let state=platform::service(ctx).wifi_state(mac);let now=crate::device_now_secs(mac).max(0) as u64;
    if state.matching_target(&target.host_ssid).is_none_or(|current|current!=target){return None;}
    let mut result=fresh_connection(&state,target,now);
    let version=reply["version"].as_u64()?;
    if result.version!=version {result.attempts=0;result.next_attempt_at=0;result.http_ready=false;}
    result.version=version;result.observed_at=now;
    result.actual_ssid=reply["actual_ssid"].as_str().filter(|s|s.len()<=32&&!s.contains('\0')).map(str::to_owned);
    result.ip=reply["ip"].as_str().filter(|s|s.parse::<std::net::Ipv4Addr>().is_ok()).map(str::to_owned);
    if reply["matches"]!=true {result.state="needs_sync".into();result.error=None;result.http_ready=false;}
    else if reply["connected"]==true {result.state="connected".into();result.error=None;result.http_ready=false;}
    else if reply["stage"]=="failed" {
        let error=reply["error"].as_str().unwrap_or("connect_timeout");
        if !matches!(error,"authentication_failed"|"handshake_timeout"|"network_not_found"|"security_mismatch"|"signal_weak"|"association_failed"|"dhcp_timeout"|"connect_timeout"|"occupied"|"disconnected"|"storage"|"cancelled"){return None;}
        let new_failure=result.state!="connection_failed" || result.error.as_deref()!=Some(error);
        result.state="connection_failed".into();result.error=Some(error.into());result.reason=reply["reason"].as_u64().and_then(|n|u16::try_from(n).ok()).unwrap_or(0);result.http_ready=false;
        if new_failure {result.next_attempt_at=result.next_attempt_at.max(now+60);}
    } else if matches!(reply["stage"].as_str(),Some("connecting"|"accepted")){result.state="connecting".into();result.http_ready=false;}
    else if result.state!="connection_failed" {result.state="waiting_for_window".into();result.http_ready=false;}
    platform::service(ctx).wifi_note_connection(mac,result.clone()).ok()?;Some(result)
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
/// Read one matched row's connection result; never fetch the full scheme.
pub async fn refresh_connection(ctx: &AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection) {
    let state=platform::service(ctx).wifi_state(mac);
    let Some(connection)=state.connection.as_ref() else{return;};
    let Some(target)=state.matching_target(&connection.host_ssid).filter(|_|connection.local_revision==state.local_revision) else{return;};
    if let Ok(reply)=link.command("wifi_match",target_query(target)).await {note_match(ctx,mac,target,&reply);}
}
pub fn http_ready(ctx:&AppCtx,mac:&str) {
    let state=platform::service(ctx).wifi_state(mac);
    if let Some(mut c)=state.connection.filter(|c|c.local_revision==state.local_revision && c.state!="requested") {
        // Count consecutive unconfirmed attempts, rather than every reconnect
        // since this configuration was saved. Only authenticated HTTP resets it.
        c.confirm_http();let _=platform::service(ctx).wifi_note_connection(mac,c);
    }
}
pub fn http_failed(ctx:&AppCtx,mac:&str,error:&str) {
    let state=platform::service(ctx).wifi_state(mac);
    if let Some(mut c)=state.connection.filter(|c|c.local_revision==state.local_revision) {
        if c.state=="connected" || matches!(error,"http_identity_mismatch"|"unauthorized") {
            c.state="bridge_unreachable".into();c.http_ready=false;
            c.error=Some(match error {"http_identity_mismatch"=>"http_identity_mismatch","unauthorized"=>"unauthorized",_=>"http_unavailable"}.into());
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
    let unified=status["wifi_management"]==3;
    let legal_remaining = if unified {0} else {status["power"]["remaining_s"].as_u64().unwrap_or(0)};
    let budget = if sent || (unified && status["wifi_control"]["mode"]=="continuous") {
        budget
    } else if unified {
        bridge_core::rendezvous::Budget::at(sample_start,Duration::from_millis(status["wifi_control"]["remaining_ms"].as_u64().unwrap_or(0).saturating_sub(1000))).capped(budget.remaining())
    } else if legal_remaining > 0 {
        bridge_core::rendezvous::Budget::at(sample_start,Duration::from_secs(legal_remaining.saturating_sub(1))).capped(budget.remaining())
    } else if let Some(remaining) = status["sync"]["remaining_ms"].as_u64() {
        bridge_core::rendezvous::Budget::at(sample_start,Duration::from_millis(remaining.saturating_sub(1000))).capped(budget.remaining())
    } else { budget };
    budget.capped(bridge_core::rendezvous::WIFI_WORK_BUDGET)

}

fn opportunity_window(status: &Value, mac: &str) -> bool {
    DeviceIdentity::normalized_mac(status["device_mac"].as_str().unwrap_or("")) == Some(mac.to_owned()) &&
        ((status["wifi_management"] == 3 && status["wifi_control"]["read_allowed"]==true &&
        (status["wifi_control"]["mode"]=="continuous" || status["wifi_control"]["remaining_ms"].as_u64().unwrap_or(0)>0)) ||
    (status["wifi_management"] == 2 &&
        ((status["power"]["mode"] == "light" && status["power"]["remaining_s"].as_u64().unwrap_or(0) > 0) || status["sync"]["phase"] == "WIFI_SYNC_ONCE")))
}

// Use the current host's saved target, within the existing rendezvous budget.
pub async fn automatic_on_link(ctx: &AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,budget:bridge_core::rendezvous::Budget) -> Result<(), String> {
    tokio::time::timeout(budget.remaining(),async {
        let contact_host=current_host(ctx).await;
        host_wifi::require_same_network(&contact_host,&contact_host)?;
        if !matches!(link.info["wifi_management"].as_u64(),Some(2|3)) {return Ok(());}
        let svc=platform::service(ctx);
        if svc.ota_job(mac).is_some_and(|j|j.blocks_network_repair()) ||
            svc.job(mac).is_some_and(|j|matches!(j["state"].as_str(),Some("waiting"|"sending"|"unknown"))) {return Ok(());}
        // At most one confirmed save followed by one activation in this budget.
        for _ in 0..2 {
        let host=current_host(ctx).await;
        host_wifi::require_same_network(&contact_host,&host)?;
        let state=platform::service(ctx).wifi_state(mac);
        let Some(target)=current_target(&state,&host).cloned() else{return Err("network_mapping_required".into());};
        let now=crate::device_now_secs(mac).max(0) as u64;
        if link.info["wc"]!=1 {let mut c=fresh_connection(&state,&target,now);c.state="firmware_upgrade_required".into();let _=platform::service(ctx).wifi_note_connection(mac,c);return Ok(());}
        let Ok(reply)=link.command("wifi_match",target_query(&target)).await else{return Ok(());};
        let Some(mut connection)=note_match(ctx,mac,&target,&reply) else{return Ok(());};
        // A new replace intent always needs a real input/confirmed write. Matching
        // public metadata cannot prove a password replacement was applied.
        if reply["matches"]!=true || target.credential=="replace" {
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
        if matches!(connection.error.as_deref(),Some("authentication_failed"|"security_mismatch")){return Err(connection.error.clone().unwrap());}
        if connection.attempts>=3 {return Err("wifi_retry_exhausted".into());}
        if now<connection.next_attempt_at {return Err("wifi_retry_backoff".into());}
        let fresh=current_host(ctx).await;
        host_wifi::require_same_network(&contact_host,&fresh)?;
        let Ok(owner)=link.command("wifi_owner",json!({})).await else{return Ok(());};
        if owner["owner_id"].as_str().is_some_and(|id|!id.is_empty()&&id!=ctx.bridge_id){connection.state="occupied".into();let _=platform::service(ctx).wifi_note_connection(mac,connection);return Ok(());}
        connection.attempts+=1;connection.next_attempt_at=now+60;connection.state="connecting".into();connection.http_ready=false;
        if platform::service(ctx).wifi_note_connection(mac,connection.clone()).is_err(){return Ok(());}
        if link.info["wifi_management"]==3 {
            if let Ok(job)=platform::service(ctx).wifi_queue_control(mac,&ctx.bridge_id,"activate",json!({"network_id":target.network_id,"version":reply["version"]}),now) {unified_on_link(ctx,mac,link,budget,Some(&job.job_id)).await;}
            return Ok(());
        }
        match link.command("wifi_activate",json!({"network_id":target.network_id,"version":reply["version"]})).await {
            Ok(result) if result["ok"]==true => {},
            Ok(result) => {connection.state="connection_failed".into();connection.error=Some(result["error"].as_str().filter(|e|e.len()<64&&e.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_')).unwrap_or("wifi_rejected").into());let _=platform::service(ctx).wifi_note_connection(mac,connection);},
            Err(_) => {connection.state="awaiting_confirmation".into();connection.error=Some("wifi_command_unknown".into());let _=platform::service(ctx).wifi_note_connection(mac,connection);},
        }
        return Ok(());
        }
        Ok(())
    }).await.map_err(|_| "wifi_window_timeout".to_owned())?
}

// A short authorization ACK only. Full reads remain on Wi-Fi after normal ACKs.
pub async fn note_link(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection) -> Value {
    if link.info["wifi_management"]==3 {
        if let Ok(status)=link.wifi_read("wifi_status").await {
            let _=platform::service(ctx).note_authenticated_status_via_at(mac,&status,"ble",crate::device_now_secs(mac).max(0) as u64);
            return json!({"ok":true,"mode":"authenticated_control","wifi_control":status["wifi_control"]});
        }
        return json!({"state":"waiting_authorization","error":"status_unavailable"});
    }
    let state = platform::service(ctx).wifi_state(mac);

    if !matches!(link.info["wifi_management"].as_u64(),Some(2|3)) {
        for j in state.jobs.iter().filter(|j| !j.terminal() && !j.sent) {
            let _ = platform::service(ctx).wifi_finish(mac, &j.job_id, "failed", Some("unsupported"), crate::device_now_secs(mac).max(0) as u64);
        }
        return json!({"state":"failed","error":"unsupported"});
    }
    let Ok(reply) = link.command("wifi_authorize", json!({})).await else { return json!({"state":"waiting_authorization"}); };
    if reply["ok"] == true {
        if let Some(id) = reply["permit"].as_str() {
            permits().lock().unwrap().insert(mac.into(), Permit { id: id.into(),
                write_until:std::time::Instant::now() + Duration::from_millis(reply["write_remaining_ms"].as_u64().unwrap_or(0).min(120000)) });
            if reply["write_remaining_ms"].as_u64().unwrap_or(0) >= 1000 {
                let now = crate::device_now_secs(mac).max(0) as u64;
                for job in state.jobs.iter().filter(|j| j.state == "waiting_authorization" && !j.sent) {
                    let _ = platform::service(ctx).wifi_resume_authorized(mac, &job.job_id, now);
                }
            }
        }
    }
    json!({"ok":reply["ok"],"error":reply["error"],"write_remaining_ms":reply["write_remaining_ms"]})
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
    let _=tokio::task::spawn_blocking(move|| {
        let budget=bridge_core::rendezvous::Budget::new(Duration::from_secs(4));
        let Ok(status)=bridge_core::device_client::status(&link.ip,&link.token,Duration::from_secs(2)) else{return;};
        if status["wifi_management"]!=3 || status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()!=Some(link.mac.as_str()){return;}
        let nonce=status["session_nonce"].as_str().unwrap_or("");
        for job in done {let remaining=budget.remaining();if remaining.is_zero(){break;}
            if bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,"result_ack",nonce,json!({"query_id":job.request_id}),remaining)
                .is_ok_and(|r|r["ok"]==true && r["outcome"]=="retired") {let _=svc.wifi_receipt_acked(&link.mac,&job.job_id);}
        }
    }).await;
}

pub async fn opportunity_admitted(ctx: &AppCtx, mac: &str, job_id: &str, budget: bridge_core::rendezvous::Budget) {
    let svc = platform::service(ctx).clone(); let now = crate::device_now_secs(mac).max(0) as u64;
    let Some(job) = svc.wifi_state(mac).jobs.into_iter().find(|j| j.job_id == job_id && bridge_core::rendezvous::admit_wifi(j, now)) else { return; };
    if work_blocked(ctx, mac) { return; }
    let Some(link) = platform::device_link_for_mac(ctx, mac) else { return; };
    let permit = permits().lock().unwrap().get(mac).cloned();
    let credentials = inputs().lock().unwrap().get(&job.job_id).cloned().unwrap_or_default();
    let job_id = job.job_id.clone();
    let task = tokio::task::spawn_blocking(move || {
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
        if !matches!(status["wifi_management"].as_u64(),Some(2|3)) {
            let _ = svc.wifi_finish(&link.mac,&job_id,"failed",Some("unsupported"),now); return;
        }
        let _=svc.note_authenticated_status_at(&link.mac,&status,now);
        // Queries remain legal after the write window closes.
        if !job.sent && !opportunity_window(&status, &link.mac) { return; }
        let unified=status["wifi_management"]==3;
        let nonce=status["session_nonce"].as_str().unwrap_or("");
        let budget = opportunity_budget(&status, job.sent, sample_start, budget);
        let legacy_writable = permit.as_ref().filter(|p| p.write_until > std::time::Instant::now()).map(|p| p.id.as_str());
        let writable=if unified && status["wifi_control"]["write_allowed"]==true {Some("authenticated_http")} else {legacy_writable};
        if unified && !job.sent && matches!(job.kind.as_str(),"sync"|"recovery_save") &&
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
        let _ = svc.wifi_attempt(&link.mac,&job_id,writable,&credentials,now,|op, body| {
            let timeout = budget.remaining();
            if timeout.is_zero() {return Err(bridge_core::rendezvous::NotDispatched.into());}
            if op == "snapshot" {
                let mut snapshot = bridge_core::device_client::wifi_snapshot(&link.ip,&link.token,&link.mac,&link.bridge_id,
                    permit.as_ref().map(|p|p.id.as_str()),nonce,timeout)?;
                snapshot.observed_at = now; Ok(serde_json::to_value(snapshot)?)
            } else if unified { bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,op,nonce,body,timeout) }
            else { bridge_core::device_client::wifi_request(&link.ip,&link.token,&link.mac,&link.bridge_id,op,body,timeout) }
        });
        if unified {
            for done in svc.wifi_state(&link.mac).jobs.into_iter().filter(|j|j.receipt_ack_pending) {
                let remaining=budget.remaining();if remaining.is_zero(){break;}
                if bridge_core::device_client::wifi_command(&link.ip,&link.token,&link.mac,&link.bridge_id,"result_ack",nonce,json!({"query_id":done.request_id}),remaining)
                    .is_ok_and(|r|r["ok"]==true && r["outcome"]=="retired") {let _=svc.wifi_receipt_acked(&link.mac,&done.job_id);}
            }
        }
    });
    let _ = task.await;
    if platform::service(ctx).wifi_state(mac).jobs.iter().find(|j| j.job_id == job.job_id).is_some_and(|j| j.terminal()) {
        inputs().lock().unwrap().remove(&job.job_id);
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

/// The synchronous durable job service drives both carriers. A single bounded
/// channel lends BLE to the async task; it never starts a second transaction.
async fn unified_on_link(ctx:&AppCtx,mac:&str,link:&mut bridge_ble::DeviceConnection,budget:bridge_core::rendezvous::Budget,job_id:Option<&str>) {
    let svc=platform::service(ctx).clone();let now=crate::device_now_secs(mac).max(0) as u64;
    let Some(job)=svc.wifi_state(mac).jobs.into_iter().find(|j|j.runnable(now) && job_id.is_none_or(|id|j.job_id==id)) else{return;};
    let writable=link.command("wifi_owner",json!({})).await.ok().is_some_and(|r|r["owner_id"]==ctx.bridge_id);
    let credentials=inputs().lock().unwrap().get(&job.job_id).cloned().unwrap_or_default();
    let (tx,mut rx)=tokio::sync::mpsc::channel::<(String,Value,std::sync::mpsc::SyncSender<anyhow::Result<Value>>)>(1);
    let owned=mac.to_owned();let id=job.job_id.clone();
    let worker=tokio::task::spawn_blocking(move||svc.wifi_attempt(&owned,&id,writable.then_some("authenticated_ble"),&credentials,now,|op,body| {
        if budget.remaining().is_zero() {return Err(bridge_core::rendezvous::NotDispatched.into());}
        let (reply,result)=std::sync::mpsc::sync_channel(1);
        tx.blocking_send((op.to_owned(),body,reply)).map_err(|_|anyhow::anyhow!("ble_window_closed"))?;
        result.recv_timeout(budget.remaining()).map_err(|_|anyhow::anyhow!("ble_window_closed"))?
    }));
    while let Some((op,body,reply))=rx.recv().await {
        let result=tokio::time::timeout(budget.remaining(),async {
            if op=="snapshot" {link.wifi_snapshot().await}
            else {let id=body["request_id"].as_str().map(str::to_owned);let wire=if op=="apply" {"wifi_save".to_owned()} else {format!("wifi_{op}")};if let Some(id)=id {link.command_with_id(&wire,body,&id).await}else{link.command(&wire,body).await}}
        }).await.unwrap_or_else(|_|Err(anyhow::anyhow!("ble_window_closed")));
        let _=reply.send(result);
    }
    let _=worker.await;
    for done in platform::service(ctx).wifi_state(mac).jobs.into_iter().filter(|j|j.receipt_ack_pending) {
        if budget.remaining().is_zero(){break;}
        if link.command("wifi_result_ack",json!({"query_id":done.request_id})).await.is_ok_and(|r|r["ok"]==true && r["outcome"]=="retired") {let _=platform::service(ctx).wifi_receipt_acked(mac,&done.job_id);}
    }
}

/// Bounded physical recovery only; shared stable transaction ID and receipt.
pub async fn recover_on_link(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection) {
    recover_on_link_budget(ctx, mac, link, bridge_core::rendezvous::Budget::new(Duration::from_secs(8))).await;
}
pub async fn recover_on_link_budget(ctx: &AppCtx, mac: &str, link: &mut bridge_ble::DeviceConnection, budget: bridge_core::rendezvous::Budget) {
    if work_blocked(ctx, mac) { return; }
    if link.info["wifi_management"]==3 {unified_on_link(ctx,mac,link,budget,None).await;return;}
    let svc = platform::service(ctx); let now = crate::device_now_secs(mac).max(0) as u64;
    let Some(job) = svc.wifi_state(mac).jobs.into_iter().find(|j|matches!(j.kind.as_str(),"sync"|"recovery_save")&&j.runnable(now)) else { return; };
    if !matches!(link.info["wifi_management"].as_u64(),Some(2|3)) {
        let _ = svc.wifi_finish(mac,&job.job_id,"failed",Some("unsupported"),now);return;
    }
    if !job.fields.is_empty() {
        let _=svc.wifi_finish(mac,&job.job_id,"conflict",Some("whole_network_required"),now);return;
    }
    if !job.sent && job.host_ssid.is_some() {
        let host = current_host(ctx).await;
        if host_wifi::current_ssid(&host) != job.host_ssid.as_deref() {
            let _ = svc.wifi_finish(mac, &job.job_id, "conflict", Some("host_network_changed"), now); return;
        }
    }
    let Some(job) = svc.wifi_begin(mac,&job.job_id,now).ok().flatten() else { return; };
    let result = tokio::time::timeout(budget.remaining(), async {
        let revision;
        let bond;
        if job.sent {
            let receipt = link.command("wifi_result",json!({"query_id":job.request_id})).await.map_err(|_|"receipt_unavailable")?;
            if receipt["outcome"] == "rejected" {
                svc.wifi_definite_rejection(mac,&job.job_id).map_err(|_|"persistence_failed")?;
                let code=receipt["error"].as_str().unwrap_or("wifi_rejected");
                svc.wifi_finish(mac,&job.job_id,"failed",Some(code),now).map_err(|_|"persistence_failed")?;return Ok(());
            }
            if receipt["outcome"] != "committed" { return Err("receipt_unknown"); }
            revision = receipt["revision"].as_u64().ok_or("receipt_unknown")?;
            bond = receipt["bond"].as_str().map(str::to_owned);
        } else {
            let mut changes=serde_json::to_value(&job.frozen).map_err(|_|"invalid_wifi_intent")?;
            let credentials=inputs().lock().unwrap().get(&job.job_id).cloned().unwrap_or_default();
            for (index, target) in job.frozen.iter().enumerate() {
                if target.deleted { continue; }
                let password=credentials.get(&target.network_id).cloned().or_else(||target.password.clone());
                if let Some(password)=password {changes[index]["credential"]=json!(if password.is_empty(){"open"}else{"replace"});changes[index]["password"]=json!(password);}
                else if target.credential=="replace" {return Err("credentials_required");}
            }
            let expected = if let Some(revision) = job.expected_revision { revision } else {
                let current = link.command("wifi_active",json!({})).await.map_err(|_|"revision_unavailable")?;
                current["revision"].as_u64().ok_or("revision_unavailable")?
            };
            if link.info["wifi_management"]!=3 {
                let grant = link.command("wifi_authorize",json!({})).await.map_err(|_|"window_required")?;
                if grant["write_remaining_ms"].as_u64().unwrap_or(0) < 1000 { return Err("window_required"); }
            }
            svc.wifi_mark_sent(mac,&job.job_id,now).map_err(|_|"persistence_failed")?;
            let reply = link.command_with_id("wifi_save",json!({"expected_revision":expected,"changes":changes}),&job.request_id)
                .await.map_err(|_|"ack_unavailable")?;
            if reply["ok"] == false && reply["error"] != "storage_unknown" {
                svc.wifi_definite_rejection(mac,&job.job_id).map_err(|_|"persistence_failed")?;
                let code=reply["error"].as_str().unwrap_or("wifi_rejected");
                svc.wifi_finish(mac,&job.job_id,if matches!(code,"physical_required"|"window_required"){"waiting_rendezvous"}else if code=="revision_conflict"{"conflict"}else if code=="credentials_required"{"waiting_input"}else{"failed"},Some(code),now).map_err(|_|"persistence_failed")?;
                return Ok(());
            }
            if reply["outcome"] != "committed" { return Err("write_unconfirmed"); }
            revision=reply["revision"].as_u64().ok_or("write_unconfirmed")?;
            bond=reply["bond"].as_str().map(str::to_owned);
        }
        svc.wifi_confirm_receipt(mac,&job.job_id,revision,bond.as_deref(),now).map_err(|_|"target_unconfirmed")?;
        Ok::<(),&str>(())
    }).await.unwrap_or(Err("ble_window_budget"));
    if let Err(error)=result {
        let sent=svc.wifi_state(mac).jobs.iter().find(|j|j.job_id==job.job_id).is_some_and(|j|j.sent);
        let state=if sent{"awaiting_confirmation"}else if error=="credentials_required"{"waiting_input"}else{"waiting_rendezvous"};
        let _=svc.wifi_finish(mac,&job.job_id,state,Some(error),now);
    } else { inputs().lock().unwrap().remove(&job.job_id); }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut status=json!({"device_mac":mac,"wifi_management":2,"power":{"mode":"deep"}});
        assert!(!opportunity_window(&status,mac));
        status["sync"]["phase"]=json!("WIFI_SYNC_ONCE");assert!(opportunity_window(&status,mac));
        status["wifi_management"]=json!(1);assert!(!opportunity_window(&status,mac));
        status["wifi_management"]=json!(2);status["device_mac"]=json!("020000000002");assert!(!opportunity_window(&status,mac));
        status["device_mac"]=json!(mac);status["sync"]["phase"]=Value::Null;status["power"]["mode"]=json!("light");
        assert!(!opportunity_window(&status,mac));
        status["power"]["remaining_s"]=json!(1);
        assert!(opportunity_window(&status,mac));
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
