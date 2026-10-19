//! Bridge side of the device protocol (HTTP transport): authenticated Status
//! read, complete Data, PowerPlan, Activate and the bounded
//! BEGIN/CHUNK/COMMIT Bundle install.
//!
//! All calls carry the device endpoint token + `bridge_id`; none of them create
//! or renew an owner (only the explicit `POST /claim` does).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex as AsyncMutex;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;

use anyhow::{anyhow, bail, Context, Result};
use crate::platform::model::DeviceIdentity;
use serde_json::{json, Value};
use crate::rendezvous::{CommandFailure, DispatchOutcome};

pub const BUNDLE_CHUNK_BYTES: usize = 4048;
const MAX_BUNDLE_BYTES: usize = crate::platform::model::MAX_BUNDLE_BYTES;

#[derive(Debug)]
pub struct HttpStatus(pub u16);
impl std::fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "device HTTP {}", self.0) }
}
impl std::error::Error for HttpStatus {}

#[derive(Debug, thiserror::Error)]
#[error("device_http_session_expired; query original business transaction")]
struct HttpSessionExpired;

#[derive(Debug, thiserror::Error)]
pub enum HttpRequestError {
    #[error("device HTTP frame exceeds 4096 bytes")]
    FrameLimit,
    #[error("unregistered device HTTP operation")]
    UnsupportedOperation,
}

