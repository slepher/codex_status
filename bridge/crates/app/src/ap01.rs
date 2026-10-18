//! Shared UI/MCP application service for AP01 LAN discovery and screen binding.
use crate::{AppCtx,platform};
use bridge_core::{ap01::{self,Candidate},device_client,platform::{model::{DeviceIdentity,DeviceCapabilities},store}};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{collections::BTreeMap,sync::atomic::{AtomicBool,Ordering},time::{Duration,Instant}};

#[derive(Default)]
pub struct Control {
    candidates:std::sync::Mutex<BTreeMap<String,Candidate>>,
    operation:tokio::sync::Mutex<()>,
    cancelled:AtomicBool,
    reconnect:std::sync::Mutex<BTreeMap<String,Retry>>,
    last_search:std::sync::Mutex<Option<(Vec<ap01::LanInterface>,Instant,ap01::DiscoveryReport)>>,
}

struct Retry {next:Instant,failures:usize,interfaces:Vec<ap01::LanInterface>}

// This private file is saved BEFORE POST, including the candidate credential.
// Never serialize it to a UI/MCP response or log it.
#[derive(Serialize,Deserialize)]
struct Pending { candidate:Candidate,request_id:String,bridge_id:String,name:String,display_code:String,
    binding_token:String,boot_id:u64,pairing_revision:u64,created_at:i64 }

pub fn is_ap01(ctx:&AppCtx,mac:&str)->bool {
    platform::service(ctx).uses_power_plan(mac).is_ok_and(|uses|!uses)
}

fn pending_path(ctx:&AppCtx,mac:&str)->std::path::PathBuf {
    platform::service(ctx).data_root().join(format!("ap01-pair-{mac}.json"))
}

async fn http(endpoint:String,method:&str,path:String,token:String,body:Option<Value>)->Result<(u16,Value),String> {
    let method=method.to_owned();
    tokio::task::spawn_blocking(move ||device_client::pairing_request(&endpoint,&method,&path,&token,
        body.as_ref(),Duration::from_secs(3))).await.map_err(|_|"pairing_worker_failed".to_string())?
        .map_err(|_|"pairing_transport_unknown".into())
}

pub async fn verify_endpoint(ctx:&AppCtx,mac:&str,endpoint:&str)->Result<Value,String> {
    let token=bridge_mcp::load_device_token_at(platform::service(ctx).data_root(),mac).ok_or("ap01_pairing_required")?;
    let endpoint=endpoint.to_owned();
    let status=tokio::task::spawn_blocking(move ||device_client::status(&endpoint,&token,Duration::from_secs(3)))
        .await.map_err(|_|"status_worker_failed")?.map_err(|_|"authenticated_status_failed")?;
    if status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()!=Some(mac) ||
        status["render_target"]!=bridge_core::platform::model::RENDER_TARGET_AP01 ||
        status["fw_target"]!="cuktech-ap01-0041" {return Err("authenticated_identity_mismatch".into());}
    Ok(status)
}

fn register(ctx:&AppCtx,mac:&str,endpoint:&str,name:&str,status:&Value)->Result<(),String> {
    let svc=platform::service(ctx);let now=crate::device_now_secs(mac) as u64;
    let name=svc.device_get(mac).and_then(|d|d["name"].as_str().map(str::to_owned)).unwrap_or_else(||name.into());
    let mut identity=DeviceIdentity::new(mac,&name).map_err(|_|"invalid_device_identity")?;
    identity.ip=Some(endpoint.into());identity.discovered_via="udp".into();identity.last_seen_at=now;
    let caps=platform::caps_from_status(status).map_err(|_|"ap01_capabilities_missing")?;
    if caps.render_target!=DeviceCapabilities::ap01().render_target {return Err("ap01_capabilities_mismatch".into());}
    svc.device_upsert(identity,caps).map_err(|_|"device_save_failed")?;
    svc.note_device_status_at(mac,status,"http",now).map_err(|_|"device_contract_invalid")?;
    svc.note_authenticated_status_at(mac,status,now).map_err(|_|"authenticated_status_save_failed")?;
    crate::ensure_runtime_record(ctx,mac);
    crate::observe_device(ctx,mac,Some(endpoint),None,"udp");
    crate::update_device_status_cache(&mut ctx.device_status_cache.lock().unwrap(),mac,true,Some(status.clone()),now as i64);
    Ok(())
}

