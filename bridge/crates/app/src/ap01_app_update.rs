//! Explicit, bounded UI/MCP commands. The AP01 service owns the operation lock.
use anyhow::{ensure,Context,Result};
use bridge_core::{ap01_app_set::Manifest,ap01_app_set_receipt::Receipt,ap01_app_set_install,
    device_client::{ap01_app_set_request,Ap01AppSetAction as Action},rendezvous::{CommandFailure,DispatchOutcome}};
use serde_json::{json,Value};
use std::{io::Read,path::{Path,PathBuf},time::Duration};

fn read(path:&str,max:usize)->Result<Vec<u8>> {
    ensure!(std::fs::metadata(path)?.is_file(),"app_set_regular_file_required");
    let file=std::fs::File::open(path)?;ensure!(file.metadata()?.is_file(),"app_set_regular_file_required");
    let mut bytes=Vec::new();file.take((max+1) as u64).read_to_end(&mut bytes)?;
    ensure!(bytes.len()<=max,"app_set_artifact_too_large");Ok(bytes)
}
fn packages(args:&Value,m:&Manifest)->Result<Vec<Vec<u8>>> {
    ensure!(args["package_paths"].is_null() || args["package_paths"].is_array(),"app_set_package_paths");
    let paths=args["package_paths"].as_array();
    ensure!(paths.is_none_or(|p|p.len()<=m.apps.len()),"app_set_package_count");
    let max=m.apps.iter().map(|a|a.package_bytes as usize).max().unwrap_or(0);
    let mut out=Vec::new();let mut remaining=64*1024*1024; // Desktop input ceiling, not a device capacity claim.
    for path in paths.into_iter().flatten(){
        let bytes=read(path.as_str().context("app_set_package_path")?,max.min(remaining))?;
        remaining-=bytes.len();out.push(bytes);
    }
    // The device decides which members changed; validate all supplied packages now.
    m.changed_packages(&out,0)?;Ok(out)
}
fn receipt_path(root:&Path,mac:&str)->PathBuf {root.join(format!("ap01-app-set-{mac}.json"))}

// Called under the shared AP01 operation lock. Only our exact cache namespace
// is eligible; original selected paths never enter cleanup.
fn cache_name(name:&str)->bool {
    let name=name.strip_suffix(".tmp").unwrap_or(name);
    name.strip_prefix("ap01-artifact-").is_some_and(|hash|hash.len()==64 && hash.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}
fn stage_artifact(root:&Path,bytes:&[u8],limit:u64)->Result<(PathBuf,String)> {
    use sha2::Digest;
    let digest=format!("{:x}",sha2::Sha256::digest(bytes));
    let path=root.join(format!("ap01-artifact-{digest}"));
    let tmp=path.with_file_name(format!("ap01-artifact-{digest}.tmp"));
    // Do not follow links (including broken ones), directories or special files.
    for candidate in [&path,&tmp] {
        match std::fs::symlink_metadata(candidate) {
            Ok(meta)=>ensure!(meta.is_file(),"app_set_cache_regular_file_required"),
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},
            Err(e)=>return Err(e.into()),
        }
    }
    let exists=path.exists();
    if exists {ensure!(read(path.to_str().context("app_set_artifact_path")?,bytes.len())?==bytes,"app_set_artifact_conflict");}
    // ponytail: any active receipt pins the whole cache; per-package references
    // are only needed if concurrent device updates routinely exhaust this cap.
    let mut pinned=false;
    for entry in std::fs::read_dir(root)? {
        let entry=entry?;let name=entry.file_name();let name=name.to_string_lossy();
        if name.strip_prefix("ap01-app-set-").and_then(|s|s.strip_suffix(".json"))
            .is_some_and(|mac|mac.len()==12 && mac.bytes().all(|b|b.is_ascii_hexdigit())) {pinned=true;break;}
    }
    let now=std::time::SystemTime::now();let mut used=0u64;
    for entry in std::fs::read_dir(root)? {
        let entry=entry?;
        if !cache_name(&entry.file_name().to_string_lossy()){continue;}
        let meta=std::fs::symlink_metadata(entry.path())?;
        ensure!(meta.is_file(),"app_set_cache_regular_file_required");
        if !pinned && entry.path()!=path && now.duration_since(meta.modified()?).is_ok_and(|age|age>=Duration::from_secs(24*3600)) {
            std::fs::remove_file(entry.path())?;
        }else{used=used.checked_add(meta.len()).context("app_set_cache_full")?;}
    }
    ensure!(used.checked_add(if exists{0}else{bytes.len() as u64}).is_some_and(|n|n<=limit),"app_set_cache_full");
    if exists {
        // A user selecting an old cached package starts a new retention window.
        std::fs::File::open(&path)?.set_modified(now)?;
    }else{bridge_core::platform::store::atomic_write(&path,bytes)?;}
    Ok((path,digest))
}

