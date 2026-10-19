//! AP01 transport/UI host port. Binding, Bundle installation, template execution
//! and Data decisions execute the shared C++ cores. Only the socket, filesystem
//! and NuttX hostfs display adapter are simulated here; no physical ROM claim.
use super::Options;
use anyhow::{ensure,Context,Result};
use axum::{body::{to_bytes,Body},extract::{Request,State,DefaultBodyLimit},http::{header,StatusCode},
    response::{IntoResponse,Response},routing::{get,post},Json,Router};
use bridge_core::{ap01::BINDING_METHOD,compact_frame::{Header,RequestFence,Admission,CONTENT_TYPE,MAX_HTTP_FRAME},
    platform::model::{DeviceCapabilities,FIRMWARE_TARGET_AP01}};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,fs,io::Write,net::SocketAddr,path::{Path,PathBuf},sync::{Arc,Mutex},time::{Instant,Duration}};

#[derive(Default)]
struct Session { id:[u8;16],open:[u8;16],fence:RequestFence,last_status:u16 }
struct Device {
    mac:String,boot:u64,nonce:String,start:Instant,epoch:u64,
    bindings:bridge_render::ap01::Pairing,bundle:bridge_render::SimulatorBundle,
    sessions:BTreeMap<String,Session>,context_counter:u64,session_counter:u64,
    ui_root:Option<PathBuf>,frame_generation:u64,displayed_generation:u64,
    frame_crc:u32,frame_context:String,event:u64,ui_status:String,
    http_port:u16,maintenance_port:u16,business_online:bool,
}
type Shared=Arc<Mutex<Device>>;
fn fail(error:&str,stage:&str)->Value {json!({"result":"rejected","error":error,"stage":stage})}
fn atomic_write(path:&Path,bytes:&[u8])->Result<()> {
    let temporary=path.with_extension("pending");
    let mut file=fs::File::create(&temporary)?;file.write_all(bytes)?;file.sync_all()?;drop(file);
    fs::rename(temporary,path)?;Ok(())
}
impl Device {
    fn now(&self)->u64 {self.start.elapsed().as_millis().min(u64::MAX as u128) as u64}
    fn context(&mut self)->String {self.context_counter+=1;format!("{:016x}{:016x}",self.boot,self.context_counter)}
    fn status(&mut self)->Result<Value> {
        let now=self.now();let mut value=self.bundle.status()?;
        let mut caps=serde_json::to_value(DeviceCapabilities::ap01())?;
        for key in ["max_bundle_bytes","free_bytes","bundle_storage_protocol","bundle_compression","max_bundle_decoded_bytes"] {
            caps[key]=value[key].clone();
        }
        for (key,item) in caps.as_object().unwrap(){value[key]=item.clone();}
        value["maintenance_http_port"]=json!(self.maintenance_port);
        value["result"]=json!("ok");value["mac"]=json!(self.mac);value["device_mac"]=json!(self.mac);
        value["fw_target"]=json!(FIRMWARE_TARGET_AP01);value["fw"]=json!("ap01-codex-sim/1");
        value["boot_id"]=json!(self.boot);value["uptime_ms"]=json!(now);value["session_nonce"]=json!(self.nonce);
        value["active_context_id"]=value["context"].clone();value["power_mode"]=json!("always_on");
        value["power_plan"]=json!(false);value["binding_method"]=json!(BINDING_METHOD);
        value["owner"]=self.bindings.owner(now);
        value["display_state"]=json!(if self.displayed_generation==self.frame_generation && self.frame_generation>0 {"displayed"}else{"pending"});
        value["frame_generation"]=json!(self.frame_generation);value["displayed_generation"]=json!(self.displayed_generation);
        Ok(value)
    }
    fn publish_ui(&mut self)->Result<()> {
        let now=self.now();let dialog=self.bindings.ui(now)?;
        let status=self.bundle.status()?;
        let context=status["context"].as_str().unwrap_or("").to_owned();
        let frame=self.bundle.frame_bits()?;
        let crc=crc(&frame);
        if !frame.is_empty() && (self.frame_generation==0 || crc!=self.frame_crc || context!=self.frame_context) {
            self.frame_generation+=1;self.frame_crc=crc;self.frame_context=context;
            if let Some(root)=&self.ui_root {atomic_write(&root.join("live-frame.palette4"),&frame)?;}
        }
        let ids=status["template_ids"].as_array().cloned().unwrap_or_default();
        let active=ids.iter().position(|id|id==&status["active_template_id"]).unwrap_or(0);
        let text=format!("AP01UI1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            self.boot,self.frame_generation,self.frame_crc,dialog["pending"].as_bool().unwrap_or(false) as u8,
            dialog["request_id"].as_str().unwrap_or("-"),dialog["revision"].as_u64().unwrap_or(0),
            dialog["display_code"].as_str().unwrap_or("----"),dialog["bridge_name"].as_str().unwrap_or("-"),
            dialog["remaining_s"].as_u64().unwrap_or(0),ids.len(),active);
        if text!=self.ui_status {
            if let Some(root)=&self.ui_root {atomic_write(&root.join("live-ui.txt"),text.as_bytes())?;}
            self.ui_status=text;
        }
        Ok(())
    }
    fn ui_events(&mut self)->Result<()> {
        let Some(root)=self.ui_root.clone() else {return Ok(())};
        let path=root.join("live-events.txt");
        let text=match fs::read_to_string(&path){Ok(text) if text.len()<=8192=>text,_=>return Ok(())};
        for line in text.lines() {
            let mut parts=line.split_whitespace();
            let Some(event)=parts.next().and_then(|v|v.parse::<u64>().ok()) else {continue};
            if event<=self.event {continue;}
            let now=self.now();
            match parts.next().unwrap_or("") {
                "displayed"=>{
                    let request=parts.next().unwrap_or("");let revision=parts.next().and_then(|v|v.parse().ok()).unwrap_or(u32::MAX);
                    self.bindings.displayed(request,revision,event)?;
                },
                "press"=>{self.bindings.button(true,event,now);},
                "release"=>{self.bindings.button(false,event,now);},
                "cancel"=>{
                    let dialog=self.bindings.ui(now)?;
                    // The UI port does not possess a binding token. A physical
                    // cancel has its own direct core callback (added below).
                    if dialog["pending"]==true {self.bindings.cancel_from_ui();}
                },
                "frame"=>{
                    if parts.next().and_then(|v|v.parse::<u64>().ok())==Some(self.frame_generation) {
                        self.displayed_generation=self.frame_generation;
                    }
                },
                "activate"=>{
                    if let Some(index)=parts.next().and_then(|v|v.parse::<usize>().ok()) {
                        let status=self.bundle.status()?;
                        if let Some(id)=status["template_ids"].get(index).and_then(Value::as_str) {
                            let id=id.to_owned();let owner=self.bindings.owner(now);
                            // Physical navigation switches the installed profile
                            // locally and keeps network ownership independent.
                            let context=self.context();
                            let message=json!({"device_mac":self.mac,"bridge_id":owner["id"].as_str().unwrap_or("local"),
                                "request_id":format!("key-{event}"),"template_id":id,"expected_active_context_id":status["context"]});
                            let result=self.bundle.activate(&message.to_string(),now,&context)?;
                            if let Some(root)=&self.ui_root {
                                let applied=result["result"]=="applied";
                                atomic_write(&root.join("live-activate-ack.txt"),format!("{event} {} {index}\n",applied as u8).as_bytes())?;
                            }
                        }
                    }
                },_=>continue,
            }
            self.event=event;
        }
        Ok(())
    }
}
fn crc(bytes:&[u8])->u32 {
    let mut c=!0u32;for &b in bytes {c^=b as u32;for _ in 0..8 {c=(c>>1)^((0u32.wrapping_sub(c&1))&0xedb88320);}}!c}
