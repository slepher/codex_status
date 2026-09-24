#![recursion_limit = "512"]

//!
//! Tools render/validate with the firmware's own C++ engine (bridge-render),
//! persist templates with version bumps and backups, and push over HTTP
//! (`POST /template`); BLE is only used for identity/token exchange.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::Engine as _;
use bridge_ble::{lan_ip, Pusher};
use bridge_core::template::{template_hash, Library};
use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2024-11-05";

pub struct McpConfig {
    pub port: u16,
    pub token: String,
    /// Runtime templates (working copies), never the repository.
    pub templates: PathBuf,
    /// Runtime profile state.
    pub profiles: PathBuf,
    /// Runtime data root (backups, previews).
    pub data_root: PathBuf,
    /// Seed template dir from the repo/bundle.
    pub seeds: PathBuf,
    /// Seed profile file from the repo/bundle.
    pub profile_seed: PathBuf,
    /// Last known device address (attribute, not identity).
    pub device_ip: String,
    /// Editable display name (`CodexStatus-<MAC suffix>` by default).
    pub device_name: String,
    /// Device identity key (Wi-Fi MAC) when known.
    pub device_mac: Option<String>,
    /// Bridge display name reported in `POST /claim`.
    pub bridge_name: String,
    /// Bridge owner id (envelope `bridge.hostId`).
    pub bridge_id: String,
    pub root: PathBuf,
}

fn host_label() -> String {
    let raw = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "bridge".to_string());
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '?' })
        .take(16)
        .collect();
    if cleaned.is_empty() {
        "bridge".to_string()
    } else {
        cleaned
    }
}

/// Display name default (Unicode kept; only the owner id stays ASCII).
fn pc_display_name() -> String {
    let raw = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "bridge".to_string());
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .take(24)
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        "bridge".to_string()
    } else {
        cleaned
    }
}

pub fn repo_root() -> PathBuf {
    bridge_core::paths::repo_root()
}

/// Seed runtime files from the repo/bundle copies (existing files win).
pub fn ensure_runtime(cfg: &McpConfig) {
    let _ = std::fs::create_dir_all(&cfg.templates);
    if let Ok(entries) = std::fs::read_dir(&cfg.seeds) {
        for entry in entries.flatten() {
            let source = entry.path();
            if source.extension().map(|e| e != "json").unwrap_or(true) {
                continue;
            }
            let Some(name) = source.file_name() else { continue };
            let target = cfg.templates.join(name);
            if !target.exists() {
                let _ = std::fs::copy(&source, &target);
            }
        }
    }
    if !cfg.profiles.exists() && cfg.profile_seed.exists() {
        if let Some(parent) = cfg.profiles.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::copy(&cfg.profile_seed, &cfg.profiles);
    }
}

