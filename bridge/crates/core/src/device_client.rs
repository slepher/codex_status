//! Bridge side of the device protocol (HTTP transport): authenticated Status
//! read, complete Data, PowerPlan, Activate and the bounded
//! BEGIN/CHUNK/COMMIT Bundle install.
//!
//! All calls carry the device endpoint token + `bridge_id`; none of them create
//! or renew an owner (only the explicit `POST /claim` does).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use crate::platform::model::DeviceIdentity;
use serde_json::{json, Value};

pub const BUNDLE_CHUNK_BYTES: usize = 4096;
const MAX_BUNDLE_BYTES: usize = 262_144;

/// One HTTP round trip with a bounded body.
fn request(
    ip: &str,
    method: &str,
    path: &str,
    token: &str,
    body: Option<&[u8]>,
    extra_headers: &[(&str, &str)],
    timeout: Duration,
) -> Result<(u16, String)> {
    let target = if ip.contains(':') {
        ip.to_string()
    } else {
        format!("{ip}:80")
    };
    let addr = target
        .parse()
        .map_err(|e| anyhow!("bad device address {ip}: {e}"))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: device\r\nConnection: close\r\nAccept: application/json\r\n"
    );
    if !token.is_empty() {
        head.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(bytes) = body {
        head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            bytes.len()
        ));
    }
    for (name, value) in extra_headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    if let Some(bytes) = body {
        stream.write_all(bytes)?;
    }
    let mut raw = Vec::new();
    let mut chunk = [0u8; 2048];
    let mut expected_len = None;
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        if raw.len() > 64 * 1024 {
            bail!("device HTTP response exceeds 64 KiB");
        }
        if let Some(head_end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            let body_start = head_end + 4;
            if expected_len.is_none() {
                let head = String::from_utf8_lossy(&raw[..head_end]);
                expected_len = head.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("Content-Length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                });
            }
            if let Some(len) = expected_len {
                if len > 64 * 1024 {
                    bail!("device HTTP response body exceeds 64 KiB");
                }
                if raw.len() >= body_start + len {
                    raw.truncate(body_start + len);
                    break;
                }
            }
        }
    }
    let body_start = raw.windows(4).position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4).context("incomplete device HTTP headers")?;
    if expected_len.is_some_and(|len| raw.len() < body_start + len) {
        bail!("incomplete device HTTP response body");
    }
    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let response_body = String::from_utf8_lossy(&raw[body_start..]).trim().to_string();
    Ok((status, response_body))
}

fn post_json(ip: &str, path: &str, token: &str, body: &Value, timeout: Duration) -> Result<Value> {
    let started = Instant::now();
    let text = serde_json::to_vec(body)?;
    let request_id = event_id(body, "request_id");
    let device_mac = event_id(body, "device_mac");
    let seq = body["seq"].as_u64();
    let plan_id = body["plan_id"].as_u64();
    let job_id = event_id(body, "job_id");
    tracing::info!(
        event = "send",
        operation = path,
        request_id = request_id.as_deref().unwrap_or(""),
        device_mac = device_mac.as_deref().unwrap_or(""),
        seq = seq.unwrap_or(0),
        plan_id = plan_id.unwrap_or(0),
        job_id = job_id.as_deref().unwrap_or(""),
        "device HTTP request"
    );
    let response = request(ip, "POST", path, token, Some(&text), &[], timeout);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let (status, response_body) = match response {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(
                event = "result",
                operation = path,
                request_id = request_id.as_deref().unwrap_or(""),
                device_mac = device_mac.as_deref().unwrap_or(""),
                seq = seq.unwrap_or(0),
                plan_id = plan_id.unwrap_or(0),
                job_id = job_id.as_deref().unwrap_or(""),
                elapsed_ms,
                error_category = safe_error_category(&error),
                "device HTTP result"
            );
            return Err(error).with_context(|| format!("POST {path}"));
        }
    };
    let error_category = if (200..300).contains(&status) {
        "none"
    } else if status == 401 || status == 409 {
        "identity_or_claim_rejected"
    } else {
        "http_non_success"
    };
    tracing::info!(
        event = "result",
        operation = path,
        request_id = request_id.as_deref().unwrap_or(""),
        device_mac = device_mac.as_deref().unwrap_or(""),
        seq = seq.unwrap_or(0),
        plan_id = plan_id.unwrap_or(0),
        job_id = job_id.as_deref().unwrap_or(""),
        status,
        elapsed_ms,
        error_category,
        "device HTTP result"
    );
    let parsed: Value =
        serde_json::from_str(&response_body).unwrap_or(json!({"raw": response_body}));
    if status == 409 {
        return Ok(json!({"result": "rejected", "error": "occupied", "detail": parsed}));
    }
    if status == 401 {
        bail!("device rejected the endpoint token (401)");
    }
    if !(200..300).contains(&status) {
        bail!("device returned HTTP {status}");
    }
    Ok(parsed)
}

