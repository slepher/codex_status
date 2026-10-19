//! Production Fake ROM + real framed HTTP client; no RF or Flash power-loss claim.
use bridge_core::platform::{model::{DeviceCapabilities, DeviceIdentity}, service::PlatformService};
use serde_json::{json, Value};
use std::{fs, io::{BufRead, BufReader, Read, Write}, net::TcpStream, path::PathBuf,
    process::{Child, Command, Stdio}, time::Duration};

const MAC: &str = "0200000000E1";
const TOKEN: &str = "synthetic-endpoint";
struct Fake { child: Child, address: String, dir: PathBuf }
impl Fake {
    fn start(dir: PathBuf) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_device-sim"))
            .args(["--listen", "127.0.0.1:0", "--mac", MAC, "--data-dir"])
            .arg(&dir).env("CODEX_STATUS_SIM_ENDPOINT_TOKEN", TOKEN)
            .env("CODEX_STATUS_SIM_DEVICE_TOKEN", "synthetic-device")
            .env("CODEX_STATUS_SIM_CONTROL_TOKEN", "synthetic-control")
            .env("CODEX_STATUS_SIM_WIFI_PRINCIPAL", "bridge-test")
            .stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let ready: Value = serde_json::from_str(&line).unwrap();
        let fake = Self { child, address: ready["http"].as_str().unwrap().trim_start_matches("http://").into(), dir };
        fake.post("/sim/power", "synthetic-control", br#"{"plugged":true}"#);
        assert_eq!(http(bridge_core::device_client::claim(&fake.address, TOKEN, "synthetic-device",
            &json!({"id":"bridge-test"}), Duration::from_secs(3))).unwrap().0, 200);
        fake
    }
    fn post(&self, path: &str, token: &str, body: &[u8]) {
        let mut stream = TcpStream::connect(&self.address).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        write!(stream, "POST {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n", body.len()).unwrap();
        stream.write_all(body).unwrap();
        let mut response = String::new(); stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    }
    fn status(&self) -> Value { http(bridge_core::device_client::status(&self.address, TOKEN, Duration::from_secs(3))).unwrap() }
    fn command(&self, op: &str, body: Value) -> anyhow::Result<Value> {
        let status = self.status();
        http(bridge_core::device_client::wifi_command(&self.address, TOKEN, MAC, "bridge-test", op, body, Duration::from_secs(3)))
    }
    fn restart(self) -> Self {
        let dir = self.dir.clone(); drop(self); Self::start(dir)
    }
}
impl Drop for Fake { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }
fn fixture(name: &str) -> (PathBuf, Fake, PlatformService, String) {
    let root = std::env::var_os("FAKE_SETTLEMENT_ROOT").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
        .join(format!("{name}-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let fake = Fake::start(root.join("rom"));
    let svc = PlatformService::open(&root.join("bridge")).unwrap();
    svc.device_upsert(DeviceIdentity::new(MAC, "Settlement Fake").unwrap(), DeviceCapabilities::ssd1681_154g()).unwrap();
    svc.wifi_save(MAC, 0, serde_json::from_value(json!([
        {"network_id":"home","host_ssid":"FakeHost","ssid":"Fake","credential":"open"}
    ])).unwrap(), 90).unwrap();
    let status = fake.status();
    let namespace = status["wifi_control"]["namespace"].as_str().unwrap().to_owned();
    (root, fake, svc, namespace)
}
fn committed_job(fake: &Fake, svc: &PlatformService, namespace: &str, revision: u64, now: u64) -> bridge_core::platform::wifi::Job {
    let job = svc.wifi_queue(MAC, "bridge-test", "recovery_save", vec!["home".into()], now).unwrap();
    svc.wifi_bind_transaction(MAC, &job.job_id, "bridge-test", namespace).unwrap();
    svc.wifi_mark_sent(MAC, &job.job_id, now + 1).unwrap();
    // The carrier deliberately withholds this committed response from the Bridge.
    let reply = fake.command("apply", json!({"request_id":job.request_id,"expected_revision":revision,
        "changes":[{"network_id":"home","host_ssid":"FakeHost","ssid":format!("Fake{revision}"),"credential":"open"}]})).unwrap();
    assert_eq!(reply["outcome"], "committed", "{reply}");
    svc.wifi_finish(MAC, &job.job_id, "cancelled", None, now + 2).unwrap()
}

#[test]
fn stopped_job_settles_original_id_after_ack_loss_and_both_restarts() {
    let (root, fake, svc, namespace) = fixture("ack-loss");
    let job = committed_job(&fake, &svc, &namespace, 0, 100);
    let before = serde_json::to_value(svc.wifi_state(MAC).snapshot).unwrap();
    let mut calls = Vec::new();
    let pending = ready(svc.wifi_settle_async(MAC, &job.job_id, "bridge-test", &namespace, 104,&mut |op: &str, body: Value| {
        calls.push(op.to_owned()); assert_eq!(body["query_id"], job.request_id);
        let reply = fake.command(op, body)?;
        if op == "result_ack" { assert_eq!(reply["outcome"], "retired"); anyhow::bail!("transport_timeout"); }
        Ok(reply)
    })).unwrap().unwrap();
    assert_eq!(calls, ["result", "result_ack"]); assert!(pending.receipt_ack_pending);
    assert!(!pending.confirmation_pending());
    drop(svc); let fake = fake.restart(); let svc = PlatformService::open(&root.join("bridge")).unwrap();
    let done = ready(svc.wifi_settle_async(MAC, &job.job_id, "bridge-test", &namespace, 165,&mut |op: &str, body: Value| {
        assert_eq!(op, "result_ack"); assert_eq!(body["query_id"], job.request_id); fake.command(op, body)
    })).unwrap().unwrap();
    assert_eq!(done.state, "cancelled"); assert!(!done.runnable(1000)); assert!(!done.receipt_ack_pending);
    assert_eq!(before, serde_json::to_value(svc.wifi_state(MAC).snapshot).unwrap());
    assert_eq!(fake.command("result", json!({"query_id":job.request_id})).unwrap()["outcome"], "retired");
    assert_eq!(fake.command("snapshot", json!({})).unwrap()["revision"], 1);
}

#[test]
fn unknown_and_identity_mismatch_never_ack_and_budget_requires_explicit_retry() {
    let (root, fake, svc, namespace) = fixture("unknown");
    let job = svc.wifi_queue_control(MAC, "bridge-test", "activate", json!({"network_id":"home","version":1}), 100).unwrap();
    svc.wifi_bind_transaction(MAC, &job.job_id, "bridge-test", &namespace).unwrap();
    svc.wifi_mark_sent(MAC, &job.job_id, 101).unwrap();
    svc.wifi_finish(MAC, &job.job_id, "cancelled", None, 102).unwrap();
    for (principal, ns) in [("other", namespace.as_str()), ("bridge-test", "another")] {
        assert!(ready(svc.wifi_settle_async(MAC, &job.job_id, principal, ns, 103,&mut |_: &str, _: Value| panic!("identity mismatch sent a command"))).is_err());
    }
    for now in [103, 164, 225] {
        let unresolved = ready(svc.wifi_settle_async(MAC, &job.job_id, "bridge-test", &namespace, now,&mut |op: &str, body: Value| {
            assert_eq!(op, "result"); fake.command(op, body)
        })).unwrap().unwrap();
        assert!(unresolved.confirmation_pending()); assert!(!unresolved.receipt_ack_pending);
    }
    drop(svc); let svc = PlatformService::open(&root.join("bridge")).unwrap();
    assert!(ready(svc.wifi_settle_async(MAC, &job.job_id, "bridge-test", &namespace, 999,&mut |_: &str, _: Value| panic!("exhausted budget sent command"))).unwrap().is_none());
    svc.wifi_retry_settlement(MAC, &job.job_id).unwrap();
    let still_unknown = ready(svc.wifi_settle_async(MAC, &job.job_id, "bridge-test", &namespace, 1000,&mut |op: &str, body: Value| {
        assert_eq!(op, "result"); fake.command(op, body)
    })).unwrap().unwrap();
    assert_eq!(still_unknown.state, "cancelled"); assert!(still_unknown.confirmation_pending());
}

#[test]
fn eight_device_receipts_and_thirty_two_bridge_jobs_require_proven_ack_to_release() {
    let (_root, fake, svc, namespace) = fixture("capacity");
    let mut jobs = Vec::new();
    for n in 0..8 { jobs.push(committed_job(&fake, &svc, &namespace, n, 100 + n * 4)); }
    let full = fake.command("apply", json!({"request_id":"w3_ffffffffffffffff","expected_revision":8,
        "changes":[{"network_id":"home","host_ssid":"Host","ssid":"Extra","credential":"open"}]})).unwrap();
    assert_eq!(full["error"], "result_capacity", "{full}");
    for n in 8..32 {
        let job = svc.wifi_queue_control(MAC, "bridge-test", "activate", json!({"network_id":"home","version":1}), 200+n).unwrap();
        svc.wifi_bind_transaction(MAC, &job.job_id, "bridge-test", &namespace).unwrap();
        svc.wifi_mark_sent(MAC, &job.job_id, 201+n).unwrap();
        svc.wifi_finish(MAC, &job.job_id, "cancelled", None, 202+n).unwrap();
    }
    assert_eq!(svc.wifi_queue_control(MAC, "bridge-test", "activate", json!({"network_id":"home","version":1}), 300).unwrap_err().to_string(), "wifi_job_capacity");
    let first = &jobs[0];
    ready(svc.wifi_settle_async(MAC, &first.job_id, "bridge-test", &namespace, 301,&mut |op: &str, body: Value| {
        if op == "result_ack" { anyhow::bail!("transport_timeout"); } fake.command(op, body)
    })).unwrap();
    assert_eq!(svc.wifi_queue_control(MAC, "bridge-test", "activate", json!({"network_id":"home","version":1}), 302).unwrap_err().to_string(), "wifi_job_capacity");
    ready(svc.wifi_settle_async(MAC, &first.job_id, "bridge-test", &namespace, 362,&mut |op: &str, body: Value| {
        assert_eq!(op, "result_ack"); fake.command(op, body)
    })).unwrap();
    let next = svc.wifi_queue_control(MAC, "bridge-test", "activate", json!({"network_id":"home","version":1}), 363).unwrap();
    assert_eq!(svc.wifi_state(MAC).jobs.len(), 32);
    let accepted = fake.command("apply", json!({"request_id":next.request_id,"expected_revision":8,
        "changes":[{"network_id":"home","host_ssid":"Host","ssid":"Extra","credential":"open"}]})).unwrap();
    assert_eq!(accepted["outcome"], "committed", "{accepted}");
    for job in jobs.iter().skip(1) {
        assert_eq!(fake.command("result", json!({"query_id":job.request_id})).unwrap()["outcome"], "committed");
    }
}

#[test]
fn committed_write_settles_after_expiry_or_execution_confirmation_budget_exhaustion() {
    for reason in ["expired", "budget"] {
        let (_root, fake, svc, namespace) = fixture(reason);
        let job = svc.wifi_queue(MAC, "bridge-test", "recovery_save", vec!["home".into()], 100).unwrap();
        let initial = ready(svc.wifi_attempt_async(MAC, &job.job_id, "bridge-test", &namespace, true,
            &Default::default(), 101,&mut |op: &str, body: Value| {
                let reply = fake.command(op, body)?;
                if op == "apply" { assert_eq!(reply["outcome"], "committed"); anyhow::bail!("transport_timeout"); }
                Ok(reply)
            })).unwrap().unwrap();
        assert!(initial.sent && initial.confirmation_pending());
        let terminal = if reason == "expired" {
            // Admission expiration uses the job's real persisted expiry.
            assert!(svc.wifi_begin(MAC, &job.job_id, initial.expires_at + 1).unwrap().is_none());
            svc.wifi_state(MAC).jobs.into_iter().find(|j| j.job_id == job.job_id).unwrap()
        } else {
            let mut terminal = initial;
            for now in [201, 301, 401] {
                terminal = ready(svc.wifi_attempt_async(MAC, &job.job_id, "bridge-test", &namespace, true,
                    &Default::default(), now,&mut |op: &str, body: Value| {
                        assert_eq!(op, "result"); assert_eq!(body["query_id"], job.request_id);
                        anyhow::bail!("transport_timeout")
                    })).unwrap().unwrap();
            }
            assert_eq!(terminal.error_code.as_deref(), Some("confirmation_budget_exhausted"));
            terminal
        };
        assert!(terminal.terminal());
        let settled = ready(svc.wifi_settle_async(MAC, &job.job_id, "bridge-test", &namespace,
            terminal.expires_at + 2,&mut |op: &str, body: Value| {
                assert!(matches!(op, "result" | "result_ack"));
                assert_eq!(body["query_id"], job.request_id); fake.command(op, body)
            })).unwrap().unwrap();
        assert_eq!(settled.state, terminal.state);
        assert!(!settled.confirmation_pending() && !settled.receipt_ack_pending);
        assert!(!settled.runnable(u64::MAX));
    }
}

// This synchronous process/socket harness joins the async client before returning.
// A scoped runtime also works inside its existing BLE runtime fixtures.
fn http<F: std::future::Future + Send>(future: F) -> F::Output where F::Output: Send {
    std::thread::scope(|scope| scope.spawn(|| tokio::runtime::Builder::new_current_thread()
        .enable_all().build().unwrap().block_on(future)).join().unwrap())
}

fn ready<F: std::future::Future>(future:F)->F::Output {
    let mut future=std::pin::pin!(future);
    match future.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())) {
        std::task::Poll::Ready(value)=>value,
        std::task::Poll::Pending=>panic!("synchronous fixture suspended"),
    }
}
