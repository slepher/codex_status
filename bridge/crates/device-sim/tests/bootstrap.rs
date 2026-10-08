use serde_json::Value;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{atomic::{AtomicUsize, Ordering}, Mutex, OnceLock},
    time::Duration,
};

const ENDPOINT: &str = "endpoint-test-secret";
const DEVICE: &str = "device-test-secret";
const CONTROL: &str = "control-test-secret";
static BLE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Copy)]
struct TestBleFrame { session:[u8;16], next:u64, generation:u64 }
static BLE_FRAMES: OnceLock<Mutex<HashMap<SocketAddr,TestBleFrame>>> = OnceLock::new();

// Characterization of the reported failure, using the production Wi-Fi engine
// and real HTTP client. HTTP here is the test carrier, not evidence of RF.
fn wifi_activate_repro_fixture(mac: &str) -> (Simulator, Value) {
    let sim = Simulator::start(mac, &["--wifi-unified-test"]);
    assert_eq!(set_time(&sim, Some(CONTROL), &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    assert_eq!(request(sim.address(), "POST", "/sim/power", Some(CONTROL), br#"{"plugged":true}"#).0, 200);
    assert_eq!(claim(&sim, "id=bridge-test", Some(DEVICE)).0, 200);
    let status = bridge_core::device_client::status(&sim.address().to_string(), ENDPOINT, Duration::from_secs(3)).unwrap();
    let saved = wifi_activate_repro_command(&sim, &status, "apply", serde_json::json!({
        "request_id":"fixture-save", "expected_revision":0,
        "changes":[{"network_id":"home","host_ssid":"FakeHost","ssid":"FakeDevice","credential":"replace","password":"synthetic-only-password"}]
    })).unwrap();
    assert_eq!(saved["outcome"], "committed");
    assert_eq!(wifi_activate_repro_command(&sim, &status, "active", serde_json::json!({})).unwrap()["network_id"], "");
    (sim, status)
}

fn wifi_activate_repro_command(sim: &Simulator, status: &Value, op: &str, body: Value) -> anyhow::Result<Value> {
    bridge_core::device_client::wifi_command(&sim.address().to_string(), ENDPOINT,
        sim.ready["mac"].as_str().unwrap(), "bridge-test", op,
        status["session_nonce"].as_str().unwrap(), body, Duration::from_secs(3))
}

fn wifi_activate_repro_poll(sim: &Simulator, outcome: i32) -> Value {
    let response = request(sim.address(), "POST", "/sim/wifi", Some(CONTROL),
        serde_json::json!({"outcome":outcome}).to_string().as_bytes());
    assert_eq!(response.0, 200);
    json_body(&response.1)
}

#[test]
fn same_source_rendezvous_waits_for_ble_release_and_real_ip() {
    let _env_guard = BLE_ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mac = "02:00:00:00:00:7A";
    let (sim, initial) = wifi_activate_repro_fixture(mac);
    assert_eq!(post_plan(&sim, Some(ENDPOINT),
        &plan_message(&initial, "bridge-test", "light-prep", 1, "light", Some(120))).1["result"], "applied");
    let registry = serde_json::json!({"02000000007A":format!("http://{}",sim.address())}).to_string();
    std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS", registry);
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut link = bridge_ble::DeviceConnection::connect("02000000007A", ENDPOINT, "bridge-test")
            .await.unwrap().expect("fake BLE advertisement");
        let status = link.command("status", serde_json::json!({})).await.unwrap();
        let plan = plan_message(&status, "bridge-test", "light-network", 2, "light", Some(120));
        assert_eq!(link.command("plan", plan).await.unwrap()["result"], "applied");
        let started = link.command("network_start", serde_json::json!({
            "device_mac":mac,"session_nonce":status["session_nonce"],"bridge_id":"bridge-test",
            "plan_id":2,"origin_request_no":3,"bridge_addr":"192.0.2.1","bridge_port":8765
        })).await.unwrap();
        assert_eq!(started["result"], "accepted", "network_start ACK: {started}");
        let before = bridge_core::device_client::status(&sim.address().to_string(), ENDPOINT,
            Duration::from_secs(3)).unwrap();
        assert_eq!(before["rendezvous"]["ready"], false);
        assert_eq!(before["transports"]["http"]["ready"], false);
        let activated = wifi_activate_repro_command(&sim, &initial, "activate", serde_json::json!({
            "request_id":"network-activate","network_id":"home","version":1
        })).unwrap();
        assert_eq!(activated["stage"], "accepted", "{activated}");
        let no_ip = wifi_activate_repro_poll(&sim, 2);
        assert_eq!(no_ip["radio"]["has_ip"], false);
        assert_eq!(bridge_core::device_client::status(&sim.address().to_string(), ENDPOINT,
            Duration::from_secs(3)).unwrap()["transports"]["http"]["ready"], false);
        wifi_activate_repro_poll(&sim, 1);
        wifi_activate_repro_poll(&sim, 1);
        let still_ble = bridge_core::device_client::status(&sim.address().to_string(), ENDPOINT,
            Duration::from_secs(3)).unwrap();
        assert_eq!(still_ble["radio"]["wifi_connected"], true);
        assert_eq!(still_ble["rendezvous"]["ready"], false);
        assert_eq!(still_ble["transports"]["http"]["ready"], false);
        link.close().await;
        let after = bridge_core::device_client::status(&sim.address().to_string(), ENDPOINT,
            Duration::from_secs(3)).unwrap();
        assert_eq!(after["rendezvous"]["ready"], true, "{after}");
        assert_eq!(after["transports"]["http"]["ready"], true);
        assert_eq!(set_time(&sim, Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":121000})).0, 200);
        let expired = bridge_core::device_client::status(&sim.address().to_string(), ENDPOINT,
            Duration::from_secs(3)).unwrap();
        assert_eq!(expired["rendezvous"]["ready"], false);
        assert_eq!(expired["rendezvous"]["remaining_ms"], 0);
    });
    std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
}

#[test]
fn wifi_activate_repro_pending_dhcp_timeout_and_same_id_success() {
    use bridge_core::platform::{service::PlatformService, model::{DeviceIdentity, DeviceCapabilities}};
    use std::collections::BTreeMap;
    let mac = "0200000000D1";
    let (sim, status) = wifi_activate_repro_fixture(mac);
    let root = temp_data_dir();
    let svc = PlatformService::open(&root).unwrap();
    svc.device_upsert(DeviceIdentity::new(mac, "Activate repro").unwrap(), DeviceCapabilities::ssd1681_154g()).unwrap();
    let saved_radio=wifi_activate_repro_poll(&sim, 0);
    assert_eq!(saved_radio["radio"]["begins"],0,"saving alone must not start RF");
    assert_eq!(saved_radio["radio"]["connected"],false);
    let body = serde_json::json!({"network_id":"home","version":1});
    let job = svc.wifi_queue_control(mac, "bridge-test", "activate", body.clone(), 100).unwrap();
    let mut calls = vec![];
    let pending = svc.wifi_attempt(mac, &job.job_id, Some("authenticated_http"), &BTreeMap::new(), 100,
        |op, body| { calls.push(op.to_owned()); wifi_activate_repro_command(&sim, &status, op, body) }).unwrap().unwrap();
    assert_eq!(pending.state, "awaiting_confirmation");
    // Deferred begin happens on the first poll; the next injects association without DHCP.
    let started=wifi_activate_repro_poll(&sim, 2);
    assert_eq!(started["radio"]["begins"],1);
    assert_eq!(started["radio"]["ssid"],"FakeDevice");
    assert_eq!(started["radio"]["candidate_credentials_used"],true);
    let no_ip=wifi_activate_repro_poll(&sim, 2);
    assert_eq!(no_ip["busy"], true);
    assert_eq!(no_ip["radio"]["connected"],true);
    assert_eq!(no_ip["radio"]["has_ip"],false);
    let pending = svc.wifi_attempt(mac, &job.job_id, None, &BTreeMap::new(), 106,
        |op, body| { calls.push(op.to_owned()); assert_eq!(body["query_id"], job.request_id); wifi_activate_repro_command(&sim, &status, op, body) }).unwrap().unwrap();
    assert_eq!(pending.state, "awaiting_confirmation");
    assert_eq!(calls, ["activate", "result"]);
    let no_dhcp = bridge_core::device_client::status(&sim.address().to_string(), ENDPOINT, Duration::from_secs(3)).unwrap();
    assert_eq!(no_dhcp["radio"]["wifi_connected"], false);
    assert_eq!(no_dhcp["transports"]["http"]["ready"], false);
    assert_eq!(set_time(&sim, Some(CONTROL), &serde_json::json!({"op":"step","delta_ms":26000})).0, 200);
    wifi_activate_repro_poll(&sim, 0);
    let failed = svc.wifi_attempt(mac, &job.job_id, None, &BTreeMap::new(), 122,
        |op, body| wifi_activate_repro_command(&sim, &status, op, body)).unwrap().unwrap();
    assert_eq!(failed.state, "failed");
    assert_eq!(failed.error_code.as_deref(), Some("dhcp_timeout"));
    assert_eq!(wifi_activate_repro_command(&sim, &status, "active", serde_json::json!({})).unwrap()["network_id"], "");
    eprintln!("REPRO: accepted is not activation success; DHCP deadline yields durable rejection dhcp_timeout and no active selection.");
    let next = svc.wifi_queue_control(mac, "bridge-test", "activate", body, 130).unwrap();
    svc.wifi_attempt(mac, &next.job_id, Some("authenticated_http"), &BTreeMap::new(), 130,
        |op, body| wifi_activate_repro_command(&sim, &status, op, body)).unwrap();
    wifi_activate_repro_poll(&sim, 1);
    assert_eq!(wifi_activate_repro_poll(&sim, 1)["busy"], false);
    let done = svc.wifi_attempt(mac, &next.job_id, None, &BTreeMap::new(), 136,
        |op, body| wifi_activate_repro_command(&sim, &status, op, body)).unwrap().unwrap();
    assert_eq!(done.state, "completed");
    let before_replay=wifi_activate_repro_poll(&sim,1)["radio"]["begins"].clone();
    let replay = wifi_activate_repro_command(&sim, &status, "activate", serde_json::json!({"request_id":next.request_id,"network_id":"home","version":1})).unwrap();
    assert_eq!(replay["outcome"], "committed");
    assert_eq!(wifi_activate_repro_poll(&sim,1)["radio"]["begins"],before_replay,"same-ID replay must not start RF again");
    assert_eq!(wifi_activate_repro_command(&sim, &status, "active", serde_json::json!({})).unwrap()["network_id"], "home");
    eprintln!("REPRO: association plus IP commits activation; same request ID returns its original committed receipt.");
    let close=wifi_activate_repro_command(&sim,&status,"activate",serde_json::json!({"request_id":"window-close","network_id":"home","version":1})).unwrap();
    assert_eq!(close["ok"],true);
    let closed=request(sim.address(),"POST","/sim/wifi",Some(CONTROL),br#"{"outcome":0,"window_ms":0}"#);
    assert_eq!(closed.0,200);
    assert_eq!(json_body(&closed.1)["radio"]["connected"],false);
    let rejected=wifi_activate_repro_command(&sim,&status,"result",serde_json::json!({"query_id":"window-close"})).unwrap();
    assert_eq!(rejected["outcome"],"rejected");assert_eq!(rejected["error"],"window_closed");
    assert_eq!(wifi_activate_repro_command(&sim,&status,"active",serde_json::json!({})).unwrap()["network_id"],"home");
    eprintln!("REPRO: production window constraint aborts RF with window_closed but retains selected home; active selection is not a live connection.");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn wifi_activate_repro_32_completed_acked_jobs_allow_next_activation() {
    use bridge_core::platform::{service::PlatformService, model::{DeviceIdentity, DeviceCapabilities}};
    use std::collections::BTreeMap;
    let mac = "0200000000D2";
    let (sim, status) = wifi_activate_repro_fixture(mac);
    let root = temp_data_dir();
    let svc = PlatformService::open(&root).unwrap();
    svc.device_upsert(DeviceIdentity::new(mac, "Queue repro").unwrap(), DeviceCapabilities::ssd1681_154g()).unwrap();
    for i in 0..32 {
        let now = 100 + i * 10;
        let job = svc.wifi_queue_control(mac, "bridge-test", "activate", serde_json::json!({"network_id":"home","version":1}), now).unwrap();
        let pending = svc.wifi_attempt(mac, &job.job_id, Some("authenticated_http"), &BTreeMap::new(), now,
            |op, body| wifi_activate_repro_command(&sim, &status, op, body)).unwrap().unwrap();
        assert_eq!(pending.state, "awaiting_confirmation");
        wifi_activate_repro_poll(&sim, 1);
        wifi_activate_repro_poll(&sim, 1);
        let done = svc.wifi_attempt(mac, &job.job_id, None, &BTreeMap::new(), now + 6,
            |op, body| wifi_activate_repro_command(&sim, &status, op, body)).unwrap().unwrap();
        assert_eq!(done.state, "completed");
        assert_eq!(wifi_activate_repro_command(&sim, &status, "result_ack", serde_json::json!({"query_id":job.request_id})).unwrap()["outcome"], "retired");
        svc.wifi_receipt_acked(mac, &job.job_id).unwrap();
    }
    drop(svc);
    let svc = PlatformService::open(&root).unwrap();
    let state = svc.wifi_state(mac);
    assert_eq!(state.jobs.len(), 32);
    assert!(state.jobs.iter().all(|j| j.state == "completed" && j.may_have_committed && !j.receipt_ack_pending));
    let next=svc.wifi_queue_control(mac,"bridge-test","activate",serde_json::json!({"network_id":"home","version":1}),500).unwrap();
    assert_eq!(svc.wifi_state(mac).jobs.len(),32);
    assert!(!svc.wifi_state(mac).jobs.iter().any(|j|j.request_id==state.jobs[0].request_id));
    svc.wifi_finish(mac,&next.job_id,"cancelled",None,501).unwrap();
    let after_ttl=svc.wifi_queue_control(mac,"bridge-test","activate",serde_json::json!({"network_id":"home","version":1}),1_000_000).unwrap();
    assert_ne!(next.request_id,after_ttl.request_id);
    assert_eq!(svc.wifi_state(mac).jobs.len(),1);
    eprintln!("FIX VERIFIED: 32 real acknowledged activations no longer block new work after restart or aging.");
    fs::remove_dir_all(root).unwrap();
}

struct Simulator {
    child: Option<Child>,
    ready: Value,
    data_dir: PathBuf,
    cleanup: bool,
}

impl Simulator {
    fn start(mac: &str, extra: &[&str]) -> Self {
        Self::start_with_dir(mac, extra, temp_data_dir(), true)
    }

    fn start_with_dir(mac: &str, extra: &[&str], data_dir: PathBuf, cleanup: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_device-sim"));
        command
            .args(["--listen", "127.0.0.1:0", "--mac", mac, "--data-dir"])
            .arg(&data_dir)
            .args(extra.iter().filter(|arg|**arg!="--wifi-unified-test"))
            .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", ENDPOINT)
            .env("CODEX_STATUS_SIM_DEVICE_TOKEN", DEVICE)
            .env("CODEX_STATUS_SIM_CONTROL_TOKEN", CONTROL)
            .env("CODEX_STATUS_SIM_WIFI_PROTOCOL",if extra.contains(&"--wifi-unified-test") {"3"}else{"2"})
            .env("CODEX_STATUS_SIM_WIFI_PRINCIPAL","bridge-test")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("start simulator");
        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).expect("read ready line");
        let ready: Value = serde_json::from_str(&line).expect("ready JSON");
        Self {
            child: Some(child),
            ready,
            data_dir,
            cleanup,
        }
    }

    fn address(&self) -> SocketAddr {
        self.ready["http"]
            .as_str()
            .unwrap()
            .trim_start_matches("http://")
            .parse()
            .unwrap()
    }

    fn stop_preserving_data(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.cleanup = false;
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if self.cleanup {
            let _ = fs::remove_dir_all(&self.data_dir);
        }
    }
}

fn temp_data_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "codex-device-sim-{}-{}",
        std::process::id(),
        NEXT_DIR.fetch_add(1, Ordering::Relaxed)
    ))
}

fn wire_request(
    address: SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &[u8],
) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(address).expect("connect simulator");
    // Advancing an hour executes 60 persistent reboots synchronously. Keep the
    // ordinary device deadline at 3s, but give this control operation time to
    // flush its state on macOS filesystems.
    let large_step = path == "/sim/time" && serde_json::from_slice::<Value>(body).ok()
        .and_then(|value| value["delta_ms"].as_u64()).is_some_and(|delta| delta >= 3_600_000);
    stream.set_read_timeout(Some(Duration::from_secs(if large_step { 30 } else { 3 }))).unwrap();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: {}\r\n", body.len()).unwrap();
    if let Some(token) = token {
        write!(stream, "Authorization: Bearer {token}\r\n").unwrap();
    }
    write!(stream, "\r\n").unwrap();
    stream.write_all(body).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let split = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("HTTP headers");
    let headers = String::from_utf8_lossy(&response[..split]);
    let status = headers
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let bytes = response[split + 4..].to_vec();
    (status, bytes)
}

fn wire_frame_request(address:SocketAddr,path:&str,frame:&[u8],content_type:bool) -> (u16,Vec<u8>) {
    let mut stream=TcpStream::connect(address).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    write!(stream,"POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nAuthorization: Bearer {ENDPOINT}\r\nContent-Length: {}\r\n",frame.len()).unwrap();
    if content_type {write!(stream,"Content-Type: application/vnd.codex-status.frame\r\n").unwrap();}
    write!(stream,"\r\n").unwrap();stream.write_all(frame).unwrap();
    let mut response=Vec::new();stream.read_to_end(&mut response).unwrap();
    let split=response.windows(4).position(|w|w==b"\r\n\r\n").unwrap();
    let code=String::from_utf8_lossy(&response[..split]).lines().next().unwrap()
        .split_whitespace().nth(1).unwrap().parse().unwrap();
    (code,response[split+4..].to_vec())
}

fn test_ble_send(address:SocketAddr, token:&str, message:&Value, frame:TestBleFrame) -> (u16,Vec<u8>) {
    use bridge_core::compact_frame::{Header,operation};
    let op=message["op"].as_str().unwrap_or("");
    let Some(op_no)=operation(op) else{return (400,br#"{"error":"operation"}"#.to_vec());};
    let payload=serde_json::to_vec(message).unwrap();
    if payload.len()+48>8192{return (413,br#"{"error":"length"}"#.to_vec());}
    let mut context=[0u8;16];
    if let Some(text)=message["active_context_id"].as_str().filter(|s|s.len()==16 && s.bytes().all(|b|b.is_ascii_hexdigit())){
        context.copy_from_slice(text.as_bytes());
    }
    let mut raw=Header{kind:0,op:op_no,result:0,payload_len:payload.len() as u16,
        session:if op=="status"{[0;16]}else{frame.session},request_no:frame.next,
        context}.encode().unwrap().to_vec();
    raw.extend_from_slice(&payload);
    let result=wire_request(address,"POST","/sim/ble/command",Some(token),&raw);
    if result.0!=200{return result;}
    wire_request(address,"GET","/sim/ble/status",Some(token),b"")
}

fn request(address:SocketAddr, method:&str, path:&str, token:Option<&str>, body:&[u8]) -> (u16,Vec<u8>) {
    if (path.starts_with("/api/") || path=="/claim") &&
        (method=="POST" || matches!(path.split('?').next(),Some("/api/status"|"/api/ota/image"))) {
        match bridge_core::device_client::raw_frame_request(
            &address.to_string(),path,token.unwrap_or(""),body,Duration::from_secs(3)) {
            Ok(result)=>return result,
            Err(error) if error.chain().any(|cause|cause.downcast_ref::<bridge_core::device_client::HttpStatus>()
                .is_some_and(|status|status.0==401))=>return (401,br#"{"result":"unauthorized","error":"unauthorized"}"#.to_vec()),
            Err(error) if error.to_string().contains("frame exceeds 4096")=>
                return (413,br#"{"result":"rejected","error":"frame_limit"}"#.to_vec()),
            Err(error) if !error.to_string().contains("unregistered device HTTP operation")=>
                return (400,br#"{"error":"frame_error"}"#.to_vec()),
            Err(_)=>{}
        }
    }
    if path=="/sim/ble/command" && method=="POST" {
        let info=wire_request(address,"GET","/sim/ble/info",token,b"");
        if info.0!=200{return info;}
        let generation=json_body(&wire_request(address,"GET","/sim/ble/attributes",Some(CONTROL),b"").1)
            ["generation"].as_u64().unwrap_or(0);
        let Ok(message)=serde_json::from_slice::<Value>(body) else{return wire_request(address,method,path,token,body);};
        let cells=BLE_FRAMES.get_or_init(||Mutex::new(HashMap::new()));
        let existing={cells.lock().unwrap().get(&address).copied()};
        let frame=if let Some(frame)=existing.filter(|f|f.generation==generation){frame}else{
            let mac=json_body(&info.1)["mac"].as_str().unwrap_or("").to_owned();
            let hello=serde_json::json!({"op":"status","request_id":"test-session-open",
                "token":token.unwrap_or(""),"device_mac":mac,"bridge_id":"bridge-test",
                "session_nonce":""});
            let first=test_ble_send(address,token.unwrap_or(""),&hello,TestBleFrame{session:[0;16],next:1,generation});
            let Ok((header,_))=bridge_core::compact_frame::Header::decode(&first.1,512) else{return first;};
            let frame=TestBleFrame{session:header.session,next:2,generation};
            cells.lock().unwrap().insert(address,frame);
            frame
        };
        let result=test_ble_send(address,token.unwrap_or(""),&message,frame);
        cells.lock().unwrap().insert(address,TestBleFrame{session:frame.session,next:frame.next+1,generation});
        return result;
    }
    if path=="/sim/ble/close" {
        if let Some(cells)=BLE_FRAMES.get(){cells.lock().unwrap().remove(&address);}
    }
    wire_request(address,method,path,token,body)
}

fn json_body(body: &[u8]) -> Value {
    let body=if body.first()==Some(&bridge_core::compact_frame::MARKER) {
        bridge_core::compact_frame::Header::decode(body,8192).expect("frame response").1
    } else {body};
    let value: Value = serde_json::from_slice(body).expect("JSON response");
    if value.get("ble_wire").is_some() { bridge_ble::decode_wire(body).expect("BLE wire response") }
    else { value }
}

#[test]
fn fixed_frame_http_rejects_malformed_identity_and_retired_requests() {
    use bridge_core::compact_frame::Header;
    let sim=Simulator::start("02:00:00:00:00:4C",&[]);
    let make=|header:Header,payload:&[u8]|{
        let mut bytes=header.encode().unwrap().to_vec();bytes.extend_from_slice(payload);bytes
    };
    let status_payload=br#"{"compact":true}"#;
    let mut status=Header{kind:0,op:1,result:0,payload_len:status_payload.len() as u16,
        session:[0xA1;16],request_no:1,context:[0;16]};
    let opening=make(status,status_payload);
    let (code,reply)=wire_frame_request(sim.address(),"/api/status",&make(status,status_payload),true);
    assert_eq!(code,200);
    let (opened,body)=Header::decode(&reply,4096).unwrap();
    assert_ne!(opened.session,[0;16]);
    assert_eq!(serde_json::from_slice::<Value>(body).unwrap()["status_projection"],"compact");
    let (replay_code,replay)=wire_frame_request(sim.address(),"/api/status",&opening,true);
    assert_eq!(replay_code,409);
    assert_eq!(json_body(&replay)["error"],"retired",
        "a reply over the 512-byte retention budget must not reopen the session");
    status.op=3;status.request_no=2;status.session=opened.session;
    let invalid=br#"{"op":"plan","request_id":"synthetic-invalid"}"#;
    status.payload_len=invalid.len() as u16;
    let frame=make(status,invalid);
    let (first_code,first)=wire_frame_request(sim.address(),"/api/plan",&frame,true);
    assert!(matches!(first_code,200|400|409));
    let (replay_code,replay)=wire_frame_request(sim.address(),"/api/plan",&frame,true);
    assert_eq!(replay_code,200);
    assert_eq!(replay,first,"the retained identical result must be returned verbatim");
    let (old_open_code,old_open)=wire_frame_request(sim.address(),"/api/status",&opening,true);
    assert_eq!(old_open_code,409);
    assert_eq!(json_body(&old_open)["error"],"retired");
    let changed=br#"{"op":"plan","request_id":"different-payload"}"#;
    status.payload_len=changed.len() as u16;
    let (code,body)=wire_frame_request(sim.address(),"/api/plan",&make(status,changed),true);
    assert_eq!(code,409);assert_eq!(json_body(&body)["error"],"request_conflict");
    status.request_no=1;status.payload_len=invalid.len() as u16;
    let (code,body)=wire_frame_request(sim.address(),"/api/plan",&make(status,invalid),true);
    assert_eq!(code,409);assert_eq!(json_body(&body)["error"],"retired");
    status.request_no=3;status.session=[0xFF;16];
    assert_eq!(wire_frame_request(sim.address(),"/api/plan",&make(status,invalid),true).0,409);
    status.session=opened.session;status.context=*b"aaaaaaaaaaaaaaaa";
    assert_eq!(wire_frame_request(sim.address(),"/api/plan",&make(status,invalid),true).0,409);
    status.context=[0;16];
    let valid=make(status,invalid);
    assert_eq!(wire_frame_request(sim.address(),"/api/plan",&valid,false).0,415);
    let mut bad=valid.clone();bad[0]=0;
    assert_eq!(wire_frame_request(sim.address(),"/api/plan",&bad,true).0,400);
    let mut bad=valid.clone();bad[7]=bad[7].wrapping_add(1);
    assert_eq!(wire_frame_request(sim.address(),"/api/plan",&bad,true).0,400);
    assert_eq!(wire_frame_request(sim.address(),"/api/data",&valid,true).0,400);
    let mut bad=valid;bad[1]=2;
    assert_eq!(wire_frame_request(sim.address(),"/api/plan",&bad,true).0,400);
}

#[test]
fn sync_v1_s04_note4_shared_protocol_replay_and_digest_conflict() {
    let sim = Simulator::start("02:00:00:00:00:B2",
        &["--target", "zectrix-note4-400x300"]);
    assert_eq!(claim(&sim, "id=bridge-test&lease=120", Some(DEVICE)).0, 200);
    let initial = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    assert_eq!(initial["sync_v1"], 1);
    assert_eq!(initial["firmware_target"], "zectrix-note4-400x300");
    let nonce = initial["session_nonce"].as_str().unwrap();
    assert_eq!(initial["device_mac"], "02:00:00:00:00:B2");
    let config = serde_json::json!({"op":"sync_config","request_id":"config-1",
        "token":ENDPOINT,"device_mac":"02:00:00:00:00:B2",
        "session_nonce":nonce,"bridge_id":"bridge-test","enabled":true});
    let (code, body) = request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), config.to_string().as_bytes());
    assert_eq!(code, 200);
    assert_eq!(json_body(&body)["result"], "applied", "{}", String::from_utf8_lossy(&body));
    let ip = sim.address().to_string();
    macro_rules! call {
        ($op:expr, $id:expr, $fields:expr) => {{
            bridge_core::device_client::sync(&ip, ENDPOINT, "02:00:00:00:00:B2",
                "bridge-test", nonce, $op, $id, &$fields, Duration::from_secs(3)).unwrap()
        }};
    }
    let begin = call!("begin", "begin-1", serde_json::json!({
        "client_serial":"1","reasons":["periodic"]}));
    assert_eq!(begin["result"], "applied");
    let id = begin["batch_id"].as_str().unwrap();
    let after_begin = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    assert_eq!(after_begin["sync"]["pending_batch"]["batch_id"], id,
        "begin={begin}; status={after_begin}");
    let mut bytes = Vec::new();
    while bytes.len() < begin["bytes"].as_u64().unwrap() as usize {
        let page_id = format!("page-{}", bytes.len());
        let page = call!("page", &page_id,
            serde_json::json!({"batch_id":id,"offset":bytes.len(),"limit":256}));
        let replay = call!("page", &page_id,
            serde_json::json!({"batch_id":id,"offset":bytes.len(),"limit":256}));
        assert_eq!(replay["data_b64"], page["data_b64"]);
        let chunk = base64::engine::general_purpose::STANDARD.decode(
            page["data_b64"].as_str().unwrap()).unwrap();
        assert!(!chunk.is_empty());
        bytes.extend_from_slice(&chunk);
        let prefix = format!("{:x}", Sha256::digest(&bytes));
        let ack_id = format!("ack-{}", bytes.len());
        let ack = call!("ack", &ack_id,
            serde_json::json!({"batch_id":id,"offset":bytes.len(),
                "prefix_sha256":prefix}));
        assert_eq!(ack["acked_offset"], bytes.len());
        let ack_replay = call!("ack", &ack_id,
            serde_json::json!({"batch_id":id,"offset":bytes.len(),
                "prefix_sha256":prefix}));
        assert_eq!(ack_replay["acked_offset"], bytes.len());
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(hash, begin["sha256"]);
    let frozen: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(frozen["format"], "device-sync-1");
    assert_eq!(frozen["snapshot"]["target"], "zectrix-note4-400x300");
    let wrong = bridge_core::device_client::sync(&ip, ENDPOINT, "02:00:00:00:00:B2",
        "bridge-test", nonce, "complete", "wrong-hash",
        &serde_json::json!({"batch_id":id,"bytes":bytes.len(),
            "sha256":"00".repeat(32)}), Duration::from_secs(3));
    assert!(wrong.unwrap_err().to_string().contains("digest_mismatch"));
    assert_eq!(json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1)["sync"]["pending_batch"]["batch_id"], id);
    let fields = serde_json::json!({"batch_id":id,"bytes":bytes.len(),"sha256":hash});
    let complete = call!("complete", "complete-1", fields.clone());
    assert_eq!(complete["receipt"], id);
    assert_eq!(call!("complete", "complete-repeat", fields)["result"], "already_complete");
    let status = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    assert_eq!(status["sync"]["rounds"], 0);
    assert_eq!(status["sync"]["due"], false);
}

#[test]
fn sync_v1_s01_note4_fifteenth_deep_round_requires_explicit_open() {
    let sim = Simulator::start("02:00:00:00:00:B3",
        &["--target", "zectrix-note4-400x300"]);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    let source: Value = serde_json::from_str(include_str!(
        "../../core/tests/fixtures/codex-status-a-400x300.json")).unwrap();
    let compiled = bridge_core::compile::compile(&source,
        "epd-ssd2683-400x300-1bpp").unwrap();
    let bundle = serde_json::json!({"bridge_id":"bridge-test","job_id":"sync-rounds",
        "firmware_target":"zectrix-note4-400x300",
        "render_target":"epd-ssd2683-400x300-1bpp","compiler_abi":2,
        "profile":{"template_ids":["codex-status-a"],
            "initial_active_id":"codex-status-a"},
        "templates":[{"key":{"template_id":"codex-status-a",
            "render_target":"epd-ssd2683-400x300-1bpp"},
            "source":source,"compiled":compiled}],"resources":[],"bindings":[]});
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    assert_eq!(bridge_core::device_client::install_bundle(&sim.address().to_string(),
        ENDPOINT, "02:00:00:00:00:B3", "bridge-test", &bytes, 4096,
        Duration::from_secs(3)).unwrap()["result"], "applied");
    assert_eq!(claim(&sim, "id=bridge-test&lease=3600", Some(DEVICE)).0, 200);
    let status = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let config = serde_json::json!({"op":"sync_config","request_id":"config-rounds",
        "token":ENDPOINT,"device_mac":"02:00:00:00:00:B3",
        "session_nonce":status["session_nonce"],"bridge_id":"bridge-test","enabled":true});
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), config.to_string().as_bytes()).1)["result"], "applied");
    let ip = sim.address().to_string();
    let nonce = status["session_nonce"].as_str().unwrap();
    let call = |op: &str, id: &str, fields: Value| bridge_core::device_client::sync(&ip,
        ENDPOINT, "02:00:00:00:00:B3", "bridge-test", nonce, op, id,
        &fields, Duration::from_secs(3)).unwrap();
    let begin = call("begin", "baseline-begin", serde_json::json!({
        "client_serial":"1","reasons":["periodic"]}));
    let id = begin["batch_id"].as_str().unwrap();
    let mut content = Vec::new();
    while content.len() < begin["bytes"].as_u64().unwrap() as usize {
        let page = call("page", "baseline-page", serde_json::json!({
            "batch_id":id,"offset":content.len(),"limit":1024}));
        content.extend(base64::engine::general_purpose::STANDARD
            .decode(page["data_b64"].as_str().unwrap()).unwrap());
        call("ack", "baseline-ack", serde_json::json!({"batch_id":id,
            "offset":content.len(),"prefix_sha256":format!("{:x}", Sha256::digest(&content))}));
    }
    call("complete", "baseline-complete", serde_json::json!({"batch_id":id,
        "bytes":content.len(),"sha256":begin["sha256"]}));
    assert_eq!(sim_state(&sim)["sync"]["rounds"], 0);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"wall","offset_ms":900000})).0, 200);
    assert_eq!(sim_state(&sim)["sync"]["rounds"], 0);
    let sleep = plan_message(&status, "bridge-test", "sleep-rounds", 1, "sleep", None);
    assert_eq!(post_plan(&sim, Some(ENDPOINT), &sleep).1["result"], "applied");
    assert_eq!(sim_state(&sim)["power"]["mode"], "deep");
    for round in 1..=15 {
        assert_eq!(set_time(&sim, Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
        let state = sim_state(&sim);
        assert_eq!(state["sync"]["rounds"], round);
        assert_eq!(state["power"]["sync_open"], false);
        assert_eq!(state["sync"]["due"], round == 15);
        if round == 14 {
            let status_cmd = serde_json::json!({"op":"status","request_id":"status-14",
                "token":ENDPOINT});
            let early = json_body(&request(sim.address(), "POST", "/sim/ble/command",
                Some(ENDPOINT), status_cmd.to_string().as_bytes()).1);
            let open = serde_json::json!({"op":"sync_open","request_id":"open-14",
                "token":ENDPOINT,"device_mac":"02:00:00:00:00:B3",
                "session_nonce":early["session_nonce"],"bridge_id":"bridge-test",
                "reason":"periodic","open_id":"too-early",
                "wake_generation":early["wake_generation"],"wake_seq":early["wake_seq"]});
            assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
                Some(ENDPOINT), open.to_string().as_bytes()).1)["result"], "rejected");
            assert_eq!(sim_state(&sim)["power"]["sync_open"], false);
        }
    }
    let ble_status = serde_json::json!({"op":"status","request_id":"status-15",
        "token":ENDPOINT});
    let current = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), ble_status.to_string().as_bytes()).1);
    assert_eq!(current["sync"]["v"], 1);
    let open = serde_json::json!({"op":"sync_open","request_id":"open-15",
        "token":ENDPOINT,"device_mac":"02:00:00:00:00:B3",
        "session_nonce":current["session_nonce"],"bridge_id":"bridge-test",
        "reason":"periodic","open_id":"fifteenth-open",
        "wake_generation":current["wake_generation"],"wake_seq":current["wake_seq"]});
    let result = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), open.to_string().as_bytes()).1);
    assert_eq!(result["result"], "applied", "{result}");
    let replay = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), open.to_string().as_bytes()).1);
    assert_eq!(replay["result"], "applied", "{replay}");
    assert_eq!(sim_state(&sim)["power"]["sync_open"], true);
    assert_eq!(json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1)["sync"]["phase"], "WIFI_SYNC_ONCE");
    let nonce2 = current["session_nonce"].as_str().unwrap();
    let call2 = |op: &str, id: &str, fields: Value| bridge_core::device_client::sync(&ip,
        ENDPOINT, "02:00:00:00:00:B3", "bridge-test", nonce2, op, id,
        &fields, Duration::from_secs(3)).unwrap();
    let next = call2("begin", "round15-begin", serde_json::json!({
        "client_serial":"2","reasons":["periodic"]}));
    let batch = next["batch_id"].as_str().unwrap();
    let mut content = Vec::new();
    while content.len() < next["bytes"].as_u64().unwrap() as usize {
        let page = call2("page", "round15-page", serde_json::json!({
            "batch_id":batch,"offset":content.len(),"limit":1024}));
        content.extend(base64::engine::general_purpose::STANDARD
            .decode(page["data_b64"].as_str().unwrap()).unwrap());
        call2("ack", "round15-ack", serde_json::json!({"batch_id":batch,
            "offset":content.len(),"prefix_sha256":format!("{:x}", Sha256::digest(&content))}));
    }
    call2("complete", "round15-complete", serde_json::json!({"batch_id":batch,
        "bytes":content.len(),"sha256":next["sha256"]}));
    let after = sim_state(&sim);
    assert_eq!(after["sync"]["rounds"], 0);
    assert_eq!(after["sync"]["due"], false);
    assert_eq!(after["power"]["sync_open"], false);
    assert_eq!(after["power"]["mode"], "deep");
}