fn bearer(request:&Request<Body>)->Option<String> {
    let mut headers=request.headers().get_all(header::AUTHORIZATION).iter();
    let value=headers.next()?.to_str().ok()?.strip_prefix("Bearer ")?.to_owned();
    if headers.next().is_some(){None}else{Some(value)}
}

async fn pairing(State(shared):State<Shared>,request:Request<Body>)->Response {
    let token=bearer(&request).unwrap_or_default();let method=request.method().as_str().to_owned();
    let path=request.uri().path().to_owned();
    let bytes=match to_bytes(request.into_body(),4096).await {Ok(value)=>value,Err(_)=>return StatusCode::PAYLOAD_TOO_LARGE.into_response()};
    let body=if bytes.is_empty(){json!({})}else{match serde_json::from_slice(&bytes){Ok(value)=>value,Err(_)=>return (StatusCode::BAD_REQUEST,Json(fail("json","pairing"))).into_response()}};
    let mut device=shared.lock().unwrap();let now=device.now();
    match device.bindings.http(&method,&path,&token,&body,now) {
        Ok((status,reply))=>{
            if device.publish_ui().is_err(){return (StatusCode::SERVICE_UNAVAILABLE,Json(fail("display_unavailable","ui_port"))).into_response();}
            (StatusCode::from_u16(status).unwrap(),Json(reply)).into_response()
        },Err(_)=>(StatusCode::INTERNAL_SERVER_ERROR,Json(fail("pairing_core","pairing"))).into_response(),
    }
}