pub async fn revalidate(ctx:&AppCtx,mac:&str,endpoint:&str)->Result<bool,String> {
    let status=verify_endpoint(ctx,mac,endpoint).await?;
    register(ctx,mac,endpoint,"AP01",&status)?;Ok(true)
}

pub async fn reconnect(ctx:&AppCtx,mac:&str)->Result<bool,String> {
    if !is_ap01(ctx,mac){return Ok(false);}
    let Ok(_operation)=ctx.ap01.operation.try_lock() else {return Ok(false);};
    let interfaces=crate::ap01_lan::interfaces()?;
    {
        let mut attempts=ctx.ap01.reconnect.lock().unwrap();
        let retry=attempts.entry(mac.into()).or_insert_with(||Retry{next:Instant::now(),failures:0,interfaces:interfaces.clone()});
        if retry.interfaces!=interfaces {retry.next=Instant::now();retry.failures=0;retry.interfaces=interfaces;}
        if retry.next>Instant::now(){return Ok(false);}
        let delays=[2,5,15,30,60];
        let delay=delays[retry.failures.min(4)];retry.failures=retry.failures.saturating_add(1);
        let jitter=(uuid::Uuid::new_v4().as_u128()%500) as u64;
        retry.next=Instant::now()+Duration::from_secs(delay)+Duration::from_millis(jitter);
    }
    let report=search(ctx,Some(mac)).await?;
    for candidate in report.candidates {
        if candidate.device_mac!=mac {continue;}
        if let Ok(status)=verify_endpoint(ctx,mac,&candidate.endpoint).await {
            register(ctx,mac,&candidate.endpoint,&candidate.name,&status)?;
            ctx.ap01.reconnect.lock().unwrap().remove(mac);return Ok(true);
        }
    }
    // Only consult existing neighbors; no unbounded /24 probe.
    let mac_owned=mac.to_owned();
    if let Some(ip)=tokio::task::spawn_blocking(move ||crate::discovery::neighbor_ip_for_mac(&mac_owned))
        .await.map_err(|_|"neighbor_worker_failed")? {
        let endpoint=format!("{ip}:{}",ap01::HTTP_PORT);
        if let Ok(status)=verify_endpoint(ctx,mac,&endpoint).await {
            register(ctx,mac,&endpoint,"AP01",&status)?;
            ctx.ap01.reconnect.lock().unwrap().remove(mac);return Ok(true);
        }
    }
    Ok(false)
}

async fn search(ctx:&AppCtx,target:Option<&str>)->Result<ap01::DiscoveryReport,String> {
    ctx.ap01.cancelled.store(false,Ordering::Relaxed);
    let interfaces=crate::ap01_lan::interfaces()?;
    if target.is_some() {
        let cache=ctx.ap01.last_search.lock().unwrap();
        if let Some((network,at,report))=cache.as_ref() {
            if network==&interfaces && at.elapsed()<Duration::from_secs(10) && !report.cancelled {return Ok(report.clone());}
        }
    }
    let nonce=uuid::Uuid::new_v4().simple().to_string();let network=interfaces.clone();
    // A worker owns its cancellation token even if the UI request is dropped.
    let cancellation=std::sync::Arc::new(AtomicBool::new(false));
    let worker_cancel=cancellation.clone();
    let mut worker=tokio::task::spawn_blocking(move ||ap01::discover(&interfaces,&nonce,None,
        Duration::from_secs(8),&worker_cancel));
    let report=loop {
        tokio::select! {
            result=&mut worker=>break result.map_err(|_|"discovery_worker_failed")?
                .map_err(|_|"discovery_failed")?,
            _=tokio::time::sleep(Duration::from_millis(50))=>{
                if ctx.ap01.cancelled.load(Ordering::Relaxed){cancellation.store(true,Ordering::Relaxed);}
            }
        }
    };
    let mut candidates=ctx.ap01.candidates.lock().unwrap();
    candidates.clear();for candidate in &report.candidates {candidates.insert(candidate.device_mac.clone(),candidate.clone());}
    *ctx.ap01.last_search.lock().unwrap()=Some((network,Instant::now(),report.clone()));
    Ok(report)
}