pub async fn action(root:&Path,mac:&str,endpoint:&str,args:&Value)->Value {
    match execute(root,mac,endpoint,args).await {
        Ok(value)=>value,
        Err(error)=>{
            let failure=error.downcast_ref::<CommandFailure>();
            let unknown=failure.is_some_and(|f|f.outcome!=DispatchOutcome::NotSent);
            json!({"result":if unknown{"unknown"}else{"error"},"stage":"app_set",
                "error":error.to_string(),"dispatch":failure.map(|f|match f.outcome {DispatchOutcome::NotSent=>"not_sent",DispatchOutcome::WriteUnknown=>"write_unknown",DispatchOutcome::AckUnknown=>"ack_unknown"}),
                "action":if unknown{"查询原回执后继续；不要重新准备事务"}else{"检查输入或原回执；不要覆盖已有事务"}})
        }
    }
}
async fn execute(root:&Path,mac:&str,endpoint:&str,args:&Value)->Result<Value> {
    ensure!(mac.len()==12 && mac.bytes().all(|c|c.is_ascii_hexdigit()) && mac==mac.to_ascii_uppercase(),"invalid_device_mac");
    let op=args["op"].as_str().context("app_set_operation")?;
    ensure!(matches!(op,"prepare"|"inspect"|"query"|"advance"|"cancel"|"archive"|"reconnect"|"stage"),"app_set_operation");
    if op=="stage" {
        use base64::Engine;
        let kind=args["kind"].as_str().context("app_set_artifact_kind")?;
        let limit=match kind {"manifest"=>3584,"package"=>16*1024*1024,_=>anyhow::bail!("app_set_artifact_kind")};
        let text=args["data_base64"].as_str().context("app_set_artifact_bytes")?;
        ensure!(text.len()<=((limit+2)/3)*4,"app_set_artifact_too_large");
        let bytes=base64::engine::general_purpose::STANDARD.decode(text)?;ensure!(bytes.len()<=limit,"app_set_artifact_too_large");
        if kind=="manifest" {Manifest::parse(&bytes)?;}else{ensure!(bytes.len()>=256 && bytes.starts_with(b"APX2"),"app_set_package_header");}
        let (path,digest)=stage_artifact(root,&bytes,128*1024*1024)?;
        return Ok(json!({"result":"staged","path":path,"bytes":bytes.len(),"sha256":digest}));
    }
    let path=receipt_path(root,mac);
    if op=="inspect" {
        if !path.exists(){return Ok(json!({"result":"absent","mac":mac}));}
        let receipt=Receipt::load_local(&path,mac)?;
        return Ok(json!({"result":"saved","mac":mac,"operation_id":receipt.operation_id,
            "progress":receipt.progress,"manifest":receipt.manifest()?,"original_boot":receipt.original_boot,"endpoint":receipt.endpoint}));
    }
    let code=args["code"].as_str().context("app_set_menu_code_required")?;
    ensure!(code.len()==8 && code.bytes().all(|b|b.is_ascii_digit()),"app_set_menu_code_invalid");
    let budget=Duration::from_secs(5);
    if op=="prepare" {
        ensure!(!path.exists(),"app_set_original_receipt_exists");
        let bytes=read(args["manifest_path"].as_str().context("app_set_manifest_path")?,3584)?;
        let manifest=Manifest::parse(&bytes)?;
        let operation=uuid::Uuid::new_v4().simple().to_string();
        let (status,value)=ap01_app_set_request(endpoint,mac,code,&operation,Action::Status,&[],budget).await?;
        ensure!(status==200,"app_set_status_rejected: {status}");
        let receipt=Receipt::prepare(endpoint,mac,&operation,&bytes,&value)?;
        let supplied=packages(args,&manifest)?;
        receipt.create(&path)?; // No device mutation until this durable fence exists.
        return Ok(json!({"result":"prepared","operation_id":operation,"mac":mac,
            "manifest":manifest,"provided_packages":supplied.len(),"next_action":"advance"}));
    }
    let mut receipt=if op=="reconnect" {Receipt::load_local(&path,mac)?}else{Receipt::load(&path,endpoint,mac)?};
    ensure!(args["operation_id"].as_str()==Some(receipt.operation_id.as_str()),"app_set_operation_mismatch");
    if op=="reconnect" {
        let step=ap01_app_set_install::reconnect(&mut receipt,&path,endpoint,code,budget).await?;
        return Ok(json!({"result":step.state,"operation_id":receipt.operation_id,"mac":mac,"endpoint":receipt.endpoint,"progress":receipt.progress,"reply":step.reply}));
    }
    if matches!(op,"query"|"archive") {
        let step=ap01_app_set_install::query(&mut receipt,&path,code,budget).await?;
        if op=="archive" {
            let phase=step.reply["phase"].as_str().unwrap_or("");
            ensure!(step.reply["cleanup_pending"]==false &&
                (step.state==ap01_app_set_install::State::Confirmed ||
                 (step.state==ap01_app_set_install::State::Terminal && matches!(phase,"failed_rolled_back"|"cancelled"))),"app_set_terminal_not_confirmed");
            receipt.archive(&path)?;
            return Ok(json!({"result":"archived","operation_id":receipt.operation_id,"mac":mac,"reply":step.reply}));
        }
        return Ok(json!({"result":step.state,"operation_id":receipt.operation_id,"mac":mac,"progress":receipt.progress,"reply":step.reply}));
    }
    if op=="cancel" {bridge_core::platform::store::atomic_write(&cancel_path(root,mac,&receipt.operation_id),receipt.operation_id.as_bytes())?;}
    let cancelling=cancellation(root,mac,&receipt.operation_id)?;
    let supplied=if cancelling {Vec::new()}else{packages(args,&receipt.manifest()?)?};
    let step=ap01_app_set_install::advance(&mut receipt,&path,code,&supplied,cancelling,budget).await?;
    Ok(json!({"result":step.state,"operation_id":receipt.operation_id,"mac":mac,
        "progress":receipt.progress,"reply":step.reply}))
}


