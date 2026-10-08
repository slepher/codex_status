//! Integration test for the bridge-side device client against a minimal
//! in-process HTTP server that emulates the firmware endpoints: bounded
//! BEGIN/CHUNK/COMMIT ordering, offset checks and ACK handling.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bridge_core::device_client;
use serde_json::{json, Value};
use bridge_core::compact_frame::{Header, CONTENT_TYPE};
thread_local! { static REQUEST_FRAME: std::cell::Cell<Option<Header>> = const {std::cell::Cell::new(None)}; }

struct FakeDevice {
    pub calls: Arc<Mutex<Vec<String>>>,
    pub payload: Arc<Mutex<Vec<u8>>>,
    pub offset: Arc<AtomicUsize>,
    pub len: Arc<AtomicUsize>,
}

fn read_request(stream: &mut TcpStream) -> (String, String, String, Vec<u8>) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    // Read until the end of headers.
    let header_end;
    loop {
        let n = stream.read(&mut tmp).unwrap();
        if n == 0 {
            return (String::new(), String::new(), String::new(), buf);
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let content_len = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_len {
        let n = stream.read(&mut tmp).unwrap();
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_len);
    let (frame,payload)=Header::decode(&body,4096).expect("client must send the compact frame");
    assert_eq!(frame.kind,0);
    REQUEST_FRAME.set(Some(frame));
    body=payload.to_vec();
    let line = head.lines().next().unwrap_or("").to_string();
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    (line, path, head, body)
}

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then_some(value.trim())
    })
}

fn respond(stream: &mut TcpStream, status: u16, body: &Value) {
    let payload = body.to_string();
    let request=REQUEST_FRAME.get().unwrap();
    let ack=Header{kind:1,op:request.op,result:if status>=400 || body["result"]=="rejected" {2}else{0},
        payload_len:payload.len() as u16,session:[9;16],request_no:request.request_no,context:request.context};
    let mut bytes=ack.encode().unwrap().to_vec();bytes.extend_from_slice(payload.as_bytes());
    let head = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: {CONTENT_TYPE}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&bytes);
}

#[test]
fn complete_ack_does_not_wait_for_socket_close() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_request(&mut stream);
        respond(&mut stream, 200, &json!({"result": "applied", "device_mac":"AABBCCDDEEFF"}));
        std::thread::sleep(Duration::from_secs(1));
    });
    let status = device_client::status(&addr.to_string(), "tok", Duration::from_millis(300)).unwrap();
    assert_eq!(status["result"], "applied");
}

#[test]
fn paged_status_preserves_the_complete_contract_and_rejects_changed_read_id() {
    for tamper in [false,true] {
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let addr=listener.local_addr().unwrap();
        let expected=json!({"result":"applied","device_mac":"AABBCCDDEEFF","active_context_id":"123456789abcdef0","data_contract":{"requirements":[{"field":"长字段".repeat(800)}]}});
        let bytes=serde_json::to_vec(&expected).unwrap();
        let server=std::thread::spawn(move || {
            for (index,chunk) in bytes.chunks(1800).enumerate() {
                let (mut socket,_)=listener.accept().unwrap();let request=read_request(&mut socket);
                if index>0 {assert!(serde_json::from_slice::<Value>(&request.3).unwrap()["status_read"].is_string());}
                let id=if tamper && index==1 {"22222222222222222222222222222222"}else{"11111111111111111111111111111111"};
                let hex=chunk.iter().map(|b|format!("{b:02x}")).collect::<String>();
                respond(&mut socket,200,&json!({"result":"applied","status_read":id,"offset":index*1800,"total_bytes":bytes.len(),"next_offset":if (index+1)*1800<bytes.len(){(index+1)*1800}else{0},"chunk_hex":hex}));
                if tamper && index==1 {break;}
            }
        });
        let actual=device_client::status(&addr.to_string(),"synthetic-pages",Duration::from_secs(2));
        if tamper {assert!(actual.unwrap_err().to_string().contains("status page identity mismatch"));}
        else {assert_eq!(actual.unwrap(),expected);}
        server.join().unwrap();
    }
}