async fn maintenance_status(State(shared):State<Shared>,request:Request<Body>)->Response {
    let token=bearer(&request).unwrap_or_default();let device=shared.lock().unwrap();
    if device.bindings.principal(&token).is_none(){return (StatusCode::UNAUTHORIZED,Json(fail("unauthorized","maintenance_auth"))).into_response();}
    // Maintenance does not read the Codex data context or generate a PowerPlan.
    Json(json!({"device_mac":device.mac,"fw_target":FIRMWARE_TARGET_AP01,"boot_id":device.boot,
        "service":"ap01-maintenance","business_online":device.business_online,"http_port":device.maintenance_port,
        "firmware_ota":"unsupported_in_host_simulator","app_install":"unsupported_in_host_simulator"})).into_response()
}
fn response(mut status:u16,header:Header,session:[u8;16],value:&Value)->(Response,Vec<u8>) {
    let mut payload=serde_json::to_vec(value).unwrap();
    if payload.len()+48>MAX_HTTP_FRAME {status=500;payload=serde_json::to_vec(&fail("reply_limit","frame")).unwrap();}
    let result=if status<400 {0}else if status==503 {3}else{2};
    let mut bytes=Header{kind:1,op:header.op,result,payload_len:payload.len() as u16,session,
        request_no:header.request_no,context:header.context}.encode().unwrap().to_vec();bytes.extend(payload);
    ((StatusCode::from_u16(status).unwrap(),[(header::CONTENT_TYPE,CONTENT_TYPE)],bytes.clone()).into_response(),bytes)
}
fn operation(path:&str)->Option<u16> {Some(match path {
    "/api/status"=>1,"/api/data"=>2,"/api/plan"=>3,"/claim"=>4,"/api/bundle/begin"=>10,
    "/api/bundle/chunk"=>11,"/api/bundle/commit"=>12,"/api/activate"=>13,_=>return None,
})}
async fn business(State(shared):State<Shared>,request:Request<Body>)->Response {
    let token=bearer(&request).unwrap_or_default();let path=request.uri().path().to_owned();
    let query: BTreeMap<String,String>=form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()).into_owned().collect();
    let operation_token=request.headers().get("X-Device-Token").and_then(|v|v.to_str().ok()).unwrap_or("").to_owned();
    let bytes=match to_bytes(request.into_body(),MAX_HTTP_FRAME).await {Ok(value)=>value,Err(_)=>return StatusCode::PAYLOAD_TOO_LARGE.into_response()};
    let Ok((header,payload))=Header::decode(&bytes,MAX_HTTP_FRAME) else {return (StatusCode::BAD_REQUEST,Json(fail("protocol_mismatch","frame"))).into_response()};
    if header.kind!=0||operation(&path)!=Some(header.op){return response(400,header,header.session,&fail("operation","frame")).0;}
    let mut device=shared.lock().unwrap();
    let Some(principal)=device.bindings.principal(&token) else {return response(401,header,header.session,&fail("unauthorized","binding")).0};
    if header.session==[0;16] {return response(409,header,header.session,&fail("session","frame")).0;}
    let mut session=device.sessions.remove(&principal).unwrap_or_default();
    if header.op==1 && header.session!=session.open {
        device.session_counter+=1;
        let digest=Sha256::digest(format!("{}:{}:{}:{:?}",device.mac,device.boot,device.session_counter,header.session));
        session.id.copy_from_slice(&digest[..16]);session.open=header.session;session.fence=RequestFence::default();
    } else if header.op!=1 && (header.session!=session.id||session.id==[0;16]) {
        let reply=response(409,header,session.id,&fail("session","frame")).0;device.sessions.insert(principal,session);return reply;
    }
    match session.fence.inspect(header.request_no,&bytes) {
        Admission::Replay(reply)=>{
            let reply=(StatusCode::from_u16(session.last_status).unwrap(),[(header::CONTENT_TYPE,CONTENT_TYPE)],reply.to_vec()).into_response();
            device.sessions.insert(principal,session);return reply;
        },
        Admission::Conflict|Admission::Retired=>{
            let reply=response(409,header,session.id,&fail("request_conflict","frame")).0;
            device.sessions.insert(principal,session);return reply;
        },Admission::New=>{},
    }
    let result=dispatch(&mut device,&principal,&token,&operation_token,&path,&query,payload);
    let (mut status,mut value)=result.unwrap_or_else(|_|(500,fail("device_core","runtime")));
    if device.publish_ui().is_err(){status=503;value=fail("display_unavailable","ui_port");}
    if value["display_state"]=="displayed" && device.displayed_generation!=device.frame_generation {
        value["display_state"]=json!("pending");
    }
    let (reply,raw)=response(status,header,session.id,&value);
    session.fence.retain(header.request_no,&bytes,&raw);session.last_status=status;
    device.sessions.insert(principal,session);reply
}
fn dispatch(device:&mut Device,principal:&str,token:&str,operation_token:&str,path:&str,
            query:&BTreeMap<String,String>,payload:&[u8])->Result<(u16,Value)> {
    let now=device.now();device.bundle.set_wall_secs(((device.epoch+now)/1000) as i64)?;
    if path=="/api/status" {return Ok((200,device.status()?));}
    if path=="/api/plan" {return Ok((409,fail("power_plan_unsupported","capability")));}
    let body:Value=if path=="/api/bundle/chunk" {json!({})}else{serde_json::from_slice(payload)?};
    if !body["op"].is_null() || !body["command"].is_null() || !body["session_nonce"].is_null() {return Ok((400,fail("retired_protocol","command")));}
    if path!="/api/bundle/chunk" && (body["bridge_id"]!=principal||body["device_mac"]!=device.mac) {
        return Ok((403,fail("identity","binding")));
    }
    if path=="/claim" {
        if operation_token!=token{return Ok((401,fail("device_token","claim")));}
        if body["force"]==true||body["release"]==true{return Ok((409,fail("unsupported","claim")));}
        let result=device.bindings.claim(principal,token,now)?;
        if result!="applied"{return Ok((409,fail(&result,"claim")));}
        return Ok((200,json!({"op":"claim","result":"applied","owner_id":principal,"lease_s":300,
            "expires_in_s":300,"device_mac":device.mac,"owner":device.bindings.owner(now)})));
    }
    if device.bindings.owner(now)["id"]!=principal{return Ok((409,fail("claim_required","owner")));}
    let message=body.to_string();let nonce=device.nonce.clone();
    let reply=match path {
        "/api/bundle/begin"=>device.bundle.begin(&message,&nonce,now)?,
        "/api/bundle/chunk"=>{
            if device.bundle.status()?["rx_owner"]!=principal{return Ok((409,fail("rx_owner","bundle")));}
            device.bundle.chunk(query.get("request_id").map(String::as_str).unwrap_or(""),
                query.get("session_nonce").map(String::as_str).unwrap_or(""),
                query.get("offset").map(String::as_str).unwrap_or(""),payload,now)?
        },
        "/api/bundle/commit"=>{let context=device.context();device.bundle.commit(&message,&nonce,now,&context)?},
        "/api/activate"=>{let context=device.context();device.bundle.activate(&message,now,&context)?},
        "/api/data"=>device.bundle.data(&message)?,
        _=>return Ok((404,fail("unsupported","routing"))),
    };
    let status=if reply["result"]=="rejected" {409}else{200};Ok((status,reply))
}

