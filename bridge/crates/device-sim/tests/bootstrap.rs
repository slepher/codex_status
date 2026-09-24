use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Child, Command, Stdio},
    time::Duration,
};

const ENDPOINT: &str = "endpoint-test-secret";
const DEVICE: &str = "device-test-secret";
const CONTROL: &str = "control-test-secret";

struct Simulator {
    child: Child,
    ready: Value,
}

impl Simulator {
    fn start(mac: &str, extra: &[&str]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_device-sim"));
        command
            .args(["--listen", "127.0.0.1:0", "--mac", mac])
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
        Self { child, ready }
    }

    fn address(&self) -> SocketAddr {
        self.ready["http"]
            .as_str()
            .unwrap()
            .trim_start_matches("http://")
            .parse()
            .unwrap()
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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

#[test]
fn startup_requires_loopback_laa_mac_and_all_tokens() {
    let executable = env!("CARGO_BIN_EXE_device-sim");
    let base = || {
        let mut command = Command::new(executable);
        command.args(["--listen", "127.0.0.1:0", "--mac", "02:00:00:00:00:01"]);
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
}

#[test]
fn ready_and_status_are_safe_and_use_shared_builder() {
    let sim = Simulator::start("02:ab:cd:00:00:01", &["--seed", "44"]);
    assert_eq!(sim.ready["schema_version"], 1);
    assert_eq!(sim.ready["mac"], "02:AB:CD:00:00:01");
    assert_eq!(
        sim.ready["capabilities"],
        serde_json::json!(["v2_status", "clock_control"])
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
        serde_json::json!(["v2_status", "clock_control"])
    );
    assert_eq!(state["clock_persistence"], "unsupported");
    assert!(state["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item != "clock-control"));
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
        "/v2/plan",
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
    assert_eq!(code, 501);
    assert_eq!(json_body(&body)["error"], "unsupported");
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