fn event_id(body: &Value, key: &str) -> Option<String> {
    if !matches!(key, "request_id" | "device_mac" | "job_id") {
        return None;
    }
    body[key]
        .as_str()
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 80
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
        })
        .map(str::to_owned)
}

fn safe_error_category(error: &anyhow::Error) -> &'static str {
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::TimedOut)
    }) {
        "timeout"
    } else if error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            io.kind() == std::io::ErrorKind::ConnectionRefused
                || io.kind() == std::io::ErrorKind::ConnectionReset
                || io.kind() == std::io::ErrorKind::NotConnected
        })
    }) {
        "connection"
    } else {
        "transport_or_protocol"
    }
}

/// Authenticated Status read: the only authoritative device state.
pub fn status(ip: &str, token: &str, timeout: Duration) -> Result<Value> {
    let started = Instant::now();
    tracing::info!(event = "send", operation = "/api/status", "device HTTP request");
    let response = request(ip, "GET", "/api/status", token, None, &[], timeout);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let (status, body) = match response {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(
                event = "result",
                operation = "/api/status",
                elapsed_ms,
                error_category = safe_error_category(&error),
                "device HTTP result"
            );
            return Err(error);
        }
    };
    tracing::info!(
        event = "result",
        operation = "/api/status",
        status,
        elapsed_ms,
        error_category = if status == 200 { "none" }
            else if status == 401 { "identity_rejected" }
            else { "http_non_success" },
        "device HTTP result"
    );
    if status == 401 {
        bail!("device rejected the endpoint token (401)");
    }
    if status != 200 {
        bail!("device /api/status HTTP {status}");
    }
    Ok(serde_json::from_str(&body).context("device status json")?)
}

/// sync-v1 keeps the device's precise error code (notably claim_required,
/// digest_mismatch and batch_lost) and validates every response identity.
pub fn sync(
    ip: &str, token: &str, mac: &str, bridge_id: &str, nonce: &str,
    op: &str, request_id: &str, fields: &Value, timeout: Duration,
) -> Result<Value> {
    let path = match op {
        "begin" | "page" | "ack" | "complete" | "arm" | "image" =>
            format!("/api/sync/{op}"),
        _ => bail!("unknown sync operation"),
    };
    let mut body = fields.clone();
    if !body.is_object() { bail!("sync fields must be an object"); }
    body["sync_version"] = json!(1);
    body["device_mac"] = json!(mac);
    body["bridge_id"] = json!(bridge_id);
    body["session_nonce"] = json!(nonce);
    body["request_id"] = json!(request_id);
    let bytes = serde_json::to_vec(&body)?;
    if bytes.len() > 4096 { bail!("sync request body exceeds 4096 bytes"); }
    let (status, text) = request(ip, "POST", &path, token, Some(&bytes), &[], timeout)?;
    let reply: Value = serde_json::from_str(&text).context("sync response JSON")?;
    if status == 401 { bail!("sync unauthorized (401)"); }
    if reply["device_mac"] != mac || reply["session_nonce"] != nonce ||
        reply["request_id"] != request_id || reply["sync_version"] != 1 ||
        reply["op"] != format!("sync_{op}") {
        bail!("sync response identity mismatch");
    }
    if status != 200 || !matches!(reply["result"].as_str(), Some("applied" | "already_complete")) {
        let error = reply["error"].as_str().unwrap_or("rejected");
        bail!("sync {op} HTTP {status}: {error}");
    }
    Ok(reply)
}

/// Complete bounded Data snapshot (atomic apply + simple ACK).
pub fn data(ip: &str, token: &str, expected_mac: &str, message: &Value, timeout: Duration) -> Result<Value> {
    session_post(ip, "/api/data", token, expected_mac, message, timeout)
}

/// Formal PowerPlan (the only thing that changes the light deadline).
pub fn plan(ip: &str, token: &str, expected_mac: &str, plan: &Value, timeout: Duration) -> Result<Value> {
    session_post(ip, "/api/plan", token, expected_mac, plan, timeout)
}

fn session_post(ip: &str, path: &str, token: &str, expected_mac: &str, message: &Value, timeout: Duration) -> Result<Value> {
    let mut command = message.clone();
    let (nonce, device_mac) = session_nonce(ip, token, expected_mac, timeout)?;
    command["session_nonce"] = json!(nonce);
    command["device_mac"] = json!(device_mac);
    command["request_id"] = json!(format!("{}-{:08x}", path.rsplit('/').next().unwrap_or("command"),
        crc32fast::hash(&crate::template::canonical_bytes(message))));
    post_json(ip, path, token, &command, timeout)
}