#[test]
fn sync_v1_s12_note4_auth_owner_session_and_low_battery() {
    let sim = Simulator::start("02:00:00:00:00:B4",
        &["--target", "zectrix-note4-400x300"]);
    let body = serde_json::json!({"sync_version":1,
        "device_mac":"02:00:00:00:00:B4","bridge_id":"bridge-a",
        "session_nonce":"bad","request_id":"s12","client_serial":"1",
        "reasons":["periodic"]});
    assert_eq!(request(sim.address(), "POST", "/api/sync/begin",
        None, body.to_string().as_bytes()).0, 401);
    assert_eq!(request(sim.address(), "POST", "/api/sync/begin",
        Some("wrong"), body.to_string().as_bytes()).0, 401);
    assert_eq!(claim(&sim, "id=bridge-a&lease=120", Some(DEVICE)).0, 200);
    let status = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let config = serde_json::json!({"op":"sync_config","request_id":"config-s12",
        "token":ENDPOINT,"device_mac":"02:00:00:00:00:B4",
        "session_nonce":status["session_nonce"],"bridge_id":"bridge-a","enabled":true});
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), config.to_string().as_bytes()).1)["result"], "applied");
    let before = sim_state(&sim)["sync"].clone();
    let mut attempt = body.clone();
    attempt["session_nonce"] = status["session_nonce"].clone();
    attempt["device_mac"] = "02:00:00:00:00:B5".into();
    assert_eq!(request(sim.address(), "POST", "/api/sync/begin",
        Some(ENDPOINT), attempt.to_string().as_bytes()).0, 400);
    attempt["device_mac"] = "02:00:00:00:00:B4".into();
    attempt["session_nonce"] = "wrong-session".into();
    assert_eq!(request(sim.address(), "POST", "/api/sync/begin",
        Some(ENDPOINT), attempt.to_string().as_bytes()).0, 400);
    attempt["session_nonce"] = status["session_nonce"].clone();
    attempt["bridge_id"] = "bridge-b".into();
    let (code, reply) = request(sim.address(), "POST", "/api/sync/begin",
        Some(ENDPOINT), attempt.to_string().as_bytes());
    assert_eq!(code, 409);
    assert_eq!(json_body(&reply)["error"], "occupied");
    let after = sim_state(&sim)["sync"].clone();
    assert_eq!(after["pending_batch"], Value::Null);
    assert_eq!(after["diag_next_seq"], before["diag_next_seq"]);
    assert_eq!(after["rounds"], before["rounds"]);
    assert_eq!(request(sim.address(), "POST", "/sim/power", Some(CONTROL),
        br#"{"battery_pct":4}"#).0, 200);
    let low = sim_state(&sim);
    assert_eq!(low["power"]["mode"], "deep");
    assert_eq!(low["sync"]["due"], true);
    assert_eq!(low["sync"]["pending_batch"], Value::Null);
}

fn note4_sync_roundtrip(sim: &Simulator, serial: u64, reasons: &[&str]) -> Value {
    let state = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let nonce = state["session_nonce"].as_str().unwrap();
    let mac = state["device_mac"].as_str().unwrap();
    let ip = sim.address().to_string();
    let call = |op: &str, fields: Value| bridge_core::device_client::sync(&ip, ENDPOINT,
        mac, "bridge-test", nonce, op, &format!("roundtrip-{serial}-{op}"),
        &fields, Duration::from_secs(3)).unwrap();
    let begin = call("begin", serde_json::json!({"client_serial":serial.to_string(),
        "reasons":reasons}));
    let id = begin["batch_id"].as_str().unwrap();
    let mut content = Vec::new();
    while content.len() < begin["bytes"].as_u64().unwrap() as usize {
        let page = call("page", serde_json::json!({"batch_id":id,
            "offset":content.len(),"limit":1024}));
        content.extend(base64::engine::general_purpose::STANDARD
            .decode(page["data_b64"].as_str().unwrap()).unwrap());
        call("ack", serde_json::json!({"batch_id":id,"offset":content.len(),
            "prefix_sha256":format!("{:x}", Sha256::digest(&content))}));
    }
    assert_eq!(format!("{:x}", Sha256::digest(&content)), begin["sha256"]);
    let frozen: Value = serde_json::from_slice(&content).unwrap();
    assert_eq!(frozen["reasons"], serde_json::json!(reasons));
    call("complete", serde_json::json!({"batch_id":id,"bytes":content.len(),
        "sha256":begin["sha256"]}));
    frozen
}

fn configured_note4_sync_sim(mac: &str) -> Simulator {
    let sim = Simulator::start(mac, &["--target", "zectrix-note4-400x300"]);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    let source: Value = serde_json::from_str(include_str!(
        "../../core/tests/fixtures/codex-status-a-400x300.json")).unwrap();
    let compiled = bridge_core::compile::compile(&source,
        "epd-ssd2683-400x300-1bpp").unwrap();
    let bundle = serde_json::json!({"bridge_id":"bridge-test","job_id":"sync-light",
        "firmware_target":"zectrix-note4-400x300",
        "render_target":"epd-ssd2683-400x300-1bpp","compiler_abi":2,
        "profile":{"template_ids":["codex-status-a"],
            "initial_active_id":"codex-status-a"},
        "templates":[{"key":{"template_id":"codex-status-a",
            "render_target":"epd-ssd2683-400x300-1bpp"},
            "source":source,"compiled":compiled}],"resources":[],"bindings":[]});
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    assert_eq!(bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        mac, "bridge-test", &bytes, 4096, Duration::from_secs(3)).unwrap()["result"],
        "applied");
    assert_eq!(claim(&sim, "id=bridge-test&lease=3600", Some(DEVICE)).0, 200);
    let status = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let config = serde_json::json!({"op":"sync_config","request_id":"config-light",
        "token":ENDPOINT,"device_mac":mac,
        "session_nonce":status["session_nonce"],"bridge_id":"bridge-test","enabled":true});
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), config.to_string().as_bytes()).1)["result"], "applied");
    note4_sync_roundtrip(&sim, 1, &["periodic"]);
    assert_eq!(sim_state(&sim)["sync"]["rounds"], 0);
    sim
}

#[test]
fn current_device_protocol_rejects_retired_routes_and_ble_markers() {
    let sim = configured_note4_sync_sim("02:00:00:00:00:BC");
    assert_eq!(request(sim.address(), "GET", "/v2/status", Some(ENDPOINT), b"").0, 404);
    let status = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    assert!(status.get("protocol").is_none());
    let public_status = json_body(&request(sim.address(), "GET", "/status.json",
        None, b"").1);
    assert_eq!(public_status["bundle_configured"], true);
    assert!(public_status["template_count"].as_u64().unwrap_or(0) > 0);
    assert!(public_status.get("v2_bundle").is_none());
    assert!(public_status.get("v2_templates").is_none());
    let old = serde_json::json!({"op":"sync_config","request_id":"retired-ble",
        "token":ENDPOINT,"protocol":2,"rv":2,"device_mac":"02:00:00:00:00:BC",
        "session_nonce":status["session_nonce"],"bridge_id":"bridge-test",
        "enabled":false});
    let reply = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), old.to_string().as_bytes()).1);
    assert_eq!(reply["result"], "rejected");
    assert_eq!(reply["error"], "retired_protocol");
    assert_eq!(sim_state(&sim)["sync"]["enabled"], true);
}

#[test]
fn sync_v1_s02_note4_light_entry_and_exit_each_drain() {
    let sim = configured_note4_sync_sim("02:00:00:00:00:B5");
    let before = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let sleep = plan_message(&before, "bridge-test", "sleep-light", 1, "sleep", None);
    assert_eq!(post_plan(&sim, Some(ENDPOINT), &sleep).1["result"], "applied");
    assert_eq!(sim_state(&sim)["power"]["mode"], "deep");
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    let wake = sim_state(&sim);
    assert!(wake["power"]["ble_window_until_ms"].is_number(), "{wake}");
    let status_cmd = serde_json::json!({"op":"status","request_id":"light-status",
        "token":ENDPOINT});
    let (code, body) = request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), status_cmd.to_string().as_bytes());
    assert_eq!(code, 200, "BLE window was not open");
    let current = json_body(&body);
    let mut light = plan_message(&current, "bridge-test", "light-enter", 2,
        "light", Some(120));
    light["op"] = "plan".into();
    light["token"] = ENDPOINT.into();
    let ack = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), light.to_string().as_bytes()).1);
    assert_eq!(ack["result"], "applied", "{ack}");
    let entered = sim_state(&sim);
    assert_eq!(entered["power"]["mode"], "light");
    assert_eq!(entered["sync"]["reasons"], serde_json::json!(["light_enter"]));
    let replay = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), light.to_string().as_bytes()).1);
    assert_eq!(replay["result"], "applied");
    let repeated = sim_state(&sim);
    assert_eq!(repeated["sync"]["diag_next_seq"], entered["sync"]["diag_next_seq"]);
    assert_eq!(repeated["plan"]["accepted_at_ms"], entered["plan"]["accepted_at_ms"]);
    let first = note4_sync_roundtrip(&sim, 2, &["periodic", "light_enter"]);
    assert_eq!(first["snapshot"]["rounds"], 1);
    assert_eq!(sim_state(&sim)["sync"]["rounds"], 0);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":120000})).0, 200);
    let leaving = sim_state(&sim);
    assert_eq!(leaving["power"]["sync_open"], true);
    assert_eq!(leaving["power"]["mode"], "deep");
    assert_eq!(leaving["sync"]["reasons"], serde_json::json!(["light_exit"]));
    note4_sync_roundtrip(&sim, 3, &["periodic", "light_exit"]);
    let drained = sim_state(&sim);
    assert_eq!(drained["power"]["sync_open"], false);
    assert_eq!(drained["sync"]["rounds"], 0);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    assert_eq!(sim_state(&sim)["sync"]["rounds"], 1);
}

