//! One bounded advancement of an existing durable collection receipt. The app
//! worker controls polling/cancellation; no background task or automatic retry.
use anyhow::{ensure,Context,Result};
use serde::Serialize;
use serde_json::Value;
use std::{path::Path,time::{Duration,Instant}};
use crate::{ap01_app_set::Manifest,ap01_app_set_receipt::{Receipt,Phase},device_client::{ap01_app_set_request,Ap01AppSetAction as Action}};

#[derive(Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum State {Pending,Unknown,Confirmed,Terminal,Abandoned}
#[derive(Debug,Serialize)]
pub struct Step {pub state:State,pub reply:Value}
fn hex(bytes:&[u8])->String {bytes.iter().map(|b|format!("{b:02x}")).collect()}

fn confirmed(r:&Receipt,m:&Manifest,v:&Value)->Result<bool> {
    let runtime=&v["runtime"];
    if v["cleanup_pending"]!=false || v["runtime_reporting"]!=true || runtime["complete"]!=true || runtime["boot_state"]!="started" || runtime["health_state"]!="healthy" || v["boot_id"].as_u64()==Some(r.original_boot) {return Ok(false);}
    ensure!(runtime["manifest"].as_str()==Some(hex(&m.digest).as_str()) && runtime["version"].as_u64()==Some(m.sequence),"app_set_running_manifest_mismatch");
    let apps=runtime["running"].as_array().context("app_set_running_list")?;
    ensure!(apps.len()==m.apps.len() && runtime["started_count"].as_u64()==Some(m.apps.len() as u64),"app_set_running_count");
    let mut expected:Vec<_>=m.apps.iter().enumerate().collect();expected.sort_by_key(|(_,app)|app.order);
    for (actual,(i,app)) in apps.iter().zip(expected) {
        ensure!(actual["index"].as_u64()==Some(i as u64) && actual["app_id"].as_str()==Some(app.id.as_str()) && actual["version"].as_str()==Some(app.version.as_str()) && actual["digest"].as_str()==Some(hex(&app.digest).as_str()),"app_set_running_app_mismatch");
        if actual["health"].as_i64()!=Some(0){return Ok(false);}
    }
    Ok(true)
}

fn next(r:&Receipt,packages:&[Vec<u8>],cancel:bool,chunk_max:u64)->Result<Option<(Action,Vec<u8>)>> {
    let p=r.progress.as_ref().context("app_set_missing_progress")?;
    if p.phase.terminal(){return Ok(None);}
    if cancel {return Ok(Some((Action::Cancel,Vec::new())));}
    match p.phase {
        Phase::Receiving => {
            let m=r.manifest()?;let changed=m.changed_packages(packages,p.changed_mask)?;
            for (i,bytes) in changed {
                if p.ready_mask&(1<<i)!=0 {continue;}
                let at=p.received[i] as usize;
                if at==bytes.len(){return Ok(Some((Action::Ready{index:i as u8},Vec::new())));}
                ensure!((1..=1024).contains(&chunk_max),"app_set_chunk_limit");
                return Ok(Some((Action::Chunk{index:i as u8,offset:at as u32},bytes[at..bytes.len().min(at+chunk_max as usize)].to_vec())));
            }
            Ok(Some((Action::Commit,Vec::new())))
        }
        Phase::Ready => Ok(Some((Action::Commit,Vec::new()))),
        _ => Ok(None),
    }
}
fn received(r:&mut Receipt,path:&Path,status:u16,value:Value)->Result<Step> {
    received_at(r,path,&r.endpoint.clone(),status,value)
}
fn received_at(r:&mut Receipt,path:&Path,endpoint:&str,status:u16,value:Value)->Result<Step> {
    if status==503 {return Ok(Step{state:State::Unknown,reply:value});}
    if status==409 && (value["error_code"]=="write_drain_pending" || value["error"]=="maintenance_busy") {return Ok(Step{state:State::Pending,reply:value});}
    ensure!(matches!(status,200|202),"app_set_http_rejected: {status}");
    r.acknowledge_at(path,endpoint,&value).map_err(|error|error.context(crate::rendezvous::CommandFailure{outcome:crate::rendezvous::DispatchOutcome::AckUnknown,stage:"app_set_receipt",reason:"receipt_unconfirmed"}))?;
    let phase=r.progress.as_ref().unwrap().phase;
    let state=if phase==Phase::Succeeded {if confirmed(r,&r.manifest()?,&value)? {State::Confirmed}else{State::Pending}}
        else if phase.terminal(){State::Terminal}else{State::Pending};
    Ok(Step{state,reply:value})
}

