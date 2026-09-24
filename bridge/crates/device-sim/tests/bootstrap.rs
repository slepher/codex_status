use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

const ENDPOINT: &str = "endpoint-test-secret";
const DEVICE: &str = "device-test-secret";
const CONTROL: &str = "control-test-secret";
static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

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
            .args(extra)
            .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", ENDPOINT)
            .env("CODEX_STATUS_SIM_DEVICE_TOKEN", DEVICE)
            .env("CODEX_STATUS_SIM_CONTROL_TOKEN", CONTROL)
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

fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &[u8],
) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(address).expect("connect simulator");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
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
    (status, response[split + 4..].to_vec())
}

fn json_body(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("JSON response")
}

fn claim(sim: &Simulator, query: &str, token: Option<&str>) -> (u16, Value) {
    let (code, body) = request(
        sim.address(),
        "POST",
        &format!("/claim?{query}"),
        token,
        b"",
    );
    (code, json_body(&body))
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
        serde_json::json!(["v2_status", "clock_control", "claim", "plan_state"])
    );
    let ready_text = sim.ready.to_string();
    assert!(!ready_text.contains(ENDPOINT));
    assert!(!ready_text.contains(DEVICE));
    assert!(!ready_text.contains(CONTROL));

    for token in [None, Some(CONTROL), Some(DEVICE)] {
        let (code, body) = request(sim.address(), "GET", "/v2/status", token, b"");
        assert_eq!(code, 401);
        assert_eq!(json_body(&body)["result"], "unauthorized");
    }
    let (code, body) = request(sim.address(), "GET", "/v2/status", Some(ENDPOINT), b"");
    assert_eq!(code, 200);
    let status = json_body(&body);
    assert_eq!(status["result"], "applied");
    assert_eq!(status["protocol"], 2);
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
        serde_json::json!(["v2_status", "clock_control", "claim", "plan_state"])
    );
    assert_eq!(state["clock_persistence"], "unsupported");
    assert_eq!(state["plan"]["accepted"], false);
    assert!(state["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "power_lifecycle"));
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
fn writes_require_their_domain_token_then_report_unsupported() {
    let sim = Simulator::start("02:00:00:00:00:03", &[]);
    for path in [
        "/v2/data",
        "/v2/activate",
        "/v2/bundle/begin",
        "/v2/bundle/chunk",
        "/v2/bundle/commit",
    ] {
        assert_eq!(
            request(sim.address(), "POST", path, Some(DEVICE), b"{}").0,
            401
        );
        let (code, body) = request(sim.address(), "POST", path, Some(ENDPOINT), b"{}");
        assert_eq!(code, 501, "{path}");
        assert_eq!(json_body(&body)["error"], "unsupported");
    }
    let (code, body) = request(sim.address(), "POST", "/claim", Some(ENDPOINT), b"{}");
    assert_eq!(code, 401);
    assert_eq!(
        json_body(&body),
        serde_json::json!({"error":"unauthorized","owner":null})
    );
    let (code, body) = request(sim.address(), "POST", "/claim", Some(DEVICE), b"{}");
    assert_eq!(code, 400);
    assert_eq!(json_body(&body), serde_json::json!({"error":"args"}));
}

#[test]
fn oversized_write_and_unknown_routes_are_bounded_and_not_implemented() {
    let sim = Simulator::start("02:00:00:00:00:04", &[]);
    let large = vec![b'x'; 65 * 1024];
    assert_eq!(
        request(sim.address(), "POST", "/v2/data", Some(ENDPOINT), &large).0,
        413
    );
    assert_eq!(
        request(sim.address(), "GET", "/status.json", Some(ENDPOINT), b"").0,
        404
    );
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
    let (code, body) = request(first.address(), "GET", "/v2/status", Some(ENDPOINT), b"");
    assert_eq!(code, 200);
    assert_eq!(json_body(&body)["device_mac"], first.ready["mac"]);
    let (code, body) = request(second.address(), "GET", "/v2/status", Some(ENDPOINT), b"");
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
        "protocol": 2,
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
    let (code, response) = request(sim.address(), "POST", "/v2/plan", token, body.as_bytes());
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
    let (code, body) = request(sim.address(), "GET", "/v2/status", Some(ENDPOINT), b"");
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
    assert_eq!(replay["accepted_remaining_s"], 120);
    let (code, body) = request(sim.address(), "GET", "/v2/status", Some(ENDPOINT), b"");
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
            request(sim.address(), "POST", "/v2/plan", token, b"{}").0,
            401
        );
    }
    let (code, malformed) = request(sim.address(), "POST", "/v2/plan", Some(ENDPOINT), b"{");
    assert_eq!(code, 200);
    assert_eq!(
        json_body(&malformed),
        serde_json::json!({
            "op":"command", "result":"rejected", "display_state":"unchanged",
            "retention":"ram", "error":"json", "fw_target":"codex-status-154g"
        })
    );
    let (code, status_body) = request(sim.address(), "GET", "/v2/status", Some(ENDPOINT), b"");
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
        let (code, body) = request(sim.address(), "GET", "/v2/status", Some(ENDPOINT), b"");
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
        "/v2/data",
        "/v2/activate",
        "/v2/bundle/begin",
        "/v2/bundle/chunk",
        "/v2/bundle/commit",
    ] {
        assert_eq!(
            request(sim.address(), "POST", path, Some(ENDPOINT), b"{}").0,
            501,
            "{path}"
        );
        let after_other_write = sim_state(&sim)["owner"].clone();
        assert_eq!(after_other_write["id"], first["owner"]["id"]);
        assert_eq!(after_other_write["last_seen_s"], seen);
    }
    assert_eq!(
        request(sim.address(), "POST", "/v2/plan", Some(ENDPOINT), b"{}").0,
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
        serde_json::json!({"schema":1,"mac":"02:00:00:00:00:0E"})
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
