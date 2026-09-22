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
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    if let Some(bytes) = body {
        stream.write_all(bytes)?;
    }
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let response_body = text
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .trim()
        .to_string();
    Ok((status, response_body))
}

fn post_json(ip: &str, path: &str, token: &str, body: &Value, timeout: Duration) -> Result<Value> {
    let text = serde_json::to_vec(body)?;
    let (status, body) = request(ip, "POST", path, token, Some(&text), timeout)?;
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
    let (status, body) = request(ip, "GET", "/v2/status", token, None, timeout)?;
    if status == 401 {
        bail!("device rejected the endpoint token (401)");
    }
    if status != 200 {
        bail!("device /v2/status HTTP {status}: {body}");
    }
    Ok(serde_json::from_str(&body).context("v2 status json")?)
}

/// Complete bounded Data snapshot (atomic apply + simple ACK).
pub fn data(ip: &str, token: &str, message: &Value, timeout: Duration) -> Result<Value> {
    post_json(ip, "/v2/data", token, message, timeout)
}

/// Formal PowerPlan (the only thing that changes the light deadline).
pub fn plan(ip: &str, token: &str, plan: &Value, timeout: Duration) -> Result<Value> {
    post_json(ip, "/v2/plan", token, plan, timeout)
}

/// Explicit remote activation.
pub fn activate(
    ip: &str,
    token: &str,
    bridge_id: &str,
    template_id: &str,
    timeout: Duration,
) -> Result<Value> {
    let path = format!("/v2/activate?id={template_id}");
    post_json(ip, &path, token, &json!({"bridge_id": bridge_id}), timeout)
}

/// Bounded BEGIN/CHUNK/COMMIT install of a complete Bundle payload.
pub fn install_bundle(
    ip: &str,
    token: &str,
    bridge_id: &str,
    payload: &[u8],
    chunk_bytes: usize,
    timeout: Duration,
) -> Result<Value> {
    if payload.is_empty() || payload.len() > MAX_BUNDLE_BYTES {
        bail!("bundle payload size {} out of range", payload.len());
    }
    let chunk_bytes = chunk_bytes.clamp(256, 16 * 1024);
    let begin = format!("/v2/bundle/begin?len={}", payload.len());
    let ack = post_json(ip, &begin, token, &json!({"bridge_id": bridge_id}), timeout)?;
    if ack["result"] != "applied" {
        return Ok(ack);
    }
    let mut offset = 0usize;
    while offset < payload.len() {
        let end = (offset + chunk_bytes).min(payload.len());
        let path = format!("/v2/bundle/chunk?offset={offset}");
        let (status, body) = request(
            ip,
            "POST",
            &path,
            token,
            Some(&payload[offset..end]),
            timeout,
        )?;
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
    let commit = format!("/v2/bundle/commit?len={}", payload.len());
    post_json(
        ip,
        &commit,
        token,
        &json!({"bridge_id": bridge_id}),
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