fn reload(r:&mut Receipt,path:&Path)->Result<()> {
    let stored=Receipt::load(path,&r.endpoint,&r.device_mac)?;
    ensure!(stored.operation_id==r.operation_id && stored.begin_body()?==r.begin_body()?,"app_set_receipt_replaced");*r=stored;Ok(())
}
/// Observe the original operation only. Unlike advance, 404 never starts begin.
pub async fn query(r:&mut Receipt,path:&Path,code:&str,budget:Duration)->Result<Step> {
    reload(r,path)?;
    let (status,value)=ap01_app_set_request(&r.endpoint,&r.device_mac,code,&r.operation_id,Action::Result,&[],budget).await?;
    received(r,path,status,value)
}

/// Query only at an explicitly rediscovered endpoint. No begin/commit is sent;
/// wrong identity, stale progress or storage errors leave the old fence intact.
pub async fn reconnect(r:&mut Receipt,path:&Path,endpoint:&str,code:&str,budget:Duration)->Result<Step> {
    reload(r,path)?;let started=Instant::now();
    let (status,value)=ap01_app_set_request(endpoint,&r.device_mac,code,&r.operation_id,Action::Result,&[],budget).await?;
    if status==404 && r.progress.is_none() {
        let (status,value)=ap01_app_set_request(endpoint,&r.device_mac,code,&r.operation_id,Action::Status,&[],budget.saturating_sub(started.elapsed())).await?;
        if status==503{return Ok(Step{state:State::Unknown,reply:value});}
        ensure!(status==200,"app_set_status_rejected: {status}");
        r.relocate_unstarted(path,endpoint,&value)?;
        return Ok(Step{state:State::Pending,reply:serde_json::json!({"operation_id":r.operation_id,"device_mac":r.device_mac,"local_only":true,"reason":"unacknowledged_operation_absent"})});
    }
    received_at(r,path,endpoint,status,value)
}