async fn udp(shared:Shared,bind:SocketAddr)->Result<()> {
    let socket=tokio::net::UdpSocket::bind(bind).await?;let mut bytes=[0u8;257];
    let mut recent=BTreeMap::new();
    loop {
        let (n,source)=socket.recv_from(&mut bytes).await?;if n>256{continue;}
        let request:Value=match serde_json::from_slice(&bytes[..n]){Ok(value)=>value,Err(_)=>continue};
        if request["magic"]!="codex-status"||request["discovery_v"]!=1||request["kind"]!="discover"{continue;}
        let Some(nonce)=request["nonce"].as_str().filter(|n|n.len()==32&&n.bytes().all(|b|b.is_ascii_hexdigit())) else {continue};
        let reply={
            let device=shared.lock().unwrap();
            if request.get("target_mac").is_some_and(|mac|mac!=&json!(device.mac)){continue;}
            let key=(source.ip(),nonce.to_owned());
            if recent.get(&key).is_some_and(|at:&Instant|at.elapsed()<Duration::from_secs(2)){continue;}
            if recent.len()>=32{recent.retain(|_,at|at.elapsed()<Duration::from_secs(2));if recent.len()>=32{continue;}}
            recent.insert(key,Instant::now());
            serde_json::to_vec(&json!({"magic":"codex-status","discovery_v":1,"kind":"discovery_reply",
                "nonce":nonce,"mac":device.mac,"name":"AP01 simulator","model":"njcuk.enstor.ap01",
                "http_port":device.http_port,"maintenance_port":device.maintenance_port,
                "boot_id":device.boot,"binding_method":BINDING_METHOD}))?
        };
        socket.send_to(&reply,source).await?;
    }
}