impl McpConfig {
    pub fn load_default() -> Self {
        let root = repo_root();
        let data_root = bridge_core::paths::data_root();
        let path = std::env::var("CODEX_STATUS_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_root.join("bridge-app.json"));
        let file: Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_else(|| json!({}));
        let port = std::env::var("CODEX_STATUS_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .or_else(|| file.get("port").and_then(|v| v.as_u64()).map(|v| v as u16))
            .unwrap_or(8765);
        let token = std::env::var("CODEX_STATUS_TOKEN")
            .ok()
            .or_else(|| file.get("token").and_then(|v| v.as_str()).map(str::to_string))
            .unwrap_or_else(|| "test-token-123".to_string());
        let device_ip = std::env::var("CODEX_STATUS_DEVICE_IP")
            .ok()
            .or_else(|| file.get("device_ip").and_then(|v| v.as_str()).map(str::to_string))
            .unwrap_or_else(|| "192.168.1.50".to_string());
        let device_name = std::env::var("CODEX_STATUS_DEVICE_NAME")
            .ok()
            .or_else(|| {
                file.get("device_name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let device_mac = std::env::var("CODEX_STATUS_DEVICE_MAC")
            .ok()
            .or_else(|| {
                file.get("device_mac")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        let bridge_name = std::env::var("CODEX_STATUS_BRIDGE_NAME")
            .ok()
            .or_else(|| {
                file.get("bridge_name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(pc_display_name);
        let templates = std::env::var("CODEX_STATUS_TEMPLATES")
            .ok()
            .map(PathBuf::from)
            .or_else(|| file.get("templates").and_then(|v| v.as_str()).map(PathBuf::from))
            .unwrap_or_else(|| data_root.join("templates"));
        let templates = if templates.is_relative() {
            data_root.join(templates)
        } else {
            templates
        };
        let cfg = Self {
            port,
            token,
            templates,
            profiles: data_root.join("profiles.json"),
            data_root,
            seeds: bridge_core::paths::seed_templates(),
            profile_seed: bridge_core::paths::profile_seed(),
            device_ip,
            device_name,
            device_mac,
            bridge_name,
            bridge_id: bridge_core::short_id(&host_label()),
            root,
        };
        ensure_runtime(&cfg);
        cfg
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn local_hhmm() -> String {
    #[cfg(windows)]
    unsafe {
        let mut time = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut time);
        return format!("{:02}:{:02}", time.wHour, time.wMinute);
    }
    #[cfg(not(windows))]
    "--:--".to_string()
}

fn load_library(cfg: &McpConfig) -> Result<Library> {
    Library::load(&cfg.templates).context("load templates")
}

// ---------------- firmware OTA (docs/power-state §9) ----------------
//
// The device token gates /claim and /doUpdate. It is disclosed only over the
// bonded BLE link, so cache it by the Wi-Fi MAC read from authenticated info.
fn device_token_path(cfg: &McpConfig, mac: &str) -> Option<PathBuf> {
    let mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(mac)?;
    Some(cfg.data_root.join(format!("device-token-{mac}.json")))
}

pub fn load_device_token(cfg: &McpConfig, expected_mac: &str) -> Option<String> {
    let path = device_token_path(cfg, expected_mac)?;
    let text = std::fs::read_to_string(&path).ok()?;
    let doc: Value = serde_json::from_str(&text).ok()?;
    let expected_mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(expected_mac)?;
    let stored_mac = doc.get("device_mac").and_then(Value::as_str)?;
    if bridge_core::platform::model::DeviceIdentity::normalized_mac(stored_mac).as_deref()
        != Some(expected_mac.as_str())
    {
        return None;
    }
    let token = doc.get("token").and_then(|v| v.as_str())?;
    (token.len() == 32).then(|| token.to_string())
}

fn save_device_token(cfg: &McpConfig, expected_mac: &str, token: &str) -> Result<()> {
    let mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(expected_mac)
        .context("invalid target device MAC")?;
    let _ = std::fs::create_dir_all(&cfg.data_root);
    let path = device_token_path(cfg, &mac).context("invalid target device MAC")?;
    let doc = json!({"device_mac": mac, "token": token, "updated_at": now_secs()});
    std::fs::write(&path, serde_json::to_string_pretty(&doc)?)
        .with_context(|| format!("write {}", path.display()))?;
    tracing::info!("device token cached ({})", path.display());
    Ok(())
}

/// Fetch the token over BLE; requires an active device BLE session (BOOT click).
pub async fn fetch_device_token(cfg: &McpConfig, expected_mac: &str) -> Result<String> {
    let mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(expected_mac)
        .context("invalid target device MAC")?;
    let adapter = Pusher::adapter().await?;
    let token = Pusher::request_device_token(&adapter, "CodexStatus-", &mac, 20000).await?;
    save_device_token(cfg, &mac, &token)?;
    Ok(token)
}

// The device answers HTTP between light-sleep windows with Wi-Fi power save
// (listen_interval=10): a cold /status.json can take >10 s (measured 13.8 s),
// while a truly-offline device fails fast. The old 2 s probe therefore flagged
// a sleeping-but-awake device as unreachable and pushed OTA into the queue.
// Two attempts with a wider timeout keep the tool fast when offline.
async fn device_firmware(ip: &str) -> Option<String> {
    let ip = ip.to_string();
    tokio::task::spawn_blocking(move || {
        for attempt in 0..2 {
            if let Ok(status) = bridge_core::device::fetch(&ip, Duration::from_secs(10)) {
                return status.get("Version").map(str::to_string);
            }
            if attempt == 0 {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

async fn device_identity_and_firmware(ip: &str) -> Option<(String, Option<String>)> {
    let ip = ip.to_string();
    tokio::task::spawn_blocking(move || {
        for attempt in 0..2 {
            if let Ok(status) = bridge_core::device::fetch(&ip, Duration::from_secs(10)) {
                let firmware = status.get("Version")?.to_string();
                let mac = status
                    .get("MAC")
                    .and_then(bridge_core::platform::model::DeviceIdentity::normalized_mac);
                return Some((firmware, mac));
            }
            if attempt == 0 {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

async fn wait_for_new_firmware(
    ip: &str,
    before: Option<&str>,
    timeout: Duration,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        if let Some(fw) = device_firmware(ip).await {
            if before != Some(fw.as_str()) {
                return Some(fw);
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn post_firmware(
    ip: &str,
    token: &str,
    bytes: Vec<u8>,
    filename: &str,
) -> reqwest::Result<reqwest::Response> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(filename.to_string())
        .mime_str("application/octet-stream")?;
    let form = reqwest::multipart::Form::new().part("firmware", part);
    client
        .post(format!("http://{ip}/doUpdate?token={token}"))
        .multipart(form)
        .send()
        .await
}

static OTA_RUNNING: AtomicBool = AtomicBool::new(false);

async fn firmware_ota(cfg: &McpConfig, args: &Value) -> Result<Vec<Value>, String> {
    if OTA_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("firmware OTA already running".to_string());
    }
    let result = firmware_ota_inner(cfg, args).await;
    OTA_RUNNING.store(false, Ordering::SeqCst);
    result
}

async fn firmware_ota_inner(cfg: &McpConfig, args: &Value) -> Result<Vec<Value>, String> {
    let rom = require_str(args, "rom")?;
    let path = {
        let p = PathBuf::from(&rom);
        if p.is_absolute() {
            p
        } else {
            cfg.root.join(p)
        }
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if bytes.len() < 1024 || bytes.len() > 0x30_0000 {
        return Err(format!(
            "rom size {} out of range (1024..=0x300000)",
            bytes.len()
        ));
    }
    let explicit_ip = args.get("device_ip").is_some();
    let ip = match args.get("device_ip") {
        Some(value) => value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "invalid device_ip".to_string())?,
        None => cfg.device_ip.clone(),
    };
    let Some((before, reported_mac)) = device_identity_and_firmware(&ip).await else {
        return Err(format!("device {ip} is not reachable over HTTP"));
    };
    let Some(target_mac) = reported_mac else {
        return Err(format!("device at {ip} reported no valid Wi-Fi MAC; refusing OTA"));
    };

    if let Some(value) = args.get("device_mac") {
        let requested_mac = value
            .as_str()
            .ok_or_else(|| "invalid device_mac".to_string())?;
        let requested_mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(requested_mac)
            .ok_or_else(|| "invalid device_mac".to_string())?;
        if requested_mac != target_mac {
            return Err(format!("device at {ip} reports MAC {target_mac}, not requested {requested_mac}"));
        }
    }
    if !explicit_ip {
        if let Some(configured_mac) = cfg.device_mac.as_deref() {
            let configured_mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(configured_mac)
                .ok_or_else(|| "configured device MAC is invalid".to_string())?;
            if configured_mac != target_mac {
                return Err(format!("device at {ip} reports MAC {target_mac}, not configured {configured_mac}"));
            }
        }
    }

    let mut token = load_device_token(cfg, &target_mac);
    if token.is_none() {
        token = Some(fetch_device_token(cfg, &target_mac).await.map_err(|e| {
            format!("device token unavailable ({e}); click BOOT on the device to open its BLE session, then retry")
        })?);
    }
    let filename = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("firmware.bin");

    // Clear a stuck/half-finished previous upload first: older firmware left
    // UpdateClass "already running" after an aborted transfer, which made every
    // following Update.begin() fail. Unknown params are ignored by old ROMs.
    if let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        let abort_url = format!(
            "http://{ip}/diag?ota_abort=1&token={}",
            token.as_deref().unwrap()
        );
        let _ = client.post(&abort_url).send().await;
    }

    let mut upload = post_firmware(&ip, token.as_deref().unwrap(), bytes.clone(), filename).await;
    if matches!(&upload, Ok(resp) if resp.status() == reqwest::StatusCode::UNAUTHORIZED) {
        tracing::warn!("device token rejected; re-requesting over BLE");
        let fresh = fetch_device_token(cfg, &target_mac).await.map_err(|e| {
            format!("device token rejected and re-fetch failed ({e}); click BOOT on the device, then retry")
        })?;
        token = Some(fresh);
        upload = post_firmware(&ip, token.as_deref().unwrap(), bytes.clone(), filename).await;
    }
    match upload {
        Ok(resp) if !resp.status().is_success() => {
            return Err(format!("doUpdate -> HTTP {}", resp.status()));
        }
        Ok(resp) => {
            // The HTTP status is 200 even for a rejected image; the body is the
            // only signal (UPDATE FAILED / UPDATE OK).
            let body = resp.text().await.unwrap_or_default();
            if body.contains("UPDATE FAILED") {
                return Err("device reported UPDATE FAILED (see its /log)".to_string());
            }
        }
        Err(e) => {
            // A reset mid-response can also mean the device already rebooted,
            // so fall through to the version check before declaring failure.
            tracing::warn!("doUpdate transport error: {e}");
        }
    }

    match wait_for_new_firmware(&ip, Some(before.as_str()), Duration::from_secs(60)).await {
        Some(fw) => {
            tracing::info!("firmware OTA ok: {before} -> {fw}");
            Ok(vec![text_block(format!(
                "firmware OTA done: {before} -> {fw} ({} bytes from {})",
                bytes.len(),
                path.display()
            ))])
        }
        None => Err(
            "upload finished but the device version did not change within 60 s; check the device /log"
                .to_string(),
        ),
    }
}

/// One template record from the device's `/status.json` (hash, active flag).
fn remote_template(status: &Value, id: &str) -> Option<(String, bool)> {
    status
        .get("templates")?
        .as_array()?
        .iter()
        .find(|t| t.get("id").and_then(Value::as_str) == Some(id))
        .map(|t| {
            (
                t.get("hash")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                t.get("active").and_then(Value::as_bool).unwrap_or(false),
            )
        })
}

/// Explicit template push over HTTP (docs/power-state.md §5/§9). BLE carries
/// identity only (pairing, endpoint/token, OTA token); the template body goes
/// to the device's `POST /template`, gated by the endpoint token the bridge
/// wrote over BLE. Templates whose hash already matches `/status.json` are
/// skipped; the activation target is re-sent only when it is not already the
/// active template.
pub async fn push_templates_http(
    cfg: &McpConfig,
    ids: &[String],
    activate: Option<&str>,
) -> Result<String, String> {
    if ids.is_empty() {
        return Err("no templates to push".to_string());
    }
    let library = load_library(cfg).map_err(|e| e.to_string())?;
    let ip = cfg.device_ip.trim().trim_end_matches('/');
    let base = format!("http://{ip}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let text = client
        .get(format!("{base}/status.json"))
        .send()
        .await
        .map_err(|e| format!("device {ip} unreachable over HTTP: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let status: Value =
        serde_json::from_str(&text).map_err(|e| format!("device {ip} /status.json: {e}"))?;

    // Occupancy pre-check (firmware >= 0.13.4): the device itself returns 409
    // for non-owners, but a clear message is better than a raw HTTP error.
    if let Some(owner) = status.get("owner").filter(|o| !o.is_null()) {
        let valid = owner
            .get("expires_in_s")
            .and_then(|v| v.as_i64())
            .map(|secs| secs > 0)
            .unwrap_or(true);
        let owner_id = owner.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if valid && !owner_id.is_empty() && owner_id != cfg.bridge_id {
            let name = owner.get("name").and_then(|v| v.as_str()).unwrap_or(owner_id);
            return Err(format!(
                "device is occupied by {name}; use device_claim (force) from the panel/MCP first"
            ));
        }
    }

    // Send the activation target last so the device ends on the chosen template.
    let mut order: Vec<String> = ids.to_vec();
    if let Some(target) = activate {
        if let Some(pos) = order.iter().position(|id| id == target) {
            let target = order.remove(pos);
            order.push(target);
        }
    }

    let mut pushed: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    let mut activated = false;
    for id in &order {
        let entry = library
            .get(id)
            .ok_or_else(|| format!("template not found: {id}"))?;
        let hash = template_hash(&entry.bytes);
        let remote = remote_template(&status, id);
        let unchanged = remote.as_ref().map(|(h, _)| h == &hash).unwrap_or(false);
        let need_activate =
            activate == Some(id.as_str()) && !remote.map(|(_, active)| active).unwrap_or(false);
        if unchanged && !need_activate {
            skipped += 1;
            continue;
        }
        let url = format!(
            "{base}/template?id={id}&version={}&hash={hash}&activate={}&bridge_id={}",
            entry.version,
            if need_activate { 1 } else { 0 },
            cfg.bridge_id
        );
        let resp = client
            .post(&url)
            .bearer_auth(&cfg.token)
            .header("Content-Type", "application/json")
            .body(entry.bytes.clone())
            .send()
            .await
            .map_err(|e| format!("POST /template {id}: {e}"))?;
        let code = resp.status();
        if !code.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("POST /template {id} -> HTTP {code} {body}"));
        }
        pushed.push(id.clone());
        if need_activate {
            activated = true;
        }
    }
    let summary = format!(
        "pushed {} template(s) over HTTP to {ip} ({}; skipped {skipped} unchanged{})",
        pushed.len(),
        if pushed.is_empty() {
            "no changes".to_string()
        } else {
            pushed.join(", ")
        },
        if activated { ", activated" } else { "" }
    );
    tracing::info!("{summary}");
    Ok(summary)
}

fn fetch_live_usage(cfg: &McpConfig) -> Option<String> {
    let addr = format!("127.0.0.1:{}", cfg.port).parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let request = format!(
        "GET /usage HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        cfg.token
    );
    stream.write_all(request.as_bytes()).ok()?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw).ok()?;
    let body = raw.split("\r\n\r\n").nth(1)?.trim();
    if body.is_empty() || body.starts_with("no data") {
        return None;
    }
    Some(body.to_string())
}

fn text_block(text: impl Into<String>) -> Value {
    json!({"type": "text", "text": text.into()})
}

fn image_block(png: &[u8]) -> Value {
    json!({
        "type": "image",
        "mimeType": "image/png",
        "data": base64::engine::general_purpose::STANDARD.encode(png),
    })
}

fn env_from_args(args: &Value) -> bridge_render::Env<'static> {
    // Leaked once per call; process is short-lived enough for this to be fine.
    let channel: &'static str = Box::leak(
        args.get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("WIFI")
            .to_string()
            .into_boxed_str(),
    );
    let ip: &'static str = Box::leak(
        args.get("ip")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(lan_ip)
            .into_boxed_str(),
    );
    let sync: &'static str = Box::leak(
        args.get("sync_hhmm")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(local_hhmm)
            .into_boxed_str(),
    );
    bridge_render::Env {
        channel,
        ip,
        sync_hhmm: sync,
        battery: args
            .get("battery")
            .and_then(|v| v.as_i64())
            .unwrap_or(bridge_render::DEFAULT_BATTERY as i64) as i32,
        state: Box::leak(
            args.get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("BLE OFF")
                .to_string()
                .into_boxed_str(),
        ),
        offline_mins: args
            .get("offline_mins")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1) as i32,
        mode: Box::leak(
            args.get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("light")
                .to_string()
                .into_boxed_str(),
        ),
    }
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "bridge_status",
            "title": "桥与设备状态",
            "description": "桥的局域网地址、设备（名称/MAC/IP/占用者）、模板目录与模板清单（id/version/hash）",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "template_get",
            "title": "读取模板 JSON",
            "description": "读取某个模板的完整 JSON",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "template_validate",
            "title": "校验模板",
            "description": "用固件同源引擎校验模板 JSON，返回通过或具体错误",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"json": {"type": "string"}},
                "required": ["json"],
                "additionalProperties": false
            }
        },
        {
            "name": "template_render",
            "title": "渲染预览",
            "description": "用固件同源引擎按模板画布渲染预览（返回 PNG 图像与文件路径）。usage 省略时取桥的实时数据；也可传 id 或 json。改模板后应先渲染给用户确认",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "json": {"type": "string"},
                    "usage": {"type": "string"},
                    "channel": {"type": "string"},
                    "ip": {"type": "string"},
                    "sync_hhmm": {"type": "string"},
                    "battery": {"type": "integer"},
                    "state": {"type": "string", "description": "设备状态字：AP / BLE ON / BLE OFF / WIFI OFF"},
                    "offline_mins": {"type": "integer", "description": "距上次成功同步的分钟数；负数表示未知（隐藏）"}
                },
                "additionalProperties": false
            }
        },
        {
            "name": "template_save",
            "title": "保存模板",
            "description": "校验并保存模板 JSON 到模板目录（只落盘，不会推送；旧版自动备份）。保存后会返回固件引擎渲染的预览图，供支持图形显示的客户端直接展示。推送请使用 profile_push",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "json": {"type": "string"}
                },
                "required": ["id", "json"],
                "additionalProperties": false
            }
        },
        {
            "name": "profiles_list",
            "title": "列出推送配置",
            "description": "列出全部推送配置（profile：名称 + 最多三个模板 + 激活模板）及可用模板",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "profile_save",
            "title": "保存推送配置",
            "description": "新建或更新推送配置（profile）。templates 为 0..=3 个模板（可传字符串或 {id, enabled} 对象）；顺序即推送顺序，第一个已启用模板为默认显示",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "templates": {
                        "type": "array",
                        "items": {
                            "anyOf": [
                                {"type": "string"},
                                {
                                    "type": "object",
                                    "properties": {
                                        "id": {"type": "string"},
                                        "enabled": {"type": "boolean"}
                                    },
                                    "required": ["id"]
                                }
                            ]
                        }
                    }
                },
                "required": ["id", "templates"],
                "additionalProperties": false
            }
        },
        {
            "name": "profile_push",
            "title": "推送配置到设备",
            "description": "用户显式动作：把某个推送配置里的模板（最多三个）通过 HTTP 推到设备，并激活第一个模板（设备 hash 未变化的模板跳过传输；空配置会报错）",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "firmware_ota",
            "title": "OTA 升级固件",
            "description": "把本地 ROM 上传到设备 /doUpdate 并等待重启后版本变化（约 20–90s）。设备 token 按状态页报告的 Wi-Fi MAC 读取对应缓存，缺失/失效时经已绑定 BLE 链路获取（需设备处于 BLE 会话：单击 BOOT）",
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "rom": {"type": "string", "description": "ROM 路径（绝对或相对仓库根）"},
                    "device_ip": {"type": "string", "description": "覆盖默认设备地址"},
                    "device_mac": {"type": "string", "description": "可选目标 Wi-Fi MAC；必须与目标设备状态页报告的 MAC 一致"}
                },
                "required": ["rom"],
                "additionalProperties": false
            }
        },
        {
            "name": "pm_stats",
            "title": "读取设备功耗统计",
            "description": "只读：拉取设备 PM 统计（light sleep 次数/时长/模式占比 + PM 锁，原始文本），需固件 ≥0.13.0；读取本身会短暂唤醒设备，勿高频调用",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "device_ip": {"type": "string", "description": "覆盖默认设备地址"}
                },
                "additionalProperties": false
            }
        },
        {
            "name": "device_rename",
            "title": "重命名设备",
            "description": "修改设备的显示名（仅桥本地，不写设备；名字可重名，MAC 才是唯一键）",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {"name": {"type": "string", "description": "新的显示名（非空，≤24 字符）"}},
                "required": ["name"],
                "additionalProperties": false
            }
        },
        {
            "name": "device_discover",
            "title": "重新发现设备",
            "description": "用户显式动作：按 via=auto（先 HTTP 再 ARP）|arp|ble（需设备 BLE 会话：单击 BOOT）重新发现设备地址并持久化",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"via": {"type": "string", "description": "auto|arp|ble，默认 auto"}},
                "additionalProperties": false
            }
        },
        {
            "name": "device_owner",
            "title": "查看设备占用",
            "description": "只读：设备名称/MAC/IP/最近发现方式与时间/当前占用者（owner）",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "device_claim",
            "title": "占用设备",
            "description": "用户显式动作：占用设备（空闲/过期时自动成功；他人占用时需 force=true 强制接管）。无 force 时相当于恢复自动占用（清除本地让步状态）",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"force": {"type": "boolean", "description": "强制接管他人占用"}},
                "additionalProperties": false
            }
        },
        {
            "name": "device_release",
            "title": "释放设备",
            "description": "用户显式动作：释放占用并本地让步（不再自动 claim/推送，直到再次 device_claim）",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "device_sleep",
            "title": "让设备进深睡（调试）",
            "description": "调试辅助：强推 mode=deep 并让后续 pull 响应也保持 deep（跳过 10 分钟安静迟滞），设备 60s 宽限后进深睡；用 device_mode auto 恢复",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "device_wake",
            "title": "请求设备回 light（调试）",
            "description": "调试辅助：pull 响应固定返回 light（设备保持在线可读状态/日志）并推 mode=light；设备在 deep 时需等它下一个 pull 生效（可先用 device_contact_s 缩短间隔），用 device_mode auto 恢复",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "device_mode",
            "title": "调试模式覆盖（调试）",
            "description": "调试辅助：把 pull 响应/推送信封的 mode 固定为 auto|deep|light，绕过安静迟滞，便于秒级驱动 deep↔light 循环",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"mode": {"type": "string", "description": "auto|deep|light"}},
                "required": ["mode"],
                "additionalProperties": false
            }
        },
        {
            "name": "device_contact_s",
            "title": "调试拉取间隔（调试）",
            "description": "调试辅助：覆盖桥在 pull 响应里下发的 next_contact_s（秒，30–3600；0 恢复自动：活跃 60/安静 900），用于加快 deep↔light 循环",
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"s": {"type": "integer", "description": "间隔秒数（30–3600），0 恢复自动"}},
                "required": ["s"],
                "additionalProperties": false
            }
        },
        {
            "name": "platform_overview",
            "description": "v2 platform overview: templates (latest per id+render_target), devices (MAC/profile/count), data sources, pending states. Same application service as the UI.",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "template_list",
            "description": "List saved templates with their render target, CRCs, sizes and referencing devices (read-only).",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "template_get_v2",
            "description": "Read a saved template source + compiled plan by id (optional render_target).",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string"}, "render_target": {"type": "string"}},
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "template_validate_v2",
            "description": "Compile/validate template JSON with the firmware-equivalent engine (no save).",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"json": {"type": "string"}},
                "required": ["json"],
                "additionalProperties": false
            }
        },
        {
            "name": "template_save_v2",
            "description": "Save a template (latest per id+render_target) WITHOUT publishing; replacement is explicit.",
            "annotations": {"readOnlyHint": false, "destructiveHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "render_target": {"type": "string"},
                    "json": {"type": "string"}
                },
                "required": ["id", "json"],
                "additionalProperties": false
            }
        },
        {
            "name": "profile_get_v2",
            "description": "Per-device Profile (1-8 ordered template ids, initial active, bindings, sync flag).",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "profile_save_v2",
            "description": "Save a Profile (1-8 ordered ids) WITHOUT publishing.",
            "annotations": {"readOnlyHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {"profile": {"type": "object"}},
                "required": ["profile"],
                "additionalProperties": false
            }
        },
        {
            "name": "family_profiles_v2",
            "description": "List supported render target families and their reusable v2 Profile drafts.",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"render_target": {"type": "string"}},
                "additionalProperties": false
            }
        },
        {
            "name": "family_profile_save_v2",
            "description": "Save a family Profile draft without changing a device or publishing.",
            "annotations": {"readOnlyHint": false, "destructiveHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {"profile": {"type": "object"}},
                "required": ["profile"],
                "additionalProperties": false
            }
        },
        {
            "name": "family_profile_delete_v2",
            "description": "Delete a family Profile draft only.",
            "annotations": {"readOnlyHint": false, "destructiveHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "render_target": {"type": "string"},
                    "id": {"type": "string"}
                },
                "required": ["render_target", "id"],
                "additionalProperties": false
            }
        },
        {
            "name": "family_profile_copy_v2",
            "description": "Copy one explicitly selected device Profile into a new family draft.",
            "annotations": {"readOnlyHint": false, "destructiveHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mac": {"type": "string"},
                    "id": {"type": "string"},
                    "name": {"type": "string"}
                },
                "required": ["mac", "id", "name"],
                "additionalProperties": false
            }
        },
        {
            "name": "platform_publish",
            "description": "Explicit publish: freeze the profile into one Bundle and deliver it at the next reachable opportunity. Save is not publish.",
            "annotations": {"readOnlyHint": false, "destructiveHint": false},
            "inputSchema": {"type": "object", "properties": {"mac": {"type": "string"}, "expected_target_id": {"type": "string"}}, "additionalProperties": false}
        },
        {
            "name": "platform_publish_preview",
            "description": "Read-only full target and conservative byte/space preview. Reuse remains unknown until authenticated asset status is available.",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "platform_font_list",
            "description": "List imported CSFN versions; TTF/OTF conversion is unavailable.",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "platform_font_import",
            "description": "Import a local CSFN .bin into the content-addressed font library; does not publish.",
            "annotations": {"readOnlyHint": false},
            "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"], "additionalProperties": false}
        },
        {
            "name": "platform_publish_cancel",
            "description": "Cancel the queued (unstarted) publish job for the device.",
            "annotations": {"readOnlyHint": false},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "template_activate",
            "description": "Explicit remote activation of an installed template (creates a new device context).",
            "annotations": {"readOnlyHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "data_sources_v2",
            "description": "DataSource list with latest SourceSnapshot, push/pull triggers, quality and validity.",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
                {
            "name": "data_source_save_v2",
            "description": "Create/replace a DataSource (Codex or Static JSON). Saving never triggers a device push.",
            "annotations": {"readOnlyHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {"source": {"type": "object"}},
                "required": ["source"],
                "additionalProperties": false
            }
        },
        {
            "name": "data_probe_v2",
            "description": "Collect once from a DataSource and report the resulting snapshot.",
            "annotations": {"readOnlyHint": false, "idempotentHint": true},
            "inputSchema": {
                "type": "object",
                "properties": {"source_id": {"type": "string"}},
                "required": ["source_id"],
                "additionalProperties": false
            }
        },
        {
            "name": "power_view_v2",
            "description": "Current PowerPlan/provisional/remaining/rendezvous state (read-only; a read never extends the light deadline).",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "power_plan",
            "description": "Explicit formal PowerPlan: light is queued durably for the next authenticated BLE rendezvous when offline; power_view_v2 reports its ACK. sleep requests sleep immediately.",
            "annotations": {"readOnlyHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {"mode": {"type": "string"}},
                "required": ["mode"],
                "additionalProperties": false
            }
        },
        {
            "name": "platform_status_refresh",
            "description": "Read the authenticated device status and reconcile the bridge view.",
            "annotations": {"readOnlyHint": true},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
                {
            "name": "platform_push_now",
            "description": "Deliver pending coordinator data to the device immediately (explicit action; never renews the light lease by itself).",
            "annotations": {"readOnlyHint": false},
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        },
        {
            "name": "platform_recovery",
            "description": "Import a profile from a device recovery digest; the imported profile starts with sync disabled.",
            "annotations": {"readOnlyHint": false},
            "inputSchema": {
                "type": "object",
                "properties": {"digest": {"type": "object"}},
                "required": ["digest"],
                "additionalProperties": false
            }
        }
    ])
}