/// Explicit remote activation.
pub fn activate(
    ip: &str,
    token: &str,
    expected_mac: &str,
    bridge_id: &str,
    template_id: &str,
    expected_context: &str,
    timeout: Duration,
) -> Result<Value> {
    let (nonce, device_mac) = session_nonce(ip, token, expected_mac, timeout)?;
    let request_id = format!("activate-{expected_context}-{template_id}");
    post_json(ip, "/api/activate", token, &json!({
        "device_mac": device_mac, "bridge_id": bridge_id, "session_nonce": nonce,
        "request_id": request_id, "template_id": template_id,
        "expected_active_context_id": expected_context,
    }), timeout)
}

fn session_nonce(ip: &str, token: &str, expected_mac: &str, timeout: Duration) -> Result<(String, String)> {
    let state = status(ip, token, timeout)?;
    let mac = state["device_mac"].as_str()
        .with_context(|| format!("authenticated status is missing device MAC for target {expected_mac}"))?;
    let expected = DeviceIdentity::normalized_mac(expected_mac)
        .with_context(|| format!("invalid expected target MAC {expected_mac}"))?;
    let actual = DeviceIdentity::normalized_mac(mac)
        .with_context(|| format!("invalid device MAC {mac} in authenticated status for target {expected_mac}"))?;
    if actual != expected {
        tracing::warn!(event = "preflight_reject", operation = "/api/status",
            device_mac = %expected, reported_mac = %actual, error_category = "identity_mismatch",
            "device HTTP identity preflight rejected");
        bail!("authenticated status MAC {mac} does not match target MAC {expected_mac}");
    }
    tracing::info!(event = "preflight_accept", operation = "/api/status",
        device_mac = %expected, reported_mac = %actual, error_category = "none",
        "device HTTP identity preflight accepted");
    let nonce = state["session_nonce"].as_str().unwrap_or("");
    if nonce.len() != 32 || !nonce.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("device lacks required session protection; update firmware before publishing or activating");
    }
    Ok((nonce.to_owned(), mac.to_owned()))
}

/// Bounded BEGIN/CHUNK/COMMIT install of a complete Bundle payload.
pub fn install_bundle(
    ip: &str,
    token: &str,
    expected_mac: &str,
    bridge_id: &str,
    payload: &[u8],
    chunk_bytes: usize,
    timeout: Duration,
) -> Result<Value> {
    if payload.is_empty() || payload.len() > MAX_BUNDLE_BYTES {
        bail!("bundle payload size {} out of range", payload.len());
    }
    let chunk_bytes = chunk_bytes.clamp(256, 16 * 1024);
    let (nonce, device_mac) = session_nonce(ip, token, expected_mac, timeout)?;
    let content_crc = format!("{:08x}", crc32fast::hash(payload));
    let bundle: Value = serde_json::from_slice(payload).context("bundle JSON")?;
    let job_id = bundle["job_id"].as_str().filter(|s| !s.is_empty())
        .context("bundle job_id")?;
    let request_id = format!("bundle-{job_id}");
    if request_id.len() > 64 || !request_id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
        bail!("invalid bundle job_id");
    }
    let command = json!({"device_mac": device_mac, "bridge_id": bridge_id,
        "request_id": request_id, "session_nonce": nonce,
        "length": payload.len(), "content_crc": content_crc});
    let ack = post_json(ip, "/api/bundle/begin", token, &command, timeout)?;
    if ack["result"] != "applied" {
        return Ok(ack);
    }
    // A completed BEGIN replay returns the original committed context.
    if ack["active_context_id"].as_str().is_some_and(|s| !s.is_empty()) {
        return Ok(ack);
    }
    let mut offset = ack["next_offset"].as_u64().context("bundle begin next_offset")? as usize;
    if offset > payload.len() { bail!("bundle begin offset exceeds payload"); }
    while offset < payload.len() {
        let end = (offset + chunk_bytes).min(payload.len());
        let path = format!("/api/bundle/chunk?request_id={request_id}&session_nonce={nonce}&offset={offset}");
        let offset_text = offset.to_string();
        let trace_device_mac = event_id(&command, "device_mac").unwrap_or_default();
        let trace_job_id = event_id(&bundle, "job_id").unwrap_or_default();
        let started = Instant::now();
        tracing::info!(event = "send", operation = "/api/bundle/chunk",
            request_id = %request_id, device_mac = %trace_device_mac, job_id = %trace_job_id,
            offset, "device HTTP request");
        let response = request(
            ip,
            "POST",
            &path,
            token,
            Some(&payload[offset..end]),
            &[("X-Request-Id", &request_id), ("X-Session-Nonce", &nonce), ("X-Offset", &offset_text)],
            timeout,
        );
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let (status, body) = match response {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(event = "result", operation = "/api/bundle/chunk",
                    request_id = %request_id, device_mac = %trace_device_mac, job_id = %trace_job_id,
                    offset, elapsed_ms, error_category = safe_error_category(&error), "device HTTP result");
                return Err(error).with_context(|| format!("bundle chunk at offset {offset}"));
            }
        };
        tracing::info!(event = "result", operation = "/api/bundle/chunk",
            request_id = %request_id, device_mac = %trace_device_mac, job_id = %trace_job_id,
            offset, status, elapsed_ms,
            error_category = if (200..300).contains(&status) { "none" } else { "http_non_success" },
            "device HTTP result");
        if !(200..300).contains(&status) {
            bail!("bundle chunk at offset {offset} failed: HTTP {status}");
        }
        let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        if parsed["result"] != "applied" {
            tracing::warn!(event = "ack", operation = "/api/bundle/chunk",
                request_id = %request_id, device_mac = %trace_device_mac, job_id = %trace_job_id,
                offset, result = parsed["result"].as_str().unwrap_or("unknown"),
                error_category = "ack_rejected", "device Bundle chunk acknowledgement");
            return Ok(parsed);
        }
        let next = parsed["next_offset"].as_u64().unwrap_or(u64::MAX);
        if next != end as u64 {
            bail!("bundle chunk offset mismatch: device says {next}, expected {end}");
        }
        offset = end;
    }
    post_json(
        ip,
        "/api/bundle/commit",
        token,
        &command,
        timeout,
    )
}