/// Always query first, including after a transport/persistence error. Never
/// generate an operation ID here. Credentials are request-local, not persisted.
pub async fn advance(r:&mut Receipt,path:&Path,code:&str,packages:&[Vec<u8>],cancel:bool,budget:Duration)->Result<Step> {
    reload(r,path)?;
    let started=Instant::now();
    let (status,value)=ap01_app_set_request(&r.endpoint,&r.device_mac,code,&r.operation_id,Action::Result,&[],budget).await?;
    let planned=if status==404 {
        ensure!(r.progress.is_none(),"app_set_acknowledged_receipt_expired");
        if cancel {
            // A bare 404 is not identity proof. Authenticate this MAC before
            // releasing an unacknowledged local receipt; never fabricate progress.
            let remaining=budget.saturating_sub(started.elapsed());
            let (status,value)=ap01_app_set_request(&r.endpoint,&r.device_mac,code,&r.operation_id,Action::Status,&[],remaining).await?;
            if status==503{return Ok(Step{state:State::Unknown,reply:value});}
            ensure!(status==200,"app_set_status_rejected: {status}");
            r.archive_unstarted(path)?;
            return Ok(Step{state:State::Abandoned,reply:serde_json::json!({"operation_id":r.operation_id,"device_mac":r.device_mac,"local_only":true,"reason":"unacknowledged_operation_absent"})});
        }
        Some((Action::Begin,r.begin_body()?))
    }else{
        let step=received(r,path,status,value)?;
        if !matches!(status,200|202) || step.state!=State::Pending || r.progress.as_ref().is_some_and(|p|p.phase.terminal()) {return Ok(step);}
        let action=next(r,packages,cancel,step.reply["chunk_max"].as_u64().unwrap_or(0))?;
        if action.is_none(){return Ok(step);}action
    };
    let (action,body)=planned.unwrap();let remaining=budget.saturating_sub(started.elapsed());
    let (status,value)=ap01_app_set_request(&r.endpoint,&r.device_mac,code,&r.operation_id,action,&body,remaining).await?;
    received(r,path,status,value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt,AsyncWriteExt};
    use sha2::Digest;
    use crate::rendezvous::{CommandFailure,DispatchOutcome};
    fn manifest()->Vec<u8> {let mut b=vec![0;64];b[..4].copy_from_slice(b"AMF3");b[4]=1;b[6]=64;b[8]=64;b[14]=160;b[16]=1;let hash=sha2::Sha256::digest(&b);b[32..64].copy_from_slice(&hash);b}
    fn status()->Value {serde_json::json!({"protocol":3,"device_mac":"AABBCCDDEEFF","phase":"idle","boot_id":11,"runtime_reporting":true,"app_max":8,
        "limits":{"package_bytes":1000,"image_bytes":1000,"heap_bytes":1000,"stack_bytes":1000,"tasks":4,"pages":4,"capabilities":127,"ram_bytes":4096,"resident_bytes":0}})}
    #[tokio::test]
    async fn lost_commit_queries_original_receipt_and_verifies_runtime() {
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let endpoint=listener.local_addr().unwrap().to_string();
        let mut receipt=Receipt::prepare(&endpoint,"AABBCCDDEEFF","01000000000000000000000000000000",&manifest(),&status()).unwrap();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("receipt.json");receipt.create(&path).unwrap();
        let copy=receipt.clone();let server=tokio::spawn(async move {
            let mut requests=Vec::new();
            for n in 0..6 {
                let (mut stream,_)=listener.accept().await.unwrap();let mut raw=Vec::new();
                loop {let mut bytes=[0;1024];let got=stream.read(&mut bytes).await.unwrap();assert!(got>0);raw.extend_from_slice(&bytes[..got]);
                    if let Some(at)=raw.windows(4).position(|w|w==b"\r\n\r\n") {
                        let head=String::from_utf8_lossy(&raw[..at]);let length=head.lines().find_map(|l|l.strip_prefix("Content-Length: ")).unwrap().parse::<usize>().unwrap();if raw.len()>=at+4+length{break;}
                    }
                }
                requests.push(String::from_utf8_lossy(&raw).lines().next().unwrap().to_owned());
                if n==3 {continue;} // commit executed, socket closed before any ACK.
                let phase=if n==5 {"succeeded"}else if n==4 {"switch_pending"}else{"receiving"};
                let mut value=serde_json::json!({"protocol":3,"device_mac":copy.device_mac,"operation_id":copy.operation_id,
                    "candidate_manifest":hex(&copy.manifest().unwrap().digest),"candidate_version":1,"old_manifest":copy.expected_manifest,"old_version":0,
                    "phase":phase,"sequence":if n>=4 {n} else {1},"boot_id":if n==5 {12}else{11},"changed_mask":0,"ready_mask":0,"received":[0,0,0,0,0,0,0,0],"chunk_max":1024});
                if n==5 {value["cleanup_pending"]=false.into();value["runtime_reporting"]=true.into();value["runtime"]=serde_json::json!({"manifest":hex(&copy.manifest().unwrap().digest),"version":1,"complete":true,"boot_state":"started","health_state":"healthy","started_count":0,"running":[]});}
                let body=serde_json::to_vec(&value).unwrap();let header=format!("HTTP/1.1 {} Reply\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",if n==0 {404}else{200},body.len());stream.write_all(header.as_bytes()).await.unwrap();stream.write_all(&body).await.unwrap();
            }
            requests
        });
        assert_eq!(advance(&mut receipt,&path,"01234567",&[],false,Duration::from_secs(2)).await.unwrap().state,State::Pending);
        let error=advance(&mut receipt,&path,"01234567",&[],false,Duration::from_secs(2)).await.unwrap_err();assert!(matches!(error.downcast_ref::<CommandFailure>().unwrap().outcome,DispatchOutcome::AckUnknown));
        receipt=Receipt::load(&path,&endpoint,"AABBCCDDEEFF").unwrap();
        assert_eq!(advance(&mut receipt,&path,"01234567",&[],false,Duration::from_secs(2)).await.unwrap().state,State::Pending);
        assert_eq!(advance(&mut receipt,&path,"01234567",&[],false,Duration::from_secs(2)).await.unwrap().state,State::Confirmed);
        let requests=server.await.unwrap();assert_eq!(requests.iter().filter(|p|p.contains("update/commit?")).count(),1);assert!(requests[4..].iter().all(|p|p.starts_with("GET ")));
    }
    #[test]
    #[ignore="requires AP01_MANIFEST, AP01_PACKAGE and AP01_PEER real artifacts"]
    fn real_package_actions_and_complete_runtime() {
        let read=|name|std::fs::read(std::env::var(name).unwrap()).unwrap();let bytes=read("AP01_MANIFEST");let m=Manifest::parse(&bytes).unwrap();
        let packages=vec![read("AP01_PACKAGE"),read("AP01_PEER")];let mut caps=status();caps["limits"]=serde_json::to_value(&m.resources).unwrap();
        caps["limits"]["ram_bytes"]=(m.resources.image_bytes+m.resources.heap_bytes+m.resources.stack_bytes+4096).into();caps["limits"]["resident_bytes"]=4096.into();
        let mut r=Receipt::prepare("127.0.0.1:18080","AABBCCDDEEFF","01000000000000000000000000000000",&bytes,&caps).unwrap();
        r.progress=Some(crate::ap01_app_set_receipt::Progress{phase:Phase::Receiving,sequence:1,boot_id:11,changed_mask:1,ready_mask:0,received:[0;8]});
        let (action,body)=next(&r,&packages[..1],false,1024).unwrap().unwrap();assert!(matches!(action,Action::Chunk{index:0,offset:0}));assert_eq!(body,packages[0][..1024]);
        assert!(next(&r,&[],false,1024).is_err());
        r.progress.as_mut().unwrap().received[0]=packages[0].len() as u32;
        assert!(matches!(next(&r,&packages[..1],false,1024).unwrap().unwrap().0,Action::Ready{index:0}));
        r.progress.as_mut().unwrap().ready_mask=1;assert!(matches!(next(&r,&packages[..1],false,1024).unwrap().unwrap().0,Action::Commit));
        r.progress.as_mut().unwrap().phase=Phase::SwitchPending;assert!(next(&r,&[],false,1024).unwrap().is_none());
        assert!(matches!(next(&r,&[],true,1024).unwrap().unwrap().0,Action::Cancel));
        let mut apps:Vec<_>=m.apps.iter().enumerate().collect();apps.sort_by_key(|(_,a)|a.order);
        let mut value=serde_json::json!({"cleanup_pending":false,"runtime_reporting":true,"boot_id":12,
            "runtime":{"complete":true,"boot_state":"started","health_state":"healthy","manifest":hex(&m.digest),"version":m.sequence,"started_count":2,
            "running":apps.iter().map(|(i,a)|serde_json::json!({"index":i,"app_id":a.id,"version":a.version,"digest":hex(&a.digest),"health":0})).collect::<Vec<_>>()}});
        assert!(confirmed(&r,&m,&value).unwrap());value["boot_id"]=11.into();assert!(!confirmed(&r,&m,&value).unwrap());value["boot_id"]=12.into();
        value["runtime"]["running"][1]["health"]=Value::Null;assert!(!confirmed(&r,&m,&value).unwrap());value["runtime"]["running"][1]["health"]=0.into();
        value["runtime"]["running"][1]["digest"]="bad".into();assert!(confirmed(&r,&m,&value).is_err());
    }

    /// Invoked by the firmware cross-language harness; endpoint is loopback only.
    #[tokio::test]
    #[ignore="requires AP01_HTTP_ENDPOINT, AP01_MANIFEST, AP01_PACKAGE, AP01_RECEIPT and AP01_RESULT"]
    async fn production_cpp_http_collection() {
        let endpoint=std::env::var("AP01_HTTP_ENDPOINT").unwrap();
        let address:std::net::SocketAddr=endpoint.parse().unwrap();assert!(address.ip().is_loopback());
        let bytes=std::fs::read(std::env::var("AP01_MANIFEST").unwrap()).unwrap();
        let mut packages=vec![std::fs::read(std::env::var("AP01_PACKAGE").unwrap()).unwrap()];
        if let Ok(path)=std::env::var("AP01_PEER"){packages.push(std::fs::read(path).unwrap());}
        let path=std::path::PathBuf::from(std::env::var("AP01_RECEIPT").unwrap());
        let operation=format!("{:032x}",Manifest::parse(&bytes).unwrap().sequence);
        let mac="010203040506";let budget=Duration::from_secs(5);
        let (status,initial)=ap01_app_set_request(&endpoint,mac,"01234567",&operation,Action::Status,&[],budget).await.unwrap();assert_eq!(status,200);
        let mut receipt=Receipt::prepare(&endpoint,mac,&operation,&bytes,&initial).unwrap();receipt.create(&path).unwrap();
        let deadline=Instant::now()+Duration::from_secs(120);let mut unknown=0;
        loop {
            assert!(Instant::now()<deadline,"cross-language transaction exceeded deadline");
            match advance(&mut receipt,&path,"01234567",&packages,false,budget).await {
                Ok(step) if matches!(step.state,State::Confirmed|State::Terminal) => {
                    let expected=std::env::var("AP01_EXPECT_PHASE").unwrap();assert_eq!(step.reply["phase"],expected);
                    assert_eq!(step.state,if expected=="succeeded" {State::Confirmed}else{State::Terminal});
                    assert_eq!(unknown,1,"expected exactly one injected lost response");
                    std::fs::write(std::env::var("AP01_RESULT").unwrap(),serde_json::to_vec_pretty(&step.reply).unwrap()).unwrap();break;
                }
                Ok(_) => {}
                Err(error) => {
                    assert!(matches!(error.downcast_ref::<CommandFailure>().map(|e|&e.outcome),Some(DispatchOutcome::AckUnknown)),"{error:#}");
                    unknown+=1;assert!(unknown<=1,"unexpected repeated transport failure: {error:#}");
                    receipt=Receipt::load(&path,&endpoint,mac).unwrap();
                }
            }
        }
    }

    #[tokio::test]
    async fn rediscovery_and_unstarted_cancel_only_query_authenticated_identity() {
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let endpoint=listener.local_addr().unwrap().to_string();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("active.json");
        let mut r=Receipt::prepare("127.0.0.1:1","AABBCCDDEEFF","01000000000000000000000000000000",&manifest(),&status()).unwrap();r.create(&path).unwrap();
        let original=std::fs::read(&path).unwrap();
        let good=serde_json::json!({"protocol":3,"device_mac":r.device_mac,"operation_id":r.operation_id,
            "candidate_manifest":hex(&r.manifest().unwrap().digest),"candidate_version":1,"old_manifest":r.expected_manifest,"old_version":0,
            "phase":"receiving","sequence":1,"boot_id":12,"changed_mask":0,"ready_mask":0,"received":[0,0,0,0,0,0,0,0],"chunk_max":1024});
        let mut wrong_mac=good.clone();wrong_mac["device_mac"]="000000000000".into();
        let mut wrong_manifest=good.clone();wrong_manifest["candidate_manifest"]="00".repeat(32).into();
        let mut identity=status();identity["boot_id"]=22.into();let mut other=identity.clone();other["device_mac"]="000000000000".into();
        let missing=serde_json::json!({"error":"receipt_not_found"});
        let responses=vec![(200,wrong_mac),(200,wrong_manifest),(200,good),(404,missing.clone()),
            (404,missing.clone()),(200,other),(404,missing.clone()),(200,identity.clone()),
            (404,missing.clone()),(503,serde_json::json!({"error":"busy"})),(404,missing),(200,identity)];
        let server=tokio::spawn(async move {
            let mut requests=Vec::new();
            for (status,value) in responses {
                let (mut stream,_)=listener.accept().await.unwrap();let mut bytes=Vec::new();
                while !bytes.windows(4).any(|w|w==b"\r\n\r\n") {
                    let mut buffer=[0;1024];let n=stream.read(&mut buffer).await.unwrap();assert!(n>0);bytes.extend_from_slice(&buffer[..n]);assert!(bytes.len()<8192);
                }
                let line=String::from_utf8_lossy(&bytes).lines().next().unwrap().to_owned();assert!(line.starts_with("GET "));requests.push(line);
                let body=serde_json::to_vec(&value).unwrap();let header=format!("HTTP/1.1 {status} Reply\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
                stream.write_all(header.as_bytes()).await.unwrap();stream.write_all(&body).await.unwrap();
            }
            requests
        });
        let budget=Duration::from_secs(2);
        for _ in 0..2 {assert!(reconnect(&mut r,&path,&endpoint,"01234567",budget).await.is_err());assert_eq!(std::fs::read(&path).unwrap(),original);}
        assert_eq!(reconnect(&mut r,&path,&endpoint,"01234567",budget).await.unwrap().state,State::Pending);
        assert_eq!(r.endpoint,endpoint);assert_eq!(Receipt::load(&path,&endpoint,&r.device_mac).unwrap().operation_id,r.operation_id);
        // A previously acknowledged receipt cannot be discarded on 404.
        assert!(advance(&mut r,&path,"01234567",&[],true,budget).await.is_err());assert!(path.exists());
        let second=dir.path().join("prepared.json");let mut prepared=Receipt::prepare("127.0.0.1:1","AABBCCDDEEFF","02000000000000000000000000000000",&manifest(),&status()).unwrap();prepared.create(&second).unwrap();
        let saved=std::fs::read(&second).unwrap();
        assert!(reconnect(&mut prepared,&second,&endpoint,"01234567",budget).await.is_err());assert_eq!(std::fs::read(&second).unwrap(),saved);
        assert_eq!(reconnect(&mut prepared,&second,&endpoint,"01234567",budget).await.unwrap().state,State::Pending);
        assert_eq!(prepared.endpoint,endpoint);assert_eq!(prepared.original_boot,11);assert!(prepared.progress.is_none());
        assert_eq!(advance(&mut prepared,&second,"01234567",&[],true,budget).await.unwrap().state,State::Unknown);assert!(second.exists());
        let result=advance(&mut prepared,&second,"01234567",&[],true,budget).await.unwrap();assert_eq!(result.state,State::Abandoned);assert_eq!(result.reply["local_only"],true);assert!(!second.exists());
        let archived=second.with_file_name(format!("prepared.json.{}.abandoned",prepared.operation_id));
        assert!(Receipt::load_local(&archived,&prepared.device_mac).unwrap().progress.is_none());
        let requests=server.await.unwrap();assert_eq!(requests.len(),12);assert!(requests.iter().all(|r|r.contains("/update/result?")||r.contains("/api/app/v3/status ")));
    }

}