async fn call_tool(cfg: &McpConfig, name: &str, args: &Value) -> Result<Vec<Value>, String> {
    match name {
        "bridge_status" => {
            let library = load_library(cfg).map_err(|e| e.to_string())?;
            let templates: Vec<Value> = library
                .entries
                .values()
                .map(|e| json!({"id": e.id, "hash": e.hash, "bytes": e.bytes.len()}))
                .collect();
            let device = match bridge_core::device::fetch(
                &cfg.device_ip,
                std::time::Duration::from_secs(2),
            ) {
                Ok(status) => {
                    let fields: serde_json::Map<String, Value> = status
                        .fields
                        .iter()
                        .map(|(k, v)| (k.clone(), json!(v)))
                        .collect();
                    let mac = status
                        .get("MAC")
                        .map(str::to_string)
                        .or_else(|| cfg.device_mac.clone());
                    let owner = status
                        .raw
                        .as_ref()
                        .and_then(|raw| raw.get("owner"))
                        .cloned()
                        .filter(|value| !value.is_null());
                    json!({
                        "online": true,
                        "name": cfg.device_name,
                        "mac": mac,
                        "ip": cfg.device_ip,
                        "owner": owner,
                        "fields": fields,
                    })
                }
                Err(e) => json!({
                    "online": false,
                    "name": cfg.device_name,
                    "mac": cfg.device_mac,
                    "ip": cfg.device_ip,
                    "error": e.to_string(),
                }),
            };
            Ok(vec![text_block(
                json!({
                    "lan_ip": lan_ip(),
                    "http_port": cfg.port,
                    "bridge": {"id": cfg.bridge_id, "name": cfg.bridge_name},
                    "templates_dir": cfg.templates.display().to_string(),
                    "templates": templates,
                    "device": device,
                })
                .to_string(),
            )])
        }
        "template_get" => {
            let id = require_str(args, "id")?;
            let library = load_library(cfg).map_err(|e| e.to_string())?;
            let entry = library.get(&id).ok_or_else(|| format!("template not found: {id}"))?;
            let text = String::from_utf8(entry.bytes.clone()).map_err(|e| e.to_string())?;
            Ok(vec![text_block(text)])
        }
        "template_validate" => {
            let body = require_str(args, "json")?;
            match bridge_render::validate(&body) {
                Ok(()) => Ok(vec![text_block("valid: 固件引擎校验通过")]),
                Err(err) => Err(format!("invalid: {err}")),
            }
        }
        "template_render" => {
            let (template, label) = if let Some(body) = args.get("json").and_then(|v| v.as_str()) {
                (body.to_string(), "inline".to_string())
            } else {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "provide id or json".to_string())?;
                let library = load_library(cfg).map_err(|e| e.to_string())?;
                let entry = library.get(id).ok_or_else(|| format!("template not found: {id}"))?;
                (
                    String::from_utf8(entry.bytes.clone()).map_err(|e| e.to_string())?,
                    id.to_string(),
                )
            };
            bridge_render::validate(&template).map_err(|e| format!("invalid: {e}"))?;
            let usage = args
                .get("usage")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| fetch_live_usage(cfg))
                .unwrap_or_else(|| "{}".to_string());
            let env = env_from_args(args);
            let bits = bridge_render::render_bits(&template, &usage, &env).map_err(|e| e.to_string())?;
            let (w, h) = bridge_render::canvas_size(&template).ok_or("unsupported canvas")?;
            let png = bridge_render::bits_to_png_size(&bits, w, h).map_err(|e| e.to_string())?;
            let preview_dir = cfg.data_root.join("previews");
            std::fs::create_dir_all(&preview_dir).map_err(|e| e.to_string())?;
            let file = preview_dir.join(format!("{label}-{}.png", now_secs()));
            std::fs::write(&file, &png).map_err(|e| e.to_string())?;
            let live = args.get("usage").is_none();
            Ok(vec![
                text_block(format!(
                    "rendered {label} -> {} ({} bytes, usage: {})",
                    file.display(),
                    png.len(),
                    if live { "bridge live data" } else { "provided" }
                )),
                image_block(&png),
            ])
        }
        "template_save" => {
            let id = require_str(args, "id")?;
            let body = require_str(args, "json")?;
            if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err("invalid id: use [a-z0-9_-]".to_string());
            }
            let doc: Value = serde_json::from_str(&body).map_err(|e| format!("json: {e}"))?;
            let file = cfg.templates.join(format!("{id}.json"));
            if file.exists() {
                let backup_dir = cfg.data_root.join("template-backups");
                std::fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
                let backup = backup_dir.join(format!("{id}-{}.json", now_secs()));
                std::fs::copy(&file, &backup).map_err(|e| e.to_string())?;
            }
            let pretty = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
            bridge_render::validate(&pretty).map_err(|e| format!("invalid: {e}"))?;
            std::fs::write(&file, &pretty).map_err(|e| e.to_string())?;
            let hash = template_hash(pretty.as_bytes());
            let mut content = vec![text_block(format!(
                "saved {} ({} bytes, hash {hash}); 仅落盘，推送请用 profile_push",
                file.display(),
                pretty.len()
            ))];
            // Return the rendered result so graphical MCP clients can show it.
            let usage = fetch_live_usage(cfg).unwrap_or_else(|| "{}".to_string());
            let ip = lan_ip();
            let sync = local_hhmm();
            let env = bridge_render::Env {
                channel: "WIFI",
                ip: &ip,
                sync_hhmm: &sync,
                battery: bridge_render::DEFAULT_BATTERY,
                ..Default::default()
            };
            if let Ok(bits) = bridge_render::render_bits(&pretty, &usage, &env) {
                let (w, h) = bridge_render::canvas_size(&pretty).unwrap_or((200, 200));
                if let Ok(png) = bridge_render::bits_to_png_size(&bits, w, h) {
                    content.push(image_block(&png));
                }
            }
            Ok(content)
        }
        "profiles_list" => {
            let library = load_library(cfg).map_err(|e| e.to_string())?;
            let known: Vec<String> = library.entries.values().map(|e| e.id.clone()).collect();
            let profiles = bridge_core::profile::ProfilesFile::load(&profiles_path(cfg))
                .map_err(|e| e.to_string())?;
            let list: Vec<Value> = profiles
                .profiles
                .iter()
                .map(|p| {
                    json!({
                        "id": p.id,
                        "name": p.name,
                        "templates": p.templates,
                        "enabled": p.enabled_ids(),
                    })
                })
                .collect();
            Ok(vec![text_block(
                json!({"profiles": list, "templates": known}).to_string(),
            )])
        }
        "profile_save" => {
            let id = require_str(args, "id")?;
            let library = load_library(cfg).map_err(|e| e.to_string())?;
            let known: Vec<String> = library.entries.values().map(|e| e.id.clone()).collect();
            let templates: Vec<bridge_core::profile::ProfileEntry> = args
                .get("templates")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            if let Some(id) = item.as_str() {
                                Some(bridge_core::profile::ProfileEntry {
                                    id: id.to_string(),
                                    enabled: true,
                                })
                            } else {
                                let id = item.get("id").and_then(|v| v.as_str())?.to_string();
                                let enabled = item
                                    .get("enabled")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(true);
                                Some(bridge_core::profile::ProfileEntry { id, enabled })
                            }
                        })
                        .collect()
                })
                .ok_or_else(|| "missing argument: templates".to_string())?;
            let profile = bridge_core::profile::Profile {
                id,
                name: args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                templates,
            };
            let path = profiles_path(cfg);
            let mut profiles = bridge_core::profile::ProfilesFile::load(&path)
                .map_err(|e| e.to_string())?;
            profiles.upsert(profile, &known).map_err(|e| e.to_string())?;
            profiles.save(&path).map_err(|e| e.to_string())?;
            Ok(vec![text_block(format!(
                "profile saved -> {}",
                path.display()
            ))])
        }
        "profile_push" => {
            let id = require_str(args, "id")?;
            let profiles = bridge_core::profile::ProfilesFile::load(&profiles_path(cfg))
                .map_err(|e| e.to_string())?;
            let profile = profiles
                .get(&id)
                .ok_or_else(|| format!("profile not found: {id}"))?;
            let enabled = profile.enabled_ids();
            if enabled.is_empty() {
                return Err("profile has no enabled templates; nothing to push".to_string());
            }
            let activate = enabled.first().cloned();
            let summary = push_templates_http(cfg, &enabled, activate.as_deref()).await?;
            Ok(vec![text_block(format!("profile {id}: {summary}"))])
        }
        "firmware_ota" => firmware_ota(cfg, args).await,
        "pm_stats" => {
            let ip = args
                .get("device_ip")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| cfg.device_ip.clone());
            let fetch_ip = ip.clone();
            let text = tokio::task::spawn_blocking(move || {
                bridge_core::device::fetch_pmstats(&fetch_ip, Duration::from_secs(5))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
            Ok(vec![text_block(text)])
        }
        "device_rename" | "device_discover" | "device_owner" | "device_claim"
        | "device_release" | "device_sleep" | "device_wake" | "device_mode"
        | "device_contact_s" => {
            // Implemented in bridge-app (live identity/occupancy state); this
            // library copy has no running app to mutate.
            Err("device tools are only available in the tray app (bridge-app)".to_string())
        }
        // v2 platform tools share the app's application service; without a
        // running app there is no live state to read or change.
        "platform_overview" | "template_list" | "template_get_v2" | "template_save_v2"
        | "template_validate_v2" | "profile_get_v2" | "profile_save_v2"
        | "family_profiles_v2" | "family_profile_save_v2"
        | "family_profile_delete_v2" | "family_profile_copy_v2"
        | "platform_publish" | "platform_publish_preview" | "platform_font_list" | "platform_font_import" | "platform_publish_cancel" | "template_activate"
        | "data_sources_v2" | "data_source_save_v2" | "data_probe_v2" | "power_view_v2" | "power_plan"
        | "platform_status_refresh" | "platform_push_now" | "platform_recovery" => {
            Err("platform tools are only available in the tray app (bridge-app)".to_string())
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

fn profiles_path(cfg: &McpConfig) -> std::path::PathBuf {
    cfg.profiles.clone()
}

fn require_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("missing argument: {key}"))
}