pub(super) async fn run(options:Options)->Result<()> {
    ensure!(options.data_dir.is_dir(),"precreate AP01 simulator data directory");
    let maintenance_bind:SocketAddr=std::env::var("CODEX_STATUS_AP01_MAINTENANCE_BIND")
        .unwrap_or_else(|_|"127.0.0.1:18080".into()).parse()?;
    ensure!(maintenance_bind.ip().is_loopback()&&maintenance_bind.port()!=options.listen.port(),"independent_loopback_maintenance_port_required");
    // Bind both before starting workers: a collision is a real startup failure.
    let maintenance_listener=tokio::net::TcpListener::bind(maintenance_bind).await?;
    let listener=tokio::net::TcpListener::bind(options.listen).await?;
    let boot_path=options.data_dir.join("ap01-boot.txt");
    let boot=fs::read_to_string(&boot_path).ok().and_then(|s|s.parse::<u64>().ok()).unwrap_or(0).checked_add(1).context("boot counter exhausted")?;
    atomic_write(&boot_path,boot.to_string().as_bytes())?;
    let ui_root=std::env::var_os("CODEX_STATUS_AP01_UI_ROOT").map(PathBuf::from);
    if let Some(root)=&ui_root {ensure!(root.is_dir(),"precreate AP01 UI hostfs directory");atomic_write(&root.join("live-events.txt"),b"")?;}
    let mac=bridge_core::platform::model::DeviceIdentity::normalized_mac(&options.mac).context("AP01 MAC")?;
    let bindings=bridge_render::ap01::Pairing::open(&options.data_dir.join("ap01-bindings.bin"),&mac,boot)?;
    let bundle=bridge_render::SimulatorBundle::new(&options.data_dir,FIRMWARE_TARGET_AP01,boot,"cold")?;
    let shared=Arc::new(Mutex::new(Device{mac,boot,nonce:format!("{boot:032x}"),start:Instant::now(),
        epoch:options.epoch_ms.unwrap_or(1792305600000),bindings,bundle,sessions:BTreeMap::new(),
        context_counter:0,session_counter:0,ui_root,frame_generation:0,displayed_generation:0,frame_crc:0,
        frame_context:String::new(),event:0,ui_status:String::new(),http_port:options.listen.port(),
        maintenance_port:maintenance_bind.port(),business_online:true}));
    let udp_bind:SocketAddr=std::env::var("CODEX_STATUS_AP01_UDP_BIND").unwrap_or_else(|_|"127.0.0.1:18767".into()).parse()?;
    let udp_shared=shared.clone();tokio::spawn(async move {if let Err(error)=udp(udp_shared,udp_bind).await{eprintln!("AP01 UDP stopped: {error}");}});
    let ui_shared=shared.clone();tokio::spawn(async move {
        let mut interval=tokio::time::interval(Duration::from_millis(50));
        loop{interval.tick().await;let mut device=ui_shared.lock().unwrap();
            if device.ui_events().and_then(|_|device.publish_ui()).is_err(){eprintln!("AP01 UI port unavailable");}}
    });
    let maintenance=Router::new().route("/api/pairing/info",get(pairing))
        .route("/api/pairing",post(pairing)).route("/api/pairing/{request}",get(pairing).delete(pairing))
        .route("/health",get(maintenance_status)).route("/api/maintenance/status",get(maintenance_status))
        .fallback(||async{(StatusCode::NOT_FOUND,Json(fail("unsupported_endpoint","maintenance_router")))})
        .layer(DefaultBodyLimit::max(4096)).with_state(shared.clone());
    let app=Router::new().route("/api/status",post(business)).route("/claim",post(business))
        .route("/api/data",post(business)).route("/api/plan",post(business))
        .route("/api/bundle/begin",post(business)).route("/api/bundle/chunk",post(business))
        .route("/api/bundle/commit",post(business)).route("/api/activate",post(business))
        .fallback(||async{(StatusCode::NOT_FOUND,Json(fail("unsupported_endpoint","business_router")))})
        .layer(DefaultBodyLimit::max(MAX_HTTP_FRAME)).with_state(shared.clone());
    let business_shared=shared.clone();
    let mut business_task=tokio::spawn(async move {
        let result=axum::serve(listener,app).await;
        business_shared.lock().unwrap().business_online=false;
        result
    });
    // Private test control, never a product HTTP shutdown route. Stopping this
    // listener demonstrates lifecycle independence, not process memory isolation.
    if let Some(path)=std::env::var_os("CODEX_STATUS_AP01_BUSINESS_STOP_FILE") {
        let path=PathBuf::from(path);let stop_shared=shared.clone();let abort=business_task.abort_handle();
        tokio::spawn(async move {loop {tokio::time::sleep(Duration::from_millis(50)).await;
            if path.is_file(){abort.abort();stop_shared.lock().unwrap().business_online=false;break;}}});
    }
    tokio::spawn(async move {let _=(&mut business_task).await;});
    println!("AP01 simulator ready maintenance={} business={} UDP={} binding={} power=always_on",maintenance_bind,options.listen,udp_bind,BINDING_METHOD);
    axum::serve(maintenance_listener,maintenance).await?;Ok(())
}