struct HttpProgress { outcome: DispatchOutcome, stage: &'static str }

async fn write_until(stream: &mut (impl tokio::io::AsyncWrite + Unpin), mut bytes: &[u8], deadline: Instant, progress: &mut HttpProgress) -> Result<()> {
    while !bytes.is_empty() {
        let written = tokio::time::timeout_at(deadline.into(), stream.write(bytes)).await??;
        anyhow::ensure!(written != 0, "device HTTP write incomplete");
        progress.outcome = DispatchOutcome::WriteUnknown;
        bytes = &bytes[written..];
    }
    Ok(())
}

/// One HTTP round trip with a bounded body.
async fn transport_request(
    ip: &str,
    method: &str,
    path: &str,
    token: &str,
    body: Option<&[u8]>,
    extra_headers: &[(&str, &str)],
    content_type: &str,
    timeout: Duration,
    progress: &mut HttpProgress,
) -> Result<(u16, Vec<u8>)> {
    anyhow::ensure!(!timeout.is_zero(), "wifi_window_budget");
    let deadline = Instant::now() + timeout;
    let target = if ip.contains(':') {
        ip.to_string()
    } else {
        format!("{ip}:80")
    };
    let addr: std::net::SocketAddr = target
        .parse()
        .map_err(|e| anyhow!("bad device address {ip}: {e}"))?;
    progress.stage = "http_connect";
    let mut stream = tokio::time::timeout_at(deadline.into(), TcpStream::connect(addr)).await??;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: device\r\nConnection: close\r\nAccept: {content_type}\r\n"
    );
    if !token.is_empty() {
        head.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(bytes) = body {
        head.push_str(&format!(
            "Content-Type: {content_type}\r\nContent-Length: {}\r\n",
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
    progress.stage = "http_write";
    write_until(&mut stream, head.as_bytes(), deadline, progress).await?;
    if let Some(bytes) = body { write_until(&mut stream, bytes, deadline, progress).await?; }
    progress.outcome = DispatchOutcome::AckUnknown;
    progress.stage = "http_ack";
    let mut raw = Vec::new();
    let mut chunk = [0u8; 2048];
    let mut expected_len = None;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        anyhow::ensure!(!remaining.is_zero(), "wifi_window_budget");
        let read = tokio::time::timeout_at(deadline.into(), stream.read(&mut chunk)).await??;
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
    Ok((status, raw[body_start..].to_vec()))
}

#[derive(Default)]
struct HttpFrameSession { id: [u8; 16], next_no: u64, context: [u8; 16] }
static FRAME_SESSIONS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<HttpFrameSession>>>>> = OnceLock::new();
static STATUS_READS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> = OnceLock::new();
static NEXT_OPEN_NONCE: AtomicU64 = AtomicU64::new(1);

fn new_open_nonce() -> [u8; 16] {
    use sha2::{Digest, Sha256};
    let counter=NEXT_OPEN_NONCE.fetch_add(1,Ordering::Relaxed);
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_nanos();
    let digest=Sha256::digest(format!("{}:{now}:{counter}",std::process::id()));
    digest[..16].try_into().unwrap()
}

fn frame_session(ip: &str, token: &str) -> Arc<AsyncMutex<HttpFrameSession>> {
    use sha2::{Digest, Sha256};
    let key = format!("{ip}:{:x}", Sha256::digest(token.as_bytes()));
    FRAME_SESSIONS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap()
        .entry(key).or_insert_with(|| Arc::new(AsyncMutex::new(HttpFrameSession::default()))).clone()
}

fn frame_operation(path: &str) -> Result<u16> {
    let path = path.split('?').next().unwrap_or(path);
    let name = match path {
        "/api/status" => "status", "/api/data" => "data", "/api/plan" => "plan",
        "/claim" => "claim",
        "/api/activate" => "activate", "/api/ota/image" => "ota_image",
        "/api/bundle/begin" => "bundle_begin", "/api/bundle/chunk" => "bundle_chunk",
        "/api/bundle/commit" => "bundle_commit",
        "/api/sync/begin" => "sync_begin", "/api/sync/page" => "sync_page",
        "/api/sync/ack" => "sync_ack", "/api/sync/complete" => "sync_complete",
        "/api/wifi/snapshot" => "wifi_snapshot", "/api/wifi/apply" => "wifi_apply",
        "/api/wifi/result" => "wifi_result", "/api/wifi/result_ack" => "wifi_result_ack",
        "/api/wifi/command" => "wifi_match",
        _ => return Err(HttpRequestError::UnsupportedOperation.into()),
    };
    crate::compact_frame::operation(name).context("unregistered frame operation")
}

async fn request(
    ip: &str, _method: &str, path: &str, token: &str, body: Option<&[u8]>,
    extra_headers: &[(&str, &str)], timeout: Duration,
) -> Result<(u16, String)> {
    use crate::compact_frame::{Header, CONTENT_TYPE, HEADER_LEN, MAX_HTTP_FRAME};
    let mut progress = HttpProgress { outcome: DispatchOutcome::NotSent, stage: "http_prepare" };
    let result = async {
    let deadline = Instant::now() + timeout;
    let mut op = frame_operation(path)?;
    let command_payload = if path.split('?').next()==Some("/api/wifi/command") {
        let mut value:Value=serde_json::from_slice(body.unwrap_or(b"{}"))?;
        op=crate::compact_frame::operation(&format!("wifi_{}",value["op"].as_str().unwrap_or("")))
            .context("invalid_wifi_operation")?;
        value.as_object_mut().context("invalid_wifi_command")?.remove("op");
        Some(serde_json::to_vec(&value)?)
    }else{None};
    let body=command_payload.as_deref().or(body).filter(|b|!b.is_empty());
    progress.stage = "http_session";
    let session = frame_session(ip, token);
    if op != 1 && op != 19 && lock_session_until(&session, deadline).await?.id == [0; 16] {
        Box::pin(status(ip, token, deadline.saturating_duration_since(Instant::now()))).await.context("establish device HTTP session")?;
    }
    let mut session = lock_session_until(&session, deadline).await?;
    let opening = op == 1 || (op == 19 && session.id == [0; 16]);
    anyhow::ensure!(opening || session.id != [0; 16], "device HTTP session is not established");
    let number = session.next_no.checked_add(1).context("HTTP request number exhausted")?;
    session.next_no = number;
    let open_payload;
    let payload = if opening && op == 19 {
        let mut value:Value=serde_json::from_slice(body.filter(|b|!b.is_empty()).unwrap_or(b"{}"))?;
        value["open"]=json!(true);
        open_payload=serde_json::to_vec(&value)?;
        open_payload.as_slice()
    }
        else { body.filter(|b|!b.is_empty()).unwrap_or(b"{}") };
    if payload.len() + HEADER_LEN > MAX_HTTP_FRAME { return Err(HttpRequestError::FrameLimit.into()); }
    let header = Header { kind: 0, op, result: 0,
        payload_len: payload.len().try_into().context("HTTP payload too large")?,
        session: if opening { new_open_nonce() } else { session.id },
        request_no: number, context: if opening { [0; 16] } else { session.context } };
    let mut frame = header.encode().map_err(anyhow::Error::msg)?.to_vec();
    frame.extend_from_slice(payload);
    let (status, response) = transport_request(ip, "POST", path, token, Some(&frame),
        extra_headers, CONTENT_TYPE, deadline.saturating_duration_since(Instant::now()), &mut progress).await?;
    progress.stage = "http_response";
    if matches!(status, 401 | 403) && op != 4 { return Err(HttpStatus(status).into()); }
    // Admission/lifecycle errors can precede frame dispatch. Preserve their
    // bounded explanation instead of misreporting a software-version mismatch.
    if status >= 400 && response.first() != Some(&crate::compact_frame::MARKER) {
        if let Ok(value) = serde_json::from_slice::<Value>(&response) {
            let field = |key: &str| value[key].as_str().filter(|text|
                !text.is_empty() && text.len() <= 64 &&
                text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
            if let (Some(code),Some(stage),Some(action)) = (field("error"),field("stage"),field("action")) {
                let detail=format!("device {code}; stage={stage}; action={action}");
                if value["result"]=="rejected" { return Err(anyhow::Error::new(HttpStatus(status)).context(detail)); }
                if value["result"]=="unknown" { bail!("device result unknown; {detail}"); }
            }
        }
    }
    let (ack, bytes) = Header::decode(&response, MAX_HTTP_FRAME)
        .map_err(|e| anyhow!("device protocol mismatch: {e}; update Bridge and device software"))?;
    if status == 409 && serde_json::from_slice::<Value>(bytes).ok()
        .is_some_and(|reply|reply["error"]=="session") {
        session.id = [0;16];
        return Err(HttpSessionExpired.into());
    }
    anyhow::ensure!(ack.kind == 1 && ack.op == op && ack.request_no == number &&
        (opening || ack.session == session.id), "device HTTP response identity mismatch");
    if opening && status == 200 && ack.result == 0 {
        anyhow::ensure!(ack.session != [0; 16], "device HTTP session missing");
        session.id = ack.session;
        session.context = [0; 16];
    }
    if status == 200 && ack.result == 0 {
        if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
            if let Some(ctx) = value["active_context_id"].as_str() {
                if ctx.len() == 16 && ctx.bytes().all(|b| b.is_ascii_hexdigit()) {
                    session.context.copy_from_slice(ctx.as_bytes());
                }
            }
        }
    }
    Ok((status, std::str::from_utf8(bytes).context("device frame payload UTF-8")?.to_owned()))
    }.await;
    result.map_err(|error: anyhow::Error| {
        if error.chain().any(|cause| cause.downcast_ref::<HttpStatus>().is_some()) { return error; }
        let reason = crate::rendezvous::wifi_error(&error).1;
        error.context(CommandFailure { outcome: progress.outcome, stage: progress.stage, reason })
    })
}

async fn lock_session_until<T>(session: &AsyncMutex<T>, deadline: Instant)
    -> Result<tokio::sync::MutexGuard<'_, T>> {
    tokio::time::timeout_at(deadline.into(), session.lock()).await.context("wifi_window_budget")
}

/// Low-level framed exchange for diagnostics and same-source protocol tests.
/// It shares the production session and request numbering with business calls.
pub async fn raw_frame_request(ip: &str, path: &str, token: &str, body: &[u8],
                         timeout: Duration) -> Result<(u16, Vec<u8>)> {
    let until = Instant::now() + timeout;
    let response = request(ip, "POST", path, token, Some(body), &[], timeout).await;
    let (status, payload) = match response {
        Err(error) if frame_operation(path).ok()==Some(19) &&
            error.downcast_ref::<HttpSessionExpired>().is_some() =>
            request(ip, "POST", path, token, Some(body), &[], until.saturating_duration_since(Instant::now())).await?,
        result => result?,
    };
    Ok((status, payload.into_bytes()))
}

/// Explicit owner operation. The endpoint token binds the HTTP frame session;
/// the separately negotiated device token gates the claim itself.
pub async fn claim(ip: &str, endpoint_token: &str, device_token: &str,
             body: &Value, timeout: Duration) -> Result<(u16, String)> {
    let payload = serde_json::to_vec(body)?;
    request(ip, "POST", "/claim", endpoint_token, Some(&payload),
        &[("X-Device-Token",device_token)], timeout).await
}

/// AP01's explicitly advertised screen-confirm bootstrap is ordinary bounded
/// JSON HTTP on the maintenance listener, including authenticated health reads.
/// It is independent from the business listener's compact-frame session.
pub async fn pairing_request(endpoint: &str, method: &str, path: &str, token: &str,
                       body: Option<&Value>, timeout: Duration) -> Result<(u16, Value)> {
    let valid_path = path == "/api/pairing/info" || path == "/api/pairing" ||
        (method=="GET" && matches!(path,"/health"|"/api/maintenance/status")) ||
        path.strip_prefix("/api/pairing/").is_some_and(|id|
            !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    anyhow::ensure!(valid_path && matches!(method,"GET"|"POST"|"DELETE"),"pairing_request_invalid");
    let payload=body.map(serde_json::to_vec).transpose()?;
    anyhow::ensure!(payload.as_ref().is_none_or(|p| p.len() <= 4096),"pairing_request_too_large");
    let mut progress=HttpProgress{outcome:DispatchOutcome::NotSent,stage:"pairing_prepare"};
    let (status,bytes)=transport_request(endpoint,method,path,token,payload.as_deref(),&[],
        "application/json",timeout.min(Duration::from_secs(5)),&mut progress).await?;
    anyhow::ensure!(bytes.len() <= 8192,"pairing_response_too_large");
    let reply=serde_json::from_slice(&bytes).context("pairing_response_json")?;
    Ok((status,reply))
}

async fn post_json(ip: &str, path: &str, token: &str, body: &Value, timeout: Duration) -> Result<Value> {
    let started = Instant::now();
    let mut payload=body.clone();
    if path!="/api/wifi/command" {if let Some(object)=payload.as_object_mut(){object.remove("op");}}
    let text = serde_json::to_vec(&payload)?;
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
    let response = request(ip, "POST", path, token, Some(&text), &[], timeout).await;
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
    if status == 409 {
        let parsed: Value = serde_json::from_str(&response_body).unwrap_or(Value::Null);
        // A protocol conflict is not necessarily an ownership conflict. Keep
        // the device's rejection (including its identity and failure stage)
        // so callers can handle stale revisions without discarding edits.
        if (parsed["result"] == "rejected" || parsed["ok"] == false)
            && parsed["error"].as_str().is_some_and(|code|
                !code.is_empty() && code.len() <= 64 && code.bytes().all(|b|b.is_ascii_alphanumeric() || b == b'_'))
        {
            return Ok(parsed);
        }
        return Ok(json!({"result": "rejected", "error": "occupied", "detail": parsed}));
    }
    if !(200..300).contains(&status) { return Err(HttpStatus(status).into()); }
    let parsed: Value = serde_json::from_str(&response_body).context("device_json_shape")?;
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
pub async fn status(ip: &str, token: &str, timeout: Duration) -> Result<Value> {
    status_mode(ip, token, timeout, false).await
}

/// Stable 900 s observation. The active context identifies an unchanged
/// device-authenticated data contract; a changed context requires a full read.
pub async fn status_compact(ip: &str, token: &str, timeout: Duration) -> Result<Value> {
    status_mode(ip, token, timeout, true).await
}

async fn status_mode(ip: &str, token: &str, timeout: Duration, compact: bool) -> Result<Value> {
    let started = Instant::now();
    let deadline = started + timeout;
    use sha2::{Digest, Sha256};
    let key = format!("{ip}:{:x}", Sha256::digest(token.as_bytes()));
    let lock = STATUS_READS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap()
        .entry(key).or_insert_with(|| Arc::new(AsyncMutex::new(()))).clone();
    let _read = lock_session_until(&lock, deadline).await?;
    tracing::info!(event = "send", operation = "/api/status", "device HTTP request");
    let projection = compact.then_some(br#"{"compact":true}"#.as_slice());
    let response = request(ip, "POST", "/api/status", token, projection, &[], deadline.saturating_duration_since(Instant::now())).await;
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
        return Err(HttpStatus(401).into());
    }
    if status != 200 {
        return Err(HttpStatus(status).into());
    }
    let mut value:Value=serde_json::from_str(&body).context("device status json")?;
    if value["status_read"].is_string() {
        let id=value["status_read"].as_str().unwrap().to_owned();
        anyhow::ensure!(id.len()==32 && id.bytes().all(|c|c.is_ascii_hexdigit()),"invalid status read ID");
        let total=value["total_bytes"].as_u64().context("status total missing")? as usize;
        anyhow::ensure!(total>0 && total<=16384,"invalid status total");
        let mut bytes=Vec::with_capacity(total);
        loop {
            anyhow::ensure!(value["result"]=="applied" && value["status_read"]==id &&
                value["total_bytes"].as_u64()==Some(total as u64) &&
                value["offset"].as_u64()==Some(bytes.len() as u64),"status page identity mismatch");
            let hex=value["chunk_hex"].as_str().context("status chunk missing")?;
            anyhow::ensure!(!hex.is_empty() && hex.len()<=3600 && hex.len()%2==0 &&
                hex.bytes().all(|c|c.is_ascii_hexdigit()),"invalid status chunk");
            for offset in (0..hex.len()).step_by(2) {bytes.push(u8::from_str_radix(&hex[offset..offset+2],16)?);}
            anyhow::ensure!(bytes.len()<=total,"status chunk exceeds total");
            let next=value["next_offset"].as_u64().context("status next offset missing")?;
            if next==0 {anyhow::ensure!(bytes.len()==total,"incomplete status read");break;}
            anyhow::ensure!(next==bytes.len() as u64 && bytes.len()<total,"status read does not advance");
            let body=serde_json::to_vec(&json!({"status_read":id,"offset":next}))?;
            let (code,reply)=request(ip,"POST","/api/status",token,Some(&body),&[],deadline.saturating_duration_since(Instant::now())).await?;
            anyhow::ensure!(code==200,"status continuation HTTP {code}");
            value=serde_json::from_str(&reply)?;
        }
        value=parse_status_body(std::str::from_utf8(&bytes)?)?;
        let session=frame_session(ip,token);
        let mut session=lock_session_until(&session,deadline).await?;
        if let Some(ctx)=value["active_context_id"].as_str().filter(|s|s.len()==16 && s.bytes().all(|c|c.is_ascii_hexdigit())) {session.context.copy_from_slice(ctx.as_bytes());}
        return Ok(value);
    }
    parse_status_body(&body)
}

fn parse_status_body(body: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(body).context("device status json")?;
    if value["error"].is_string() || matches!(value["result"].as_str(), Some("unknown" | "rejected")) {
        let code = match value["error"].as_str() {
            Some("reply_limit") => "reply_limit",
            Some("batch_lost") => "batch_lost",
            _ => "device_status_failed",
        };
        tracing::warn!(event="result", operation="/api/status", error_category="device_status", error_code=code,
            "device Status returned an error payload");
        bail!("device Status failed: {code}");
    }
    anyhow::ensure!(value["device_mac"].as_str().is_some_and(|mac| DeviceIdentity::normalized_mac(mac).is_some()),
        "device Status missing identity");
    Ok(value)
}

/// Running OTA partition hash, authorized by the device operation token alone.
pub async fn ota_image(ip: &str, token: &str, mac: &str, image_bytes: usize, timeout: Duration) -> Result<Value> {
    let until = Instant::now() + timeout;
    let expected_mac = DeviceIdentity::normalized_mac(mac).context("invalid OTA target MAC")?;
    let path = "/api/ota/image";
    let payload=serde_json::to_vec(&json!({"image_bytes":image_bytes}))?;
    let (status, body) = match request(ip, "POST", path, token, Some(&payload), &[], timeout).await {
        Err(error) if error.downcast_ref::<HttpSessionExpired>().is_some() =>
            request(ip, "POST", path, token, Some(&payload), &[], until.saturating_duration_since(Instant::now())).await?,
        result => result?,
    };
    if status != 200 { bail!("device /api/ota/image HTTP {status}"); }
    let mut image: Value = serde_json::from_str(&body).context("OTA image JSON")?;
    let reported_mac = image["device_mac"].as_str()
        .and_then(DeviceIdentity::normalized_mac).context("OTA image MAC missing")?;
    if reported_mac != expected_mac { bail!("OTA image MAC mismatch"); }
    image["device_mac"] = json!(reported_mac);
    Ok(image)
}

/// sync-v1 keeps the device's precise error code (notably claim_required,
/// digest_mismatch and batch_lost) and validates every response identity.
pub async fn sync(
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
    let expected_mac = DeviceIdentity::normalized_mac(mac).context("invalid sync target MAC")?;
    let wire_mac = format!("{}:{}:{}:{}:{}:{}", &expected_mac[0..2], &expected_mac[2..4],
        &expected_mac[4..6], &expected_mac[6..8], &expected_mac[8..10], &expected_mac[10..12]);
    body["sync_version"] = json!(1);
    body["device_mac"] = json!(wire_mac);
    body["bridge_id"] = json!(bridge_id);
    body["request_id"] = json!(request_id);
    let bytes = serde_json::to_vec(&body)?;
    if bytes.len() > 4096 { bail!("sync request body exceeds 4096 bytes"); }
    let (status, text) = request(ip, "POST", &path, token, Some(&bytes), &[], timeout).await?;
    let reply: Value = serde_json::from_str(&text).context("sync response JSON")?;
    if status == 401 { bail!("sync unauthorized (401)"); }
    if reply["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()
            != Some(expected_mac.as_str()) || reply["session_nonce"] != nonce ||
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
pub async fn data(ip: &str, token: &str, expected_mac: &str, message: &Value, timeout: Duration) -> Result<Value> {
    session_post(ip, "/api/data", token, expected_mac, message, timeout).await
}

/// Formal PowerPlan (the only thing that changes the light deadline).
pub async fn plan(ip: &str, token: &str, expected_mac: &str, plan: &Value, timeout: Duration) -> Result<Value> {
    session_post(ip, "/api/plan", token, expected_mac, plan, timeout).await
}

async fn session_post(ip: &str, path: &str, token: &str, expected_mac: &str, message: &Value, timeout: Duration) -> Result<Value> {
    let mut command = message.clone();
    let (_, device_mac) = session_nonce(ip, token, expected_mac, timeout).await?;
    command["device_mac"] = json!(device_mac);
    command["request_id"] = json!(format!("{}-{:08x}", path.rsplit('/').next().unwrap_or("command"),
        crc32fast::hash(&crate::template::canonical_bytes(message))));
    post_json(ip, path, token, &command, timeout).await
}

/// Explicit remote activation.
pub async fn activate(
    ip: &str,
    token: &str,
    expected_mac: &str,
    bridge_id: &str,
    template_id: &str,
    expected_context: &str,
    timeout: Duration,
) -> Result<Value> {
    let (_, device_mac) = session_nonce(ip, token, expected_mac, timeout).await?;
    let request_id = format!("activate-{expected_context}-{template_id}");
    post_json(ip, "/api/activate", token, &json!({
        "device_mac": device_mac, "bridge_id": bridge_id,
        "request_id": request_id, "template_id": template_id,
        "expected_active_context_id": expected_context,
    }), timeout).await
}

async fn session_nonce(ip: &str, token: &str, expected_mac: &str, timeout: Duration) -> Result<(String, String)> {
    let state = status(ip, token, timeout).await?;
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
pub async fn install_bundle(
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
    let chunk_bytes = chunk_bytes.clamp(256, BUNDLE_CHUNK_BYTES);
    let (nonce, device_mac) = session_nonce(ip, token, expected_mac, timeout).await?;
    let content_crc = format!("{:08x}", crc32fast::hash(payload));
    // Service freezes CSZ1 bytes at publish time; retries send those exact bytes.
    let decoded = bridge_render::bundle_codec::decode(payload, 16 * 1024 * 1024)
        .map_err(|e| anyhow::anyhow!("bundle encoding: {e}"))?;
    let bundle: Value = serde_json::from_slice(&decoded).context("bundle JSON")?;
    let job_id = bundle["job_id"].as_str().filter(|s| !s.is_empty())
        .context("bundle job_id")?;
    let request_id = format!("bundle-{job_id}");
    if request_id.len() > 64 || !request_id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
        bail!("invalid bundle job_id");
    }
    let command = json!({"device_mac": device_mac, "bridge_id": bridge_id,
        "request_id": request_id,
        "length": payload.len(), "content_crc": content_crc});
    let ack = post_json(ip, "/api/bundle/begin", token, &command, timeout).await?;
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
        ).await;
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
    ).await
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
    use std::net::TcpListener;
    use std::thread;
    use std::io::{Read, Write};

    #[tokio::test]
    async fn cancelled_zero_and_partial_writes_preserve_dispatch_evidence() {
        for full_before in [true,false] {
            let (mut writer,mut reader)=tokio::io::duplex(16);
            if full_before {writer.write_all(&[9;16]).await.unwrap();}
            let mut progress=HttpProgress{outcome:DispatchOutcome::NotSent,stage:"http_write"};
            // Outer cancellation drops the pending writer future; no worker can
            // send the remaining suffix after the caller releases its permit.
            let result=tokio::time::timeout(Duration::from_millis(20),
                write_until(&mut writer,&[7;64],Instant::now()+Duration::from_secs(2),&mut progress)).await;
            assert!(result.is_err());drop(writer);
            let mut received=Vec::new();reader.read_to_end(&mut received).await.unwrap();
            assert_eq!(received,vec![if full_before{9}else{7};16]);
            assert_eq!(progress.outcome,if full_before{DispatchOutcome::NotSent}else{DispatchOutcome::WriteUnknown});
        }
    }

    #[tokio::test]
    async fn pre_dispatch_json_errors_keep_stage_without_exposing_other_fields() {
        for result in ["rejected","unknown"] {
            let listener=TcpListener::bind("127.0.0.1:0").unwrap();
            let address=listener.local_addr().unwrap().to_string();
            let server=thread::spawn(move || {
                let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut head=Vec::new();
                while !head.ends_with(b"\r\n\r\n") {let mut byte=[0];stream.read_exact(&mut byte).unwrap();head.push(byte[0]);assert!(head.len()<4096);}
                let text=String::from_utf8(head).unwrap();
                let length=text.lines().find_map(|line|line.split_once(':').filter(|(name,_)|name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_,value)|value.trim().parse::<usize>().ok())).unwrap();
                let mut body=vec![0;length];stream.read_exact(&mut body).unwrap();
                let body=json!({"result":result,"error":"business_offline","stage":"lifecycle","action":"read_device_status","secret":"response-secret"}).to_string();
                write!(stream,"HTTP/1.1 503 Busy\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            });
            let error=request(&address,"POST","/api/status","request-secret",None,&[],Duration::from_secs(2)).await.unwrap_err();
            server.join().unwrap();let text=format!("{error:#}");
            assert!(text.contains("business_offline")&&text.contains("lifecycle")&&text.contains("read_device_status"));
            assert!(!text.contains("protocol mismatch")&&!text.contains("response-secret")&&!text.contains("request-secret"));
            let refused=error.chain().any(|cause|cause.downcast_ref::<HttpStatus>().is_some());
            assert_eq!(refused,result=="rejected");
            if result=="unknown" {assert_eq!(error.downcast_ref::<CommandFailure>().unwrap().outcome,DispatchOutcome::AckUnknown);}
        }
    }

    fn respond_once(status: u16, payload: String) -> (String, thread::JoinHandle<()>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let server = thread::spawn(move || {
                for opening in [true,false] {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut request = Vec::new();
                    let mut chunk = [0; 1024];
                    let start = loop {
                        let count = stream.read(&mut chunk).unwrap();
                        if count == 0 { panic!("incomplete HTTP frame"); }
                        request.extend_from_slice(&chunk[..count]);
                        if let Some(head_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                            let header = String::from_utf8_lossy(&request[..head_end]);
                            let content_len = header.lines().find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("Content-Length")
                                    .then(|| value.trim().parse::<usize>().ok()).flatten()
                            }).unwrap_or(0);
                            if request.len() >= head_end + 4 + content_len { break head_end + 4; }
                        }
                    };
                    let (frame, body) = crate::compact_frame::Header::decode(&request[start..],4096).unwrap();
                    assert_eq!(frame.op==1,opening);
                    if frame.op==14 {
                        let body:Value=serde_json::from_slice(body).unwrap();
                        assert!(body["session_nonce"].is_null());
                        assert!(body.get("permit").is_none(), "retired permit must not enter the frame");
                    }
                    let code = if opening { if status==401 {401}else{200} } else {status};
                    let text = if opening {r#"{"device_mac":"AABBCCDDEEFF","session_nonce":"snapshot-test-nonce"}"#} else {&payload};
                    let header=crate::compact_frame::Header{kind:1,op:frame.op,result:if code>=400 {2}else{0},
                        payload_len:text.len() as u16,session:[7;16],request_no:frame.request_no,context:frame.context};
                    let mut reply=header.encode().unwrap().to_vec(); reply.extend_from_slice(text.as_bytes());
                    let response = format!("HTTP/1.1 {code} Test\r\nContent-Type: application/vnd.codex-status.frame\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",reply.len());
                    stream.write_all(response.as_bytes()).unwrap();
                    stream.write_all(&reply).unwrap();
                    if status==401 {break;}
                }
            });
        (address, server)
    }

    #[tokio::test]
    async fn http_progress_distinguishes_connect_failure_session_open_and_sent_frame() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        drop(listener);
        let error = request(&address, "POST", "/api/status", "connect-test", None, &[], Duration::from_millis(100)).await.unwrap_err();
        let evidence = crate::rendezvous::command_failure(&error).unwrap();
        assert_eq!(evidence.outcome, DispatchOutcome::NotSent);
        assert_eq!(evidence.stage, "http_connect");

        for established in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let token = if established { "sent-test" } else { "opening-test" };
            if established { frame_session(&address, token).lock().await.id = [7; 16]; }
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut wire = Vec::new(); let mut chunk = [0u8; 1024];
                loop {
                    let n = stream.read(&mut chunk).unwrap(); assert_ne!(n, 0);
                    wire.extend_from_slice(&chunk[..n]);
                    if let Some(end) = wire.windows(4).position(|w| w == b"\r\n\r\n") {
                        let length: usize = String::from_utf8_lossy(&wire[..end]).lines().find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("Content-Length").then(||value.trim().parse().unwrap())
                        }).unwrap();
                        if wire.len() == end + 4 + length {
                            let (frame, _) = crate::compact_frame::Header::decode(&wire[end+4..], 4096).unwrap();
                            assert_eq!(frame.op, if established { 18 } else { 1 });
                            break;
                        }
                    }
                }
                // Complete frame received, but no ACK is returned.
            });
            let error = request(&address, "POST", "/api/wifi/command", token, Some(br#"{"op":"match"}"#), &[], Duration::from_secs(2)).await.unwrap_err();
            server.join().unwrap();
            let evidence = crate::rendezvous::command_failure(&error).unwrap();
            assert_eq!(evidence.outcome, if established { DispatchOutcome::AckUnknown } else { DispatchOutcome::NotSent });
            assert_eq!(evidence.stage, if established { "http_ack" } else { "http_session" });
        }
    }

    #[tokio::test]
    async fn chunk_plan_is_bounded_and_contiguous() {
        let plan = chunk_plan(10_000, 4096);
        assert_eq!(plan, vec![(0, 4096), (4096, 4096), (8192, 1808)]);
        assert_eq!(plan.iter().map(|(_, n)| n).sum::<usize>(), 10_000);
    }

    #[tokio::test]
    async fn status_deadline_includes_session_lock_without_sending_or_consuming_number() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let session = frame_session(&address, "synthetic-lock-test");
        let held = session.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _guard = held.blocking_lock();
            ready_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });
        ready_rx.recv().unwrap();
        let started = Instant::now();
        let error = status(&address, "synthetic-lock-test", Duration::from_millis(30)).await.unwrap_err();
        assert!(started.elapsed() < Duration::from_millis(150));
        let evidence = crate::rendezvous::command_failure(&error).unwrap();
        assert_eq!(evidence.outcome, DispatchOutcome::NotSent);
        assert_eq!(evidence.stage, "http_session");
        assert_eq!(evidence.reason, "window_budget_exhausted");
        assert!(matches!(listener.accept(), Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
        worker.join().unwrap();
        assert_eq!(session.lock().await.next_no, 0);
    }

    #[tokio::test]
    async fn status_error_payload_is_not_an_identity_failure_or_success() {
        let error = parse_status_body(r#"{"result":"unknown","error":"reply_limit","stage":"frame"}"#).unwrap_err();
        assert_eq!(error.to_string(), "device Status failed: reply_limit");
        assert_eq!(parse_status_body("{}").unwrap_err().to_string(), "device Status missing identity");
        assert!(parse_status_body(r#"{"device_mac":"AA:BB:CC:DD:EE:FF","result":"applied"}"#).is_ok());
    }

    #[tokio::test]
    async fn event_metadata_keeps_correlators_and_rejects_unbounded_values() {
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

    #[tokio::test]
    async fn post_json_reports_http_status_without_exposing_request_or_response_bodies() {
        async fn serve_once(status: u16, payload: &'static str) -> String {
            let (address, server) = respond_once(status, payload.to_owned());
            let result = post_json(
                &address,
                "/api/data",
                "secret-token",
                &json!({"device_mac": "AABBCCDDEEFF", "request_id": "data-1", "seq": 1,
                    "session_nonce": "request-secret", "snapshot": {"account": "request-secret"}}),
                Duration::from_secs(2),
            ).await;
            server.join().unwrap();
            match result {
                Ok(value) => value.to_string(),
                Err(error) => error.to_string(),
            }
        }

        assert_eq!(
            serve_once(200, r#"{"result":"applied","data_seq":1}"#).await,
            r#"{"data_seq":1,"result":"applied"}"#
        );
        let failed = serve_once(503, r#"{"secret":"response-secret"}"#).await;
        assert!(failed.contains("HTTP 503"));
        assert!(!failed.contains("request-secret"));
        assert!(!failed.contains("response-secret"));
        assert_eq!(
            safe_error_category(&anyhow!(failed)),
            "transport_or_protocol"
        );
    }

    #[tokio::test]
    async fn wifi_snapshot_http_errors_and_invalid_pages_keep_finite_categories() {
        let mac = "AABBCCDDEEFF";
        for (status, body, expected) in [
            (401, json!({"secret":"must-not-expose"}).to_string(), ("failed", "unauthorized")),
            (403, json!({"secret":"must-not-expose"}).to_string(), ("failed", "unauthorized")),
            (200, "{".into(), ("failed", "snapshot_shape")),
            (200, json!({"device_mac":mac,"ok":false,"error":"snapshot_expired"}).to_string(), ("waiting_rendezvous", "snapshot_expired")),
            (200, json!({"device_mac":mac,"ok":false,"error":"revision_conflict"}).to_string(), ("conflict", "revision_conflict")),
            (409, json!({"device_mac":mac,"ok":false,"error":"revision_conflict","stage":"revision"}).to_string(), ("conflict", "revision_conflict")),
            (200, json!({"device_mac":"112233445566","ok":true}).to_string(), ("failed", "snapshot_mismatch")),
            (200, json!({"device_mac":mac,"ok":true,"schema":2,"snapshot_id":"s","revision":1,"total":2,"offset":1,"next_offset":2,"chunk":"{}","done":true}).to_string(), ("failed", "snapshot_mismatch")),
        ] {
            let (ip, server) = respond_once(status, body);
            let error = wifi_snapshot(&ip, "synthetic-token", mac, "bridge", Duration::from_secs(2)).await.unwrap_err();
            server.join().unwrap();
            assert_eq!(crate::rendezvous::wifi_error(&error), expected);
            assert!(!error.to_string().contains("must-not-expose"));
        }
    }

    #[tokio::test]
    async fn ota_image_uses_token_without_sync_owner_and_checks_device_mac() {
        for (reported, accepted) in [("AA:BB:CC:DD:EE:FF", true), ("11:22:33:44:55:66", false)] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = Vec::new();
                let mut chunk = [0u8; 1024];
                let start = loop {
                    let count = stream.read(&mut chunk).unwrap();
                    assert!(count > 0,"incomplete OTA request frame");
                    buffer.extend_from_slice(&chunk[..count]);
                    if let Some(head_end)=buffer.windows(4).position(|w|w==b"\r\n\r\n") {
                        let headers=String::from_utf8_lossy(&buffer[..head_end]);
                        let content_len=headers.lines().find_map(|line| {
                            let (name,value)=line.split_once(':')?;
                            name.eq_ignore_ascii_case("Content-Length")
                                .then(||value.trim().parse::<usize>().ok()).flatten()
                        }).unwrap_or(0);
                        if buffer.len()>=head_end+4+content_len {break head_end+4;}
                    }
                };
                let request = String::from_utf8_lossy(&buffer);
                assert!(request.starts_with("POST /api/ota/image HTTP/1.1"));
                assert!(request.contains("Authorization: Bearer secret-token\r\n"));
                let body = format!(r#"{{"device_mac":"{reported}","image_bytes":1024,"sha256":"abc"}}"#);
                let (frame,payload) = crate::compact_frame::Header::decode(&buffer[start..],4096).unwrap();
                let input:Value=serde_json::from_slice(payload).unwrap();
                assert_eq!(input["image_bytes"],1024);assert_eq!(input["open"],true);
                assert_eq!(frame.op,19);
                let header=crate::compact_frame::Header{kind:1,op:19,result:0,
                    payload_len:body.len() as u16,session:[7;16],request_no:frame.request_no,context:frame.context};
                let mut bytes=header.encode().unwrap().to_vec();bytes.extend_from_slice(body.as_bytes());
                let reply = format!("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.codex-status.frame\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len());
                stream.write_all(reply.as_bytes()).unwrap();stream.write_all(&bytes).unwrap();
            });
            let result = ota_image(&address, "secret-token", "AABBCCDDEEFF", 1024, Duration::from_secs(2)).await;
            server.join().unwrap();
            if accepted { assert_eq!(result.unwrap()["device_mac"], "AABBCCDDEEFF"); }
            else { assert!(result.unwrap_err().to_string().contains("MAC mismatch")); }
        }
    }
}

/// Wi-Fi management uses the endpoint token and borrows the current window.
/// It never claims ownership or sends a PowerPlan.
pub async fn wifi_request(ip: &str, token: &str, mac: &str, bridge: &str, op: &str,
                    mut body: Value, timeout: Duration) -> Result<Value> {
    if !matches!(op, "snapshot" | "apply" | "result" | "result_ack") { bail!("invalid_wifi_operation"); }
    let compact = DeviceIdentity::normalized_mac(mac).context("invalid_device_mac")?;
    let wire_mac = compact.as_bytes().chunks(2).map(|p| std::str::from_utf8(p).unwrap()).collect::<Vec<_>>().join(":");
    body["device_mac"] = json!(wire_mac); body["bridge_id"] = json!(bridge);
    let reply = post_json(ip, &format!("/api/wifi/{op}"), token, &body, timeout).await?;
    if reply["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()
        != DeviceIdentity::normalized_mac(mac).as_deref() {
        if reply["error"] == "occupied" { bail!("occupied"); }
        bail!("wifi_mac_mismatch");
    }
    Ok(reply)
}

/// New control transport, same identity and transaction envelope as GATT.
pub async fn wifi_command(ip:&str,token:&str,mac:&str,bridge:&str,op:&str,mut body:Value,timeout:Duration)->Result<Value> {
    let compact=DeviceIdentity::normalized_mac(mac).context("invalid_device_mac")?;
    // Setup reads share the GATT correlation envelope. Mutation IDs must always
    // come from the durable job; never generate a new transaction here.
    if body["request_id"].is_null() && matches!(op,"match"|"active"|"setup_status"|"scheme_get") {
        body["request_id"]=json!(format!("query-{op}-{:08x}",crc32fast::hash(&crate::template::canonical_bytes(&body))));
    }
    body["device_mac"]=json!(compact.as_bytes().chunks(2).map(|p|std::str::from_utf8(p).unwrap()).collect::<Vec<_>>().join(":"));
    body["bridge_id"]=json!(bridge);body["op"]=json!(op);
    let reply=post_json(ip,"/api/wifi/command",token,&body,timeout).await?;
    anyhow::ensure!(reply["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()==Some(compact.as_str()),"wifi_mac_mismatch");
    Ok(reply)
}

/// Freeze and consume every page before exposing a complete snapshot. One hard
/// deadline covers the entire read; a failed page never becomes an empty list.
pub async fn wifi_snapshot(ip: &str, token: &str, mac: &str, bridge: &str,
                     timeout: Duration) -> Result<crate::platform::wifi::Snapshot> {
    let until = Instant::now() + timeout;
    let mut offset = 0usize; let mut snapshot = String::new(); let mut revision = None;
    let mut total = None; let mut text = String::new();
    loop {
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() { bail!("wifi_snapshot_timeout"); }
        let reply = wifi_request(ip, token, mac, bridge, "snapshot",
            json!({"offset":offset,"snapshot_id":snapshot}), remaining).await?;
        if reply["ok"] != true { bail!("{}", reply["error"].as_str().unwrap_or("wifi_snapshot_rejected")); }
        let current = reply["snapshot_id"].as_str().context("wifi_snapshot_id")?;
        let rev = reply["revision"].as_u64().context("wifi_revision")?;
        let size = reply["total"].as_u64().context("wifi_snapshot_length")? as usize;
        let next = reply["next_offset"].as_u64().context("wifi_snapshot_offset")? as usize;
        let chunk = reply["chunk"].as_str().context("wifi_snapshot_chunk")?;
        if reply["schema"] != 2 || size > 16384 || reply["offset"] != offset || next <= offset
            || next > size || chunk.len() != next - offset || (offset > 0 && (current != snapshot || revision != Some(rev) || total != Some(size))) {
            bail!("wifi_snapshot_mismatch");
        }
        snapshot = current.to_owned(); revision = Some(rev); total = Some(size); text.push_str(chunk); offset = next;
        if reply["complete"] == true {
            if offset != size { bail!("wifi_snapshot_incomplete"); }
            let parsed: crate::platform::wifi::Snapshot = serde_json::from_str(&text).context("wifi_snapshot_shape")?;
            let normalized = DeviceIdentity::normalized_mac(mac).context("invalid_device_mac")?;
            parsed.validate(&normalized)?;
            if parsed.snapshot_id != snapshot || parsed.revision != rev { bail!("wifi_snapshot_mismatch"); }
            return Ok(parsed);
        }
        if offset == size { bail!("wifi_snapshot_incomplete"); }
    }
}
