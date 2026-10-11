// Explicit, bounded hardware acceptance driver. Uses real BLE and HTTP only.
use anyhow::{ensure,Result};
use serde_json::{json,Value};
use std::{collections::BTreeMap,fs,time::{Duration,Instant}};
use bridge_core::{coordinator::Coordinator,platform::model::{DeviceCapabilities,SourceSnapshot,SnapshotField}};
const MAC:&str="7C4FADB93408";
struct Evidence { events:Vec<Value>, path:String }
impl std::ops::Deref for Evidence {type Target=Vec<Value>;fn deref(&self)->&Vec<Value>{&self.events}}
impl std::ops::DerefMut for Evidence {fn deref_mut(&mut self)->&mut Vec<Value>{&mut self.events}}
impl Drop for Evidence {fn drop(&mut self){let _=fs::write(&self.path,serde_json::to_vec_pretty(&self.events).unwrap());}}
fn read(path:&str)->Value {serde_json::from_str(fs::read_to_string(path).unwrap().trim_start_matches('\u{feff}')).unwrap()}
#[tokio::main]
async fn main()->Result<()> {
    let mode=std::env::args().nth(1).unwrap_or("cross".into());
    ensure!(mode=="cross" || mode=="sleep","invalid case");
    let root=std::env::current_dir()?.parent().unwrap().to_owned();
    std::env::set_current_dir(&root)?;
    let input=read("artifacts/note4-83-live-input.private.json");
    let registry=read("bridge/target/debug/data/platform/state.json");
    let record=registry["devices"].as_array().unwrap().iter().find(|r|r["identity"]["device_mac"]==MAC).unwrap();
    let ip=record["identity"]["ip"].as_str().unwrap();
    let token=read("bridge/target/debug/data/bridge-app.json")["token"].as_str().unwrap().to_owned();
    let owner=input["status"]["owner_id"].as_str().unwrap();
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let mut events=Evidence {events:Vec::new(),path:format!("artifacts/note4-83-live-{mode}.private.json")};
    let until=Instant::now()+Duration::from_secs(220);
    let mut link=loop {
        ensure!(Instant::now()<until,"natural BLE rendezvous not reached in 220s");
        match bridge_ble::DeviceConnection::connect(MAC,&token,owner).await {
            Ok(Some(link))=>break link,
            Ok(None)=>{},
            Err(_)=>{},
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    };
    let ble=link.command("status",json!({})).await?;
    let operation_token=read("bridge/target/debug/data/device-token-7C4FADB93408.json")["token"].as_str().unwrap().to_owned();
    let claim=link.command("claim",json!({"operation_token":operation_token,"name":"Codex Status Bridge","host":bridge_ble::lan_ip(),"port":8765,"lease_s":300})).await?;
    ensure!(claim["result"]=="applied" && claim["owner_id"]==owner,"claim rejected");
    let plan_id=ble["power"]["plan_id"].as_u64().unwrap_or(0)+1;
    let seq=input["status"]["applied_seq"].as_u64().unwrap_or(0)+if mode=="cross" {1}else{2};
    events.push(json!({"stage":"natural_ble_status","status":ble,"at":bridge_core::now_secs()}));
    let plan=json!({"plan_id":plan_id,"mode":if mode=="cross" {"light"}else{"sleep"},
        "light_duration_s":if mode=="cross" {60}else{0},"rendezvous_period_s":60,"need_http":true});
    let ack=link.command("plan",plan.clone()).await?; ensure!(ack["result"]=="applied","plan rejected: {ack}");
    events.push(json!({"stage":"plan_ack","plan":plan,"ack":ack,"at":bridge_core::now_secs()}));
    let open=json!({"reason":"data","data_seq":seq,"open_id":format!("{:032x}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()),
        "wake_generation":ble["wake_generation"],"wake_seq":ble["wake_seq"]});
    let ack=link.command("sync_open",open.clone()).await?; ensure!(ack["result"]=="applied","sync_open rejected: {ack}");
    events.push(json!({"stage":"sync_open_ack","request":open,"ack":ack,"at":bridge_core::now_secs()}));
    let replay=link.command("sync_open",open).await?; ensure!(replay["result"]=="applied","replay rejected");
    events.push(json!({"stage":"same_open_replay","ack":replay,"at":bridge_core::now_secs()}));
    let route=std::net::UdpSocket::bind("0.0.0.0:0")?; route.connect(format!("{ip}:80"))?;
    let ack=link.command("network_start",json!({"plan_id":plan_id,"bridge_addr":route.local_addr()?.ip().to_string(),"bridge_port":8765,"origin_request_no":plan_id})).await?;
    ensure!(ack["result"]=="accepted","network start rejected: {ack}");
    events.push(json!({"stage":"network_start_ack","ack":ack,"at":bridge_core::now_secs()}));
    link.close_bounded(Duration::from_secs(2)).await;
    let http_until=Instant::now()+Duration::from_secs(25);
    let status=loop {
        if let Ok(s)=bridge_core::device_client::status(ip,&token,Duration::from_secs(2)) {break s}
        ensure!(Instant::now()<http_until,"HTTP not reachable");
        tokio::time::sleep(Duration::from_millis(400)).await;
    };
    ensure!(status["device_mac"].as_str().unwrap().replace(':',"")==MAC,"wrong device");
    ensure!(status["sync"]["phase"]=="WIFI_SYNC_ONCE","missing temporary window");
    events.push(json!({"stage":"temporary_http_before","status":status,"at":bridge_core::now_secs()}));
    if mode=="cross" {
        let left=status["power"]["remaining_s"].as_u64().unwrap();
        ensure!(left>0 && left<=60,"invalid formal remaining");
        tokio::time::sleep(Duration::from_secs(left+1)).await;
    }
    let current=bridge_core::device_client::status(ip,&token,Duration::from_secs(3))?;
    ensure!(current["power"]["remaining_s"]==0,"formal Light still active");
    ensure!(current["sync"]["phase"]=="WIFI_SYNC_ONCE" && current["sync"]["remaining_ms"].as_u64().unwrap()>5000,"temporary window cut off");
    events.push(json!({"stage":"formal_expired_http_alive","status":current,"at":bridge_core::now_secs()}));
    let mut c=Coordinator::new(MAC,serde_json::from_value::<DeviceCapabilities>(record["capabilities"].clone())?);
    c.new_context("live-acceptance",bridge_core::now_secs());
    c.set_contract("codex-status-a",serde_json::from_value(input["status"]["data_contract"]["requirements"].clone())?,BTreeMap::new());
    let p=&input["probe"];
    let mut snapshot=SourceSnapshot::empty_good("codex");
    for f in p["fields"].as_array().unwrap() {snapshot.fields.insert(f["field"].as_str().unwrap().to_owned(),SnapshotField {
        value:f["value"].clone(),quality:serde_json::from_value(f["quality"].clone())?,observed_at:p["observed_at"].as_u64().unwrap(),valid_until:p["valid_until"].as_u64().unwrap()});}
    c.note_snapshot(snapshot);
    let full=c.snapshot().unwrap();
    let fields:Vec<Value>=c.requirements.iter().map(|r| {let f=full.fields.get(&r.field).unwrap();json!({"i":r.index,"k":r.field,"v":f.value,"q":f.quality})}).collect();
    let data=json!({"bridge_id":owner,"active_context_id":current["active_context_id"],"seq":seq,
        "crc":format!("{:08x}",bridge_core::coordinator::data_fields_crc(&fields)),"fields":fields});
    let ack=bridge_core::device_client::data(ip,&token,MAC,&data,Duration::from_secs(10))?;
    ensure!(ack["result"]=="applied","Data not applied: {ack}");
    events.push(json!({"stage":"full_data_ack","ack":ack,"seq":seq,"field_count":data["fields"].as_array().unwrap().len(),"payload_bytes":serde_json::to_vec(&data)?.len(),"at":bridge_core::now_secs()}));
    fs::write(&events.path,serde_json::to_vec_pretty(&events.events)?)?;
    println!("{}",json!({"case":mode,"result":"applied","seq":seq,"plan_id":plan_id,"events":events.len()}));
    Ok(())
}