/// Where the next chunk would go (used by tests/diagnostics).
pub fn chunk_plan(payload_len: usize, chunk_bytes: usize) -> Vec<(usize, usize)> {
    let chunk_bytes = chunk_bytes.clamp(256, 16 * 1024);
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < payload_len {
        let end = (offset + chunk_bytes).min(payload_len);
        out.push((offset, end - offset));
        offset = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn chunk_plan_is_bounded_and_contiguous() {
        let plan = chunk_plan(10_000, 4096);
        assert_eq!(plan, vec![(0, 4096), (4096, 4096), (8192, 1808)]);
        assert_eq!(plan.iter().map(|(_, n)| n).sum::<usize>(), 10_000);
    }

    #[test]
    fn event_metadata_keeps_correlators_and_rejects_unbounded_values() {
        let body = json!({"request_id": "data-0012", "device_mac": "AABBCCDDEEFF",
            "seq": 12, "plan_id": 7, "job_id": "job-1", "token": "must-not-log"});
        assert_eq!(event_id(&body, "request_id").as_deref(), Some("data-0012"));
        assert_eq!(
            event_id(&body, "device_mac").as_deref(),
            Some("AABBCCDDEEFF")
        );
        assert_eq!(body["seq"].as_u64(), Some(12));
        assert_eq!(body["plan_id"].as_u64(), Some(7));
        assert_eq!(event_id(&body, "token"), None);
        assert_eq!(
            event_id(&json!({"request_id": "x".repeat(81)}), "request_id"),
            None
        );
    }

    #[test]
    fn post_json_reports_http_status_without_exposing_request_or_response_bodies() {
        fn serve_once(status: u16, payload: &'static str) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 1024];
                loop {
                    let count = stream.read(&mut chunk).unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                    if let Some(head_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&request[..head_end]);
                        let content_len = header.lines().find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("Content-Length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        }).unwrap_or(0);
                        if request.len() >= head_end + 4 + content_len {
                            break;
                        }
                    }
                }
                let response = format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len());
                stream.write_all(response.as_bytes()).unwrap();
            });
            let result = post_json(
                &address,
                "/api/data",
                "secret-token",
                &json!({"device_mac": "AABBCCDDEEFF", "request_id": "data-1", "seq": 1,
                    "session_nonce": "request-secret", "snapshot": {"account": "request-secret"}}),
                Duration::from_secs(2),
            );
            server.join().unwrap();
            match result {
                Ok(value) => value.to_string(),
                Err(error) => error.to_string(),
            }
        }

        assert_eq!(
            serve_once(200, r#"{"result":"applied","data_seq":1}"#),
            r#"{"data_seq":1,"result":"applied"}"#
        );
        let failed = serve_once(503, r#"{"secret":"response-secret"}"#);
        assert!(failed.contains("HTTP 503"));
        assert!(!failed.contains("request-secret"));
        assert!(!failed.contains("response-secret"));
        assert_eq!(
            safe_error_category(&anyhow!(failed)),
            "transport_or_protocol"
        );
    }
}