#[derive(Default)]
pub struct WorkerControl {job:std::sync::Mutex<Option<WorkerJob>>}
struct WorkerJob {mac:String,operation:String,run_id:String,manifest:Manifest,active:bool,view:Value}
fn cancel_path(root:&Path,mac:&str,operation:&str)->PathBuf {root.join(format!("ap01-app-set-{mac}-{operation}.cancel"))}
fn cancellation(root:&Path,mac:&str,operation:&str)->Result<bool> {
    let path=cancel_path(root,mac,operation);if !path.exists(){return Ok(false);}
    ensure!(read(path.to_str().context("app_set_cancel_path")?,64)?==operation.as_bytes(),"app_set_cancel_marker_invalid");Ok(true)
}
fn worker_view(job:&WorkerJob)->Value {json!({"mac":job.mac,"operation_id":job.operation,"active":job.active,"status":job.view,"manifest":job.manifest})}
fn note_worker(ctx:&crate::AppCtx,view:Value,active:bool) {
    if let Some(job)=ctx.ap01.updates.job.lock().unwrap().as_mut(){job.view=view;job.active=active;}
}
struct WorkerExit(std::sync::Arc<crate::AppCtx>,String);
impl Drop for WorkerExit {
    fn drop(&mut self){let mut slot=self.0.ap01.updates.job.lock().unwrap();if let Some(job)=slot.as_mut(){if job.active && job.run_id==self.1 {job.active=false;job.view=json!({"result":"unknown","error":"worker_stopped","action":"inspect_original_receipt"});}}}
}
/// Worker controls are shared by UI and MCP. A single updater owns the AP01
/// maintenance queue; mutation requests cannot race its receipt writes.
pub async fn worker_command(ctx:&std::sync::Arc<crate::AppCtx>,mac:&str,args:&Value)->Option<Value> {
    let op=args["op"].as_str().unwrap_or("");
    {
        let mut jobs=ctx.ap01.updates.job.lock().unwrap();
        if op=="worker_status" {return Some(jobs.as_ref().filter(|j|j.mac==mac).map(worker_view).unwrap_or_else(||json!({"result":"absent","active":false})));}
        if let Some(job)=jobs.as_mut().filter(|j|j.active) {
            if job.mac!=mac {return Some(json!({"result":"pending","error":"app_set_worker_busy"}));}
            if op=="run" && args["operation_id"].as_str()!=Some(job.operation.as_str()){return Some(json!({"result":"error","error":"app_set_operation_mismatch"}));}
            if op=="cancel" {
                if args["operation_id"].as_str()!=Some(job.operation.as_str()){return Some(json!({"result":"error","error":"app_set_operation_mismatch"}));}
                let root=crate::platform::service(ctx).data_root();
                if bridge_core::platform::store::atomic_write(&cancel_path(root,mac,&job.operation),job.operation.as_bytes()).is_err(){return Some(json!({"result":"error","error":"app_set_cancel_save_failed"}));}
                job.view["cancel_requested"]=true.into();return Some(json!({"result":"cancelling","operation_id":job.operation}));
            }
            return Some(worker_view(job));
        }
    }
    if op!="run" {return None;}
    let result=(||->Result<_>{
        let _operation=ctx.ap01.operation.try_lock().map_err(|_|anyhow::anyhow!("ap01_operation_busy"))?;
        let mut jobs=ctx.ap01.updates.job.lock().unwrap();ensure!(!jobs.as_ref().is_some_and(|j|j.active),"app_set_worker_busy");
        let root=crate::platform::service(ctx).data_root();let path=receipt_path(root,mac);let receipt=Receipt::load_local(&path,mac)?;
        ensure!(args["operation_id"].as_str()==Some(receipt.operation_id.as_str()),"app_set_operation_mismatch");
        let code=args["code"].as_str().context("app_set_menu_code_required")?.to_owned();ensure!(code.len()==8&&code.bytes().all(|b|b.is_ascii_digit()),"app_set_menu_code_invalid");
        let seconds=args.get("budget_secs").map(|v|v.as_u64().context("app_set_budget")).transpose()?.unwrap_or(180);ensure!((1..=3600).contains(&seconds),"app_set_budget");
        let cancelling=cancellation(root,mac,&receipt.operation_id)?;
        let blobs=if cancelling {Vec::new()}else{packages(args,&receipt.manifest()?)?};
        let run_id=uuid::Uuid::new_v4().simple().to_string();
        *jobs=Some(WorkerJob{mac:mac.into(),operation:receipt.operation_id.clone(),run_id:run_id.clone(),manifest:receipt.manifest()?,active:true,view:json!({"result":"running","cancel_requested":cancelling})});
        Ok((receipt,path,code,blobs,Duration::from_secs(seconds),run_id))
    })();
    match result {
        Err(error)=>Some(json!({"result":"error","error":error.to_string()})),
        Ok((receipt,path,code,blobs,budget,run_id))=>{
            let result=json!({"result":"running","operation_id":receipt.operation_id,"active":true});let ctx=ctx.clone();
            tokio::spawn(async move {let _exit=WorkerExit(ctx.clone(),run_id);run_worker(&ctx,receipt,&path,&code,&blobs,budget).await;});Some(result)
        }
    }
}
async fn run_worker(ctx:&std::sync::Arc<crate::AppCtx>,mut receipt:Receipt,path:&Path,code:&str,blobs:&[Vec<u8>],budget:Duration) {
    let deadline=std::time::Instant::now()+budget;let mut failures=0u32;
    loop {
        let remaining=deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero(){note_worker(ctx,json!({"result":"unknown","error":"budget_exhausted","action":"inspect_original_receipt"}),false);return;}
        let cancelling=match cancellation(crate::platform::service(ctx).data_root(),&receipt.device_mac,&receipt.operation_id){Ok(v)=>v,Err(error)=>{note_worker(ctx,json!({"result":"error","error":error.to_string()}),false);return;}};
        let outcome={
            let guard=tokio::time::timeout(remaining.min(Duration::from_secs(1)),ctx.ap01.operation.lock()).await;
            if guard.is_err(){continue;}
            let _guard=guard.unwrap();
            let remaining=deadline.saturating_duration_since(std::time::Instant::now()).min(Duration::from_secs(5));
            let known=crate::device_facts_for(ctx,&receipt.device_mac).map(|f|f.endpoint()).unwrap_or_default();
            let ip=known.parse::<std::net::SocketAddr>().map(|a|a.ip()).or_else(|_|known.parse::<std::net::IpAddr>());
            let endpoint=ip.map(|ip|std::net::SocketAddr::new(ip,18080).to_string()).unwrap_or_else(|_|receipt.endpoint.clone());
            if endpoint!=receipt.endpoint {ap01_app_set_install::reconnect(&mut receipt,path,&endpoint,code,remaining).await}
            else {ap01_app_set_install::advance(&mut receipt,path,code,blobs,cancelling,remaining).await}
        };
        let retry;
        match outcome {
            Ok(step)=>{
                let done=matches!(step.state,ap01_app_set_install::State::Confirmed|ap01_app_set_install::State::Terminal|ap01_app_set_install::State::Abandoned);
                retry=step.state==ap01_app_set_install::State::Unknown;
                note_worker(ctx,json!({"result":step.state,"progress":receipt.progress,"reply":step.reply,"cancel_requested":cancelling}),!done);
                if done{return;}
            }
            Err(error)=>{
                let failure=error.downcast_ref::<CommandFailure>();
                retry=failure.is_some_and(|f|matches!(f.reason,"transport_timeout"|"transport_unavailable"));
                let unknown=failure.is_some_and(|f|f.outcome!=DispatchOutcome::NotSent);
                note_worker(ctx,json!({"result":if unknown{"unknown"}else{"error"},"error":error.to_string(),"retrying":retry,"cancel_requested":cancelling}),retry);
                if !retry{return;}
            }
        }
        if retry {
            failures=failures.saturating_add(1);
            // Reuse the existing bounded authenticated discovery/backoff. It
            // cannot replace the transaction fence; next loop verifies v3 too.
            if failures%3==0 {
                let remaining=deadline.saturating_duration_since(std::time::Instant::now()).min(Duration::from_secs(10));
                let _=tokio::time::timeout(remaining,crate::ap01::reconnect(ctx,&receipt.device_mac)).await;
            }
        }else{failures=0;}
        let delay=if retry {Duration::from_millis((250u64<<failures.min(4)).min(5000))}else{Duration::from_millis(20)};
        tokio::time::sleep(delay.min(deadline.saturating_duration_since(std::time::Instant::now()))).await;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn staged_cache_bounds_retention_and_protects_recovery() {
        let dir=tempfile::tempdir().unwrap();let root=dir.path();
        let (first,_)=stage_artifact(root,b"first",10).unwrap();
        let old=std::time::SystemTime::now()-Duration::from_secs(25*3600);
        std::fs::File::open(&first).unwrap().set_modified(old).unwrap();
        let receipt=receipt_path(root,"AABBCCDDEEFF");std::fs::write(&receipt,b"corrupt receipt still pins cache").unwrap();
        assert!(stage_artifact(root,b"second",10).unwrap_err().to_string().contains("cache_full"));assert!(first.exists());
        std::fs::rename(&receipt,root.join("archived.completed")).unwrap();
        let original=root.join("selected.apx2");std::fs::write(&original,b"original").unwrap();
        std::fs::File::open(&original).unwrap().set_modified(old).unwrap();
        let temp=root.join(format!("ap01-artifact-{}.tmp","a".repeat(64)));std::fs::write(&temp,b"partial").unwrap();
        std::fs::File::open(&temp).unwrap().set_modified(old).unwrap();
        let (second,_)=stage_artifact(root,b"second",10).unwrap();
        assert!(!first.exists());assert!(!temp.exists());assert_eq!(std::fs::read(&original).unwrap(),b"original");
        std::fs::File::open(&second).unwrap().set_modified(old).unwrap();
        assert_eq!(stage_artifact(root,b"second",10).unwrap().0,second);
        assert!(second.metadata().unwrap().modified().unwrap()>old);
        assert!(stage_artifact(root,b"12345",10).is_err());assert!(second.exists());
        std::fs::File::open(&second).unwrap().set_modified(std::time::SystemTime::now()+Duration::from_secs(3600)).unwrap();
        assert!(stage_artifact(root,b"12345",10).is_err());assert!(second.exists());
        std::fs::write(&second,b"broken").unwrap();assert!(stage_artifact(root,b"second",10).unwrap_err().to_string().contains("conflict"));
        #[cfg(unix)] {
            std::fs::remove_file(&second).unwrap();std::os::unix::fs::symlink(&original,&second).unwrap();
            assert!(stage_artifact(root,b"second",100).is_err());assert_eq!(std::fs::read(&original).unwrap(),b"original");
            std::fs::remove_file(&second).unwrap();
            let temp=second.with_file_name(format!("{}.tmp",second.file_name().unwrap().to_str().unwrap()));
            std::os::unix::fs::symlink(root.join("missing"),&temp).unwrap();
            assert!(stage_artifact(root,b"second",100).is_err());assert!(!root.join("missing").exists());
            std::fs::remove_file(temp).unwrap();
        }
    }

    use super::*;
    #[tokio::test]
    async fn rejects_unregistered_input_and_preserves_existing_receipt_without_network() {
        let dir=tempfile::tempdir().unwrap();let mac="010203040506";
        assert!(read(dir.path().to_str().unwrap(),3584).is_err());
        let large=dir.path().join("large");std::fs::write(&large,[0;65]).unwrap();assert!(read(large.to_str().unwrap(),64).is_err());
        let value=action(dir.path(),"../invalid","127.0.0.1:1",&json!({"op":"inspect"})).await;
        assert_eq!(value["result"],"error");
        assert_eq!(action(dir.path(),mac,"127.0.0.1:1",&json!({"op":"inspect"})).await["result"],"absent");
        let path=receipt_path(dir.path(),mac);std::fs::write(&path,b"damaged receipt").unwrap();
        let value=action(dir.path(),mac,"127.0.0.1:1",&json!({"op":"prepare","code":"01234567"})).await;
        assert_eq!(value["error"],"app_set_original_receipt_exists");
        assert_eq!(std::fs::read(path).unwrap(),b"damaged receipt");assert!(!value.to_string().contains("01234567"));
    }
    #[tokio::test]
    async fn prepare_is_read_only_and_advance_uses_saved_operation() {
        use axum::{Router,Json,routing::{get,post},extract::State};
        use std::sync::{Arc,Mutex,atomic::{AtomicUsize,Ordering}};
        use sha2::Digest;
        struct Peer {root:PathBuf,endpoint:String,posts:AtomicUsize,reply:Mutex<Value>}
        async fn status()->Json<Value> {Json(json!({"protocol":3,"device_mac":"010203040506","phase":"idle","boot_id":11,"runtime_reporting":true,"app_max":8,
            "limits":{"package_bytes":1000,"image_bytes":1000,"heap_bytes":1000,"stack_bytes":1000,"tasks":4,"pages":4,"capabilities":127,"ram_bytes":4096,"resident_bytes":0}}))}
        async fn begin(State(peer):State<Arc<Peer>>)->Json<Value> {
            let receipt=Receipt::load(&receipt_path(&peer.root,"010203040506"),&peer.endpoint,"010203040506").unwrap();
            peer.posts.fetch_add(1,Ordering::SeqCst);
            let reply=json!({"protocol":3,"device_mac":receipt.device_mac,"operation_id":receipt.operation_id,
                "candidate_manifest":receipt.manifest().unwrap().digest.iter().map(|b|format!("{b:02x}")).collect::<String>(),"candidate_version":1,
                "old_manifest":receipt.expected_manifest,"old_version":0,"phase":"receiving","sequence":1,"boot_id":11,
                "changed_mask":0,"ready_mask":0,"received":[0,0,0,0,0,0,0,0],"chunk_max":1024});
            *peer.reply.lock().unwrap()=reply.clone();Json(reply)
        }
        async fn query(State(peer):State<Arc<Peer>>)->(axum::http::StatusCode,Json<Value>) {
            let reply=peer.reply.lock().unwrap().clone();
            if reply.is_null(){(axum::http::StatusCode::NOT_FOUND,Json(json!({"error":"missing"})))}else{(axum::http::StatusCode::OK,Json(reply))}
        }
        let dir=tempfile::tempdir().unwrap();let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint=listener.local_addr().unwrap().to_string();let peer=Arc::new(Peer{root:dir.path().into(),endpoint:endpoint.clone(),posts:AtomicUsize::new(0),reply:Mutex::new(Value::Null)});
        let app=Router::new().route("/api/app/v3/status",get(status)).route("/api/app/v3/update/result",get(query))
            .route("/api/app/v3/update/begin",post(begin)).with_state(peer.clone());
        let first_app=app.clone();let server=tokio::spawn(async move{axum::serve(listener,first_app).await.unwrap()});
        let mut bytes=vec![0u8;64];bytes[..4].copy_from_slice(b"AMF3");bytes[4]=1;bytes[6]=64;bytes[8]=64;bytes[14]=160;bytes[16]=1;
        let hash=sha2::Sha256::digest(&bytes);bytes[32..64].copy_from_slice(&hash);let manifest=dir.path().join("set.amf3");std::fs::write(&manifest,bytes).unwrap();
        let mac="010203040506";
        use base64::Engine;
        let staged=action(dir.path(),mac,"",&json!({"op":"stage","kind":"manifest","data_base64":base64::engine::general_purpose::STANDARD.encode(std::fs::read(&manifest).unwrap())})).await;
        assert_eq!(staged["result"],"staged","{staged}");assert_eq!(std::fs::read(staged["path"].as_str().unwrap()).unwrap(),std::fs::read(&manifest).unwrap());
        let bad=action(dir.path(),mac,"",&json!({"op":"stage","kind":"package","data_base64":"AAAA"})).await;assert_eq!(bad["result"],"error");
        let prepared=action(dir.path(),mac,&endpoint,&json!({"op":"prepare","code":"01234567","manifest_path":manifest})).await;
        assert_eq!(prepared["result"],"prepared","{prepared}");assert_eq!(peer.posts.load(Ordering::SeqCst),0);
        let inspect=action(dir.path(),mac,&endpoint,&json!({"op":"inspect"})).await;
        assert_eq!(inspect["operation_id"],prepared["operation_id"]);
        let wrong=action(dir.path(),mac,&endpoint,&json!({"op":"advance","code":"01234567","operation_id":"wrong"})).await;
        assert_eq!(wrong["error"],"app_set_operation_mismatch");assert_eq!(peer.posts.load(Ordering::SeqCst),0);
        let advanced=action(dir.path(),mac,&endpoint,&json!({"op":"advance","code":"01234567","operation_id":prepared["operation_id"]})).await;
        assert_eq!(advanced["result"],"pending","{advanced}");assert_eq!(advanced["progress"]["phase"],"receiving");
        assert_eq!(peer.posts.load(Ordering::SeqCst),1);
        let saved=std::fs::read_to_string(receipt_path(dir.path(),mac)).unwrap();assert!(!saved.contains("01234567"));
        let offline=action(dir.path(),mac,"",&json!({"op":"inspect"})).await;
        assert_eq!(offline["operation_id"],prepared["operation_id"]);
        let archive_args=json!({"op":"archive","code":"01234567","operation_id":prepared["operation_id"]});
        let refused=action(dir.path(),mac,&endpoint,&archive_args).await;
        assert_eq!(refused["error"],"app_set_terminal_not_confirmed");assert!(receipt_path(dir.path(),mac).exists());
        // Query/archive must never mutate a receiving transaction.
        assert_eq!(peer.posts.load(Ordering::SeqCst),1);
        {
            let mut reply=peer.reply.lock().unwrap();let digest=reply["candidate_manifest"].clone();
            reply["phase"]="succeeded".into();reply["sequence"]=2.into();reply["boot_id"]=12.into();
            reply["cleanup_pending"]=true.into();reply["runtime_reporting"]=true.into();
            reply["runtime"]=json!({"complete":true,"boot_state":"started","health_state":"healthy","manifest":digest,"version":1,"started_count":0,"running":[]});
        }
        assert_eq!(action(dir.path(),mac,&endpoint,&archive_args).await["error"],"app_set_terminal_not_confirmed");
        {let mut reply=peer.reply.lock().unwrap();reply["cleanup_pending"]=false.into();reply["boot_id"]=11.into();}
        assert_eq!(action(dir.path(),mac,&endpoint,&archive_args).await["error"],"app_set_terminal_not_confirmed");
        peer.reply.lock().unwrap()["boot_id"]=12.into();
        let archived=action(dir.path(),mac,&endpoint,&archive_args).await;
        assert_eq!(archived["result"],"archived","{archived}");assert!(!receipt_path(dir.path(),mac).exists());
        assert_eq!(peer.posts.load(Ordering::SeqCst),1);
        let next=action(dir.path(),mac,&endpoint,&json!({"op":"prepare","code":"01234567","manifest_path":manifest})).await;
        assert_eq!(next["result"],"prepared","{next}");assert_ne!(next["operation_id"],prepared["operation_id"]);
        *peer.reply.lock().unwrap()=Value::Null;
        let moved=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let new_endpoint=moved.local_addr().unwrap().to_string();
        let new_server=tokio::spawn(async move{axum::serve(moved,app).await.unwrap()});
        let rebound=action(dir.path(),mac,&new_endpoint,&json!({"op":"reconnect","code":"01234567","operation_id":next["operation_id"]})).await;
        assert_eq!(rebound["result"],"pending","{rebound}");assert_eq!(rebound["endpoint"],new_endpoint);
        let cancelled=action(dir.path(),mac,&new_endpoint,&json!({"op":"cancel","code":"01234567","operation_id":next["operation_id"]})).await;
        assert_eq!(cancelled["result"],"abandoned","{cancelled}");assert_eq!(cancelled["reply"]["local_only"],true);
        assert!(!receipt_path(dir.path(),mac).exists());assert_eq!(peer.posts.load(Ordering::SeqCst),1);
        new_server.abort();server.abort();
    }

    #[tokio::test]
    async fn worker_outlives_request_persists_cancel_and_bounds_unknown_wait() {
        use axum::{Router,Json,routing::{get,post},extract::State};
        use std::sync::{Arc,Mutex};
        use sha2::Digest;
        struct Peer {mode:u8,reply:Mutex<Value>,posts:Mutex<Vec<&'static str>>}
        async fn query(State(p):State<Arc<Peer>>)->(axum::http::StatusCode,Json<Value>) {
            if p.mode==2{return (axum::http::StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":"busy"})));}
            (axum::http::StatusCode::OK,Json(p.reply.lock().unwrap().clone()))
        }
        async fn commit(State(p):State<Arc<Peer>>)->Json<Value> {
            p.posts.lock().unwrap().push("commit");let mut r=p.reply.lock().unwrap();r["phase"]="succeeded".into();r["sequence"]=2.into();r["boot_id"]=12.into();
            let digest=r["candidate_manifest"].clone();r["cleanup_pending"]=false.into();r["runtime_reporting"]=true.into();
            r["runtime"]=json!({"complete":true,"boot_state":"started","health_state":"healthy","manifest":digest,"version":1,"started_count":0,"running":[]});Json(r.clone())
        }
        async fn cancel(State(p):State<Arc<Peer>>)->Json<Value> {
            p.posts.lock().unwrap().push("cancel");let mut r=p.reply.lock().unwrap();r["phase"]="cancelled".into();r["sequence"]=2.into();r["cleanup_pending"]=false.into();Json(r.clone())
        }
        for mode in 0..3 {
            let dir=tempfile::tempdir().unwrap();let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let endpoint=listener.local_addr().unwrap().to_string();
            let mac="010203040506";let operation="01000000000000000000000000000000";
            let mut bytes=vec![0u8;64];bytes[..4].copy_from_slice(b"AMF3");bytes[4]=1;bytes[6]=64;bytes[8]=64;bytes[14]=160;bytes[16]=1;
            let hash=sha2::Sha256::digest(&bytes);bytes[32..64].copy_from_slice(&hash);
            let caps=json!({"protocol":3,"device_mac":mac,"phase":"idle","boot_id":11,"runtime_reporting":true,"app_max":8,
                "limits":{"package_bytes":1000,"image_bytes":1000,"heap_bytes":1000,"stack_bytes":1000,"tasks":4,"pages":4,"capabilities":127,"ram_bytes":4096,"resident_bytes":0}});
            let receipt=Receipt::prepare(&endpoint,mac,operation,&bytes,&caps).unwrap();let path=receipt_path(dir.path(),mac);receipt.create(&path).unwrap();
            let peer=Arc::new(Peer{mode,posts:Mutex::new(Vec::new()),reply:Mutex::new(json!({"protocol":3,"device_mac":mac,"operation_id":operation,
                "candidate_manifest":hash.iter().map(|b|format!("{b:02x}")).collect::<String>(),"candidate_version":1,"old_manifest":receipt.expected_manifest,"old_version":0,
                "phase":"receiving","sequence":1,"boot_id":11,"changed_mask":0,"ready_mask":0,"received":[0,0,0,0,0,0,0,0],"chunk_max":1024}))});
            let app=Router::new().route("/api/app/v3/update/result",get(query)).route("/api/app/v3/update/commit",post(commit)).route("/api/app/v3/update/cancel",post(cancel)).with_state(peer.clone());
            let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap()});
            let ctx=Arc::new(crate::platform::ble_close_lock_tests::test_ctx(dir.path(),&[]));
            let args=json!({"op":"run","operation_id":operation,"code":"01234567","budget_secs":1});
            assert_eq!(worker_command(&ctx,mac,&args).await.unwrap()["result"],"running");
            let duplicate=worker_command(&ctx,mac,&args).await.unwrap();assert_eq!(duplicate["active"],true);
            if mode==1 {
                let result=worker_command(&ctx,mac,&json!({"op":"cancel","operation_id":operation})).await.unwrap();assert_eq!(result["result"],"cancelling");
                assert!(cancellation(dir.path(),mac,operation).unwrap());
            }
            let deadline=std::time::Instant::now()+Duration::from_secs(3);
            let final_view=loop {
                let view=worker_command(&ctx,mac,&json!({"op":"worker_status"})).await.unwrap();
                if view["active"]==false{break view;}
                assert!(std::time::Instant::now()<deadline);tokio::time::sleep(Duration::from_millis(10)).await;
            };
            assert_eq!(final_view["status"]["result"],["confirmed","terminal","unknown"][mode as usize],"{final_view}");
            assert_eq!(*peer.posts.lock().unwrap(),match mode {0=>vec!["commit"],1=>vec!["cancel"],_=>Vec::new()});
            assert!(path.exists());assert!(!final_view.to_string().contains("01234567"));
            if mode==2 {assert_eq!(final_view["status"]["error"],"budget_exhausted");}
            if mode==1 {
                // Fresh application state must retain cancellation intent from disk.
                let restarted=Arc::new(crate::platform::ble_close_lock_tests::test_ctx(dir.path(),&[]));
                assert!(cancellation(dir.path(),mac,operation).unwrap());
                assert_eq!(worker_command(&restarted,mac,&args).await.unwrap()["result"],"running");
                loop {let view=worker_command(&restarted,mac,&json!({"op":"worker_status"})).await.unwrap();if view["active"]==false{assert_eq!(view["status"]["result"],"terminal");break;}assert!(std::time::Instant::now()<deadline);tokio::time::sleep(Duration::from_millis(10)).await;}
                assert_eq!(*peer.posts.lock().unwrap(),vec!["cancel"]);
            }
            server.abort();
        }
    }

    #[tokio::test]
    #[ignore="run through the firmware --rust-worker loopback C++ harness"]
    async fn production_cpp_worker_collection() {
        use std::sync::Arc;
        let endpoint=std::env::var("AP01_HTTP_ENDPOINT").unwrap();let address:std::net::SocketAddr=endpoint.parse().unwrap();assert!(address.ip().is_loopback());
        let bytes=std::fs::read(std::env::var("AP01_MANIFEST").unwrap()).unwrap();let manifest=Manifest::parse(&bytes).unwrap();
        let mac="010203040506";let operation=format!("{:032x}",manifest.sequence);
        let (status,value)=ap01_app_set_request(&endpoint,mac,"01234567",&operation,Action::Status,&[],Duration::from_secs(5)).await.unwrap();assert_eq!(status,200);
        let dir=tempfile::tempdir().unwrap();let receipt=Receipt::prepare(&endpoint,mac,&operation,&bytes,&value).unwrap();receipt.create(&receipt_path(dir.path(),mac)).unwrap();
        let mut paths=vec![std::env::var("AP01_PACKAGE").unwrap()];if let Ok(path)=std::env::var("AP01_PEER"){paths.push(path);}
        let ctx=Arc::new(crate::platform::ble_close_lock_tests::test_ctx(dir.path(),&[]));
        let result=worker_command(&ctx,mac,&json!({"op":"run","operation_id":operation,"code":"01234567","package_paths":paths,"budget_secs":120})).await.unwrap();
        assert_eq!(result["result"],"running","{result}");
        let deadline=std::time::Instant::now()+Duration::from_secs(125);
        loop {
            let view=worker_command(&ctx,mac,&json!({"op":"worker_status"})).await.unwrap();
            if view["active"]==false {
                let expected=std::env::var("AP01_EXPECT_PHASE").unwrap();
                assert_eq!(view["status"]["result"],if expected=="succeeded"{"confirmed"}else{"terminal"},"{view}");
                assert_eq!(view["status"]["reply"]["phase"],expected);
                std::fs::write(std::env::var("AP01_RESULT").unwrap(),serde_json::to_vec_pretty(&view["status"]["reply"]).unwrap()).unwrap();break;
            }
            assert!(std::time::Instant::now()<deadline,"{view}");tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    #[ignore="run through the firmware --rust-ui loopback C++ harness"]
    async fn production_ui_rust_cpp_collection() {
        use axum::{Router,Json,routing::post,extract::{State,DefaultBodyLimit},http::HeaderMap};
        use std::sync::Arc;
        struct Rpc {ctx:Arc<crate::AppCtx>,endpoint:String,token:String}
        async fn invoke(State(rpc):State<Arc<Rpc>>,headers:HeaderMap,Json(request):Json<Value>)->Json<Value> {
            if headers.get("x-test-token").and_then(|h|h.to_str().ok())!=Some(rpc.token.as_str()) {return Json(json!({"result":"error","error":"test_auth"}));}
            let body=&request["args"]["body"];let mac="010203040506";
            if request["name"]!="platform_ap01_app_update" || body["mac"]!=mac {return Json(json!({"result":"error","error":"test_identity"}));}
            // Only the Tauri IPC transport and fixed test endpoint are adapted.
            // Real application control, staging, receipts and worker run below.
            if let Some(value)=worker_command(&rpc.ctx,mac,body).await{return Json(value);}
            let _guard=rpc.ctx.ap01.operation.lock().await;
            Json(action(crate::platform::service(&rpc.ctx).data_root(),mac,&rpc.endpoint,body).await)
        }
        let endpoint=std::env::var("AP01_HTTP_ENDPOINT").unwrap();let address:std::net::SocketAddr=endpoint.parse().unwrap();assert!(address.ip().is_loopback());
        let dir=tempfile::tempdir().unwrap();let ctx=Arc::new(crate::platform::ble_close_lock_tests::test_ctx(dir.path(),&[]));
        let token=uuid::Uuid::new_v4().simple().to_string();let rpc=Arc::new(Rpc{ctx,endpoint,token:token.clone()});
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let url=format!("http://{}/invoke",listener.local_addr().unwrap());
        let app=Router::new().route("/invoke",post(invoke)).layer(DefaultBodyLimit::max(24*1024*1024)).with_state(rpc);
        let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap()});
        let script=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/test-ap01-app-ui-rpc.mjs");
        let child=tokio::task::spawn_blocking(move||std::process::Command::new("node").arg(script)
            .env("AP01_UI_RPC",url).env("AP01_UI_TOKEN",token).status().unwrap()).await.unwrap();
        server.abort();assert!(child.success(),"production UI/Rust cross-test failed");
    }

}