fn respond(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn respond_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

pub async fn handle_request(cfg: &McpConfig, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
    match method {
        "initialize" => Some(respond(
            &id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "codex-status", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "模板编辑工具：先 template_get 读取现状，template_render 用固件同源引擎出图给用户确认。模板保存只落盘；推送到设备是用户显式动作，用 profile_push（配置=最多三个模板的组合）。不要跳过渲染确认，也不要在用户未同意时推送。设备功耗/light sleep 诊断用 pm_stats（只读，勿高频）。设备身份/发现/占用：bridge_status/device_owner 只读；device_rename、device_discover、device_claim(force)、device_release 会改变桥或设备状态，需用户明确要求。调试四件套：device_sleep（推 deep 并保持 deep）、device_wake（pull 固定 light，回在线读日志）、device_mode（auto|deep|light，绕过 10 分钟安静迟滞）、device_contact_s（覆盖 pull 间隔，加速循环）。"
            }),
        )),
        "notifications/initialized" => None,
        "ping" => Some(respond(&id, json!({}))),
        "tools/list" => Some(respond(&id, json!({"tools": tool_definitions()}))),
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = match call_tool(cfg, name, &args).await {
                Ok(content) => json!({"content": content, "isError": false}),
                Err(message) => json!({
                    "content": [text_block(format!("error: {message}"))],
                    "isError": true
                }),
            };
            Some(respond(&id, result))
        }
        other => Some(respond_error(&id, -32601, &format!("method not found: {other}"))),
    }
}