#[test]
fn sync_v1_s08_note4_ring_overflow_crc_and_text_redaction() {
    let sim = Simulator::start("02:00:00:00:00:B6",
        &["--target", "zectrix-note4-400x300"]);
    assert_eq!(claim(&sim, "id=bridge-test&lease=120", Some(DEVICE)).0, 200);
    let status = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let config = serde_json::json!({"op":"sync_config","request_id":"config-s08",
        "token":ENDPOINT,"device_mac":"02:00:00:00:00:B6",
        "session_nonce":status["session_nonce"],"bridge_id":"bridge-test","enabled":true});
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), config.to_string().as_bytes()).1)["result"], "applied");
    let append = |text: String| {
        let body = serde_json::json!({"op":"append_text","text":text});
        assert_eq!(request(sim.address(), "POST", "/sim/diagnostics", Some(CONTROL),
            body.to_string().as_bytes()).0, 200);
    };
    for i in 0..15 { append(format!("[sync] normal wake {i}")); }
    assert!(sim_state(&sim)["sync"]["diag_used_bytes"].as_u64().unwrap() <= 3600);
    append(format!("[sync] {}é", "x".repeat(96)));
    append("[sync] token=SECRET-WITNESS fail".into());
    let first = note4_sync_roundtrip(&sim, 1, &["periodic"]);
    let first_raw = base64::engine::general_purpose::STANDARD.decode(
        first["diag"]["records_b64"].as_str().unwrap()).unwrap();
    assert!(!String::from_utf8_lossy(&first_raw).contains("SECRET-WITNESS"));
    assert!(String::from_utf8_lossy(&first_raw).contains("[redacted]"));
    let mut at = 0usize;
    let mut truncated = false;
    while at < first_raw.len() {
        let length = u16::from_le_bytes([first_raw[at], first_raw[at + 1]]) as usize;
        truncated |= first_raw[at + 3] & 1 != 0;
        at += length;
    }
    assert!(truncated);
    assert_eq!(request(sim.address(), "POST", "/sim/diagnostics", Some(CONTROL),
        br#"{"op":"corrupt_header"}"#).0, 200);
    for i in 0..70 { append(format!("[sync] fail {i:02} {}", "z".repeat(75))); }
    let frozen = note4_sync_roundtrip(&sim, 2, &["periodic"]);
    let diag = &frozen["diag"];
    assert!(diag["from_seq"].as_str().unwrap().parse::<u64>().unwrap() > 1);
    assert!(diag["gaps"].as_array().unwrap().iter()
        .any(|gap| gap["reason"] == "overwritten"));
    let raw = base64::engine::general_purpose::STANDARD.decode(
        diag["records_b64"].as_str().unwrap()).unwrap();
    assert!(raw.len() <= 4096);
    assert!(!String::from_utf8_lossy(&raw).contains("SECRET-WITNESS"));
    let next = sim_state(&sim)["sync"]["diag_next_seq"].as_str().unwrap()
        .parse::<u64>().unwrap();
    append("[sync] fail corrupt me".into());
    let corrupt = serde_json::json!({"op":"corrupt_record","seq":next.to_string()});
    assert_eq!(request(sim.address(), "POST", "/sim/diagnostics", Some(CONTROL),
        corrupt.to_string().as_bytes()).0, 200);
    assert_eq!(sim_state(&sim)["sync"]["baseline"], "unknown");
    let after = note4_sync_roundtrip(&sim, 3, &["periodic"]);
    assert!(after["diag"]["gaps"].as_array().unwrap().iter()
        .any(|gap| gap["reason"] == "corrupt"));
    append("[sync] fail header boundary".into());
    assert_eq!(request(sim.address(), "POST", "/sim/diagnostics", Some(CONTROL),
        br#"{"op":"corrupt_header"}"#).0, 200);
    assert_eq!(sim_state(&sim)["sync"]["baseline"], "unknown");
}

#[test]
fn sync_v1_s03_note4_frozen_pages_survive_light_deadline() {
    let sim = configured_note4_sync_sim("02:00:00:00:00:B7");
    let status = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let sleep = plan_message(&status, "bridge-test", "sleep-s03", 1, "sleep", None);
    assert_eq!(post_plan(&sim, Some(ENDPOINT), &sleep).1["result"], "applied");
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    let status_cmd = serde_json::json!({"op":"status","request_id":"s03-status",
        "token":ENDPOINT});
    let current = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), status_cmd.to_string().as_bytes()).1);
    let mut light = plan_message(&current, "bridge-test", "s03-light", 2,
        "light", Some(60));
    light["op"] = "plan".into();
    light["token"] = ENDPOINT.into();
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), light.to_string().as_bytes()).1)["result"], "applied");
    for i in 0..15 {
        let text = serde_json::json!({"op":"append_text",
            "text":format!("[sync] freeze seed {i:02} {}", "x".repeat(48))});
        assert_eq!(request(sim.address(), "POST", "/sim/diagnostics", Some(CONTROL),
            text.to_string().as_bytes()).0, 200);
    }
    let before = sim_state(&sim);
    let deadline = before["power"]["boot_ms"].as_u64().unwrap()
        + before["plan"]["accepted_at_ms"].as_u64().unwrap()
        + before["plan"]["granted_s"].as_u64().unwrap() * 1000;
    let state = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let nonce = state["session_nonce"].as_str().unwrap();
    let mac = state["device_mac"].as_str().unwrap();
    let ip = sim.address().to_string();
    let call = |op: &str, fields: Value| bridge_core::device_client::sync(&ip, ENDPOINT,
        mac, "bridge-test", nonce, op, &format!("s03-{op}"),
        &fields, Duration::from_secs(3)).unwrap();
    let begin = call("begin", serde_json::json!({"client_serial":"2",
        "reasons":["periodic","light_enter"]}));
    let id = begin["batch_id"].as_str().unwrap();
    let frozen_size = begin["bytes"].as_u64().unwrap() as usize;
    assert!(frozen_size > 1024);
    let mut content = Vec::new();
    let mut pages = 0;
    while content.len() < frozen_size {
        let page = call("page", serde_json::json!({"batch_id":id,
            "offset":content.len(),"limit":64}));
        content.extend(base64::engine::general_purpose::STANDARD
            .decode(page["data_b64"].as_str().unwrap()).unwrap());
        call("ack", serde_json::json!({"batch_id":id,"offset":content.len(),
            "prefix_sha256":format!("{:x}", Sha256::digest(&content))}));
        pages += 1;
        if pages == 1 {
            for i in 0..20 {
                let text = serde_json::json!({"op":"append_text",
                    "text":format!("[sync] later event {i}")});
                assert_eq!(request(sim.address(), "POST", "/sim/diagnostics", Some(CONTROL),
                    text.to_string().as_bytes()).0, 200);
            }
        }
        assert_eq!(set_time(&sim, Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":10000})).0, 200);
        let during = sim_state(&sim);
        assert_eq!(during["plan"]["accepted_at_ms"], before["plan"]["accepted_at_ms"]);
        assert_eq!(during["plan"]["granted_s"], before["plan"]["granted_s"]);
        if during["clock"]["monotonic_ms"].as_u64().unwrap() >= deadline {
            assert_eq!(during["power"]["sync_open"], true, "{during}");
        }
    }
    assert!(pages > 16);
    assert_eq!(format!("{:x}", Sha256::digest(&content)), begin["sha256"]);
    let frozen: Value = serde_json::from_slice(&content).unwrap();
    assert!(!String::from_utf8_lossy(&content).contains("later event"));
    assert_eq!(frozen["reasons"], serde_json::json!(["periodic","light_enter"]));
    call("complete", serde_json::json!({"batch_id":id,"bytes":content.len(),
        "sha256":begin["sha256"]}));
    let after = sim_state(&sim);
    assert_eq!(after["sync"]["reasons"], serde_json::json!(["light_exit"]));
    assert_eq!(after["power"]["sync_open"], true);
}

#[test]
fn sync_v1_s05_note4_stalled_transfer_skips_full_retry_window() {
    let sim = configured_note4_sync_sim("02:00:00:00:00:B9");
    let status = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let sleep = plan_message(&status, "bridge-test", "s05-sleep", 1, "sleep", None);
    assert_eq!(post_plan(&sim, Some(ENDPOINT), &sleep).1["result"], "applied");
    for _ in 0..15 {
        assert_eq!(set_time(&sim, Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    }
    let status_cmd = serde_json::json!({"op":"status","request_id":"s05-status",
        "token":ENDPOINT});
    let current = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), status_cmd.to_string().as_bytes()).1);
    assert_eq!(current["sync"]["due"], true);
    let open = serde_json::json!({"op":"sync_open","request_id":"s05-open",
        "token":ENDPOINT,"device_mac":"02:00:00:00:00:B9",
        "session_nonce":current["session_nonce"],"bridge_id":"bridge-test",
        "reason":"periodic","open_id":"s05-open-id",
        "wake_generation":current["wake_generation"],"wake_seq":current["wake_seq"]});
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), open.to_string().as_bytes()).1)["result"], "applied");
    let nonce = current["session_nonce"].as_str().unwrap();
    let begin = bridge_core::device_client::sync(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:B9", "bridge-test", nonce, "begin", "s05-begin",
        &serde_json::json!({"client_serial":"2","reasons":["periodic"]}),
        Duration::from_secs(3)).unwrap();
    assert_eq!(begin["result"], "applied");
    let batch_id = begin["batch_id"].clone();
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":90000})).0, 200);
    let stalled = sim_state(&sim);
    assert_eq!(stalled["power"]["sync_open"], false, "{stalled}");
    assert_eq!(stalled["sync"]["pending_batch"]["batch_id"], batch_id);
    assert_eq!(stalled["sync"]["retry_skip"], 1);
    assert_eq!(stalled["sync"]["due"], true);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    let skipped = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), status_cmd.to_string().as_bytes()).1);
    assert_eq!(skipped["sync"]["retry_skip"], 1);
    let mut retry = open.clone();
    retry["request_id"] = "s05-skipped-open".into();
    retry["reason"] = "retry".into();
    retry["open_id"] = "s05-retry-id".into();
    retry["session_nonce"] = skipped["session_nonce"].clone();
    retry["wake_generation"] = skipped["wake_generation"].clone();
    retry["wake_seq"] = skipped["wake_seq"].clone();
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), retry.to_string().as_bytes()).1)["result"], "rejected");
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    let next = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), status_cmd.to_string().as_bytes()).1);
    assert_eq!(next["sync"]["retry_skip"], 0);
    retry["request_id"] = "s05-next-open".into();
    retry["open_id"] = "s05-next-id".into();
    retry["session_nonce"] = next["session_nonce"].clone();
    retry["wake_generation"] = next["wake_generation"].clone();
    retry["wake_seq"] = next["wake_seq"].clone();
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), retry.to_string().as_bytes()).1)["result"], "applied");
    assert_eq!(sim_state(&sim)["sync"]["pending_batch"]["batch_id"], batch_id);
}