pub async fn action(ctx:&AppCtx,op:&str,args:&Value)->Value {
    match execute(ctx,op,args).await {
        Ok(value)=>value,
        Err(error)=>json!({"result":"error","stage":op,"error":error,
            "action":match error.as_str() {
                "pairing_transport_unknown"|"pairing_transaction_exists_query_or_cancel"=>"查询或取消原配对事务；不要重新发起",
                "ap01_operation_busy"=>"等待当前搜索或配对请求结束",
                "rediscover_device_first"=>"重新搜索设备后选择配对",
                "pairing_transaction_missing"=>"本机没有待确认事务；重新搜索设备后配对",
                "binding_credential_save_failed"|"device_save_failed"|"pairing_save_before_send_failed"=>"检查 Bridge 数据目录是否可写，然后查询原事务",
                _=>"检查设备与电脑是否在同一局域网，并允许本地网络通信；身份不符时重新发现目标设备",
            }}),
    }
}

async fn execute(ctx:&AppCtx,op:&str,args:&Value)->Result<Value,String> {
    if op=="cancel_search" {ctx.ap01.cancelled.store(true,Ordering::Relaxed);return Ok(json!({"result":"cancelling"}));}
    let _guard=ctx.ap01.operation.try_lock().map_err(|_|"ap01_operation_busy")?;
    if op=="search" {
        let report=search(ctx,None).await?;
        return Ok(json!({"result":if report.cancelled{"cancelled"}else{"complete"},"candidates":report.candidates,
            "errors":report.errors,"partial":report.partial,"action":"选择设备后核对两端四位码，在设备上按下旋钮"}));
    }
    let mac=args["mac"].as_str().and_then(DeviceIdentity::normalized_mac).ok_or("invalid_device_mac")?;
    if op=="pair" {
        let path=pending_path(ctx,&mac);
        if path.exists(){return Err("pairing_transaction_exists_query_or_cancel".into());}
        let candidate=ctx.ap01.candidates.lock().unwrap().get(&mac).cloned().ok_or("rediscover_device_first")?;
        let (code,info)=http(candidate.maintenance_endpoint.clone(),"GET","/api/pairing/info".into(),String::new(),None).await?;
        if code!=200{return Ok(info);}
        ap01::validate_info(&candidate,&info).map_err(|_|"pairing_info_identity_changed")?;
        if info["can_pair"]!=true {return Ok(json!({"result":"rejected","stage":"pairing_info","error":info["reason"],
            "remaining_s":info["owner_remaining_s"],"action":"等待当前占用、配对或设备维护结束后重试"}));}
        let code_path=platform::service(ctx).data_root().join("ap01-display-code.json");
        let display_code=match store::read_json::<String>(&code_path).map_err(|_|"display_code_read_failed")? {
            Some(code) if code.len()==4 && code.bytes().all(|b|b.is_ascii_digit())=>code,
            Some(_)=>return Err("display_code_corrupt".into()),
            None=>{let n=uuid::Uuid::new_v4().as_u128()%10000;let code=format!("{n:04}");
                store::write_json(&code_path,&code).map_err(|_|"display_code_save_failed")?;code},
        };
        let pending=Pending{candidate,request_id:uuid::Uuid::new_v4().simple().to_string(),bridge_id:ctx.bridge_id.clone(),
            name:crate::sanitize_display(&ctx.bridge_name.lock().unwrap(),96),display_code,
            binding_token:uuid::Uuid::new_v4().simple().to_string(),boot_id:info["boot_id"].as_u64().ok_or("pairing_boot_missing")?,
            pairing_revision:info["pairing_revision"].as_u64().ok_or("pairing_revision_missing")?,created_at:crate::now_secs()};
        store::write_json(&path,&pending).map_err(|_|"pairing_save_before_send_failed")?;
        let body=json!({"device_mac":mac,"request_id":pending.request_id,"bridge_id":pending.bridge_id,"name":pending.name,
            "display_code":pending.display_code,"binding_token":pending.binding_token,"boot_id":pending.boot_id,"pairing_revision":pending.pairing_revision});
        let result=http(pending.candidate.maintenance_endpoint.clone(),"POST","/api/pairing".into(),String::new(),Some(body)).await;
        return Ok(match result {
            Ok((code,reply))=>{if code!=202 && code!=200 && code!=503 { std::fs::remove_file(&path).map_err(|_|"pairing_rejection_save_failed")?; }
                json!({"result":if code>=400 {if code==503{"unknown"}else{"rejected"}}else{"pending"},
                    "state":reply["state"],"reply":reply,"mac":mac,"display_code":pending.display_code})},
            Err(_)=>json!({"result":"unknown","stage":"pairing_post","mac":mac,"display_code":pending.display_code,
                "action":"查询原配对事务；不要重新发起"}),
        });
    }
    if !matches!(op,"pair_status"|"cancel_pair"){return Err("unknown_ap01_operation".into());}
    let path=pending_path(ctx,&mac);
    if std::fs::metadata(&path).map_err(|_|"pairing_transaction_missing")?.len()>8192{return Err("pairing_transaction_oversize".into());}
    let pending=store::read_json::<Pending>(&path).map_err(|_|"pairing_transaction_corrupt")?.ok_or("pairing_transaction_missing")?;
    if pending.bridge_id!=ctx.bridge_id || pending.candidate.device_mac!=mac {return Err("pairing_transaction_identity_mismatch".into());}
    let (code,reply)=http(pending.candidate.maintenance_endpoint.clone(),if op=="cancel_pair"{"DELETE"}else{"GET"},
        format!("/api/pairing/{}",pending.request_id),pending.binding_token.clone(),None).await?;
    if reply["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()!=Some(mac.as_str()) ||
        reply["request_id"]!=pending.request_id {return Err("pairing_result_identity_mismatch".into());}
    // An authoritative not_found means this saved transaction was never
    // accepted, or its uncommitted RAM state was lost across a reboot.
    if code==404 && reply["state"]=="not_found" {
        std::fs::remove_file(path).map_err(|_|"pairing_cleanup_failed")?;
        return Ok(json!({"result":"rejected","stage":op,"error":"pairing_not_found","action":"设备未保留该事务，可重新发现并配对"}));
    }
    if code==401 && op=="cancel_pair" {
        std::fs::remove_file(path).map_err(|_|"pairing_cleanup_failed")?;
        return Ok(json!({"result":"cancelled","action":"已停止本机事务；设备上的未确认提示最多在120秒后过期"}));
    }
    if code==500 && reply["error"]=="binding_storage" {
        std::fs::remove_file(path).map_err(|_|"pairing_cleanup_failed")?;
        return Ok(json!({"result":"rejected","stage":"storage","error":"binding_storage","action":"设备未保存配对，请检查存储后重新配对"}));
    }
    if code==409 && matches!(reply["state"].as_str(),Some("owner_changed"|"binding_oom"|"rejected")) {
        std::fs::remove_file(path).map_err(|_|"pairing_cleanup_failed")?;
        return Ok(json!({"result":"rejected","stage":op,"reply":reply,"action":"配对未完成；等待设备空闲后重新配对"}));
    }
    if code!=200{return Ok(json!({"result":if code>=500{"unknown"}else{"rejected"},"stage":op,"reply":reply,
        "display_code":pending.display_code,"action":if code>=500{"查询原事务以确认设备保存结果"}else{"可取消本机事务，重新发现并配对"}}));}
    if reply["state"]=="approved" {
        // Promote only after the device has committed its binding. Failed
        // authentication keeps this same transaction recoverable on restart.
        bridge_mcp::save_device_token_at(platform::service(ctx).data_root(),&mac,&pending.binding_token)
            .map_err(|_|"binding_credential_save_failed")?;
        let status=verify_endpoint(ctx,&mac,&pending.candidate.endpoint).await?;
        register(ctx,&mac,&pending.candidate.endpoint,&pending.candidate.name,&status)?;
        std::fs::remove_file(path).map_err(|_|"pairing_cleanup_failed")?;
        return Ok(json!({"result":"paired","mac":mac,"display_code":pending.display_code,"published":false,"sync_enabled":false}));
    }
    if matches!(reply["state"].as_str(),Some("cancelled"|"expired"|"rejected")) {
        std::fs::remove_file(path).map_err(|_|"pairing_cleanup_failed")?;
    }
    Ok(json!({"result":"pending","reply":reply,"display_code":pending.display_code,"mac":mac}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router,Json,routing::get,extract::State};
    use std::sync::{Arc,Mutex};
    const MAC:&str="02000000A001";

    struct Peer { root:std::path::PathBuf,posted:Mutex<Option<Value>> }
    async fn info()->Json<Value> {Json(json!({"mac":MAC,"model":"njcuk.enstor.ap01",
        "binding_method":ap01::BINDING_METHOD,"boot_id":1,"pairing_revision":0,"can_pair":true}))}
    async fn begin(State(peer):State<Arc<Peer>>,Json(body):Json<Value>)->Json<Value> {
        // A request observed on the socket must already be recoverable on disk.
        let path=peer.root.join(format!("ap01-pair-{MAC}.json"));
        let persisted:Pending=store::read_json(&path).unwrap().unwrap();
        assert_eq!(body["request_id"],persisted.request_id);
        assert_eq!(body["binding_token"],persisted.binding_token);
        *peer.posted.lock().unwrap()=Some(body);
        tokio::time::sleep(Duration::from_secs(4)).await; // response lost past deadline
        Json(json!({"state":"pending_display","result":"pending_display"}))
    }
    async fn query(State(peer):State<Arc<Peer>>)->Json<Value> {
        let body=peer.posted.lock().unwrap().clone().unwrap();
        Json(json!({"device_mac":MAC,"boot_id":1,"request_id":body["request_id"],
            "result":"awaiting_confirmation","state":"awaiting_confirmation"}))
    }
    #[tokio::test]
    async fn known_devices_reuse_one_recent_search_on_the_same_interfaces() {
        let dir=tempfile::tempdir().unwrap();
        let ctx=platform::ble_close_lock_tests::test_ctx(dir.path(),&[]);
        let interfaces=crate::ap01_lan::interfaces().unwrap();
        let report=ap01::DiscoveryReport{candidates:vec![Candidate{device_mac:MAC.into(),name:"AP01".into(),
            endpoint:"127.0.0.1:1".into(),maintenance_endpoint:"127.0.0.1:2".into(),boot_id:1,binding_method:ap01::BINDING_METHOD.into()}],..Default::default()};
        *ctx.ap01.last_search.lock().unwrap()=Some((interfaces,Instant::now(),report));
        for mac in [MAC,"02000000A002"] {
            let cached=search(&ctx,Some(mac)).await.unwrap();
            assert_eq!(cached.candidates.len(),1);
            assert_eq!(cached.candidates[0].device_mac,MAC);
        }
        assert!(platform::service(&ctx).devices().is_empty(),"cached UDP hints cannot register devices");
    }
    #[tokio::test(flavor="multi_thread",worker_threads=2)]
    async fn lost_pair_ack_keeps_original_persisted_transaction_across_restart() {
        let dir=tempfile::tempdir().unwrap();
        let peer=Arc::new(Peer{root:dir.path().into(),posted:Mutex::new(None)});
        let app=Router::new().route("/api/pairing/info",get(info))
            .route("/api/pairing",axum::routing::post(begin))
            .route("/api/pairing/{id}",get(query)).with_state(peer.clone());
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint=listener.local_addr().unwrap().to_string();
        let server=tokio::spawn(async move {axum::serve(listener,app).await.unwrap()});
        let ctx=platform::ble_close_lock_tests::test_ctx(dir.path(),&[]);
        ctx.ap01.candidates.lock().unwrap().insert(MAC.into(),Candidate{device_mac:MAC.into(),name:"AP01".into(),
            maintenance_endpoint:endpoint.clone(),endpoint,boot_id:1,binding_method:ap01::BINDING_METHOD.into()});
        let result=action(&ctx,"pair",&json!({"mac":MAC})).await;
        assert_eq!(result["result"],"unknown","{result}");
        assert!(platform::service(&ctx).device_get(MAC).is_none());
        let posted=peer.posted.lock().unwrap().clone().unwrap();
        assert!(!result.to_string().contains(posted["binding_token"].as_str().unwrap()));
        drop(ctx);
        let restarted=platform::ble_close_lock_tests::test_ctx(dir.path(),&[]);
        let queried=action(&restarted,"pair_status",&json!({"mac":MAC})).await;
        assert_eq!(queried["reply"]["state"],"awaiting_confirmation");
        assert_eq!(queried["reply"]["request_id"],posted["request_id"]);
        assert_eq!(queried["display_code"],result["display_code"]);
        let duplicate=action(&restarted,"pair",&json!({"mac":MAC})).await;
        assert_eq!(duplicate["error"],"pairing_transaction_exists_query_or_cancel");
        assert!(platform::service(&restarted).devices().is_empty());
        server.abort();
    }
}