fn spawn_fake() -> (String, FakeDevice) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let ip = listener.local_addr().unwrap().ip().to_string();
    let port = listener.local_addr().unwrap().port();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let payload = Arc::new(Mutex::new(Vec::new()));
    let offset = Arc::new(AtomicUsize::new(0));
    let len = Arc::new(AtomicUsize::new(0));
    let device = FakeDevice {
        calls: calls.clone(),
        payload: payload.clone(),
        offset: offset.clone(),
        len: len.clone(),
    };
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (line, path, head, body) = read_request(&mut stream);
            let method = line.split_whitespace().next().unwrap_or("");
            calls.lock().unwrap().push(format!("{method} {path}"));
            if path == "/api/status" {
                respond(
                    &mut stream,
                    200,
                    &json!({
                        "result": "applied",
                        "active_context_id": "0000000000000001",
                        "device_mac": "70:04:1D:D7:A3:40", "session_nonce": "0123456789abcdef0123456789abcdef",
                        "active_template_id": "quad",
                        "data_seq": 4,
                        "power": {"mode": "light", "plan_id": 7, "remaining_s": 120}
                    }),
                );
            } else if path == "/api/data" {
                let message: Value = serde_json::from_slice(&body).unwrap();
                let ok = message["seq"] == 5 && message["context_id"] == "0000000000000001";
                respond(
                    &mut stream,
                    200,
                    &json!({"result": if ok {"applied"} else {"rejected"}, "display_state": "displayed"}),
                );
            } else if path == "/api/plan" {
                let plan: Value = serde_json::from_slice(&body).unwrap();
                respond(
                    &mut stream,
                    200,
                    &json!({
                        "result": "applied",
                        "plan_id": plan["plan_id"],
                        "accepted_remaining_s": plan["light_duration_s"],
                    }),
                );
            } else if path.starts_with("/api/bundle/begin") {
                let command: Value = serde_json::from_slice(&body).unwrap();
                assert!(command.get("protocol").is_none());
                assert_eq!(command["bridge_id"], "bridge-1");
                assert_eq!(command["session_nonce"], "0123456789abcdef0123456789abcdef");
                assert_eq!(command["request_id"], "bundle-job-1");
                len.store(command["length"].as_u64().unwrap() as usize, Ordering::SeqCst);
                offset.store(0, Ordering::SeqCst);
                payload.lock().unwrap().clear();
                respond(&mut stream, 200, &json!({"result": "applied", "next_offset": 0}));
            } else if path.starts_with("/api/bundle/chunk") {
                let stated: usize = path
                    .split("offset=")
                    .nth(1)
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0);
                assert_eq!(header(&head, "X-Request-Id"), Some("bundle-job-1"));
                assert_eq!(header(&head, "X-Session-Nonce"), Some("0123456789abcdef0123456789abcdef"));
                let expected_offset = stated.to_string();
                assert_eq!(header(&head, "X-Offset"), Some(expected_offset.as_str()));
                let expected = offset.load(Ordering::SeqCst);
                if stated != expected {
                    respond(
                        &mut stream,
                        200,
                        &json!({"result": "rejected", "error": "offset"}),
                    );
                    continue;
                }
                let mut store = payload.lock().unwrap();
                store.extend_from_slice(&body);
                let next = store.len();
                offset.store(next, Ordering::SeqCst);
                respond(
                    &mut stream,
                    200,
                    &json!({"result": "applied", "next_offset": next}),
                );
            } else if path.starts_with("/api/bundle/commit") {
                let store = payload.lock().unwrap();
                let command: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(command["content_crc"], format!("{:08x}", crc32fast::hash(&store)));
                assert_eq!(command["request_id"], "bundle-job-1");
                let ok = store.len() == len.load(Ordering::SeqCst) && !store.is_empty();
                respond(
                    &mut stream,
                    200,
                    &json!({"result": if ok {"applied"} else {"rejected"}, "error": if ok {""} else {"length"}}),
                );
            } else if path.starts_with("/api/activate") {
                let command: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(command["expected_active_context_id"], "0000000000000001");
                assert_eq!(command["request_id"], "activate-0000000000000001-mini");
                assert_eq!(command["template_id"], "mini");
                assert_eq!(command["session_nonce"], "0123456789abcdef0123456789abcdef");
                respond(
                    &mut stream,
                    200,
                    &json!({"result": "applied", "active_context_id": "0000000000000002"}),
                );
            } else {
                respond(&mut stream, 404, &json!({"result": "rejected"}));
            }
        }
    });
    (format!("{ip}:{port}"), device)
}