#[test]
fn sync_v1_s07_note4_hard_restart_resumes_frozen_batch() {
    let mut sim = configured_note4_sync_sim("02:00:00:00:00:BA");
    for i in 0..15 {
        let text = serde_json::json!({"op":"append_text",
            "text":format!("[sync] persisted event {i:02} {}", "y".repeat(48))});
        assert_eq!(request(sim.address(), "POST", "/sim/diagnostics", Some(CONTROL),
            text.to_string().as_bytes()).0, 200);
    }
    let initial = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let sleep = plan_message(&initial, "bridge-test", "s07-sleep", 1, "sleep", None);
    assert_eq!(post_plan(&sim, Some(ENDPOINT), &sleep).1["result"], "applied");
    for _ in 0..15 {
        assert_eq!(set_time(&sim, Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    }
    let status_cmd = serde_json::json!({"op":"status","request_id":"s07-status",
        "token":ENDPOINT});
    let current = json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), status_cmd.to_string().as_bytes()).1);
    let open = serde_json::json!({"op":"sync_open","request_id":"s07-open",
        "token":ENDPOINT,"device_mac":"02:00:00:00:00:BA",
        "session_nonce":current["session_nonce"],"bridge_id":"bridge-test",
        "reason":"periodic","open_id":"s07-open-id",
        "wake_generation":current["wake_generation"],"wake_seq":current["wake_seq"]});
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), open.to_string().as_bytes()).1)["result"], "applied");
    let state = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    let first = bridge_core::device_client::sync(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:BA", "bridge-test", state["session_nonce"].as_str().unwrap(),
        "begin", "s07-begin", &serde_json::json!({"client_serial":"2",
        "reasons":["periodic"]}), Duration::from_secs(3)).unwrap();
    let batch_id = first["batch_id"].as_str().unwrap().to_owned();
    let bytes = first["bytes"].as_u64().unwrap() as usize;
    assert!(bytes > 64);
    let page = bridge_core::device_client::sync(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:BA", "bridge-test", state["session_nonce"].as_str().unwrap(),
        "page", "s07-page", &serde_json::json!({"batch_id":batch_id,
            "offset":0,"limit":64}), Duration::from_secs(3)).unwrap();
    let mut content = base64::engine::general_purpose::STANDARD
        .decode(page["data_b64"].as_str().unwrap()).unwrap();
    let ack = bridge_core::device_client::sync(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:BA", "bridge-test", state["session_nonce"].as_str().unwrap(),
        "ack", "s07-ack", &serde_json::json!({"batch_id":batch_id,
            "offset":content.len(),"prefix_sha256":format!("{:x}", Sha256::digest(&content))}),
        Duration::from_secs(3)).unwrap();
    assert_eq!(ack["acked_offset"], content.len());
    let data_dir = sim.data_dir.clone();
    sim.stop_preserving_data();
    let restarted = Simulator::start_with_dir("02:00:00:00:00:BA",
        &["--target", "zectrix-note4-400x300", "--wake-cause", "soft"],
        data_dir, true);
    let resumed = json_body(&request(restarted.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    assert_eq!(resumed["sync"]["pending_batch"]["batch_id"], batch_id);
    assert_eq!(resumed["sync"]["pending_batch"]["acked_offset"], 64);
    assert_eq!(resumed["sync"]["diag_generation"], state["sync"]["diag_generation"]);
    assert_ne!(resumed["session_nonce"], state["session_nonce"]);
    let nonce = resumed["session_nonce"].as_str().unwrap();
    while content.len() < bytes {
        let page = bridge_core::device_client::sync(&restarted.address().to_string(), ENDPOINT,
            "02:00:00:00:00:BA", "bridge-test", nonce, "page", "s07-resume-page",
            &serde_json::json!({"batch_id":batch_id,"offset":content.len(),"limit":1024}),
            Duration::from_secs(3)).unwrap();
        content.extend(base64::engine::general_purpose::STANDARD
            .decode(page["data_b64"].as_str().unwrap()).unwrap());
        bridge_core::device_client::sync(&restarted.address().to_string(), ENDPOINT,
            "02:00:00:00:00:BA", "bridge-test", nonce, "ack", "s07-resume-ack",
            &serde_json::json!({"batch_id":batch_id,"offset":content.len(),
                "prefix_sha256":format!("{:x}", Sha256::digest(&content))}),
            Duration::from_secs(3)).unwrap();
    }
    assert_eq!(format!("{:x}", Sha256::digest(&content)), first["sha256"]);
    let complete = bridge_core::device_client::sync(&restarted.address().to_string(), ENDPOINT,
        "02:00:00:00:00:BA", "bridge-test", nonce, "complete", "s07-complete",
        &serde_json::json!({"batch_id":batch_id,"bytes":bytes,
            "sha256":first["sha256"]}), Duration::from_secs(3)).unwrap();
    assert_eq!(complete["result"], "applied");
}

#[test]
fn sync_v1_s07_note4_crash_before_blob_reference_keeps_previous_metadata() {
    let mut sim = configured_note4_sync_sim("02:00:00:00:00:BB");
    let state = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    for (id, enabled) in [("s07-disable", false), ("s07-enable", true)] {
        let config = serde_json::json!({"op":"sync_config","request_id":id,
            "token":ENDPOINT,"device_mac":"02:00:00:00:00:BB",
            "session_nonce":state["session_nonce"],"bridge_id":"bridge-test",
            "enabled":enabled});
        assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/command",
            Some(ENDPOINT), config.to_string().as_bytes()).1)["result"], "applied");
    }
    assert_eq!(sim_state(&sim)["sync"]["due"], true);
    assert_eq!(request(sim.address(), "POST", "/sim/storage", Some(CONTROL),
        br#"{"crash_after_sync":"sync_blob","count":2}"#).0, 200);
    let addr = sim.address().to_string();
    let nonce = state["session_nonce"].as_str().unwrap();
    assert!(bridge_core::device_client::sync(&addr, ENDPOINT, "02:00:00:00:00:BB",
        "bridge-test", nonce, "begin", "s07-crash-begin",
        &serde_json::json!({"client_serial":"2","reasons":["periodic"]}),
        Duration::from_secs(3)).is_err());
    let data_dir = sim.data_dir.clone();
    sim.stop_preserving_data();
    let mut restarted = Simulator::start_with_dir("02:00:00:00:00:BB",
        &["--target", "zectrix-note4-400x300", "--wake-cause", "soft"],
        data_dir, true);
    let recovered = json_body(&request(restarted.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    assert_eq!(recovered["sync"]["pending_batch"], Value::Null);
    assert_eq!(recovered["sync"]["last_completed"]["client_serial"], "1");
    assert_eq!(recovered["sync"]["due"], true);
    let retried = bridge_core::device_client::sync(&restarted.address().to_string(), ENDPOINT,
        "02:00:00:00:00:BB", "bridge-test",
        recovered["session_nonce"].as_str().unwrap(), "begin", "s07-retry-begin",
        &serde_json::json!({"client_serial":"2","reasons":["periodic"]}),
        Duration::from_secs(3)).unwrap();
    assert_eq!(retried["result"], "applied");
    assert_eq!(retried["acked_offset"], 0);
    let batch_id = retried["batch_id"].as_str().unwrap();
    let page = bridge_core::device_client::sync(&restarted.address().to_string(), ENDPOINT,
        "02:00:00:00:00:BB", "bridge-test",
        recovered["session_nonce"].as_str().unwrap(), "page", "s07-retry-page",
        &serde_json::json!({"batch_id":batch_id,"offset":0,"limit":64}),
        Duration::from_secs(3)).unwrap();
    let prefix = base64::engine::general_purpose::STANDARD
        .decode(page["data_b64"].as_str().unwrap()).unwrap();
    assert_eq!(request(restarted.address(), "POST", "/sim/storage", Some(CONTROL),
        br#"{"crash_after_sync":"sync_meta","count":2}"#).0, 200);
    assert!(bridge_core::device_client::sync(&restarted.address().to_string(), ENDPOINT,
        "02:00:00:00:00:BB", "bridge-test",
        recovered["session_nonce"].as_str().unwrap(), "ack", "s07-crash-ack",
        &serde_json::json!({"batch_id":batch_id,"offset":prefix.len(),
            "prefix_sha256":format!("{:x}", Sha256::digest(&prefix))}),
        Duration::from_secs(3)).is_err());
    let data_dir = restarted.data_dir.clone();
    restarted.stop_preserving_data();
    let after_ack = Simulator::start_with_dir("02:00:00:00:00:BB",
        &["--target", "zectrix-note4-400x300", "--wake-cause", "soft"],
        data_dir, true);
    let final_state = json_body(&request(after_ack.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    assert_eq!(final_state["sync"]["pending_batch"]["batch_id"], batch_id);
    assert_eq!(final_state["sync"]["pending_batch"]["acked_offset"], 64);
}

fn ota_request_result(sim: &Simulator, path: &str, firmware: &[u8], timeout: Duration)
    -> std::io::Result<(u16, String)> {
    let boundary = "sim-boundary";
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"firmware\"; filename=\"fixture.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
    body.extend_from_slice(firmware);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut stream = TcpStream::connect(sim.address())?;
    stream.set_read_timeout(Some(timeout))?;
    write!(stream, "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\n\r\n", body.len()).unwrap();
    stream.write_all(&body)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let status = String::from_utf8_lossy(&response[..split]).lines().next().unwrap()
        .split_whitespace().nth(1).unwrap().parse().unwrap();
    Ok((status, String::from_utf8_lossy(&response[split + 4..]).into_owned()))
}

fn ota_request(sim: &Simulator, path: &str, firmware: &[u8]) -> (u16, String) {
    ota_request_result(sim, path, firmware, Duration::from_secs(3)).unwrap()
}

fn inert_rom(target: &str, fw: &str) -> Vec<u8> {
    let mut bytes = vec![0u8; 1024];
    let marker = format!("codex-status-ota-v1|{target}|{fw}\0");
    bytes[..marker.len()].copy_from_slice(marker.as_bytes());
    bytes
}

#[test]
fn ota_switches_only_after_a_complete_catalogued_upload_or_explicit_override() {
    let mut sim = Simulator::start("02:00:00:00:00:34", &[]);
    let fw = || json_body(&request(sim.address(), "GET", "/status.json", None, b"").1)["fw"].clone();
    assert_eq!(fw(), "0.18.24-bw");
    let valid = inert_rom("codex-status-154g", "0.18.25-bw");
    assert_eq!(ota_request(&sim, "/doUpdate?token=wrong&target=codex-status-154g", &valid).0, 401);
    assert_eq!(ota_request(&sim, "/doUpdate?token=device-test-secret&target=zectrix-note4-400x300", &valid).0, 401);
    assert!(ota_request(&sim, "/doUpdate?token=device-test-secret&target=codex-status-154g", &valid[..100]).1.contains("UPDATE FAILED"));
    assert!(ota_request(&sim, "/doUpdate?token=device-test-secret&target=codex-status-154g", &vec![7u8;1024]).1.contains("UPDATE FAILED"));
    assert_eq!(fw(), "0.18.24-bw");
    let before_nonce = json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1)["session_nonce"].clone();
    let (code, ack) = ota_request(&sim, "/doUpdate?token=device-test-secret&target=codex-status-154g", &valid);
    assert_eq!(code, 200);
    if !ack.contains("UPDATE OK") {
        let mut child = sim.child.take().unwrap();
        let _ = child.kill(); let _ = child.wait();
        let mut stderr = String::new();
        child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
        panic!("{ack}: {stderr}");
    }
    assert_eq!(fw(), "0.18.24-bw");
    std::thread::sleep(Duration::from_millis(1700));
    assert_eq!(fw(), "0.18.25-bw");
    let versions = json_body(&request(sim.address(), "GET", "/sim/versions", Some(CONTROL), b"").1);
    assert_eq!(versions["active"], "v2");
    assert_eq!(versions["source"], "ota_upload");
    assert_ne!(json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1)["session_nonce"], before_nonce);
    let data_dir = sim.data_dir.clone();
    sim.stop_preserving_data();
    let sim = Simulator::start_with_dir("02:00:00:00:00:34", &[], data_dir, true);
    assert_eq!(json_body(&request(sim.address(), "GET", "/status.json", None, b"").1)["fw"], "0.18.25-bw");
    let override_body = serde_json::to_vec(&serde_json::json!({"id":"v1","reboot":true})).unwrap();
    let (code, body) = request(sim.address(), "POST", "/sim/versions", Some(CONTROL), &override_body);
    assert_eq!(code, 200);
    assert_eq!(json_body(&body)["source"], "test_override");
    assert_eq!(json_body(&request(sim.address(), "GET", "/status.json", None, b"").1)["fw"], "0.18.24-bw");
}

#[test]
fn sync_v1_s10_running_image_hash_uses_active_bytes_not_version_label() {
    let sha256_hex = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let dir = temp_data_dir();
    fs::create_dir_all(&dir).unwrap();
    let target = "codex-status-154g";
    let fw = "same-version";
    let x = inert_rom(target, fw);
    let mut y = x.clone();
    y[1023] = 7;
    let catalog = dir.join("catalog.json");
    fs::write(&catalog, serde_json::to_vec(&serde_json::json!({"initial":"x","versions":[
        {"id":"x","fw":fw,"target":target,"size":x.len(),"sha256":sha256_hex(&x)},
        {"id":"y","fw":fw,"target":target,"size":y.len(),"sha256":sha256_hex(&y)},
    ]})).unwrap()).unwrap();
    let mut sim = Simulator::start_with_dir("02:00:00:00:00:A1", &[
        "--catalog", catalog.to_str().unwrap()], dir, true);
    let image = |n: usize, token| request(sim.address(), "GET",
        &format!("/api/ota/image?image_bytes={n}"), token, b"");
    assert_eq!(image(1024, None).0, 401);
    assert_eq!(image(1025, Some(DEVICE)).0, 400);
    let before = json_body(&image(1024, Some(DEVICE)).1);
    assert_eq!(before["sha256"], sha256_hex(&x));
    assert_eq!(before["running_slot"], "ota_0");
    assert_eq!(json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1)["fw"], fw);
    let upload = ota_request(&sim, "/doUpdate?token=device-test-secret", &y);
    if !upload.1.contains("UPDATE OK") {
        let mut child = sim.child.take().unwrap();
        let _ = child.kill(); let _ = child.wait();
        let mut stderr = String::new();
        child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
        panic!("{upload:?}: {stderr}");
    }
    std::thread::sleep(Duration::from_millis(1700));
    let after = json_body(&image(1024, Some(DEVICE)).1);
    assert_eq!(after["sha256"], sha256_hex(&y));
    assert_eq!(after["running_slot"], "ota_1");
    assert_eq!(after["fw"], fw);
    assert_ne!(after["sha256"], before["sha256"]);
}

#[test]
fn ota_upload_can_commit_after_its_ack_is_lost() {
    let mut sim = Simulator::start("02:00:00:00:00:37", &[]);
    assert_eq!(request(sim.address(), "POST", "/sim/fault", Some(CONTROL),
        br#"{"stall_ack_after":"ota_upload"}"#).0, 200);
    let rom = inert_rom("codex-status-154g", "0.18.25-bw");
    let lost = ota_request_result(&sim,
        "/doUpdate?token=device-test-secret&target=codex-status-154g",
        &rom, Duration::from_millis(300));
    assert!(lost.is_err());
    let queued = json_body(&request(sim.address(), "GET", "/sim/versions", Some(CONTROL), b"").1);
    assert_eq!(queued["pending"], "v2");
    assert_eq!(queued["active"], "v1");
    std::thread::sleep(Duration::from_millis(1700));
    let switched = json_body(&request(sim.address(), "GET", "/sim/versions", Some(CONTROL), b"").1);
    assert_eq!(switched["active"], "v2");
    assert_eq!(switched["source"], "ota_upload");
    let data_dir = sim.data_dir.clone();
    sim.stop_preserving_data();
    let restarted = Simulator::start_with_dir("02:00:00:00:00:37", &[], data_dir, true);
    assert_eq!(json_body(&request(restarted.address(), "GET", "/status.json",
        None, b"").1)["fw"], "0.18.25-bw");
}

#[test]
fn ota_pending_survives_process_death_before_delayed_reboot() {
    let mut sim = Simulator::start("02:00:00:00:00:38", &[]);
    let rom = inert_rom("codex-status-154g", "0.18.25-bw");
    let (code, ack) = ota_request(&sim,
        "/doUpdate?token=device-test-secret&target=codex-status-154g", &rom);
    assert_eq!(code, 200);
    assert!(ack.contains("UPDATE OK"));
    let queued = json_body(&request(sim.address(), "GET", "/sim/versions", Some(CONTROL), b"").1);
    assert_eq!(queued["active"], "v1");
    assert_eq!(queued["pending"], "v2");
    let data_dir = sim.data_dir.clone();
    sim.stop_preserving_data();
    let recovered = Simulator::start_with_dir("02:00:00:00:00:38", &[], data_dir, true);
    let versions = json_body(&request(recovered.address(), "GET", "/sim/versions", Some(CONTROL), b"").1);
    assert_eq!(versions["active"], "v2");
    assert_eq!(versions["source"], "ota_upload");
    assert_eq!(json_body(&request(recovered.address(), "GET", "/status.json", None, b"").1)["fw"],
        "0.18.25-bw");
}

fn claim(sim: &Simulator, query: &str, token: Option<&str>) -> (u16, Value) {
    let mut input=serde_json::Map::new();
    for (key,value) in form_urlencoded::parse(query.as_bytes()) {
        input.insert(key.into_owned(),serde_json::json!(value.into_owned()));
    }
    for flag in ["force","release"] {
        if let Some(value)=input.get_mut(flag) {*value=serde_json::json!(value!="0");}
    }
    let (code,body)=match bridge_core::device_client::claim(&sim.address().to_string(),ENDPOINT,
        token.unwrap_or(""),&Value::Object(input),Duration::from_secs(3)) {
        Ok(response)=>response,
        Err(error) if error.chain().any(|cause|cause.downcast_ref::<bridge_core::device_client::HttpStatus>()
            .is_some_and(|status|status.0==401))=>(401,r#"{"error":"unauthorized","owner":null}"#.into()),
        Err(error)=>panic!("claim transport: {error}"),
    };
    (code,json_body(body.as_bytes()))
}

fn sim_state(sim: &Simulator) -> Value {
    let (code, body) = request(sim.address(), "GET", "/sim/state", Some(CONTROL), b"");
    assert_eq!(code, 200);
    json_body(&body)
}

fn claim_query(pairs: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

#[test]
fn committed_frame_matches_shared_preview_bits_byte_for_byte() {
    for (firmware_target, render_target, width, height, extra) in [
        ("codex-status-154g", "epd-ssd1681-200x200-1bpp", 200, 200, &[][..]),
        ("zectrix-note4-400x300", "epd-ssd2683-400x300-1bpp", 400, 300,
            &["--target", "zectrix-note4-400x300"][..]),
    ] {
        let sim = Simulator::start("02:00:00:00:00:1A", extra);
        assert_eq!(request(sim.address(), "GET", "/sim/frame", Some(ENDPOINT), b"").0, 401);
        assert_eq!(request(sim.address(), "GET", "/sim/frame", Some(CONTROL), b"").0, 404);
        let mut source: Value = serde_json::from_str(include_str!(
            "../../../../tools/test-bridge/templates/quad.json"
        )).unwrap();
        source["id"] = "pixel".into();
        source["canvas"] = serde_json::json!({"w":width,"h":height});
        source["elements"] = serde_json::json!([
            {"type":"rect","rect":[4,4,37,29],"color":"black","fill":true},
            {"type":"text","text":"PIXEL","font":"f16","color":"black","x":50,"y":50}
        ]);
        let compiled = bridge_core::compile::compile(&source, render_target).unwrap();
        let bundle = serde_json::json!({
            "bridge_id":"bridge-test", "job_id":"pixel-job",
            "firmware_target":firmware_target,
            "render_target":render_target, "compiler_abi":2,
            "profile":{"template_ids":["pixel"],"initial_active_id":"pixel"},
            "templates":[{"key":{"template_id":"pixel",
                "render_target":render_target},
                "source":source,"compiled":compiled}],"resources":[],"bindings":[]
        });
        let bytes = bridge_core::template::canonical_bytes(&bundle);
        let ack = bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
            "02:00:00:00:00:1A", "bridge-test", &bytes, 4096,
            Duration::from_secs(3)).unwrap();
        assert_eq!(ack["result"], "applied", "{ack}");
        let (code, frame) = request(sim.address(), "GET", "/sim/frame", Some(CONTROL), b"");
        assert_eq!(code, 200);
        let env = bridge_render::Env { channel:"PULL", battery:75, state:"WIFI ON",
            ..bridge_render::Env::default() };
        let preview = bridge_render::render_bits(&bundle["templates"][0]["source"].to_string(),
            "{}", &env).unwrap();
        assert_eq!(frame.len(), ((width + 7) / 8 * height) as usize);
        assert_eq!(frame, preview);
    }
}

#[test]
fn device_clock_bind_uses_the_experiment_wall_time() {
    let options = ["--target", "zectrix-note4-400x300", "--epoch-ms", "1790420000000"];
    let a = Simulator::start("02:00:00:00:00:5A", &options);
    let b = Simulator::start("02:00:00:00:00:5B", &options);
    for sim in [&a, &b] {
        assert_eq!(set_time(sim, Some(CONTROL),
            &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    }
    assert_eq!(set_time(&b, Some(CONTROL),
        &serde_json::json!({"op":"wall","offset_ms":3_600_000})).0, 200);
    let source = serde_json::json!({
        "schema":1,"id":"clock-pixel","version":1,
        "render_target":"epd-ssd2683-400x300-1bpp",
        "canvas":{"w":400,"h":300},
        "elements":[{"type":"text","x":8,"y":8,"font":"f16",
            "color":"black","bind":"device.now"}]
    });
    let compiled = bridge_core::compile::compile(&source,
        "epd-ssd2683-400x300-1bpp").unwrap();
    let bundle = serde_json::json!({
        "bridge_id":"bridge-test","job_id":"clock-job",
        "firmware_target":"zectrix-note4-400x300",
        "render_target":"epd-ssd2683-400x300-1bpp","compiler_abi":2,
        "profile":{"template_ids":["clock-pixel"],"initial_active_id":"clock-pixel"},
        "templates":[{"key":{"template_id":"clock-pixel",
            "render_target":"epd-ssd2683-400x300-1bpp"},
            "source":source,"compiled":compiled}],"resources":[],"bindings":[]
    });
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    for (sim, mac) in [(&a, "02:00:00:00:00:5A"), (&b, "02:00:00:00:00:5B")] {
        assert_eq!(bridge_core::device_client::install_bundle(&sim.address().to_string(),
            ENDPOINT, mac, "bridge-test", &bytes, 4096,
            Duration::from_secs(3)).unwrap()["result"], "applied");
    }
    let a_crc = sim_state(&a)["bundle"]["frame_crc"].as_u64().unwrap();
    let b_crc = sim_state(&b)["bundle"]["frame_crc"].as_u64().unwrap();
    assert_ne!(a_crc, b_crc, "one hour of virtual wall time must change device.now pixels");
    assert_eq!(set_time(&b, Some(CONTROL),
        &serde_json::json!({"op":"wall","offset_ms":0})).0, 200);
    assert_eq!(request(b.address(), "POST", "/sim/button", Some(CONTROL),
        br#"{"hold_ms":3000}"#).0, 200);
    assert_eq!(sim_state(&b)["bundle"]["frame_crc"], a_crc);
}

#[test]
fn data_bound_frame_matches_shared_preview_after_value_change() {
    let sim = Simulator::start("02:00:00:00:00:1C", &[]);
    let mut source: Value = serde_json::from_str(include_str!(
        "../../../../tools/test-bridge/templates/quad.json"
    )).unwrap();
    source["id"] = "dynamic".into();
    source["elements"] = serde_json::json!([{
        "type":"text", "bind":"account.plan", "font":"f16", "color":"black",
        "x":8,"y":8
    }]);
    let compiled = bridge_core::compile::compile(&source,
        "epd-ssd1681-200x200-1bpp").unwrap();
    let requirement = compiled.requirements.iter()
        .find(|field| field.field == "account.plan").unwrap();
    let field_index = requirement.index;
    let bundle = serde_json::json!({
        "bridge_id":"bridge-test", "job_id":"dynamic-job",
        "firmware_target":"codex-status-154g",
        "render_target":"epd-ssd1681-200x200-1bpp", "compiler_abi":2,
        "profile":{"template_ids":["dynamic"],"initial_active_id":"dynamic"},
        "templates":[{"key":{"template_id":"dynamic",
            "render_target":"epd-ssd1681-200x200-1bpp"},
            "source":source,"compiled":compiled}],
        "resources":[],"bindings":[]
    });
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    let ack = bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:1C", "bridge-test", &bytes, 4096,
        Duration::from_secs(3)).unwrap();
    assert_eq!(ack["result"], "applied");
    let context = ack["active_context_id"].as_str().unwrap();
    for (seq, plan) in [(1, "Plus"), (2, "Pro")] {
        let fields = vec![serde_json::json!({
            "i":field_index,"k":"account.plan","v":plan,"q":"good"
        })];
        let data = serde_json::json!({
            "bridge_id":"bridge-test", "active_context_id":context,"seq":seq,
            "crc":format!("{:08x}",bridge_core::coordinator::data_fields_crc(&fields)),
            "fields":fields
        });
        let ack = bridge_core::device_client::data(&sim.address().to_string(), ENDPOINT,
            "02:00:00:00:00:1C", &data, Duration::from_secs(3)).unwrap();
        assert_eq!(ack["result"], "applied");
        let frame = request(sim.address(), "GET", "/sim/frame", Some(CONTROL), b"").1;
        let usage = serde_json::json!({"account":{"plan":plan}}).to_string();
        let env = bridge_render::Env { channel:"PULL", battery:75, state:"WIFI ON",
            ..bridge_render::Env::default() };
        let expected = bridge_render::render_bits(&bundle["templates"][0]["source"].to_string(),
            &usage, &env).unwrap();
        assert_eq!(frame, expected, "dynamic frame for {plan}");
    }
}

#[test]
fn bundle_hard_exit_during_slot_or_metadata_sync_restores_prior_job() {
    for kind in ["slot", "meta"] {
        let mut sim = Simulator::start("02:00:00:00:00:1B", &[]);
        let mut source: Value = serde_json::from_str(include_str!(
            "../../../../tools/test-bridge/templates/quad.json"
        )).unwrap();
        source["id"] = "quad0".into();
        let compiled = bridge_core::compile::compile(
            &source, "epd-ssd1681-200x200-1bpp"
        ).unwrap();
        let old_bundle = serde_json::json!({
            "bridge_id":"bridge-test", "job_id":"old-job",
            "firmware_target":"codex-status-154g",
            "render_target":"epd-ssd1681-200x200-1bpp", "compiler_abi":2,
            "profile":{"template_ids":["quad0"],"initial_active_id":"quad0"},
            "templates":[{"key":{"template_id":"quad0",
                "render_target":"epd-ssd1681-200x200-1bpp"},
                "source":source,"compiled":compiled}],
            "resources":[],"bindings":[]
        });
        let old_bytes = bridge_core::template::canonical_bytes(&old_bundle);
        let ack = bridge_core::device_client::install_bundle(
            &sim.address().to_string(), ENDPOINT, "02:00:00:00:00:1B",
            "bridge-test", &old_bytes, 4096, Duration::from_secs(3)
        ).unwrap();
        assert_eq!(ack["result"], "applied");
        let fault = serde_json::json!({"crash_after_sync":kind,"count":1});
        assert_eq!(request(sim.address(), "POST", "/sim/storage", Some(CONTROL),
            fault.to_string().as_bytes()).0, 200);
        let mut new_bundle = old_bundle;
        new_bundle["job_id"] = "new-job".into();
        let new_bytes = bridge_core::template::canonical_bytes(&new_bundle);
        assert!(bridge_core::device_client::install_bundle(
            &sim.address().to_string(), ENDPOINT, "02:00:00:00:00:1B",
            "bridge-test", &new_bytes, 4096, Duration::from_secs(3)
        ).is_err(), "{kind} sync must terminate process before ACK");
        let data_dir = sim.data_dir.clone();
        sim.stop_preserving_data();
        let recovered = Simulator::start_with_dir(
            "02:00:00:00:00:1B", &[], data_dir, true
        );
        let (code, body) = request(recovered.address(), "GET", "/api/status", Some(ENDPOINT), b"");
        assert_eq!(code, 200);
        let status = json_body(&body);
        assert_eq!(status["configured"], true, "{kind}: {status}");
        assert_eq!(status["committed_job_id"], "old-job", "{kind}: {status}");
        assert_eq!(status["template_ids"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn real_device_client_installs_one_and_eight_template_bundles() {
    for count in [1usize, 8] {
        let mut sim = Simulator::start("02:00:00:00:00:31", &[]);
        let mut source: Value = serde_json::from_str(include_str!(
            "../../../../tools/test-bridge/templates/quad.json"
        )).unwrap();
        let mut ids = Vec::new();
        let mut templates = Vec::new();
        let mut requirements = Vec::new();
        for index in 0..count {
            let id = format!("quad{index}");
            source["id"] = id.clone().into();
            let compiled = bridge_core::compile::compile(
                &source, "epd-ssd1681-200x200-1bpp"
            ).unwrap();
            if index == 0 { requirements = compiled.requirements.clone(); }
            ids.push(id.clone());
            templates.push(serde_json::json!({
                "key": {"template_id": id, "render_target": "epd-ssd1681-200x200-1bpp"},
                "source": source, "compiled": compiled
            }));
        }
        let bundle = serde_json::json!({
            "bridge_id": "bridge-test", "job_id": format!("job-{count}"),
            "firmware_target": "codex-status-154g",
            "render_target": "epd-ssd1681-200x200-1bpp", "compiler_abi": 2,
            "profile": {"template_ids": ids, "initial_active_id": "quad0"},
            "templates": templates, "resources": [], "bindings": []
        });
        let bytes = bridge_core::template::canonical_bytes(&bundle);
        let ack = bridge_core::device_client::install_bundle(
            &sim.address().to_string(), ENDPOINT, "02:00:00:00:00:31",
            "bridge-test", &bytes, 4096, Duration::from_secs(10)
        ).unwrap_or_else(|error| {
            let mut child = sim.child.take().unwrap();
            let _ = child.kill(); let _ = child.wait();
            let mut stderr = String::new();
            child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
            panic!("install bundle failed: {error:#}; simulator stderr: {stderr}");
        });
        assert_eq!(ack["result"], "applied", "{ack}");
        let (code, body) = request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"");
        assert_eq!(code, 200);
        let status = json_body(&body);
        assert_eq!(status["configured"], true);
        assert_eq!(status["template_ids"].as_array().unwrap().len(), count);
        assert_eq!(status["active_template_id"], "quad0");
        assert_eq!(status["committed_job_id"], format!("job-{count}"));
        assert_eq!(status["active_context_id"], ack["active_context_id"]);
        let fields: Vec<Value> = requirements.iter().filter(|r| !r.local).map(|r|
            serde_json::json!({"i":r.index,"k":r.field,"v":null,"q":"missing"})
        ).collect();
        let context = status["active_context_id"].as_str().unwrap();
        let data = serde_json::json!({
            "bridge_id":"bridge-test", "active_context_id":context, "seq":1,
            "crc":format!("{:08x}", bridge_core::coordinator::data_fields_crc(&fields)),
            "fields":fields
        });
        let data_ack = bridge_core::device_client::data(&sim.address().to_string(), ENDPOINT,
            "02:00:00:00:00:31", &data, Duration::from_secs(3)).unwrap();
        assert_eq!(data_ack["result"], "applied", "{data_ack}");
        let frame_crc = sim_state(&sim)["bundle"]["frame_crc"].clone();
        let replay = bridge_core::device_client::data(&sim.address().to_string(), ENDPOINT,
            "02:00:00:00:00:31", &data, Duration::from_secs(3)).unwrap();
        assert_eq!(replay["display_state"], "unchanged", "{replay}");
        assert_eq!(sim_state(&sim)["bundle"]["frame_crc"], frame_crc);
        if count == 1 {
            let writes = sim_state(&sim)["bundle"]["display_writes"].as_u64().unwrap();
            assert_eq!(request(sim.address(), "POST", "/sim/display", Some(CONTROL),
                br#"{"fail_next":true}"#).0, 200);
            let mut changed = fields.clone();
            changed[0]["v"] = 42.into();
            changed[0]["q"] = "good".into();
            let update = |seq| serde_json::json!({
                "bridge_id":"bridge-test", "active_context_id":context, "seq":seq,
                "crc":format!("{:08x}", bridge_core::coordinator::data_fields_crc(&changed)),
                "fields":changed
            });
            let failed = bridge_core::device_client::data(&sim.address().to_string(), ENDPOINT,
                "02:00:00:00:00:31", &update(2), Duration::from_secs(3)).unwrap();
            assert_eq!(failed["result"], "applied");
            assert_eq!(failed["display_state"], "failed");
            let after_fail = sim_state(&sim)["bundle"].clone();
            assert_eq!(after_fail["frame_crc"], frame_crc);
            assert_eq!(after_fail["frame_trusted"], false);
            assert_eq!(after_fail["display_writes"], writes);
            let recovered = bridge_core::device_client::data(&sim.address().to_string(), ENDPOINT,
                "02:00:00:00:00:31", &update(3), Duration::from_secs(3)).unwrap();
            assert_eq!(recovered["display_state"], "displayed");
            let after_recovery = sim_state(&sim)["bundle"].clone();
            assert_eq!(after_recovery["frame_trusted"], true);
            assert_eq!(after_recovery["display_writes"], writes + 1);
            assert_eq!(after_recovery["frame_crc"], after_fail["candidate_crc"]);
        }
        if count == 8 {
            let activated = bridge_core::device_client::activate(&sim.address().to_string(),
                ENDPOINT, "02:00:00:00:00:31", "bridge-test", "quad1",
                context, Duration::from_secs(3)).unwrap();
            assert_eq!(activated["result"], "applied", "{activated}");
            let (code, body) = request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"");
            assert_eq!(code, 200);
            let switched = json_body(&body);
            assert_eq!(switched["active_template_id"], "quad1");
            assert_ne!(switched["active_context_id"], context);
            assert_eq!(switched["data_seq"], 0);
            let mut current_context = switched["active_context_id"].clone();
            for index in [2, 3, 4, 5, 6, 7, 0, 1] {
                let (code, body) = request(sim.address(), "POST", "/sim/button", Some(CONTROL),
                    br#"{"hold_ms":2100}"#);
                assert_eq!(code, 200);
                let button = json_body(&body);
                assert_eq!(button["active_template_id"], format!("quad{index}"));
                assert_ne!(button["active_context_id"], current_context);
                current_context = button["active_context_id"].clone();
            }
            assert_eq!(sim_state(&sim)["bundle"]["template_ids"].as_array().unwrap().len(), 8);
        }
        let before_restart = json_body(&request(sim.address(), "GET", "/api/status",
            Some(ENDPOINT), b"").1);
        let data_dir = sim.data_dir.clone();
        sim.stop_preserving_data();
        let wake = if count == 1 { &["--wake-cause", "deep"][..] } else { &[][..] };
        let mut restarted = Simulator::start_with_dir("02:00:00:00:00:31", wake, data_dir, true);
        let restored = json_body(&request(restarted.address(), "GET", "/api/status",
            Some(ENDPOINT), b"").1);
        assert_eq!(restored["configured"], true);
        assert_eq!(restored["committed_job_id"], before_restart["committed_job_id"]);
        assert_eq!(restored["active_template_id"], before_restart["active_template_id"]);
        if count == 1 {
            assert_eq!(restored["active_context_id"], before_restart["active_context_id"]);
            assert_eq!(restored["data_seq"], 3);
        } else {
            assert_ne!(restored["active_context_id"], before_restart["active_context_id"]);
            assert_eq!(restored["data_seq"], 0);
        }
        assert_eq!(restored["template_ids"], before_restart["template_ids"]);
        assert_ne!(restored["session_nonce"], before_restart["session_nonce"]);
        if count == 1 {
            let mut replacement = bundle.clone();
            replacement["job_id"] = "torn-install".into();
            let bytes = bridge_core::template::canonical_bytes(&replacement);
            let limit = serde_json::to_vec(&serde_json::json!({
                "write_budget": bytes.len() as i64 + 64
            })).unwrap();
            assert_eq!(request(restarted.address(), "POST", "/sim/storage",
                Some(CONTROL), &limit).0, 200);
            let failed = bridge_core::device_client::install_bundle(
                &restarted.address().to_string(), ENDPOINT, "02:00:00:00:00:31",
                "bridge-test", &bytes, 4096, Duration::from_secs(3)).unwrap();
            assert_eq!(failed["result"], "rejected", "{failed}");
            let data_dir = restarted.data_dir.clone();
            restarted.stop_preserving_data();
            let recovered = Simulator::start_with_dir("02:00:00:00:00:31", &[], data_dir, true);
            let after_torn = json_body(&request(recovered.address(), "GET", "/api/status",
                Some(ENDPOINT), b"").1);
            assert_eq!(after_torn["committed_job_id"], "job-1");
            assert_ne!(after_torn["active_context_id"], before_restart["active_context_id"]);
        }
    }
}

#[test]
fn configured_power_sleeps_and_timer_and_button_wakes_have_distinct_windows() {
    let sim = Simulator::start("02:00:00:00:00:35", &[]);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    let source: Value = serde_json::from_str(include_str!(
        "../../../../tools/test-bridge/templates/quad.json"
    )).unwrap();
    let compiled = bridge_core::compile::compile(&source,
        "epd-ssd1681-200x200-1bpp").unwrap();
    let fields: Vec<Value> = compiled.requirements.iter().filter(|r| !r.local).map(|r|
        serde_json::json!({"i":r.index,"k":r.field,"v":null,"q":"missing"})
    ).collect();
    let bundle = serde_json::json!({
        "bridge_id":"bridge-test", "job_id":"power-job",
        "firmware_target":"codex-status-154g",
        "render_target":"epd-ssd1681-200x200-1bpp", "compiler_abi":2,
        "profile":{"template_ids":["quad"],"initial_active_id":"quad"},
        "templates":[{"key":{"template_id":"quad",
            "render_target":"epd-ssd1681-200x200-1bpp"},
            "source":source,"compiled":compiled}],"resources":[],"bindings":[]
    });
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    let ack = bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:35", "bridge-test", &bytes, 4096,
        Duration::from_secs(3)).unwrap();
    assert_eq!(ack["result"], "applied");
    let status = json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1);
    let data = serde_json::json!({
        "bridge_id":"bridge-test", "active_context_id":status["active_context_id"],
        "seq":1, "crc":format!("{:08x}", bridge_core::coordinator::data_fields_crc(&fields)),
        "fields":fields
    });
    assert_eq!(bridge_core::device_client::data(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:35", &data, Duration::from_secs(3)).unwrap()["result"], "applied");
    assert_eq!(claim(&sim, "id=bridge-test&lease=120", Some(DEVICE)).0, 200);
    let nonce = status["session_nonce"].clone();
    assert_eq!(request(sim.address(), "POST", "/sim/power", Some(CONTROL),
        br#"{"plugged":true,"deep_on_usb":false}"#).0, 200);
    let sleep = plan_message(&status, "bridge-test", "sleep-now", 1, "sleep", None);
    assert_eq!(post_plan(&sim, Some(ENDPOINT), &sleep).1["result"], "applied");
    assert_eq!(sim_state(&sim)["power"]["mode"], "light");
    assert_eq!(request(sim.address(), "POST", "/sim/power", Some(CONTROL),
        br#"{"plugged":false}"#).0, 200);
    let asleep = sim_state(&sim);
    assert_eq!(asleep["power"]["mode"], "deep");
    assert_eq!(asleep["power"]["last_sleep_reason"], "plan");
    let mut stream = TcpStream::connect(sim.address()).unwrap();
    stream.set_read_timeout(Some(Duration::from_millis(150))).unwrap();
    write!(stream, "GET /status.json HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();
    let mut byte = [0u8;1];
    assert!(stream.read(&mut byte).is_err(), "deep sleep must not answer the device surface");

    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    let timer = sim_state(&sim);
    assert_eq!(timer["power"]["mode"], "deep");
    assert_eq!(timer["power"]["provisional"], false);
    assert_eq!(timer["power"]["wake_count"], 1);
    assert_eq!(timer["uptime_ms"], 0);
    assert_eq!(timer["owner"]["last_seen_s"], 0);
    let (code, body) = request(sim.address(), "POST", "/sim/wake", Some(CONTROL),
        br#"{"cause":"button"}"#);
    assert_eq!(code, 200);
    assert_eq!(json_body(&body)["power"]["provisional"], true);
    let button_status = json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1);
    assert_ne!(button_status["session_nonce"], nonce);
    assert_eq!(button_status["committed_job_id"], "power-job");
    assert_eq!(button_status["active_context_id"], status["active_context_id"]);
    assert_eq!(button_status["power"]["provisional"], true);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":300000})).0, 200);
    let ended = sim_state(&sim);
    assert_eq!(ended["power"]["mode"], "deep");
    assert_eq!(ended["power"]["last_sleep_reason"], "provisional");
    for _ in 0..24 {
        assert_eq!(set_time(&sim, Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":3_600_000})).0, 200);
    }
    let day = sim_state(&sim);
    assert_eq!(day["power"]["mode"], "deep");
    assert_eq!(day["power"]["wake_count"], 1442);
    assert_eq!(day["bundle"]["job_id"], "power-job");
}

#[test]
fn low_battery_powers_off_without_scheduling_timer_contact() {
    let sim = Simulator::start("02:00:00:00:00:39", &[]);
    assert_eq!(request(sim.address(), "POST", "/sim/power", Some(ENDPOINT),
        br#"{"battery_pct":4}"#).0, 401);
    let (code, body) = request(sim.address(), "POST", "/sim/power", Some(CONTROL),
        br#"{"battery_pct":4}"#);
    assert_eq!(code, 200);
    let power = json_body(&body);
    assert_eq!(power["mode"], "deep");
    assert_eq!(power["last_sleep_reason"], "low battery");
    assert_eq!(power["next_contact_ms"], Value::Null);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":3600000})).0, 200);
    assert_eq!(sim_state(&sim)["power"]["wake_count"], 0);
    assert_eq!(request(sim.address(), "POST", "/sim/power", Some(CONTROL),
        br#"{"plugged":true}"#).0, 200);
    assert_eq!(request(sim.address(), "POST", "/sim/wake", Some(CONTROL),
        br#"{"cause":"button"}"#).0, 200);
    assert_eq!(sim_state(&sim)["power"]["mode"], "light");
}

#[test]
fn unconfigured_note4_timer_ble_plan_opens_first_bundle_install() {
    let mac = "02:00:00:00:00:3C";
    let sim = Simulator::start(mac, &["--target", "zectrix-note4-400x300"]);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    assert_eq!(claim(&sim, "id=bridge-test&lease=3600", Some(DEVICE)).0, 200);
    let initial = json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1);
    assert_eq!(initial["configured"], false);
    assert_eq!(post_plan(&sim, Some(ENDPOINT),
        &plan_message(&initial, "bridge-test", "unconfigured-sleep", 1, "sleep", None)).1["result"],
        "applied");
    assert_eq!(sim_state(&sim)["power"]["mode"], "deep");
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    assert_eq!(request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"").0, 200);
    let command = serde_json::json!({"op":"status","request_id":"unconfigured-status",
        "token":ENDPOINT});
    let status = json_body(&request(sim.address(), "POST", "/sim/ble/command", Some(ENDPOINT),
        command.to_string().as_bytes()).1);
    assert_eq!(status["result"], "applied");
    assert!(status.get("configured").is_none());
    assert_eq!(json_body(&request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"").1)["bundle_configured"], false);
    let mut light = plan_message(&status, "bridge-test", "unconfigured-light", 2,
        "light", Some(120));
    light["op"] = "plan".into();
    light["token"] = ENDPOINT.into();
    let ack = json_body(&request(sim.address(), "POST", "/sim/ble/command", Some(ENDPOINT),
        light.to_string().as_bytes()).1);
    assert_eq!(ack["result"], "applied", "{ack}");
    assert_eq!(request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").0, 200);

    let source: Value = serde_json::from_str(include_str!(
        "../../core/tests/fixtures/codex-status-a-400x300.json")).unwrap();
    let compiled = bridge_core::compile::compile(&source,
        "epd-ssd2683-400x300-1bpp").unwrap();
    let bundle = serde_json::json!({"bridge_id":"bridge-test","job_id":"first-bundle",
        "firmware_target":"zectrix-note4-400x300",
        "render_target":"epd-ssd2683-400x300-1bpp","compiler_abi":2,
        "profile":{"template_ids":["codex-status-a"],
            "initial_active_id":"codex-status-a"},
        "templates":[{"key":{"template_id":"codex-status-a",
            "render_target":"epd-ssd2683-400x300-1bpp"},
            "source":source,"compiled":compiled}],"resources":[],"bindings":[]});
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    let installed = bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        mac, "bridge-test", &bytes, 4096, Duration::from_secs(3)).unwrap();
    assert_eq!(installed["result"], "applied", "{installed}");
    let configured = json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1);
    assert_eq!(configured["configured"], true);
    assert_eq!(configured["committed_job_id"], "first-bundle");
}

#[test]
fn timer_ble_rendezvous_accepts_formal_plan_before_http_opens() {
    let sim = Simulator::start("02:00:00:00:00:3A", &[]);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    let source: Value = serde_json::from_str(include_str!(
        "../../../../tools/test-bridge/templates/quad.json"
    )).unwrap();
    let compiled = bridge_core::compile::compile(&source,
        "epd-ssd1681-200x200-1bpp").unwrap();
    let bundle = serde_json::json!({
        "bridge_id":"bridge-test", "job_id":"ble-job",
        "firmware_target":"codex-status-154g",
        "render_target":"epd-ssd1681-200x200-1bpp", "compiler_abi":2,
        "profile":{"template_ids":["quad"],"initial_active_id":"quad"},
        "templates":[{"key":{"template_id":"quad",
            "render_target":"epd-ssd1681-200x200-1bpp"},
            "source":source,"compiled":compiled}],"resources":[],"bindings":[]
    });
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    assert_eq!(bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:3A", "bridge-test", &bytes, 4096,
        Duration::from_secs(3)).unwrap()["result"], "applied");
    let initial = json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1);
    assert_eq!(post_plan(&sim, Some(ENDPOINT),
        &plan_message(&initial, "bridge-test", "sleep", 1, "sleep", None)).1["result"], "applied");
    assert_eq!(request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"").0, 404);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    let (code, body) = request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"");
    assert_eq!(code, 200);
    let info = json_body(&body);
    assert_eq!(info["mac"], "02:00:00:00:00:3A");
    assert!(info.get("wake_cause").is_none(), "diagnostics remain on Wi-Fi");
    assert_eq!(sim_state(&sim)["power"]["mode"], "deep");
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":5_001})).0, 200);
    assert_eq!(request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"").0, 404);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":54_999})).0, 200);
    assert_eq!(request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"").0, 200);
    let ble_status = serde_json::json!({"op":"status","request_id":"ble-status",
        "token":ENDPOINT});
    let (code, body) = request(sim.address(), "POST", "/sim/ble/command", Some(ENDPOINT),
        ble_status.to_string().as_bytes());
    assert_eq!(code, 200);
    let status = json_body(&body);
    assert_eq!(status["ack"], "command");
    assert_eq!(status["request_id"], "ble-status");
    assert_eq!(status["result"], "applied");
    assert_ne!(status["session_nonce"], initial["session_nonce"]);
    let mut light = plan_message(&status, "bridge-test", "ble-light", 2, "light", Some(120));
    light["op"] = "plan".into();
    light["token"] = ENDPOINT.into();
    let (code, body) = request(sim.address(), "POST", "/sim/ble/command", Some(ENDPOINT),
        light.to_string().as_bytes());
    assert_eq!(code, 200);
    let ack = json_body(&body);
    assert_eq!(ack["result"], "applied", "{ack}");
    assert_eq!(sim_state(&sim)["power"]["mode"], "light");
    assert_eq!(request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").0, 200);
}

#[test]
fn bridge_device_connection_uses_fake_ble_without_os_radio() {
    let _env_guard=BLE_ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let sim = Simulator::start("02:00:00:00:00:3B", &[]);
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    let source: Value = serde_json::from_str(include_str!(
        "../../../../tools/test-bridge/templates/quad.json"
    )).unwrap();
    let compiled = bridge_core::compile::compile(&source,
        "epd-ssd1681-200x200-1bpp").unwrap();
    let bundle = serde_json::json!({
        "bridge_id":"bridge-test","job_id":"ble-client-job",
        "firmware_target":"codex-status-154g",
        "render_target":"epd-ssd1681-200x200-1bpp","compiler_abi":2,
        "profile":{"template_ids":["quad"],"initial_active_id":"quad"},
        "templates":[{"key":{"template_id":"quad",
            "render_target":"epd-ssd1681-200x200-1bpp"},
            "source":source,"compiled":compiled}],"resources":[],"bindings":[]
    });
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    assert_eq!(bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:3B", "bridge-test", &bytes, 4096,
        Duration::from_secs(3)).unwrap()["result"], "applied");
    let initial = json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1);
    assert_eq!(post_plan(&sim, Some(ENDPOINT),
        &plan_message(&initial,"bridge-test","sleep",1,"sleep",None)).1["result"],"applied");
    assert_eq!(set_time(&sim, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":60000})).0, 200);
    assert_eq!(request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"").0, 200,
        "{}", sim_state(&sim));
    assert_eq!(request(sim.address(), "POST", "/sim/ble/fragment?offset=1&final=1",
        Some(ENDPOINT), b"x").0, 409);
    assert_eq!(request(sim.address(), "POST", "/sim/ble/fragment?offset=0&final=0",
        Some(ENDPOINT), b"x").0, 200);
    assert_eq!(request(sim.address(), "POST", "/sim/ble/fragment?offset=0&final=1",
        Some(ENDPOINT), b"y").0, 409);
    let registry = serde_json::json!({
        "02000000003A":"http://127.0.0.1:1",
        "02000000003B":format!("http://{}",sim.address())
    }).to_string();
    std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS", registry);
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (mac, mut link) = bridge_ble::DeviceConnection::connect_any(
            &["02000000003A".into(), "02000000003B".into()], ENDPOINT, "bridge-test"
        ).await.unwrap().expect("fake BLE advertisement");
        assert_eq!(mac, "02000000003B");
        let status = link.command("status", serde_json::json!({})).await.unwrap();
        assert_eq!(status["result"], "applied");
        assert_ne!(status["session_nonce"], initial["session_nonce"]);
        assert_eq!(link.request_device_token().await.unwrap(), DEVICE);
        let plan = plan_message(&status,"bridge-test","light",2,"light",Some(120));
        let ack = link.command("plan", plan).await.unwrap();
        assert_eq!(ack["result"], "applied", "{ack}");
        link.write_endpoint("127.0.0.1", 8765, ENDPOINT).await.unwrap();
        link.close().await;
        let mut link = bridge_ble::DeviceConnection::connect("02000000003B", ENDPOINT, "bridge-test").await.unwrap().unwrap();
        assert_eq!(request(sim.address(), "POST", "/sim/ble/fault", Some(ENDPOINT), br#"{"kind":"empty"}"#).0, 401);
        for size in [511,512] {
            assert_eq!(request(sim.address(), "POST", "/sim/ble/fault", Some(CONTROL),
                serde_json::json!({"kind":"none","size":size}).to_string().as_bytes()).0, 200);
            let reply = link.command("status", serde_json::json!({})).await.unwrap();
            assert_eq!(reply["result"], "applied");
            let attrs = json_body(&request(sim.address(), "GET", "/sim/ble/attributes", Some(CONTROL), b"").1);
            assert_eq!(attrs["status_bytes"], size);
        }
        for size in [513,635] {
            assert_eq!(request(sim.address(), "POST", "/sim/ble/fault", Some(CONTROL),
                serde_json::json!({"kind":"legacy_overflow","size":size}).to_string().as_bytes()).0, 200);
            let started = std::time::Instant::now();
            assert!(link.command("status", serde_json::json!({})).await.unwrap_err().to_string().contains("timed out"));
            assert!(started.elapsed() < Duration::from_millis(5500));
            let attrs = json_body(&request(sim.address(), "GET", "/sim/ble/attributes", Some(CONTROL), b"").1);
            assert_eq!(attrs["status_bytes"], 0);
            assert_eq!(attrs["events"].as_array().unwrap().last().unwrap()["bytes"], size);
        }
        for kind in ["notify_lost", "delay"] {
            assert_eq!(request(sim.address(), "POST", "/sim/ble/fault", Some(CONTROL),
                serde_json::json!({"kind":kind,"delay_ms":0}).to_string().as_bytes()).0, 200);
            assert_eq!(link.command("status", serde_json::json!({})).await.unwrap()["result"], "applied");
        }
        assert_eq!(request(sim.address(), "POST", "/sim/ble/fault", Some(CONTROL), br#"{"kind":"delay","delay_ms":1000}"#).0, 200);
        let events_before_delay = json_body(&request(sim.address(), "GET", "/sim/ble/attributes", Some(CONTROL), b"").1)["events"].as_array().unwrap().len();
        let advance = async {
            for _ in 0..40 {
                let attrs = json_body(&request(sim.address(), "GET", "/sim/ble/attributes", Some(CONTROL), b"").1);
                if attrs["events"].as_array().unwrap().len() > events_before_delay && attrs["events"].as_array().unwrap().last().is_some_and(|e| e["result"] == "delayed") {
                    assert_eq!(set_time(&sim, Some(CONTROL), &serde_json::json!({"op":"step","delta_ms":1000})).0, 200);
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("delayed property was never scheduled");
        };
        let (result, _) = tokio::join!(link.command("status", serde_json::json!({})), advance);
        assert_eq!(result.unwrap()["result"], "applied");
        // Other stale/invalid attribute cases use an outer cancellation budget;
        // the representative 513/635 cases above exercise the real 5s deadline.
        for kind in ["guard_overflow","empty","malformed","stale","overwrite","wrong_request","wrong_wake","wrong_mac","wrong_op"] {
            let before = json_body(&request(sim.address(), "GET", "/sim/ble/attributes", Some(CONTROL), b"").1)["status_bytes"].clone();
            assert_eq!(request(sim.address(), "POST", "/sim/ble/fault", Some(CONTROL),
                serde_json::json!({"kind":kind,"size":if kind == "guard_overflow" {635} else {0}}).to_string().as_bytes()).0, 200);
            assert!(tokio::time::timeout(Duration::from_millis(200), link.command("status", serde_json::json!({}))).await.is_err(), "{kind}");
            if kind == "guard_overflow" {
                assert_eq!(json_body(&request(sim.address(), "GET", "/sim/ble/attributes", Some(CONTROL), b"").1)["status_bytes"], before);
            }
            assert_eq!(link.command("status", serde_json::json!({})).await.unwrap()["result"], "applied");
        }
        // A side effect survives lost ACK; never turn it into rejected/success.
        let status = link.command("status", serde_json::json!({})).await.unwrap();
        assert_eq!(request(sim.address(), "POST", "/sim/ble/fault", Some(CONTROL), br#"{"kind":"drop_ack"}"#).0, 200);
        let plan = plan_message(&status, "bridge-test", "lost-plan", 3, "light", Some(120));
        assert!(link.command_with_id("plan", plan.clone(), "stable-plan-transaction").await.is_err());
        assert_eq!(sim_state(&sim)["plan"]["plan_id"], 3);
        let reply = link.command_with_id("plan", plan, "stable-plan-transaction").await.unwrap();
        assert_eq!(reply["request_id"], "stable-plan-transaction");
        assert_ne!(reply["result"], "rejected");
        assert!(link.command("wifi_setup", serde_json::json!({"padding":"x".repeat(1024)})).await.unwrap_err().to_string().contains("1024"));
        // Fake has no Wi-Fi persistence HAL: fail explicitly, without inventing support.
        assert_eq!(link.wifi_read("wifi_scheme_get").await.unwrap()["ok"], true);
        for kind in ["read_failed","disconnect","window_closed"] {
            assert_eq!(request(sim.address(), "POST", "/sim/ble/fault", Some(CONTROL),
                serde_json::json!({"kind":kind}).to_string().as_bytes()).0, 200);
            assert!(link.command("status", serde_json::json!({})).await.is_err(), "{kind}");
            link.close().await;
            link = bridge_ble::DeviceConnection::connect("02000000003B", ENDPOINT, "bridge-test").await.unwrap().unwrap();
        }
        link.close().await;
    });
    std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
    assert_eq!(request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").0, 200);
}

#[test]
fn ble_request_limits_and_disconnect_clear_the_real_reassembly_path() {
    let sim = Simulator::start("02:00:00:00:00:C2", &[]);
    assert_eq!(request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"").0, 200);
    for size in [179,180,181] {
        let payload = vec![b' ';size];
        assert_eq!(request(sim.address(), "POST", "/sim/ble/fragment?offset=0&final=0", Some(ENDPOINT), &payload).0,
            if size <= 180 {200} else {413});
        assert_eq!(request(sim.address(), "POST", "/sim/ble/close", Some(ENDPOINT), b"").0, 200);
        assert_eq!(request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"").0, 200);
    }
    for total in [8191,8192,8193] {
        let payload = vec![b' ';total];
        let mut outcome = 200;
        for (index, bytes) in payload.chunks(180).enumerate() {
            outcome = request(sim.address(), "POST", &format!("/sim/ble/fragment?offset={}&final=0", index*180), Some(ENDPOINT), bytes).0;
            if outcome != 200 { break; }
        }
        assert_eq!(outcome, if total <= 8192 {200} else {409});
        request(sim.address(), "POST", "/sim/ble/close", Some(ENDPOINT), b"");
        request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"");
    }
    let partial = br#"{"op":"status","request_id":"#;
    assert_eq!(request(sim.address(), "POST", "/sim/ble/fragment?offset=0&final=0", Some(ENDPOINT), partial).0, 200);
    request(sim.address(), "POST", "/sim/ble/close", Some(ENDPOINT), b"");
    request(sim.address(), "GET", "/sim/ble/info", Some(ENDPOINT), b"");
    assert_eq!(request(sim.address(), "POST", &format!("/sim/ble/fragment?offset={}&final=1", partial.len()), Some(ENDPOINT), b"tail").0, 409);
    // A complete fixed frame is dispatched even without the fake final flag.
    let payload = serde_json::json!({"op":"status","request_id":"frame-complete","token":ENDPOINT}).to_string();
    let mut frame=bridge_core::compact_frame::Header {kind:0,op:1,result:0,
        payload_len:payload.len() as u16,session:[0;16],request_no:1,context:[0;16]}.encode().unwrap().to_vec();
    frame.extend_from_slice(payload.as_bytes());
    assert_eq!(json_body(&request(sim.address(), "POST", "/sim/ble/fragment?offset=0&final=0", Some(ENDPOINT), &frame).1)["result"], "more");
    assert_eq!(json_body(&request(sim.address(), "GET", "/sim/ble/status", Some(ENDPOINT), b"").1)["request_id"], "frame-complete");
}

#[test]
fn committed_bundle_survives_a_lost_ack_and_replays_the_same_job() {
    let mut sim = Simulator::start("02:00:00:00:00:36", &[]);
    let source: Value = serde_json::from_str(include_str!(
        "../../../../tools/test-bridge/templates/quad.json"
    )).unwrap();
    let compiled = bridge_core::compile::compile(&source,
        "epd-ssd1681-200x200-1bpp").unwrap();
    let bundle = serde_json::json!({
        "bridge_id":"bridge-test", "job_id":"lost-ack-job",
        "firmware_target":"codex-status-154g",
        "render_target":"epd-ssd1681-200x200-1bpp", "compiler_abi":2,
        "profile":{"template_ids":["quad"],"initial_active_id":"quad"},
        "templates":[{"key":{"template_id":"quad",
            "render_target":"epd-ssd1681-200x200-1bpp"},
            "source":source,"compiled":compiled}],"resources":[],"bindings":[]
    });
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    assert_eq!(request(sim.address(), "POST", "/sim/fault", Some(CONTROL),
        br#"{"stall_ack_after":"bundle_commit"}"#).0, 200);
    let lost = bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:36", "bridge-test", &bytes, 4096,
        Duration::from_secs(1));
    assert!(lost.is_err(), "the commit response must time out");
    let truth = sim_state(&sim);
    assert_eq!(truth["fault"]["stall_ack_after"], Value::Null);
    assert_eq!(truth["bundle"]["job_id"], "lost-ack-job");
    let status = json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1);
    let context = status["active_context_id"].clone();
    let replay = bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:36", "bridge-test", &bytes, 4096,
        Duration::from_secs(10)).unwrap();
    assert_eq!(replay["result"], "applied");
    assert_eq!(replay["active_context_id"], context);
    assert_eq!(sim_state(&sim)["bundle"]["commit_seq"], truth["bundle"]["commit_seq"]);
    let data_dir = sim.data_dir.clone();
    sim.stop_preserving_data();
    let restarted = Simulator::start_with_dir("02:00:00:00:00:36", &[], data_dir, true);
    assert_eq!(json_body(&request(restarted.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1)["committed_job_id"], "lost-ack-job");
}

#[test]
fn a_second_process_cannot_open_the_same_instance_directory() {
    let sim = Simulator::start("02:00:00:00:00:33", &[]);
    let output = Command::new(env!("CARGO_BIN_EXE_device-sim"))
        .args(["--listen", "127.0.0.1:0", "--mac", "02:00:00:00:00:33", "--data-dir"])
        .arg(&sim.data_dir)
        .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", ENDPOINT)
        .env("CODEX_STATUS_SIM_DEVICE_TOKEN", DEVICE)
        .env("CODEX_STATUS_SIM_CONTROL_TOKEN", CONTROL)
        .output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(request(sim.address(), "GET", "/status.json", None, b"").0, 200);
}

#[test]
fn note4_reports_its_own_target_and_rejects_a_154g_bundle() {
    let mut sim = Simulator::start("02:00:00:00:00:32", &["--target", "zectrix-note4-400x300"]);
    let public = json_body(&request(sim.address(), "GET", "/status.json", None, b"").1);
    assert_eq!(public["fw_target"], "zectrix-note4-400x300");
    assert_eq!(public["render_target"], "epd-ssd2683-400x300-1bpp");
    assert_eq!(public["width"], 400);
    assert_eq!(public["max_templates"], 8);
    let source: Value = serde_json::from_str(include_str!(
        "../../../../tools/test-bridge/templates/quad.json"
    )).unwrap();
    let compiled = bridge_core::compile::compile(&source, "epd-ssd1681-200x200-1bpp").unwrap();
    let wrong = serde_json::json!({
        "bridge_id":"bridge-test", "job_id":"wrong-target",
        "firmware_target":"codex-status-154g",
        "render_target":"epd-ssd1681-200x200-1bpp", "compiler_abi":2,
        "profile":{"template_ids":["quad"],"initial_active_id":"quad"},
        "templates":[{"key":{"template_id":"quad",
            "render_target":"epd-ssd1681-200x200-1bpp"},
            "source":source,"compiled":compiled}], "resources":[],"bindings":[]
    });
    let bytes = bridge_core::template::canonical_bytes(&wrong);
    let ack = bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:32", "bridge-test", &bytes, 4096, Duration::from_secs(3)).unwrap();
    assert_eq!(ack["result"], "rejected", "{ack}");
    assert_eq!(json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1)["configured"], false);

    let data_dir = sim.data_dir.clone();
    sim.stop_preserving_data();
    let sim = Simulator::start_with_dir("02:00:00:00:00:32",
        &["--target", "zectrix-note4-400x300"], data_dir, true);

    let source: Value = serde_json::from_str(include_str!(
        "../../core/tests/fixtures/codex-status-a-400x300.json"
    )).unwrap();
    let id = source["id"].as_str().unwrap();
    let compiled = bridge_core::compile::compile(&source, "epd-ssd2683-400x300-1bpp").unwrap();
    let right = serde_json::json!({
        "bridge_id":"bridge-test", "job_id":"note4-good",
        "firmware_target":"zectrix-note4-400x300",
        "render_target":"epd-ssd2683-400x300-1bpp", "compiler_abi":2,
        "profile":{"template_ids":[id],"initial_active_id":id},
        "templates":[{"key":{"template_id":id,
            "render_target":"epd-ssd2683-400x300-1bpp"},
            "source":source,"compiled":compiled}], "resources":[],"bindings":[]
    });
    let bytes = bridge_core::template::canonical_bytes(&right);
    let ack = bridge_core::device_client::install_bundle(&sim.address().to_string(), ENDPOINT,
        "02:00:00:00:00:32", "bridge-test", &bytes, 4096, Duration::from_secs(3)).unwrap();
    assert_eq!(ack["result"], "applied", "{ack}");
    assert_eq!(json_body(&request(sim.address(), "GET", "/api/status",
        Some(ENDPOINT), b"").1)["committed_job_id"], "note4-good");
}

#[test]
fn startup_requires_loopback_laa_mac_and_all_tokens() {
    let executable = env!("CARGO_BIN_EXE_device-sim");
    let base = || {
        let mut command = Command::new(executable);
        command
            .args([
                "--listen",
                "127.0.0.1:0",
                "--mac",
                "02:00:00:00:00:01",
                "--data-dir",
            ])
            .arg(temp_data_dir());
        command.env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", ENDPOINT);
        command.env("CODEX_STATUS_SIM_DEVICE_TOKEN", DEVICE);
        command.env("CODEX_STATUS_SIM_CONTROL_TOKEN", CONTROL);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command
    };

    let output = base().args(["--listen", "0.0.0.0:0"]).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());

    let output = base()
        .args(["--mac", "70:04:1D:AA:BB:CC"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());

    let mut missing = base();
    missing.env_remove("CODEX_STATUS_SIM_DEVICE_TOKEN");
    let output = missing.output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());

    let mut equal_tokens = base();
    equal_tokens.env("CODEX_STATUS_SIM_DEVICE_TOKEN", ENDPOINT);
    let output = equal_tokens.output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());

    let mut relative_dir = Command::new(executable);
    relative_dir
        .args([
            "--listen",
            "127.0.0.1:0",
            "--mac",
            "02:00:00:00:00:01",
            "--data-dir",
            "relative-simulator-data",
        ])
        .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", ENDPOINT)
        .env("CODEX_STATUS_SIM_DEVICE_TOKEN", DEVICE)
        .env("CODEX_STATUS_SIM_CONTROL_TOKEN", CONTROL)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = relative_dir.output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());

    let executable_data = PathBuf::from(executable).parent().unwrap().join("data");
    let mut protected_dir = Command::new(executable);
    protected_dir
        .args([
            "--listen",
            "127.0.0.1:0",
            "--mac",
            "02:00:00:00:00:01",
            "--data-dir",
        ])
        .arg(executable_data)
        .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", ENDPOINT)
        .env("CODEX_STATUS_SIM_DEVICE_TOKEN", DEVICE)
        .env("CODEX_STATUS_SIM_CONTROL_TOKEN", CONTROL)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = protected_dir.output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn ready_and_status_are_safe_and_use_shared_builder() {
    let sim = Simulator::start("02:ab:cd:00:00:01", &["--seed", "44"]);
    assert_eq!(sim.ready["schema_version"], 1);
    assert_eq!(sim.ready["mac"], "02:AB:CD:00:00:01");
    assert_eq!(
        sim.ready["capabilities"],
        serde_json::json!(["device_status", "clock_control", "claim", "plan_state",
            "bundle_transfer", "bundle_persistence", "data_render", "activate",
            "power_sleep_http", "power_lifecycle", "button_cycle", "ota_catalog", "fake_ble_rendezvous", "sync_v1", "wifi_setup_transactions", "wifi_full_snapshot_sync"])
    );
    let ready_text = sim.ready.to_string();
    assert!(!ready_text.contains(ENDPOINT));
    assert!(!ready_text.contains(DEVICE));
    assert!(!ready_text.contains(CONTROL));

    for token in [None, Some(CONTROL), Some(DEVICE)] {
        let (code, body) = request(sim.address(), "GET", "/api/status", token, b"");
        assert_eq!(code, 401);
        assert_eq!(json_body(&body)["result"], "unauthorized");
    }
    let (code, body) = request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"");
    assert_eq!(code, 200);
    let status = json_body(&body);
    assert_eq!(status["result"], "applied");
    assert!(status.get("protocol").is_none());
    assert_eq!(status["device_mac"], "02:AB:CD:00:00:01");
    assert_eq!(status["configured"], false);
    assert_eq!(status["active_template_id"], "");
    assert_eq!(status["data_seq"], 0);
    assert_eq!(status["power"]["battery"], 75);
    assert_eq!(status["session_nonce"].as_str().unwrap().len(), 32);
}

#[test]
fn token_domains_are_separate_and_sim_state_discloses_no_secrets() {
    let sim = Simulator::start("02:00:00:00:00:02", &[]);
    for token in [None, Some(ENDPOINT), Some(DEVICE)] {
        let (code, _) = request(sim.address(), "GET", "/sim/state", token, b"");
        assert_eq!(code, 401);
    }
    let (code, body) = request(sim.address(), "GET", "/sim/state", Some(CONTROL), b"");
    assert_eq!(code, 200);
    let state = json_body(&body);
    assert_eq!(state["mac"], "02:00:00:00:00:02");
    assert_eq!(
        state["capabilities"],
        serde_json::json!(["device_status", "clock_control", "claim", "plan_state",
            "bundle_transfer", "bundle_persistence", "data_render", "activate",
            "power_sleep_http", "power_lifecycle", "button_cycle", "ota_catalog", "fake_ble_rendezvous", "sync_v1", "wifi_setup_transactions", "wifi_full_snapshot_sync"])
    );
    assert_eq!(state["clock_persistence"], "instance_file");
    assert_eq!(state["plan"]["accepted"], false);
    assert!(state["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "physical_power"));
    assert_eq!(state["owner"], Value::Null);
    assert!(state["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item != "clock-control" && item != "claim"));
    assert!(state.get("session_nonce").is_none());
    let text = state.to_string();
    assert!(!text.contains(ENDPOINT));
    assert!(!text.contains(DEVICE));
    assert!(!text.contains(CONTROL));
}

#[test]
fn writes_require_their_domain_token_then_reject_unconfigured_commands() {
    let sim = Simulator::start("02:00:00:00:00:03", &[]);
    for path in [
        "/api/data",
        "/api/activate",
    ] {
        assert_eq!(
            request(sim.address(), "POST", path, Some(DEVICE), b"{}").0,
            401
        );
        let (code, body) = request(sim.address(), "POST", path, Some(ENDPOINT), b"{}");
        assert_eq!(code, 200, "{path}");
        assert_eq!(json_body(&body)["result"], "rejected");
    }
    for path in ["/api/bundle/begin", "/api/bundle/chunk", "/api/bundle/commit"] {
        assert_eq!(request(sim.address(), "POST", path, Some(DEVICE), b"{}").0, 401);
        assert_ne!(request(sim.address(), "POST", path, Some(ENDPOINT), b"{}").0, 501);
    }
    let (code, body) = request(sim.address(), "POST", "/claim", Some(ENDPOINT), b"{}");
    assert_eq!(code, 401);
    assert_eq!(json_body(&body)["error"],"unauthorized");
    let (code, body) = request(sim.address(), "POST", "/claim", Some(DEVICE), b"{}");
    assert_eq!(code, 401);
    assert_eq!(json_body(&body)["error"],"unauthorized");
}

#[test]
fn oversized_write_and_unknown_routes_are_bounded_and_not_implemented() {
    let sim = Simulator::start("02:00:00:00:00:04", &[]);
    let large = vec![b'x'; 65 * 1024];
    assert_eq!(
        request(sim.address(), "POST", "/api/data", Some(ENDPOINT), &large).0,
        413
    );
    assert_eq!(request(sim.address(), "GET", "/status.json", None, b"").0, 200);
    assert_eq!(
        request(sim.address(), "GET", "/sim/unknown", Some(CONTROL), b"").0,
        404
    );
}

#[test]
fn simulator_instances_have_independent_addresses_and_identity() {
    let first = Simulator::start("02:00:00:00:00:05", &[]);
    let second = Simulator::start("02:00:00:00:00:06", &[]);
    assert_ne!(first.address(), second.address());
    assert_ne!(first.ready["mac"], second.ready["mac"]);
    assert_ne!(first.data_dir, second.data_dir);
    let (code, claimed) = claim(&first, "id=first-owner", Some(DEVICE));
    assert_eq!(code, 200);
    assert_eq!(claimed["owner"]["id"], "first-owner");
    assert_eq!(sim_state(&second)["owner"], Value::Null);
    let (code, body) = request(first.address(), "GET", "/api/status", Some(ENDPOINT), b"");
    assert_eq!(code, 200);
    assert_eq!(json_body(&body)["device_mac"], first.ready["mac"]);
    let (code, body) = request(second.address(), "GET", "/api/status", Some(ENDPOINT), b"");
    assert_eq!(code, 200);
    assert_eq!(json_body(&body)["device_mac"], second.ready["mac"]);
}

fn time(sim: &Simulator, token: Option<&str>) -> (u16, Value) {
    let (code, body) = request(sim.address(), "GET", "/sim/time", token, b"");
    (code, json_body(&body))
}

fn set_time(sim: &Simulator, token: Option<&str>, command: &Value) -> (u16, Value) {
    let body = command.to_string();
    let (code, response) = request(sim.address(), "POST", "/sim/time", token, body.as_bytes());
    let result = serde_json::from_slice(&response).unwrap_or(Value::Null);
    (code, result)
}

fn plan_message(
    status: &Value,
    bridge_id: &str,
    request_id: &str,
    plan_id: u64,
    mode: &str,
    duration: Option<u64>,
) -> Value {
    let mut message = serde_json::json!({
        "device_mac": status["device_mac"],
        "session_nonce": status["session_nonce"],
        "bridge_id": bridge_id,
        "request_id": request_id,
        "plan_id": plan_id,
        "mode": mode,
    });
    if let Some(duration) = duration {
        message["light_duration_s"] = duration.into();
    }
    message
}

fn post_plan(sim: &Simulator, token: Option<&str>, message: &Value) -> (u16, Value) {
    let body = message.to_string();
    let (code, response) = request(sim.address(), "POST", "/api/plan", token, body.as_bytes());
    (code, json_body(&response))
}

#[test]
fn sim_clock_defaults_to_one_x_and_pause_step_and_rate_are_continuous() {
    let sim = Simulator::start("02:00:00:00:00:07", &["--epoch-ms", "5000"]);
    assert_eq!(time(&sim, Some(CONTROL)).0, 200);
    for token in [None, Some(ENDPOINT), Some(DEVICE)] {
        assert_eq!(time(&sim, token).0, 401);
        assert_eq!(
            set_time(&sim, token, &serde_json::json!({"op":"rate","rate_ppm":0})).0,
            401
        );
    }

    let (code, initial) = time(&sim, Some(CONTROL));
    assert_eq!(code, 200);
    assert_eq!(initial["rate_ppm"], 1_000_000);
    assert_eq!(initial["uptime_ms"], initial["monotonic_ms"]);
    assert_eq!(
        initial["wall_ms"].as_u64().unwrap() - initial["monotonic_ms"].as_u64().unwrap(),
        5000
    );
    let (code, _) = set_time(
        &sim,
        Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":1}),
    );
    assert_eq!(code, 400, "step is only valid while paused");
    assert_eq!(time(&sim, Some(CONTROL)).1["rate_ppm"], 1_000_000);
    std::thread::sleep(Duration::from_millis(60));
    let (code, running) = time(&sim, Some(CONTROL));
    assert_eq!(code, 200);
    assert!(running["monotonic_ms"].as_u64().unwrap() >= initial["monotonic_ms"].as_u64().unwrap());

    let (code, paused) = set_time(
        &sim,
        Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0}),
    );
    assert_eq!(code, 200);
    let stopped_at = paused["monotonic_ms"].as_u64().unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(time(&sim, Some(CONTROL)).1["monotonic_ms"], stopped_at);

    let (code, stepped) = set_time(
        &sim,
        Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":12345}),
    );
    assert_eq!(code, 200);
    assert_eq!(stepped["monotonic_ms"], stopped_at + 12345);
    let (code, accelerated) = set_time(
        &sim,
        Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":2_000_000}),
    );
    assert_eq!(code, 200);
    let resumed_at = accelerated["monotonic_ms"].as_u64().unwrap();
    assert_eq!(resumed_at, stopped_at + 12345);
    assert_eq!(accelerated["rate_ppm"], 2_000_000);
    std::thread::sleep(Duration::from_millis(60));
    let (code, resumed) = time(&sim, Some(CONTROL));
    assert_eq!(code, 200);
    assert!(resumed["monotonic_ms"].as_u64().unwrap() >= resumed_at);
}

#[test]
fn plan_acks_share_firmware_decisions_and_replays_keep_the_deadline() {
    let sim = Simulator::start("02:00:00:00:00:20", &[]);
    assert_eq!(
        set_time(
            &sim,
            Some(CONTROL),
            &serde_json::json!({"op":"rate","rate_ppm":0})
        )
        .0,
        200
    );
    let initial_ms = time(&sim, Some(CONTROL)).1["monotonic_ms"]
        .as_u64()
        .unwrap();
    let (code, body) = request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"");
    assert_eq!(code, 200);
    let status = json_body(&body);
    let first = plan_message(&status, "bridge-a", "plan-1", 1, "light", Some(120));
    let (code, ack) = post_plan(&sim, Some(ENDPOINT), &first);
    assert_eq!(code, 200);
    assert_eq!(ack["result"], "applied");
    assert_eq!(ack["display_state"], "unchanged");
    assert_eq!(ack["retention"], "ram");
    assert_eq!(ack["plan_id"], 1);
    assert_eq!(ack["accepted_remaining_s"], 120);
    assert_eq!(ack["fw_target"], "codex-status-154g");
    assert!(ack.get("active_context_id").is_none());

    let before = sim_state(&sim)["plan"].clone();
    assert_eq!(before["accepted"], true);
    assert_eq!(before["accepted_at_ms"], initial_ms);
    assert_eq!(
        set_time(
            &sim,
            Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":10000})
        )
        .0,
        200
    );
    let (code, replay) = post_plan(&sim, Some(ENDPOINT), &first);
    assert_eq!(code, 200);
    assert_eq!(replay["result"], "applied");
    assert_eq!(replay["accepted_remaining_s"], 110);
    let (code, body) = request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"");
    assert_eq!(code, 200);
    assert_eq!(json_body(&body)["power"]["plan_id"], 1);
    assert_eq!(json_body(&body)["power"]["remaining_s"], 110);
    assert_eq!(json_body(&body)["power"]["granted_s"], 120);

    let conflict = plan_message(&status, "bridge-a", "plan-conflict", 1, "light", Some(121));
    let (code, rejected) = post_plan(&sim, Some(ENDPOINT), &conflict);
    assert_eq!(code, 200);
    assert_eq!(rejected["result"], "rejected");
    assert_eq!(rejected["display_state"], "unchanged");
    assert_eq!(rejected["error"], "plan_conflict");
    assert_eq!(rejected["plan_id"], 1);
    assert_eq!(rejected["accepted_remaining_s"], 0);
    assert_eq!(sim_state(&sim)["plan"]["accepted_at_ms"], initial_ms);

    let stale = plan_message(&status, "bridge-a", "plan-stale", 0, "sleep", None);
    let (code, rejected) = post_plan(&sim, Some(ENDPOINT), &stale);
    assert_eq!(code, 200);
    assert_eq!(rejected["error"], "stale_plan");
    assert!(rejected.get("plan_id").is_none());
    assert_eq!(rejected["accepted_remaining_s"], 0);
    assert_eq!(sim_state(&sim)["plan"]["plan_id"], 1);

    let malformed_shape = plan_message(&status, "bridge-a", "plan-shape", 2, "light", None);
    let (code, rejected) = post_plan(&sim, Some(ENDPOINT), &malformed_shape);
    assert_eq!(code, 200);
    assert_eq!(rejected["error"], "plan_shape");
    assert_eq!(rejected["accepted_remaining_s"], 0);
    assert_eq!(sim_state(&sim)["plan"]["plan_id"], 1);

    let sleep = plan_message(&status, "bridge-a", "plan-sleep", 2, "sleep", None);
    let (code, accepted_sleep) = post_plan(&sim, Some(ENDPOINT), &sleep);
    assert_eq!(code, 200);
    assert_eq!(accepted_sleep["result"], "applied");
    assert_eq!(accepted_sleep["plan_id"], 2);
    assert_eq!(accepted_sleep["accepted_remaining_s"], 0);
}

#[test]
fn joint_plan_keeps_valid_power_when_optional_data_is_rejected() {
    let sim = Simulator::start("02:00:00:00:00:2A", &[]);
    let status = json_body(&request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"").1);
    let mut plan = plan_message(&status, "bridge-a", "joint-1", 1, "light", Some(120));
    plan["data"] = serde_json::json!({"seq":1,"active_context_id":"bad-context",
        "crc":"00000000","fields":[]});
    let (code, ack) = post_plan(&sim, Some(ENDPOINT), &plan);
    assert_eq!(code, 200);
    assert_eq!(ack["result"], "applied");
    assert_eq!(ack["plan_id"], 1);
    assert_eq!(ack["data"]["result"], "rejected");
    assert_eq!(ack["data"]["error"], "unconfigured");
    assert_eq!(sim_state(&sim)["plan"]["plan_id"], 1);
    let mut next = plan_message(&status, "bridge-a", "joint-ble", 2, "light", Some(120));
    next["op"] = "plan".into();
    next["token"] = ENDPOINT.into();
    next["data"] = plan["data"].clone();
    let (ble_code, ble_body) = request(sim.address(), "POST", "/sim/ble/command",
        Some(ENDPOINT), next.to_string().as_bytes());
    assert_eq!(ble_code, 200);
    let ble_ack = json_body(&ble_body);
    assert_eq!(ble_ack["result"], "applied");
    assert_eq!(ble_ack["data"]["error"], "unconfigured");
    assert_eq!(sim_state(&sim)["plan"]["plan_id"], 2);
    let mut invalid = plan_message(&status, "bridge-a", "joint-bad", 2, "light", Some(120));
    invalid["session_nonce"] = "invalid".into();
    invalid["data"] = plan["data"].clone();
    let (_, rejected) = post_plan(&sim, Some(ENDPOINT), &invalid);
    assert_eq!(rejected["result"], "rejected");
    assert_eq!(sim_state(&sim)["plan"]["plan_id"], 2);
}

#[test]
fn plan_checks_json_session_owner_and_token_order_without_advancing_state() {
    let sim = Simulator::start("02:00:00:00:00:21", &[]);
    assert_eq!(
        set_time(
            &sim,
            Some(CONTROL),
            &serde_json::json!({"op":"rate","rate_ppm":0})
        )
        .0,
        200
    );
    for token in [None, Some(DEVICE), Some(CONTROL)] {
        assert_eq!(
            request(sim.address(), "POST", "/api/plan", token, b"{}").0,
            401
        );
    }
    let (code, malformed) = request(sim.address(), "POST", "/api/plan", Some(ENDPOINT), b"{");
    assert_eq!(code, 200);
    assert_eq!(
        json_body(&malformed),
        serde_json::json!({
            "op":"command", "result":"rejected", "display_state":"unchanged",
            "retention":"ram", "error":"json", "fw_target":"codex-status-154g"
        })
    );
    let (code, status_body) = request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"");
    assert_eq!(code, 200);
    let status = json_body(&status_body);

    let mut bad_session = plan_message(&status, "bridge-a", "bad-session", 1, "light", Some(90));
    bad_session["session_nonce"] = "wrong".into();
    let (code, rejected) = post_plan(&sim, Some(ENDPOINT), &bad_session);
    assert_eq!(code, 200);
    assert_eq!(rejected["op"], "command");
    assert_eq!(rejected["result"], "rejected");
    assert_eq!(rejected["display_state"], "unchanged");
    assert_eq!(rejected["error"], "session");
    let mut bad_mac = plan_message(&status, "bridge-a", "bad-mac", 1, "light", Some(90));
    bad_mac["device_mac"] = "02:00:00:00:00:FF".into();
    let (code, rejected) = post_plan(&sim, Some(ENDPOINT), &bad_mac);
    assert_eq!(code, 200);
    assert_eq!(rejected["error"], "session");
    assert_eq!(sim_state(&sim)["plan"]["plan_id"], 0);

    let (code, _) = claim(&sim, "id=held-owner", Some(DEVICE));
    assert_eq!(code, 200);
    let mut occupied = plan_message(&status, "other-owner", "occupied", 1, "light", Some(90));
    let (code, body) = post_plan(&sim, Some(ENDPOINT), &occupied);
    assert_eq!(code, 409);
    assert_eq!(
        body,
        serde_json::json!({
            "result":"rejected", "error":"occupied",
            "owner": {"id":"held-owner", "name":"", "host":"", "port":0,
                "since_s":0, "last_seen_s":0, "lease_s":300, "expires_in_s":300}
        })
    );
    assert_eq!(sim_state(&sim)["plan"]["plan_id"], 0);
    occupied["bridge_id"] = "held-owner".into();
    let persisted: Value =
        serde_json::from_slice(&fs::read(sim.data_dir.join("owner.json")).unwrap()).unwrap();
    let old_seen = persisted["owner"]["last_seen"].clone();
    assert_eq!(
        set_time(
            &sim,
            Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":2000})
        )
        .0,
        200
    );
    let (code, ack) = post_plan(&sim, Some(ENDPOINT), &occupied);
    assert_eq!(code, 200);
    assert_eq!(ack["result"], "applied");
    assert_eq!(sim_state(&sim)["owner"]["last_seen_s"], 2);
    let persisted: Value =
        serde_json::from_slice(&fs::read(sim.data_dir.join("owner.json")).unwrap()).unwrap();
    assert_eq!(persisted["owner"]["last_seen"], old_seen);
}

#[test]
fn plan_deadline_uses_uptime_and_each_process_has_independent_plan_state() {
    let first = Simulator::start("02:00:00:00:00:22", &[]);
    let second = Simulator::start("02:00:00:00:00:23", &[]);
    for sim in [&first, &second] {
        assert_eq!(
            set_time(
                sim,
                Some(CONTROL),
                &serde_json::json!({"op":"rate","rate_ppm":0})
            )
            .0,
            200
        );
    }
    let get_status = |sim: &Simulator| {
        let (code, body) = request(sim.address(), "GET", "/api/status", Some(ENDPOINT), b"");
        assert_eq!(code, 200);
        json_body(&body)
    };
    let first_status = get_status(&first);
    let second_status = get_status(&second);
    let (code, _) = post_plan(
        &first,
        Some(ENDPOINT),
        &plan_message(&first_status, "bridge", "first", 7, "light", Some(120)),
    );
    assert_eq!(code, 200);
    let (code, _) = post_plan(
        &second,
        Some(ENDPOINT),
        &plan_message(&second_status, "bridge", "second", 1, "light", Some(120)),
    );
    assert_eq!(code, 200);
    assert_eq!(get_status(&first)["power"]["plan_id"], 7);
    assert_eq!(get_status(&second)["power"]["plan_id"], 1);

    assert_eq!(
        set_time(
            &first,
            Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":5000})
        )
        .0,
        200
    );
    let remaining_before_wall = get_status(&first)["power"]["remaining_s"].clone();
    assert_eq!(
        set_time(
            &first,
            Some(CONTROL),
            &serde_json::json!({"op":"wall","offset_ms":900000})
        )
        .0,
        200
    );
    assert_eq!(
        get_status(&first)["power"]["remaining_s"],
        remaining_before_wall
    );
}

#[test]
fn wall_offsets_do_not_change_monotonic_and_invalid_commands_are_atomic() {
    let sim = Simulator::start("02:00:00:00:00:08", &["--epoch-ms", "1000"]);
    let (_, paused) = set_time(
        &sim,
        Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0}),
    );
    let mono = paused["monotonic_ms"].as_u64().unwrap();

    let (code, forward) = set_time(
        &sim,
        Some(CONTROL),
        &serde_json::json!({"op":"wall","offset_ms":500}),
    );
    assert_eq!(code, 200);
    assert_eq!(forward["monotonic_ms"], mono);
    assert_eq!(forward["wall_offset_ms"], 500);
    assert_eq!(forward["wall_ms"], mono + 1500);

    let (code, backward) = set_time(
        &sim,
        Some(CONTROL),
        &serde_json::json!({"op":"wall","offset_ms":-250}),
    );
    assert_eq!(code, 200);
    assert_eq!(backward["monotonic_ms"], mono);
    assert_eq!(backward["wall_offset_ms"], -250);
    assert_eq!(backward["wall_ms"], mono + 750);

    for invalid in [
        serde_json::json!({"op":"rate","rate_ppm":1_000_000_001}),
        serde_json::json!({"op":"step","delta_ms":86_400_001}),
        serde_json::json!({"op":"wall","offset_ms":-10000}),
        serde_json::json!({"op":"wall","offset_ms":0,"extra":true}),
        serde_json::json!({"op":"max"}),
    ] {
        let (code, _) = set_time(&sim, Some(CONTROL), &invalid);
        assert_eq!(code, 400, "{invalid}");
        let (_, current) = time(&sim, Some(CONTROL));
        assert_eq!(current["monotonic_ms"], mono);
        assert_eq!(current["wall_offset_ms"], -250);
        assert_eq!(current["rate_ppm"], 0);
    }
    let (code, _) = set_time(
        &sim,
        Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":1.5}),
    );
    assert_eq!(code, 400);
}

#[test]
fn instances_advance_at_independent_rates() {
    let first = Simulator::start("02:00:00:00:00:09", &[]);
    let second = Simulator::start("02:00:00:00:00:0A", &[]);
    let (code, paused) = set_time(
        &second,
        Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0}),
    );
    assert_eq!(code, 200);
    let second_at = paused["monotonic_ms"].as_u64().unwrap();
    let (_, first_at) = time(&first, Some(CONTROL));
    std::thread::sleep(Duration::from_millis(60));
    let (_, first_later) = time(&first, Some(CONTROL));
    let (_, second_later) = time(&second, Some(CONTROL));
    assert!(
        first_later["monotonic_ms"].as_u64().unwrap() > first_at["monotonic_ms"].as_u64().unwrap()
    );
    assert_eq!(second_later["monotonic_ms"].as_u64().unwrap(), second_at);
}

#[test]
fn claim_uses_shared_decision_and_enforces_token_owner_actions() {
    let sim = Simulator::start("02:00:00:00:00:0B", &[]);
    let empty_release = claim(&sim, "id=owner-a&release=1", Some(DEVICE));
    assert_eq!(empty_release.0, 200);
    assert_eq!(
        empty_release.1,
        serde_json::json!({"owner":null,"released":false})
    );
    assert_eq!(
        claim(&sim, "id=%20%20%20", Some(DEVICE)),
        (400, serde_json::json!({"error":"args"}))
    );

    let (code, unauthorized) = claim(&sim, "id=owner-a", None);
    assert_eq!(code, 401);
    assert_eq!(
        unauthorized,
        serde_json::json!({"error":"unauthorized","owner":null})
    );
    let (code, unauthorized) = claim(&sim, "id=owner-a", Some(ENDPOINT));
    assert_eq!(code, 401);
    assert_eq!(unauthorized["owner"], Value::Null);

    let oversized_id = format!("{}zQ", "é".repeat(31));
    let long_name = format!("{}z", "é".repeat(16));
    let query = claim_query(&[
        ("id", &oversized_id),
        ("name", &long_name),
        ("host", "host\nname"),
        ("port", "65535"),
        ("lease", "9999"),
    ]);
    let (code, first) = claim(&sim, &query, Some(DEVICE));
    assert_eq!(code, 200);
    assert_eq!(first["renew"], false);
    assert_eq!(first["owner"]["id"], format!("{}z", "é".repeat(31)));
    assert_eq!(first["owner"]["name"], "é".repeat(16));
    assert_eq!(first["owner"]["host"], "host?name");
    assert_eq!(first["owner"]["port"], 65535);
    assert_eq!(first["owner"]["lease_s"], 3600);
    let since = first["owner"]["since_s"].clone();
    assert_eq!(
        set_time(
            &sim,
            Some(CONTROL),
            &serde_json::json!({"op":"rate","rate_ppm":0})
        )
        .0,
        200
    );
    let seen = first["owner"]["last_seen_s"].clone();
    assert_eq!(claim(&sim, "id=%20%20%20", Some(DEVICE)).0, 400);
    for path in [
        "/api/data",
        "/api/activate",
    ] {
        assert_eq!(request(sim.address(), "POST", path, Some(ENDPOINT), b"{}").0, 409);
        let after_other_write = sim_state(&sim)["owner"].clone();
        assert_eq!(after_other_write["id"], first["owner"]["id"]);
        assert_eq!(after_other_write["last_seen_s"], seen);
    }
    for path in ["/api/bundle/begin", "/api/bundle/chunk", "/api/bundle/commit"] {
        assert_eq!(request(sim.address(), "POST", path, Some(ENDPOINT), b"{}").0, 409);
    }
    assert_eq!(
        request(sim.address(), "POST", "/api/plan", Some(ENDPOINT), b"{}").0,
        409
    );
    assert_eq!(sim_state(&sim)["owner"]["last_seen_s"], seen);

    let (code, unauthorized) = claim(&sim, "id=ignored", Some(CONTROL));
    assert_eq!(code, 401);
    assert_eq!(unauthorized["owner"]["id"], first["owner"]["id"]);

    let renew_query = claim_query(&[("id", &format!("{}z", "é".repeat(31))), ("name", "renewed")]);
    let (code, renewed) = claim(&sim, &renew_query, Some(DEVICE));
    assert_eq!(code, 200);
    assert_eq!(renewed["renew"], true);
    assert_eq!(renewed["owner"]["since_s"], since);
    assert_eq!(renewed["owner"]["name"], "renewed");

    let (code, occupied) = claim(&sim, "id=other", Some(DEVICE));
    assert_eq!(code, 409);
    assert_eq!(occupied["error"], "occupied");
    assert_eq!(occupied["owner"]["id"], first["owner"]["id"]);

    let (code, forced) = claim(&sim, "id=other&force=1", Some(DEVICE));
    assert_eq!(code, 200);
    assert_eq!(forced["renew"], false);
    assert_eq!(forced["owner"]["id"], "other");
    let (code, released) = claim(&sim, "id=other&release=1", Some(DEVICE));
    assert_eq!(code, 200);
    assert_eq!(released, serde_json::json!({"owner":null,"released":true}));
    assert_eq!(sim_state(&sim)["owner"], Value::Null);
}

#[test]
fn claim_lease_uses_paused_uptime_exact_boundary_and_ignores_wall_steps() {
    let sim = Simulator::start("02:00:00:00:00:0C", &[]);
    assert_eq!(
        set_time(
            &sim,
            Some(CONTROL),
            &serde_json::json!({"op":"rate","rate_ppm":0})
        )
        .0,
        200
    );
    let (code, claimed) = claim(&sim, "id=lease-owner&lease=60", Some(DEVICE));
    assert_eq!(code, 200);
    let seen = claimed["owner"]["last_seen_s"].as_u64().unwrap() as u32;
    let before = time(&sim, Some(CONTROL)).1["monotonic_ms"]
        .as_u64()
        .unwrap();
    let target = u64::from(seen.wrapping_add(60)) * 1000;
    assert!(target >= before);
    assert_eq!(
        set_time(
            &sim,
            Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":target-before})
        )
        .0,
        200
    );
    assert_eq!(
        set_time(
            &sim,
            Some(CONTROL),
            &serde_json::json!({"op":"wall","offset_ms":500000})
        )
        .0,
        200
    );
    let at_boundary = sim_state(&sim)["owner"].clone();
    assert_eq!(at_boundary["id"], "lease-owner");
    assert_eq!(at_boundary["expires_in_s"], 0);
    assert_eq!(at_boundary["last_seen_s"], seen);

    assert_eq!(
        set_time(
            &sim,
            Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":1000})
        )
        .0,
        200
    );
    assert_eq!(sim_state(&sim)["owner"], Value::Null);
    let saved: Value =
        serde_json::from_slice(&fs::read(sim.data_dir.join("owner.json")).unwrap()).unwrap();
    assert_eq!(saved["owner"], Value::Null);
}

#[test]
fn experiment_clock_survives_process_restart_without_reusing_boot_uptime() {
    let data_dir = temp_data_dir();
    let mac = "02:00:00:00:00:19";
    let mut first = Simulator::start_with_dir(mac, &["--epoch-ms", "1000000"],
        data_dir.clone(), false);
    assert_eq!(set_time(&first, Some(CONTROL),
        &serde_json::json!({"op":"rate","rate_ppm":0})).0, 200);
    assert_eq!(set_time(&first, Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":24000})).0, 200);
    assert_eq!(set_time(&first, Some(CONTROL),
        &serde_json::json!({"op":"wall","offset_ms":-500})).0, 200);
    let before = sim_state(&first);
    first.stop_preserving_data();
    let restarted = Simulator::start_with_dir(mac, &["--epoch-ms", "1000000"],
        data_dir.clone(), false);
    let after = sim_state(&restarted);
    assert_eq!(after["clock"]["monotonic_ms"], before["clock"]["monotonic_ms"]);
    assert_eq!(after["clock"]["wall_ms"], before["clock"]["wall_ms"]);
    assert_eq!(after["clock"]["rate_ppm"], 0);
    assert_eq!(after["clock"]["wall_offset_ms"], -500);
    assert_eq!(after["uptime_ms"], 0);
    assert_eq!(time(&restarted, Some(CONTROL)).1["uptime_ms"], 0);
    assert_ne!(after["boot_id"], before["boot_id"]);
    drop(restarted);
    fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn owner_is_restored_clamped_and_renewable_after_restart() {
    let data_dir = temp_data_dir();
    let mut first = Simulator::start_with_dir("02:00:00:00:00:0D", &[], data_dir.clone(), false);
    assert_eq!(
        set_time(
            &first,
            Some(CONTROL),
            &serde_json::json!({"op":"rate","rate_ppm":0})
        )
        .0,
        200
    );
    assert_eq!(
        set_time(
            &first,
            Some(CONTROL),
            &serde_json::json!({"op":"step","delta_ms":12000})
        )
        .0,
        200
    );
    let (code, claimed) = claim(&first, "id=restart-owner&lease=120", Some(DEVICE));
    assert_eq!(code, 200);
    assert!(claimed["owner"]["since_s"].as_u64().unwrap() >= 12);
    first.stop_preserving_data();

    let restarted = Simulator::start_with_dir("02:00:00:00:00:0D", &[], data_dir.clone(), false);
    let restored = sim_state(&restarted)["owner"].clone();
    assert_eq!(restored["id"], "restart-owner");
    assert_eq!(restored["since_s"], 0);
    assert_eq!(restored["last_seen_s"], 0);
    let (code, renewed) = claim(&restarted, "id=restart-owner&lease=120", Some(DEVICE));
    assert_eq!(code, 200);
    assert_eq!(renewed["renew"], true);
    assert_eq!(renewed["owner"]["since_s"], 0);
    drop(restarted);
    fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn instance_identity_rejects_mac_conflict_and_corrupt_owner_file() {
    let data_dir = temp_data_dir();
    let mut first = Simulator::start_with_dir("02:00:00:00:00:0E", &[], data_dir.clone(), false);
    let marker: Value =
        serde_json::from_slice(&fs::read(data_dir.join("simulator.json")).unwrap()).unwrap();
    assert_eq!(
        marker,
        serde_json::json!({"schema":1,"mac":"02:00:00:00:00:0E","target":"codex-status-154g"})
    );
    first.stop_preserving_data();

    let mut conflict = Command::new(env!("CARGO_BIN_EXE_device-sim"));
    conflict
        .args([
            "--listen",
            "127.0.0.1:0",
            "--mac",
            "02:00:00:00:00:0F",
            "--data-dir",
        ])
        .arg(&data_dir)
        .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", ENDPOINT)
        .env("CODEX_STATUS_SIM_DEVICE_TOKEN", DEVICE)
        .env("CODEX_STATUS_SIM_CONTROL_TOKEN", CONTROL)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = conflict.output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());

    fs::write(
        data_dir.join("owner.json"),
        br#"{"schema":1,"mac":"02:00:00:00:00:FF","owner":null}"#,
    )
    .unwrap();
    let mut wrong_owner_mac = Command::new(env!("CARGO_BIN_EXE_device-sim"));
    wrong_owner_mac
        .args([
            "--listen",
            "127.0.0.1:0",
            "--mac",
            "02:00:00:00:00:0E",
            "--data-dir",
        ])
        .arg(&data_dir)
        .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", ENDPOINT)
        .env("CODEX_STATUS_SIM_DEVICE_TOKEN", DEVICE)
        .env("CODEX_STATUS_SIM_CONTROL_TOKEN", CONTROL)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = wrong_owner_mac.output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());

    fs::write(data_dir.join("owner.json"), b"not-json").unwrap();
    let mut corrupt = Command::new(env!("CARGO_BIN_EXE_device-sim"));
    corrupt
        .args([
            "--listen",
            "127.0.0.1:0",
            "--mac",
            "02:00:00:00:00:0E",
            "--data-dir",
        ])
        .arg(&data_dir)
        .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", ENDPOINT)
        .env("CODEX_STATUS_SIM_DEVICE_TOKEN", DEVICE)
        .env("CODEX_STATUS_SIM_CONTROL_TOKEN", CONTROL)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = corrupt.output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn wifi_jobs_use_production_hal_real_ble_ack_and_query_after_lost_http_ack() {
    let _env_guard=BLE_ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    use bridge_core::platform::{service::PlatformService, model::{DeviceIdentity,DeviceCapabilities}};
    use std::collections::BTreeMap;
    let mac = "02000000004A";
    let mut sim = Simulator::start(mac, &[]);
    let root = temp_data_dir(); let svc = PlatformService::open(&root).unwrap();
    svc.device_upsert(DeviceIdentity::new(mac,"Wi-Fi fake").unwrap(),DeviceCapabilities::ssd1681_154g()).unwrap();
    let read = svc.wifi_queue(mac,"bridge-test","read",vec![],100).unwrap();
    let first = svc.wifi_attempt(mac,&read.job_id,None,&BTreeMap::new(),101,|_,_|anyhow::bail!("offline")).unwrap().unwrap();
    assert_eq!(first.state,"waiting_rendezvous");
    drop(svc); let svc = PlatformService::open(&root).unwrap();
    std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",serde_json::json!({mac:format!("http://{}",sim.address())}).to_string());
    let address = sim.address().to_string();
    let permit = tokio::runtime::Runtime::new().unwrap().block_on(async {
        assert_eq!(request(sim.address(),"POST","/sim/wifi",Some(CONTROL),br#"{"maintenance_ms":120000}"#).0,200);
        let mut link = bridge_ble::DeviceConnection::connect(mac,ENDPOINT,"bridge-test").await.unwrap().unwrap();
        assert_eq!(link.info["wifi_management"],2);
        let ack = link.command("wifi_authorize",serde_json::json!({})).await.unwrap();
        assert_eq!(ack["ok"],true,"{ack}");
        assert_eq!(ack["ble_wire"],1);
        let permit = ack["permit"].as_str().unwrap().to_owned();
        link.close().await; permit
    });
    let mut transport_ops=Vec::new();
    let mut transport = |op: &str, body: Value| {
        transport_ops.push(op.to_owned());
        if op == "snapshot" {
            let mut s = bridge_core::device_client::wifi_snapshot(&address,ENDPOINT,mac,"bridge-test",Some(&permit),"",Duration::from_secs(3)).map_err(|error| { eprintln!("Wi-Fi snapshot failed: {error:#}"); error })?;
            s.observed_at = 107; Ok(serde_json::to_value(s)?)
        } else { bridge_core::device_client::wifi_request(&address,ENDPOINT,mac,"bridge-test",op,body,Duration::from_secs(3)) }
    };
    assert_eq!(svc.wifi_attempt(mac,&read.job_id,None,&BTreeMap::new(),107,&mut transport).unwrap().unwrap().state,"completed");
    let targets = serde_json::from_value(serde_json::json!([{"network_id":"home","host_ssid":"Host5G","ssid":"Device2G","credential":"open"}])).unwrap();
    svc.wifi_save(mac,0,targets,108).unwrap();
    let job = svc.wifi_queue(mac,"bridge-test","sync",vec!["home".into()],109).unwrap();
    assert_eq!(request(sim.address(),"POST","/sim/wifi",Some(CONTROL),br#"{"drop_http_ack":true}"#).0,200);
    let awaiting = svc.wifi_attempt(mac,&job.job_id,Some(&permit),&BTreeMap::new(),110,&mut transport).unwrap().unwrap();
    assert_eq!(awaiting.state,"awaiting_confirmation"); assert!(awaiting.sent && awaiting.may_have_committed);
    let snapshot = bridge_core::device_client::wifi_snapshot(&address,ENDPOINT,mac,"bridge-test",Some(&permit),"",Duration::from_secs(3)).unwrap();
    assert_eq!(snapshot.revision,1); assert_eq!(snapshot.active["network_id"],"");
    drop(svc); let svc = PlatformService::open(&root).unwrap();
    let mut calls=Vec::new();
    let completed = svc.wifi_attempt(mac,&job.job_id,Some(&permit),&BTreeMap::new(),116,|op,body|{calls.push(op.to_owned());transport(op,body)}).unwrap().unwrap();
    assert_eq!(completed.state,"completed"); assert_eq!(completed.confirmation.as_deref(),Some("durable_receipt"));
    assert_eq!(calls,["result"]); // Never another apply after lost ACK.
    assert_eq!(svc.wifi_state(mac).snapshot.unwrap().revision,1);
    drop(transport);
    assert_eq!(transport_ops,["snapshot","apply","result"]); // Exactly one explicit full read.
    // Actual process restart restores the same blob and receipt; permits do not survive.
    sim.stop_preserving_data();
    let restarted = Simulator::start_with_dir(mac,&[],sim.data_dir.clone(),true);
    let receipt = bridge_core::device_client::wifi_request(&restarted.address().to_string(),ENDPOINT,mac,"bridge-test","result",
        serde_json::json!({"query_id":job.request_id}),Duration::from_secs(3)).unwrap();
    assert_eq!(receipt["outcome"],"committed");assert_eq!(receipt["revision"],1);
    std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS"); let _ = fs::remove_dir_all(root);
}

#[test]
fn wifi_fake_full_projection_freezes_pages_and_preserves_bond_legacy_and_atomic_rejects() {
    let _env_guard=BLE_ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mac="02000000004B";let sim=Simulator::start(mac,&[]);let address=sim.address().to_string();
    assert_eq!(request(sim.address(),"POST","/sim/wifi",Some(CONTROL),br#"{"maintenance_ms":120000,"legacy":[{"network_id":"legacy-0","ssid":"USB-AP"},{"network_id":"legacy-1","ssid":"Second-AP"}]}"#).0,200);
    std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",serde_json::json!({mac:format!("http://{}",sim.address())}).to_string());
    let runtime=tokio::runtime::Runtime::new().unwrap();
    let permit=runtime.block_on(async {
        let mut link=bridge_ble::DeviceConnection::connect(mac,ENDPOINT,"bridge-test").await.unwrap().unwrap();
        let reply=link.command("wifi_authorize",serde_json::json!({})).await.unwrap();link.close().await;
        reply["permit"].as_str().unwrap().to_owned()
    });
    let changes:Vec<_>=(0..4).map(|i|serde_json::json!({"network_id":format!("n{i}"),"host_ssid":format!("Host{i}"),"ssid":"中文网络".repeat(2),"label":"灯\\\"".repeat(8),"credential":"open"})).collect();
    let apply=bridge_core::device_client::wifi_request(&address,ENDPOINT,mac,"bridge-test","apply",serde_json::json!({
        "request_id":"batch","expected_revision":0,"permit":permit,"changes":changes}),Duration::from_secs(3)).unwrap();
    assert_eq!(apply["outcome"],"committed");
    let full=bridge_core::device_client::wifi_snapshot(&address,ENDPOINT,mac,"bridge-test",Some(&permit),"",Duration::from_secs(3)).unwrap();
    assert_eq!(full.networks.len(),6);assert_eq!(full.networks.iter().filter(|n|n.writable).count(),4);
    assert_eq!(full.networks.last().unwrap().bond,"legacy");assert_eq!(full.active["network_id"],"");
    let legacy:Vec<_>=full.networks.iter().filter(|n|n.bond=="legacy")
        .map(|n|(n.network.network_id.as_str(),n.network.ssid.as_str())).collect();
    assert_eq!(legacy,vec![("legacy-0","USB-AP"),("legacy-1","Second-AP")]);
    let first=bridge_core::device_client::wifi_request(&address,ENDPOINT,mac,"bridge-test","snapshot",serde_json::json!({"offset":0,"permit":permit}),Duration::from_secs(3)).unwrap();
    assert_eq!(first["complete"],false);
    let bad=bridge_core::device_client::wifi_request(&address,ENDPOINT,mac,"bridge-test","apply",serde_json::json!({
        "request_id":"invalid-batch","expected_revision":1,"permit":permit,"changes":[
            {"network_id":"n0","host_ssid":"Renamed","ssid":"Changed","credential":"keep"},
            // The shared store now holds 12 rows; a fifth stored row is valid.
            // Missing credentials for a new row rejects after validating n0,
            // preserving the durable-rejection and partial-update rollback check.
            {"network_id":"extra","host_ssid":"Extra","ssid":"Extra","credential":"keep"}]}),Duration::from_secs(3)).unwrap();
    assert_eq!(bad["error"],"credentials_required");
    let receipt=bridge_core::device_client::wifi_request(&address,ENDPOINT,mac,"bridge-test","result",serde_json::json!({"query_id":"invalid-batch"}),Duration::from_secs(3)).unwrap();
    assert_eq!(receipt["outcome"],"rejected");assert_eq!(receipt["revision"],1);
    assert_eq!(bridge_core::device_client::wifi_snapshot(&address,ENDPOINT,mac,"bridge-test",Some(&permit),"",Duration::from_secs(3)).unwrap().networks[0].network.host_ssid,"Host0");
    assert_eq!(request(sim.address(),"POST","/sim/wifi",Some(CONTROL),br#"{"bond":"02:00:00:00:00:03/1","maintenance_ms":120000}"#).0,200);
    let second=runtime.block_on(async {
        let mut link=bridge_ble::DeviceConnection::connect(mac,ENDPOINT,"other-installation").await.unwrap().unwrap();
        let reply=link.command("wifi_authorize",serde_json::json!({})).await.unwrap();
        // A new generation cannot consume a stale ACK; the original raw bytes
        // are still subject to the same attribute capacity guard.
        link.close().await;assert_eq!(reply["ok"],true,"{}",reply["error"]);reply["permit"].as_str().unwrap().to_owned()
    });
    let foreign=bridge_core::device_client::wifi_snapshot(&address,ENDPOINT,mac,"other-installation",Some(&second),"",Duration::from_secs(3)).unwrap();
    assert!(foreign.networks.iter().all(|n|!n.writable));
    let added=bridge_core::device_client::wifi_request(&address,ENDPOINT,mac,"other-installation","apply",serde_json::json!({
        "request_id":"foreign-own","expected_revision":1,"permit":second,"changes":[
            {"network_id":"other","host_ssid":"OtherHost","ssid":"OtherDevice","credential":"open"}]}),Duration::from_secs(3)).unwrap();
    assert_eq!(added["outcome"],"committed");
    let stale=bridge_core::device_client::wifi_request(&address,ENDPOINT,mac,"bridge-test","snapshot",serde_json::json!({
        "offset":first["next_offset"],"snapshot_id":first["snapshot_id"],"permit":permit}),Duration::from_secs(3)).unwrap();
    assert_eq!(stale["error"],"snapshot_expired");
    let full=bridge_core::device_client::wifi_snapshot(&address,ENDPOINT,mac,"other-installation",Some(&second),"",Duration::from_secs(3)).unwrap();
    assert_eq!(full.networks.len(),7);assert_eq!(full.networks.iter().filter(|n|n.writable).count(),1);
    let attrs=json_body(&request(sim.address(),"GET","/sim/ble/attributes",Some(CONTROL),b"").1);
    assert!(attrs.to_string().contains("512"));
    std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
}

#[test]
fn wifi_bounded_recovery_and_history_share_production_bytes_and_keep_display() {
    let _env_guard=BLE_ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mac="02000000004C";let sim=Simulator::start(mac,&[]);
    assert_eq!(request(sim.address(),"POST","/sim/wifi",Some(CONTROL),br#"{"maintenance_ms":120000}"#).0,200);
    let before=request(sim.address(),"GET","/sim/frame",Some(CONTROL),b"").1;
    std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",serde_json::json!({mac:format!("http://{}",sim.address())}).to_string());
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut link=bridge_ble::DeviceConnection::connect(mac,ENDPOINT,"bridge-test").await.unwrap().unwrap();
        let trial=link.command("wifi_trial",serde_json::json!({"network_id":"candidate","host_ssid":"Host","ssid":"Candidate","password":"","replace":false})).await.unwrap();
        assert_eq!(trial["stage"],"connecting");
        let outcome=link.command("wifi_setup_status",serde_json::json!({"query_id":trial["query_id"]})).await.unwrap();
        assert_eq!(outcome["stage"],"trial_complete");assert_eq!(outcome["persisted"],false);
        assert!(!sim.data_dir.join("wifi-schemes.bin").exists());
        assert_eq!(request(sim.address(),"POST","/sim/ble/fault",Some(CONTROL),br#"{"kind":"drop_ack"}"#).0,200);
        assert!(link.command_with_id("wifi_save",serde_json::json!({"expected_revision":0,"changes":[
            {"network_id":"home","host_ssid":"Host","ssid":"Stored","label":"Desk","credential":"open"}]}),"stable-recovery").await.is_err());
        let receipt=link.command("wifi_result",serde_json::json!({"query_id":"stable-recovery"})).await.unwrap();
        assert_eq!(receipt["outcome"],"committed");assert_eq!(receipt["revision"],1);
        let saved=link.wifi_read("wifi_scheme_get").await.unwrap();assert_eq!(saved["networks"][0]["ssid"],"Stored");
        assert_eq!(link.command("wifi_active",serde_json::json!({})).await.unwrap()["network_id"],"");
        let activation=link.command("wifi_activate",serde_json::json!({"network_id":"home","version":1})).await.unwrap();
        assert_eq!(activation["stage"],"connecting");
        assert_eq!(link.command("wifi_setup_status",serde_json::json!({"query_id":activation["query_id"]})).await.unwrap()["persisted"],true);
        assert_eq!(link.command("wifi_active",serde_json::json!({})).await.unwrap()["version"],1);
        link.command("status",serde_json::json!({})).await.unwrap();
        for seq in 1..=8 {
            assert_eq!(request(sim.address(),"POST","/sim/history",Some(CONTROL),serde_json::json!({
                "wake_generation":sim.ready["boot_id"],"seq":seq,"awake_ms":u32::MAX}).to_string().as_bytes()).0,200);
        }
        let history=link.command_with_id("history",serde_json::json!({"since":0,"limit":8}),&"x".repeat(64)).await.unwrap();
        assert!(!history["records"].as_array().unwrap().is_empty());assert_eq!(history["more"],true);
        assert_eq!(history["records"][0]["format"],2);
        let attributes=json_body(&request(sim.address(),"GET","/sim/ble/attributes",Some(CONTROL),b"").1);
        assert!(attributes.to_string().contains("write_failed"));
        link.close().await;
    });
    assert_eq!(request(sim.address(),"GET","/sim/frame",Some(CONTROL),b"").1,before);
    std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
}

#[test]
fn rendezvous_audit_queued_wifi_read_grants_bounded_light_window() {
    use bridge_core::platform::{service::PlatformService, model::{DeviceIdentity,DeviceCapabilities}};
    let mac = "0200000000C1";
    let root = temp_data_dir();
    let svc = PlatformService::open(&root).unwrap();
    svc.device_upsert(DeviceIdentity::new(mac,"Audit").unwrap(),DeviceCapabilities::ssd1681_154g()).unwrap();
    let before = svc.plan_for_rendezvous(mac,100,"rendezvous",0).unwrap();
    assert_eq!(serde_json::to_value(&before).unwrap()["mode"],"sleep");
    let job = svc.wifi_queue(mac,"bridge-test","read",vec![],100).unwrap();
    // W4-1 fix: the queued read is real pending work, so the rendezvous must
    // grant it a bounded formal light window. All values come from the plan API
    // itself: the old assertion required the broken "sleep" behaviour and had to
    // be inverted, not kept (w4-w6-closure-plan.md §3.2 C06).
    for now in [101,161,1001] {
        let after = svc.plan_for_rendezvous(mac,now,"rendezvous",0).unwrap();
        assert_eq!(serde_json::to_value(&after).unwrap()["mode"],"light");
        assert_eq!(after.light_duration_s, bridge_core::coordinator::WIFI_READ_LIGHT_S);
        assert!(after.light_duration_s >= bridge_core::coordinator::MIN_LIGHT_S);
        assert!(after.light_duration_s < bridge_core::coordinator::MAX_LIGHT_S);
        let job = svc.wifi_state(mac).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap();
        assert!(!job.terminal());
        // Admission must not itself retry, claim or otherwise mutate the job.
        assert_eq!(job.attempt_count, 0);
        assert!(!job.sent);
    }
    eprintln!("CONFIRMED: a queued Wi-Fi read obtains a bounded formal light window ({}s) without mutating the job.",
        bridge_core::coordinator::WIFI_READ_LIGHT_S);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rendezvous_audit_sync_completion_closes_window_before_wifi_snapshot() {
    let mac = "02:00:00:00:00:C2";
    let sim = configured_note4_sync_sim(mac);
    let state = json_body(&request(sim.address(),"GET","/api/status",Some(ENDPOINT),b"").1);
    let light = plan_message(&state,"bridge-test","audit-light",1,"light",Some(120));
    assert_eq!(post_plan(&sim,Some(ENDPOINT),&light).1["result"],"applied");
    assert_eq!(set_time(&sim,Some(CONTROL),
        &serde_json::json!({"op":"step","delta_ms":120000})).0,200);
    assert_eq!(sim_state(&sim)["power"]["sync_open"],true);
    let ip = sim.address().to_string();
    let before = bridge_core::device_client::wifi_snapshot(&ip,ENDPOINT,mac,"bridge-test",None,"",Duration::from_secs(1));
    assert!(before.is_ok(),"snapshot must be reachable before diagnostic drain: {before:?}");
    // The production Bridge drains sync diagnostics BEFORE wifi::opportunity_locked.
    note4_sync_roundtrip(&sim,3,&["light_exit"]);
    assert_eq!(sim_state(&sim)["power"]["sync_open"],false);
    let after = bridge_core::device_client::wifi_snapshot(&ip,ENDPOINT,mac,"bridge-test",None,"",Duration::from_millis(250));
    assert!(after.is_err(),"the closed HTTP surface must reject a following snapshot");
    eprintln!("CONFIRMED: snapshot succeeds before sync_complete and fails afterwards without a formal light window.");
}

#[test]
fn unified_usb_http_without_ble_survives_bridge_restart_lost_ack_and_receipt_retirement() {
    use bridge_core::platform::{service::PlatformService,model::{DeviceIdentity,DeviceCapabilities}};
    use std::collections::BTreeMap;
    let mac="0200000000C1";let mut sim=Simulator::start(mac,&["--wifi-unified-test"]);let address=sim.address().to_string();
    assert_eq!(request(sim.address(),"POST","/sim/power",Some(CONTROL),br#"{"plugged":true}"#).0,200);
    assert_eq!(claim(&sim,"id=bridge-test",Some(DEVICE)).0,200);
    let root=temp_data_dir();let svc=PlatformService::open(&root).unwrap();
    svc.device_upsert(DeviceIdentity::new(mac,"Unified fake").unwrap(),DeviceCapabilities::ssd1681_154g()).unwrap();
    let status=bridge_core::device_client::status(&address,ENDPOINT,Duration::from_secs(3)).unwrap();
    assert_eq!(status["wifi_management"],3);assert_eq!(status["wifi_control"]["mode"],"continuous");assert_eq!(status["wifi_control"]["write_allowed"],true);
    svc.note_authenticated_status_at(mac,&status,100).unwrap();
    let matched=bridge_core::device_client::wifi_command(&address,ENDPOINT,mac,"bridge-test","match",status["session_nonce"].as_str().unwrap(),serde_json::json!({"network_id":"missing","open":true,"metadata_key":"0".repeat(64)}),Duration::from_secs(3)).unwrap();
    assert_eq!(matched["ok"],true,"read-only HTTP setup command needs its own correlation ID");
    assert_eq!(matched["version"],0);
    let read=svc.wifi_queue(mac,"bridge-test","read",vec![],100).unwrap();
    let transport=|op:&str,body:Value| {
        if op=="snapshot" {Ok(serde_json::to_value(bridge_core::device_client::wifi_snapshot(&address,ENDPOINT,mac,"bridge-test",None,"",Duration::from_secs(3))?)?)}
        else {bridge_core::device_client::wifi_command(&address,ENDPOINT,mac,"bridge-test",op,status["session_nonce"].as_str().unwrap(),body,Duration::from_secs(3))}
    };
    assert_eq!(svc.wifi_attempt(mac,&read.job_id,None,&BTreeMap::new(),101,transport).unwrap().unwrap().state,"completed");
    let targets=serde_json::from_value(serde_json::json!([{"network_id":"home","host_ssid":"Host5G","ssid":"Device2G","credential":"open"}])).unwrap();svc.wifi_save(mac,0,targets,102).unwrap();
    let job=svc.wifi_queue(mac,"bridge-test","sync",vec!["home".into()],103).unwrap();assert!(job.request_id.starts_with("w3_"));
    assert_eq!(request(sim.address(),"POST","/sim/wifi",Some(CONTROL),br#"{"drop_http_ack":true}"#).0,200);
    assert_eq!(svc.wifi_attempt(mac,&job.job_id,Some("authenticated_http"),&BTreeMap::new(),104,transport).unwrap().unwrap().state,"awaiting_confirmation");
    drop(svc);let svc=PlatformService::open(&root).unwrap();let mut calls=vec![];
    let done=svc.wifi_attempt(mac,&job.job_id,None,&BTreeMap::new(),110,|op,body|{calls.push(op.to_owned());transport(op,body)}).unwrap().unwrap();
    assert_eq!(done.state,"completed");assert!(done.receipt_ack_pending);assert_eq!(calls,["result"]);
    {
        let _env_guard=BLE_ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",serde_json::json!({mac:format!("http://{}",sim.address())}).to_string());
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut link=bridge_ble::DeviceConnection::connect(mac,ENDPOINT,"bridge-test").await.unwrap().unwrap();assert_eq!(link.info["wifi_management"],3);
            link.write_endpoint("127.0.0.1",8765,ENDPOINT).await.unwrap();
            let full=link.wifi_read("wifi_status").await.unwrap();assert_eq!(full["presence"]["mode"],"continuous");
            assert_eq!(link.command("wifi_result",serde_json::json!({"query_id":job.request_id})).await.unwrap()["outcome"],"committed");
            let snapshot=link.wifi_snapshot().await.unwrap();assert_eq!(snapshot["revision"],1);assert_eq!(snapshot["networks"].as_array().unwrap().len(),1);
            link.close().await;
        });
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
    }
    assert_eq!(transport("result_ack",serde_json::json!({"query_id":job.request_id})).unwrap()["outcome"],"retired");svc.wifi_receipt_acked(mac,&job.job_id).unwrap();
    let redundant=svc.wifi_queue(mac,"bridge-test","sync",vec!["home".into()],120).unwrap();
    let confirmed=svc.wifi_confirm_current(mac,&redundant.job_id,1,&BTreeMap::from([("home".into(),1)]),121).unwrap();
    assert_eq!(confirmed.request_id,redundant.request_id);assert!(!confirmed.sent && !confirmed.receipt_ack_pending);assert_eq!(confirmed.confirmation.as_deref(),Some("verified_post_state"));assert!(!svc.wifi_state(mac).snapshot.as_ref().unwrap().complete);
    let old=serde_json::json!({"request_id":job.request_id,"expected_revision":1,"changes":[{"network_id":"home","host_ssid":"Host5G","ssid":"Device2G","credential":"open"}]});
    assert_eq!(transport("apply",old.clone()).unwrap()["error"],"request_retired");
    sim.stop_preserving_data();let restarted=Simulator::start_with_dir(mac,&["--wifi-unified-test"],sim.data_dir.clone(),true);
    let recovered=bridge_core::device_client::status(&restarted.address().to_string(),ENDPOINT,Duration::from_secs(3)).unwrap();
    assert_eq!(bridge_core::device_client::wifi_command(&restarted.address().to_string(),ENDPOINT,mac,"bridge-test","apply",recovered["session_nonce"].as_str().unwrap(),old,Duration::from_secs(3)).unwrap()["error"],"request_retired");
    // Principal cannot select a namespace in the request.
    assert_eq!(bridge_core::device_client::wifi_command(&restarted.address().to_string(),ENDPOINT,mac,"other","result","",serde_json::json!({"query_id":job.request_id,"bond":"arbitrary"}),Duration::from_secs(3)).unwrap()["error"],"principal_mismatch");
    let _=fs::remove_dir_all(root);
}

#[test]
fn new_wifi_sync_with_old_snapshot_reports_real_device_revision_rejection() {
    use bridge_core::platform::{service::PlatformService,model::{DeviceIdentity,DeviceCapabilities}};
    use std::collections::BTreeMap;
    let mac="0200000000C2";let sim=Simulator::start(mac,&["--wifi-unified-test"]);let address=sim.address().to_string();
    assert_eq!(request(sim.address(),"POST","/sim/power",Some(CONTROL),br#"{"plugged":true}"#).0,200);
    assert_eq!(claim(&sim,"id=bridge-test",Some(DEVICE)).0,200);
    let status=bridge_core::device_client::status(&address,ENDPOINT,Duration::from_secs(3)).unwrap();
    let root=temp_data_dir();let svc=PlatformService::open(&root).unwrap();svc.device_upsert(DeviceIdentity::new(mac,"Stale cache fake").unwrap(),DeviceCapabilities::ssd1681_154g()).unwrap();
    let read=svc.wifi_queue(mac,"bridge-test","read",vec![],100).unwrap();
    let snapshot=bridge_core::device_client::wifi_snapshot(&address,ENDPOINT,mac,"bridge-test",None,"",Duration::from_secs(3)).unwrap();assert_eq!(snapshot.revision,0);
    svc.wifi_complete_read(mac,&read.job_id,snapshot,100).unwrap();
    let transport=|op:&str,body:Value|bridge_core::device_client::wifi_command(&address,ENDPOINT,mac,"bridge-test",op,status["session_nonce"].as_str().unwrap(),body,Duration::from_secs(3));
    let external=transport("apply",serde_json::json!({"request_id":"external-config","expected_revision":0,"changes":[{"network_id":"home","host_ssid":"Host","ssid":"External","credential":"open"}]})).unwrap();assert_eq!(external["revision"],1);
    svc.wifi_save(mac,0,serde_json::from_value(serde_json::json!([{"network_id":"home","host_ssid":"Host","ssid":"Desired","credential":"open"}])).unwrap(),101).unwrap();
    let job=svc.wifi_queue(mac,"bridge-test","sync",vec!["home".into()],102).unwrap();assert_eq!(job.expected_revision,Some(0));assert!(!job.sent);
    let result=svc.wifi_attempt(mac,&job.job_id,Some("authenticated_http"),&BTreeMap::new(),103,|op,body| {
        assert_eq!(op,"apply");assert_eq!(body["expected_revision"],0);let reply=transport(op,body)?;assert_eq!(reply["error"],"revision_conflict");Ok(reply)
    }).unwrap().unwrap();
    assert_eq!(result.state,"conflict");assert_eq!(result.error_code.as_deref(),Some("revision_conflict"));assert!(!result.sent&&!result.may_have_committed);assert!(!result.runnable(200));
    assert!(svc.wifi_attempt(mac,&job.job_id,None,&BTreeMap::new(),200,|_,_|panic!("terminal refusal must not poll forever")).unwrap().is_none());
    assert_eq!(svc.wifi_state(mac).snapshot.unwrap().revision,0);assert_eq!(svc.wifi_state(mac).targets[0].ssid,"Desired");
    let fresh=bridge_core::device_client::status(&address,ENDPOINT,Duration::from_secs(3)).unwrap();
    assert_eq!(fresh["wifi_storage"]["revision"],1);assert!(fresh["status_seq"].as_u64().unwrap()>status["status_seq"].as_u64().unwrap());
    let _=fs::remove_dir_all(root);
}
