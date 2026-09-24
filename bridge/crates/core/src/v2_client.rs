//! Bridge side of the v2 device protocol (HTTP transport): authenticated Status
//! read, complete Data, PowerPlan, Activate and the bounded
//! BEGIN/CHUNK/COMMIT Bundle install.
//!
//! All calls carry the device endpoint token + `bridge_id`; none of them create
//! or renew an owner (only the explicit `POST /claim` does).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

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
    let text = serde_json::to_vec(body)?;
    let (status, body) = request(ip, "POST", path, token, Some(&text), &[], timeout)
        .with_context(|| format!("POST {path}"))?;
    let parsed: Value = serde_json::from_str(&body).unwrap_or(json!({"raw": body}));
    if status == 409 {
        return Ok(json!({"result": "rejected", "error": "occupied", "detail": parsed}));
    }
    if status == 401 {
        bail!("device rejected the endpoint token (401)");
    }
    if !(200..300).contains(&status) {
        bail!("device returned HTTP {status}: {body}");
    }
    Ok(parsed)
}

/// Authenticated Status read: the only authoritative device state.
pub fn status(ip: &str, token: &str, timeout: Duration) -> Result<Value> {
    let (status, body) = request(ip, "GET", "/v2/status", token, None, &[], timeout)?;
    if status == 401 {
        bail!("device rejected the endpoint token (401)");
    }
    if status != 200 {
        bail!("device /v2/status HTTP {status}: {body}");
    }
    Ok(serde_json::from_str(&body).context("v2 status json")?)
}

/// Complete bounded Data snapshot (atomic apply + simple ACK).
pub fn data(ip: &str, token: &str, expected_mac: &str, message: &Value, timeout: Duration) -> Result<Value> {
    session_post(ip, "/v2/data", token, expected_mac, message, timeout)
}

/// Formal PowerPlan (the only thing that changes the light deadline).
pub fn plan(ip: &str, token: &str, expected_mac: &str, plan: &Value, timeout: Duration) -> Result<Value> {
    session_post(ip, "/v2/plan", token, expected_mac, plan, timeout)
}

fn session_post(ip: &str, path: &str, token: &str, expected_mac: &str, message: &Value, timeout: Duration) -> Result<Value> {
    let mut command = message.clone();
    let (nonce, device_mac) = session_nonce(ip, token, expected_mac, timeout)?;
    command["protocol"] = json!(2);
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
    post_json(ip, "/v2/activate", token, &json!({
        "protocol": 2, "device_mac": device_mac, "bridge_id": bridge_id, "session_nonce": nonce,
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
        bail!("authenticated status MAC {mac} does not match target MAC {expected_mac}");
    }
    let nonce = state["session_nonce"].as_str().unwrap_or("");
    if nonce.len() != 32 || !nonce.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("device lacks v2 session protection; update firmware before publishing or activating");
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
    let command = json!({"protocol": 2, "device_mac": device_mac, "bridge_id": bridge_id,
        "request_id": request_id, "session_nonce": nonce,
        "length": payload.len(), "content_crc": content_crc});
    let ack = post_json(ip, "/v2/bundle/begin", token, &command, timeout)?;
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
        let path = format!("/v2/bundle/chunk?request_id={request_id}&session_nonce={nonce}&offset={offset}");
        let offset_text = offset.to_string();
        let (status, body) = request(
            ip,
            "POST",
            &path,
            token,
            Some(&payload[offset..end]),
            &[("X-Request-Id", &request_id), ("X-Session-Nonce", &nonce), ("X-Offset", &offset_text)],
            timeout,
        ).with_context(|| format!("bundle chunk at offset {offset}"))?;
        if !(200..300).contains(&status) {
            bail!("bundle chunk at {offset} failed: HTTP {status} {body}");
        }
        let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        if parsed["result"] != "applied" {
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
        "/v2/bundle/commit",
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

    #[test]
    fn chunk_plan_is_bounded_and_contiguous() {
        let plan = chunk_plan(10_000, 4096);
        assert_eq!(plan, vec![(0, 4096), (4096, 4096), (8192, 1808)]);
        assert_eq!(plan.iter().map(|(_, n)| n).sum::<usize>(), 10_000);
    }
}