#[test]
fn status_data_plan_activate_round_trip() {
    let (addr, fake) = spawn_fake();
    let token = "tok";
    let timeout = Duration::from_secs(2);
    let status = device_client::status(&addr, token, timeout).unwrap();
    assert_eq!(status["active_template_id"], "quad");
    assert_eq!(status["power"]["plan_id"], 7);

    let ack = device_client::data(
        &addr,
        token,
        "70:04:1D:D7:A3:40",
        &json!({"context_id": "0000000000000001", "seq": 5, "fields": []}),
        timeout,
    )
    .unwrap();
    assert_eq!(ack["result"], "applied");

    let ack = device_client::plan(
        &addr,
        token,
        "70:04:1D:D7:A3:40",
        &json!({"plan_id": 8, "mode": "light", "light_duration_s": 300, "rendezvous_period_s": 60}),
        timeout,
    )
    .unwrap();
    assert_eq!(ack["accepted_remaining_s"], 300);

    let ack = device_client::activate(&addr, token, "70:04:1D:D7:A3:40", "bridge-1", "mini", "0000000000000001", timeout).unwrap();
    assert_eq!(ack["active_context_id"], "0000000000000002");
    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(calls[0], "POST /api/status");
    let writes: Vec<_> = calls.iter().filter(|p| *p != "POST /api/status").cloned().collect();
    assert_eq!(writes, vec!["POST /api/data", "POST /api/plan", "POST /api/activate"]);
}

#[test]
fn bundle_install_is_ordered_and_complete() {
    let (addr, fake) = spawn_fake();
    let payload = serde_json::to_vec(&json!({
        "job_id": "job-1",
        "firmware_target": "codex-status-154g",
        "render_target": "epd-ssd1681-200x200-1bpp",
        "compiler_abi": 1,
        "profile": {"template_ids": ["quad"], "initial_active_id": "quad"},
        "templates": [{"key": {"template_id": "quad"}, "source": {"id": "quad"}}],
        "crc": "abcd1234",
    }))
    .unwrap();
    let padded = {
        let mut v = payload.clone();
        while v.len() < 10_000 {
            v.push(b' ');
        }
        v
    };
    let ack = device_client::install_bundle(
        &addr,
        "tok",
        "70:04:1D:D7:A3:40",
        "bridge-1",
        &padded,
        device_client::BUNDLE_CHUNK_BYTES,
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(ack["result"], "applied");
    assert_eq!(fake.payload.lock().unwrap().len(), padded.len());
    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(calls[0], "POST /api/status");
    assert_eq!(calls[1], "POST /api/bundle/begin");
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.starts_with("POST /api/bundle/chunk"))
            .count(),
        3,
        "10000 bytes at 4048 -> 3 chunks, ordered by offset"
    );
    assert_eq!(calls.last().unwrap(), "POST /api/bundle/commit");
}

#[test]
fn wrong_target_mac_stops_all_writes_after_status_read() {
    let (addr, fake) = spawn_fake();
    let timeout = Duration::from_secs(2);
    let wrong_mac = "AA:BB:CC:DD:EE:FF";

    assert!(device_client::data(&addr, "tok", wrong_mac, &json!({"seq": 1}), timeout).is_err());
    assert!(device_client::plan(&addr, "tok", wrong_mac, &json!({"plan_id": 1}), timeout).is_err());
    assert!(device_client::activate(&addr, "tok", wrong_mac, "bridge-1", "mini", "0000000000000001", timeout).is_err());
    let bundle = serde_json::to_vec(&json!({"job_id": "job-guard"})).unwrap();
    assert!(device_client::install_bundle(
        &addr, "tok", wrong_mac, "bridge-1", &bundle, device_client::BUNDLE_CHUNK_BYTES, timeout
    ).is_err());

    assert_eq!(
        fake.calls.lock().unwrap().as_slice(),
        ["POST /api/status", "POST /api/status", "POST /api/status", "POST /api/status"],
        "MAC mismatch must perform only authenticated POST /api/status, with no write POST"
    );
}

#[test]
fn owner_conflict_stops_the_write_without_retrying() {
    // 409 must be returned as a protocol outcome, never retried as a failure.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (_, path, _, _) = read_request(&mut stream);
            if path == "/api/status" {
                respond(&mut stream, 200, &json!({"device_mac": "70:04:1D:D7:A3:40", "session_nonce": "0123456789abcdef0123456789abcdef"}));
            } else {
                respond(&mut stream, 409, &json!({"owner": {"name": "other"}}));
            }
        }
    });
    let ack = device_client::data(
        &addr,
        "tok",
        "70:04:1D:D7:A3:40",
        &json!({"context_id": "ctx", "seq": 1}),
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(ack["result"], "rejected");
    assert_eq!(ack["error"], "occupied");
}

#[test]
fn unauthorized_is_an_error_not_a_silent_success() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let _ = read_request(&mut stream);
            respond(&mut stream, 401, &json!({"result": "unauthorized"}));
        }
    });
    assert!(device_client::status(&addr, "bad", Duration::from_secs(2)).is_err());
}