#[cfg(test)]
mod device_token_tests {
    use super::{load_device_token, save_device_token, McpConfig};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn config() -> McpConfig {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codex-device-token-{id}"));
        std::fs::create_dir_all(&root).unwrap();
        McpConfig {
            port: 8765,
            token: String::new(),
            templates: PathBuf::new(),
            profiles: PathBuf::new(),
            data_root: root.clone(),
            seeds: PathBuf::new(),
            profile_seed: PathBuf::new(),
            device_ip: String::new(),
            device_name: String::new(),
            device_mac: None,
            bridge_name: String::new(),
            bridge_id: String::new(),
            root,
        }
    }

    #[test]
    fn cache_is_mac_scoped_and_rejects_unbound_or_mismatched_documents() {
        let cfg = config();
        let mac_a = "70:04:1d:aa:bb:cc";
        let mac_b = "70:04:1d:aa:bb:cd";
        let token_a = "a".repeat(32);
        let token_b = "b".repeat(32);
        save_device_token(&cfg, mac_a, &token_a).unwrap();
        save_device_token(&cfg, mac_b, &token_b).unwrap();
        assert_eq!(load_device_token(&cfg, mac_a).as_deref(), Some(token_a.as_str()));
        assert_eq!(load_device_token(&cfg, mac_b).as_deref(), Some(token_b.as_str()));

        let path_a = cfg.data_root.join("device-token-70041DAABBCC.json");
        std::fs::write(
            &path_a,
            serde_json::json!({"device_mac": mac_b, "token": token_a}).to_string(),
        )
        .unwrap();
        assert_eq!(load_device_token(&cfg, mac_a), None);

        std::fs::write(
            cfg.data_root.join("device-token.json"),
            serde_json::json!({"token": token_a}).to_string(),
        )
        .unwrap();
        assert_eq!(load_device_token(&cfg, mac_a), None);
        let _ = std::fs::remove_dir_all(cfg.data_root);
    }
}
