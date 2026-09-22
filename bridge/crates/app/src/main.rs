#![cfg_attr(windows, windows_subsystem = "windows")]

mod autostart;
mod config;
mod discovery;
mod icon;
mod platform;
mod watchdog;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bridge_ble::{lan_ip, BleConfig, Pusher};
use bridge_core::activity::{usage_fingerprint, Activity};
use bridge_core::codex::locate_codex;
use bridge_core::http::{serve, AppState};
use bridge_core::runtime::{run_poller, PollerConfig};
use bridge_core::template::Library;
use bridge_core::short_id;
use config::Config;
use discovery::{arp_scan_for_mac, default_device_name, normalize_mac};
use icon::State as IconState;
use serde_json::{json, Value};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tokio::sync::{Notify, RwLock};

struct RuntimeStatus {
    last_sync: Option<i64>,
    last_error: Option<String>,
    last_error_at: Option<i64>,
    paused: bool,
    last_push_at: Option<i64>,
    last_push_error: Option<String>,
    /// Last successful `POST /usage`; while fresh, the HTTP path is the healthy
    /// data route and BLE scanning stays off (docs/history/sleep-plan-v4.md §4.6).
    last_push_ok_at: Option<i64>,
    /// Consecutive failed push attempts; a single timeout must not invalidate
    /// the HTTP path (the device's WebServer occasionally misses a request).
    push_fail_streak: u32,
    /// Informational device note (owner, yielded, discovery), not an error.
    device_note: Option<String>,
}

/// Last device status read, served to the panel without a blocking fetch.
#[derive(Clone)]
struct CachedDevice {
    fetched_at: i64,
    online: bool,
    ip: String,
    fields: Vec<(String, String)>,
    /// `owner` object from `/status.json` (firmware >= 0.13.4), null = free.
    owner: Option<Value>,
}

/// Last `GET /pmstats` response; short TTL so a tab open + manual refresh
/// cannot hammer the device (each read briefly wakes it from light sleep).
#[derive(Clone)]
struct CachedPmStats {
    fetched_at: i64,
    online: bool,
    ip: String,
    text: String,
}

/// How/when the current device IP was last learned (udp/arp/ble/http/config).
#[derive(Clone)]
struct Discovery {
    via: String,
    at: i64,
}

/// Last `owner` seen in `/status.json` plus when it was read.
#[derive(Clone)]
struct CachedOwner {
    fetched_at: i64,
    owner: Option<Value>,
}

struct AppCtx {
    config: Config,
    root: PathBuf,
    app_handle: OnceLock<AppHandle>,
    envelope: Arc<RwLock<Option<Value>>>,
    library: Arc<RwLock<Library>>,
    force_ble: Arc<Notify>,
    v2_delivery: tokio::sync::Mutex<()>,
    status: Mutex<RuntimeStatus>,
    /// Live device address (attribute): seeded from config, updated by UDP.
    device_ip: Mutex<String>,
    /// Device Wi-Fi MAC (identity key): learned from UDP/HTTP/BLE and persisted.
    device_mac: Mutex<Option<String>>,
    /// Editable display name (not a key); defaults to `CodexStatus-<suffix>`.
    device_name: Mutex<String>,
    /// Bridge display name reported in `POST /claim` (defaults to the host name).
    bridge_name: Mutex<String>,
    /// Reused as the device-side owner id (envelope `bridge.hostId`).
    bridge_id: String,
    /// Last discovery method/time for the device page.
    discover: Mutex<Option<Discovery>>,
    device_cache: Mutex<Option<CachedDevice>>,
    pmstats_cache: Mutex<Option<CachedPmStats>>,
    /// Last owner seen in `/status.json` (10 s cache window).
    owner_cache: Mutex<Option<CachedOwner>>,
    /// Timestamp of the last successful `/claim` (renew throttle).
    last_claim_at: Mutex<Option<i64>>,
    /// User released the device: no auto-claim and no pushes until resumed.
    yielded: AtomicBool,
    /// Firmware without `/claim` (404): fall back to the legacy push behavior.
    claim_unsupported: AtomicBool,
    /// An ARP fallback scan is already running.
    arp_running: AtomicBool,
    /// Consecutive failed `/status.json` reads (ARP trigger threshold).
    device_fail_streak: AtomicU32,
    /// v0.14 mode/activity state shared with the HTTP pull path
    /// (usage_rev / last_change_at / pending queues; docs §13.4).
    activity: Arc<Activity>,
    /// ROM path of an OTA queued while the device was asleep (`pending.ota`).
    pending_ota_rom: Mutex<Option<PathBuf>>,
    /// The device asked for a BLE handshake in its UDP announce (`ble=1`).
    udp_ble: AtomicBool,
    /// Set when the device address changed; the push loop sends immediately.
    device_ip_dirty: AtomicBool,
    force_push: Arc<Notify>,
    mcp_port: Mutex<u16>,
    mcp_error: Mutex<Option<String>>,
    mcp_tx: tokio::sync::watch::Sender<u16>,
    /// v2 platform application service: the single business model shared by the
    /// four UI pages and MCP (templates, devices, data sources, power, MCP).
    platform: Arc<bridge_core::platform::service::PlatformService>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Tray stays OK while a successful push is this recent (5 min heartbeat + jitter).
const HTTP_PUSH_HEALTHY_SECS: u64 = 360;

/// ASCII-sanitized host label. Feeds `bridge.hostId` (owner id) and must stay
/// stable across versions for existing installs.
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
        "bridge".into()
    } else {
        cleaned
    }
}

/// Display-friendly host name (Unicode kept); defaults for `bridge_name`.
fn pc_name() -> String {
    let raw = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "bridge".to_string());
    let cleaned = sanitize_display(&raw, 24);
    if cleaned.is_empty() {
        "bridge".to_string()
    } else {
        cleaned
    }
}

/// Display labels (device/bridge names) may be Unicode; only control
/// characters are replaced. They never reach the device template renderer.
fn sanitize_display(raw: &str, max: usize) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .take(max)
        .collect();
    cleaned.trim().to_string()
}

/// Percent-encode a query value (names can contain spaces or non-ASCII).
fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Persist `{device_name, device_mac, device_ip, bridge_name}` into
/// `<data>/bridge-app.json` (read-modify-write, like `persist_mcp_port`).
fn save_identity(ctx: &AppCtx) {
    let name = ctx.device_name.lock().unwrap().clone();
    let mac = ctx.device_mac.lock().unwrap().clone();
    let ip = ctx.device_ip.lock().unwrap().clone();
    let bridge_name = ctx.bridge_name.lock().unwrap().clone();
    let path = bridge_core::paths::data_root().join("bridge-app.json");
    let mut doc: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    if let Some(object) = doc.as_object_mut() {
        object.insert("device_name".to_string(), json!(name));
        object.insert("device_mac".to_string(), json!(mac));
        object.insert("device_ip".to_string(), json!(ip));
        object.insert("bridge_name".to_string(), json!(bridge_name));
    }
    if let Ok(text) = serde_json::to_string_pretty(&doc) {
        let _ = std::fs::write(&path, text);
    }
}

/// First-learn a MAC: adopts it as the identity key, generates the default
/// display name when none is set, persists. A mismatch is always rejected
/// (anti-crosstalk); unknown/empty input is ignored.
fn learn_mac(ctx: &AppCtx, mac: &str, source: &str) -> bool {
    let normalized = normalize_mac(mac);
    if normalized.len() != 12 {
        return false;
    }
    let mut known = ctx.device_mac.lock().unwrap();
    match known.as_deref() {
        Some(existing) if existing == normalized => true,
        Some(_) => {
            tracing::debug!("{source}: device mac mismatch ignored");
            false
        }
        None => {
            *known = Some(normalized.clone());
            drop(known);
            let info = {
                let mut name = ctx.device_name.lock().unwrap();
                if name.trim().is_empty() {
                    *name = default_device_name(&normalized);
                    Some(name.clone())
                } else {
                    None
                }
            };
            if let Some(name) = info {
                tracing::info!("device identity learned via {source}: {normalized} ({name})");
            } else {
                tracing::info!("device identity learned via {source}: {normalized}");
            }
            save_identity(ctx);
            true
        }
    }
}

/// Update the address attribute; `via` records the discovery method shown in
/// the panel/MCP. Returns true when the address actually changed.
fn set_device_ip(ctx: &AppCtx, ip: &str, via: &str) -> bool {
    let ip = ip.trim();
    if ip.is_empty() {
        return false;
    }
    let changed = {
        let mut current = ctx.device_ip.lock().unwrap();
        if current.as_str() == ip {
            false
        } else {
            *current = ip.to_string();
            true
        }
    };
    *ctx.discover.lock().unwrap() = Some(Discovery {
        via: via.to_string(),
        at: now_secs(),
    });
    if changed {
        tracing::info!("device endpoint updated: {ip} (via {via})");
        ctx.device_ip_dirty.store(true, Ordering::SeqCst);
        ctx.force_push.notify_one();
    }
    save_identity(ctx);
    changed
}

fn owner_id(owner: &Value) -> Option<&str> {
    owner.get("id").and_then(|v| v.as_str())
}

fn owner_valid(owner: &Value) -> bool {
    if owner.is_null() {
        return false;
    }
    match owner.get("expires_in_s").and_then(|v| v.as_i64()) {
        Some(secs) => secs > 0,
        None => true,
    }
}

fn owner_line(owner: &Value) -> String {
    let id = owner_id(owner).unwrap_or("?");
    let name = owner.get("name").and_then(|v| v.as_str()).unwrap_or(id);
    let host = owner.get("host").and_then(|v| v.as_str()).unwrap_or("?");
    let port = owner.get("port").and_then(|v| v.as_i64()).unwrap_or(0);
    match owner.get("expires_in_s").and_then(|v| v.as_i64()) {
        Some(secs) => format!("{name}@{host}:{port}（剩余 {secs}s）"),
        None => format!("{name}@{host}:{port}"),
    }
}

fn set_owner_cache(ctx: &AppCtx, owner: Option<Value>) {
    let owner = match owner {
        Some(value) if !value.is_null() => Some(value),
        _ => None,
    };
    *ctx.owner_cache.lock().unwrap() = Some(CachedOwner {
        fetched_at: now_secs(),
        owner,
    });
}

fn cached_owner(ctx: &AppCtx) -> Option<Value> {
    let cache = ctx.owner_cache.lock().unwrap();
    cache
        .as_ref()
        .filter(|c| now_secs() - c.fetched_at <= 30)
        .and_then(|c| c.owner.clone())
}

fn set_device_note(ctx: &AppCtx, note: Option<String>) {
    let mut status = ctx.status.lock().unwrap();
    status.device_note = note;
}

/// Adopt identity from a BLE info JSON (`{mac, ip, http_port}`).
fn adopt_ble_info(ctx: &AppCtx, info: &Value) -> Result<(), String> {
    let mac = info.get("mac").and_then(|v| v.as_str()).unwrap_or("");
    let ip = info.get("ip").and_then(|v| v.as_str()).unwrap_or("");
    if mac.is_empty() && ip.is_empty() {
        return Err("device info has no mac/ip (firmware < 0.13.4?)".to_string());
    }
    if !mac.is_empty() && !learn_mac(ctx, mac, "ble") {
        return Err(format!("device mac mismatch: {mac}"));
    }
    if !ip.is_empty() {
        set_device_ip(ctx, ip, "ble");
    }
    Ok(())
}

/// Weekly remaining percent, mirroring the firmware window pick (>= 10080 min).
fn weekly_remaining(usage: &Option<Value>) -> Option<i32> {
    let buckets = usage.as_ref()?.get("buckets")?.as_array()?;
    let bucket = buckets
        .iter()
        .find(|b| b.get("id").and_then(|v| v.as_str()) == Some("codex"))?;
    let windows = bucket.get("windows")?.as_array()?;
    let window = windows
        .iter()
        .find(|w| w.get("windowMins").and_then(|v| v.as_i64()).unwrap_or(0) >= 10080)
        .or_else(|| windows.first())?;
    let used = window.get("usedPercent")?.as_i64()?;
    Some((100 - used).clamp(0, 100) as i32)
}

fn tray_snapshot(ctx: &AppCtx) -> (Option<i32>, IconState, String) {
    let usage = ctx.envelope.try_read().ok().and_then(|g| g.clone());
    let percent = weekly_remaining(&usage);
    let status = ctx.status.lock().unwrap();
    let now = now_secs();
    let state = if let Some(at) = status.last_error_at {
        if now - at < 300 {
            IconState::Error
        } else if status.last_sync.is_none() {
            IconState::NoData
        } else {
            stale_or_ok(&status, now)
        }
    } else {
        stale_or_ok(&status, now)
    };
    let sync_text = status
        .last_sync
        .map(|s| {
            let diff = now - s;
            if diff < 60 {
                "刚刚".to_string()
            } else if diff < 3600 {
                format!("{} 分钟前", diff / 60)
            } else {
                format!("{} 小时前", diff / 3600)
            }
        })
        .unwrap_or_else(|| "--".to_string());
    let mut tip = format!(
        "Codex Status 桥 · 周余量 {}",
        percent
            .map(|p| format!("{p}%"))
            .unwrap_or_else(|| "--".to_string())
    );
    if status.paused {
        tip.push_str(" · 已暂停");
    } else {
        tip.push_str(&format!(" · 同步 {sync_text}"));
    }
    if let Some(err) = status.last_error.as_deref() {
        tip.push_str(&format!(" · {err}"));
    }
    if let Some(note) = status.device_note.as_deref() {
        tip.push_str(&format!(" · {note}"));
    }
    (percent, state, tip)
}

fn stale_or_ok(status: &RuntimeStatus, now: i64) -> IconState {
    match status.last_sync {
        Some(sync) if now - sync <= HTTP_PUSH_HEALTHY_SECS as i64 => IconState::Ok,
        _ => IconState::Stale,
    }
}

fn refresh_tray(app: &AppHandle, ctx: &Arc<AppCtx>) {
    let (percent, state, tip) = tray_snapshot(ctx);
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(tray) = app.tray_by_id("bridge-tray") {
            let _ = tray.set_icon(Some(icon::render(percent, state)));
            let _ = tray.set_tooltip(Some(tip.as_str()));
        }
    });
}

fn spawn_tray_loop(app: AppHandle, ctx: Arc<AppCtx>) {
    tauri::async_runtime::spawn(async move {
        loop {
            refresh_tray(&app, &ctx);
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

fn open_path(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
}

fn show_panel(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
async fn get_status(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    let usage = state.envelope.read().await.clone();
    refresh_library(&state).await;
    let library = state.library.read().await;
    let templates: Vec<Value> = library
        .entries
        .values()
        .map(|e| json!({"id": e.id, "hash": e.hash}))
        .collect();
    // Compute the device snapshot before locking `status` (it takes the same
    // locks in the opposite order and would deadlock against a 3 s poll).
    let device = device_identity_json(&state);
    let activity = state.activity.snapshot();
    let status = state.status.lock().unwrap();
    Ok(json!({
        "http": format!("http://0.0.0.0:{}", state.config.port),
        "lan_ip": lan_ip(),
        "interval_secs": state.config.interval_secs,
        "ble_interval_secs": state.config.ble_interval_secs,
        "templates_dir": state.config.templates.display().to_string(),
        "usage": usage,
        "weekly_remaining": weekly_remaining(&usage),
        "templates": templates,
        "paused": status.paused,
        "last_push_at": status.last_push_at,
        "last_push_error": status.last_push_error,
        "last_push_ok_at": status.last_push_ok_at,
        "last_sync": status.last_sync,
        "last_error": status.last_error,
        "device_note": status.device_note,
        "device": device,
        "activity": activity,
        "updated": usage.as_ref().and_then(|u| u.get("server_time")).and_then(|v| v.as_i64()),
    }))
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

fn mcp_config(ctx: &AppCtx) -> bridge_mcp::McpConfig {
    bridge_mcp::McpConfig {
        port: ctx.config.port,
        token: ctx.config.token.clone(),
        templates: ctx.config.templates.clone(),
        profiles: ctx.config.profiles.clone(),
        data_root: bridge_core::paths::data_root(),
        seeds: ctx.config.seeds.clone(),
        profile_seed: ctx.config.profile_seed.clone(),
        device_ip: ctx.device_ip.lock().unwrap().clone(),
        device_name: ctx.device_name.lock().unwrap().clone(),
        device_mac: ctx.device_mac.lock().unwrap().clone(),
        bridge_name: ctx.bridge_name.lock().unwrap().clone(),
        bridge_id: ctx.bridge_id.clone(),
        root: ctx.root.clone(),
    }
}

async fn mcp_handler(
    axum::extract::State(ctx): axum::extract::State<Arc<AppCtx>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let host_ok = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| {
            h.starts_with("127.0.0.1") || h.starts_with("localhost") || h.starts_with("[::1]")
        })
        .unwrap_or(false);
    if !host_ok {
        return (axum::http::StatusCode::FORBIDDEN, "host not allowed").into_response();
    }
    let request: Value = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(err) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!("parse error: {err}"),
            )
                .into_response();
        }
    };
    let tool = request
        .pointer("/params/name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    // Device identity/discovery/occupancy tools mutate live app state, so the
    // tray app handles them here (definitions live in bridge-mcp).
    if let Some(name) = tool.as_deref() {
        if matches!(
            name,
            "device_rename"
                | "device_discover"
                | "device_owner"
                | "device_claim"
                | "device_release"
                | "device_sleep"
                | "device_wake"
                | "device_mode"
                | "device_contact_s"
        ) {
            let id = request.get("id").cloned().unwrap_or(Value::Null);
            let args = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let response = match device_tool(&ctx, name, &args).await {
                Ok(text) => json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {"content": [{"type": "text", "text": text}], "isError": false}
                }),
                Err(e) => json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {"content": [{"type": "text", "text": format!("error: {e}")}], "isError": true}
                }),
            };
            return (
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                response.to_string(),
            )
                .into_response();
        }
        // v2 platform tools share the exact application service the UI uses.
        if matches!(
            name,
            "platform_overview"
                | "template_list"
                | "template_get_v2"
                | "template_save_v2"
                | "profile_get_v2"
                | "profile_save_v2"
                | "platform_publish"
                | "platform_publish_cancel"
                | "template_activate"
                | "data_sources_v2"
                | "data_source_save_v2"
                | "data_probe_v2"
                | "power_view_v2"
                | "power_plan"
                | "platform_status_refresh"
                | "platform_push_now"
                | "platform_recovery"
        ) {
            let id = request.get("id").cloned().unwrap_or(Value::Null);
            let args = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let response = match platform::tool(&ctx, name, &args).await {
                Ok(text) => json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {"content": [{"type": "text", "text": text}], "isError": false}
                }),
                Err(e) => json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {"content": [{"type": "text", "text": format!("error: {e}")}], "isError": true}
                }),
            };
            return (
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                response.to_string(),
            )
                .into_response();
        }
    }
    let mcp_cfg = mcp_config(&ctx);
    match bridge_mcp::handle_request(&mcp_cfg, &request).await {
        Some(response) => {
            let is_error = response
                .pointer("/result/isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let text = response
                .pointer("/result/content/0/text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let unreachable = text.contains("unreachable")
                || text.contains("not reachable")
                || text.contains("did not change");
            // v0.14 queued pushes (docs §13.4): a push aimed at a sleeping
            // device is queued instead of failed; the next pull contact keeps
            // the device awake for `pending` and the flush task sends it.
            if is_error
                && matches!(tool.as_deref(), Some("profile_push") | Some("firmware_ota"))
                && (ctx.activity.expects_deep() || unreachable)
            {
                let queued = if tool.as_deref() == Some("profile_push") {
                    let profile_id = request
                        .pointer("/params/arguments/id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let profiles = bridge_core::profile::ProfilesFile::load(&ctx.config.profiles);
                    match profiles {
                        Ok(profiles) => match profiles.get(profile_id) {
                            Some(profile) => {
                                let enabled = profile.enabled_ids();
                                let activate = enabled.first().cloned();
                                ctx.activity.queue_templates(enabled, activate);
                                format!(
                                    "设备在 deep 睡眠；已排队推送 profile '{profile_id}'，下次联系窗口自动发送"
                                )
                            }
                            None => format!("profile not found: {profile_id}"),
                        },
                        Err(e) => format!("load profiles: {e}"),
                    }
                } else {
                    let rom = request
                        .pointer("/params/arguments/rom")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let path = PathBuf::from(rom);
                    let path = if path.is_absolute() {
                        path
                    } else {
                        ctx.root.join(path)
                    };
                    *ctx.pending_ota_rom.lock().unwrap() = Some(path);
                    ctx.activity.queue_ota();
                    "设备在 deep 睡眠；OTA 已排队，下次联系窗口自动执行".to_string()
                };
                let queued_response = json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "result": {"content": [{"type": "text", "text": queued}], "isError": false}
                });
                return (
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    queued_response.to_string(),
                )
                    .into_response();
            }
            let ok = !is_error;
            if ok && matches!(tool.as_deref(), Some("template_save") | Some("profile_save")) {
                ctx.activity.note_activity("template-save");
                if let Some(handle) = ctx.app_handle.get() {
                    let _ = handle.emit("templates-changed", ());
                }
            }
            if ok && tool.as_deref() == Some("profile_push") {
                ctx.activity.note_activity("template-push");
            }
            (
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                response.to_string(),
            )
                .into_response()
        }
        None => axum::http::StatusCode::ACCEPTED.into_response(),
    }
}

async fn mcp_serve(ctx: Arc<AppCtx>) {
    let mut rx = ctx.mcp_tx.subscribe();
    loop {
        let port = *rx.borrow();
        match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                *ctx.mcp_error.lock().unwrap() = None;
                tracing::info!("mcp http listening on http://127.0.0.1:{port}/mcp");
                let app = axum::Router::new()
                    .route("/mcp", axum::routing::post(mcp_handler))
                    .with_state(ctx.clone());
                tokio::select! {
                    _ = axum::serve(listener, app) => {}
                    _ = rx.changed() => {}
                }
            }
            Err(err) => {
                *ctx.mcp_error.lock().unwrap() = Some(format!("bind 127.0.0.1:{port}: {err}"));
                tracing::warn!("mcp bind failed: {err}");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    _ = rx.changed() => {}
                }
            }
        }
    }
}

fn persist_mcp_port(_root: &Path, port: u16) {
    let path = bridge_core::paths::data_root().join("bridge-app.json");
    let mut doc: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    if let Some(object) = doc.as_object_mut() {
        object.insert("mcp_port".to_string(), json!(port));
    }
    if let Ok(text) = serde_json::to_string_pretty(&doc) {
        let _ = std::fs::write(&path, text);
    }
}

// ---------------- device identity / discovery / claim (task-1..4) ----------------

fn claim_error_text(error: ClaimError) -> String {
    match error {
        ClaimError::Unsupported => "device firmware has no /claim (needs 0.13.4+)".to_string(),
        ClaimError::Unauthorized => {
            "device rejected the token (401); click BOOT to open the BLE session so the bridge can refresh it"
                .to_string()
        }
        ClaimError::Occupied(owner) => format!("occupied by {}", owner_line(&owner)),
        ClaimError::Other(text) => text,
    }
}

enum ClaimError {
    Unsupported,
    Unauthorized,
    Occupied(Value),
    Other(String),
}

impl std::fmt::Display for ClaimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaimError::Unsupported => write!(f, "unsupported"),
            ClaimError::Unauthorized => write!(
                f,
                "device token missing/rejected; click BOOT to open the BLE session"
            ),
            ClaimError::Occupied(owner) => write!(f, "occupied by {}", owner_line(owner)),
            ClaimError::Other(text) => write!(f, "{text}"),
        }
    }
}

/// One `POST /claim` with an explicit token (the device's `/claim` is gated by
/// the same device token as `/doUpdate`, not the endpoint token).
async fn post_claim_once(
    ctx: &AppCtx,
    token: &str,
    force: bool,
    release: bool,
) -> Result<(reqwest::StatusCode, String), ClaimError> {
    let ip = ctx.device_ip.lock().unwrap().clone();
    let name = ctx.bridge_name.lock().unwrap().clone();
    let mut url = format!(
        "http://{ip}/claim?id={}&name={}&host={}&port={}&lease=300",
        url_encode(&ctx.bridge_id),
        url_encode(&name),
        url_encode(&lan_ip()),
        ctx.config.port
    );
    if force {
        url.push_str("&force=1");
    }
    if release {
        url.push_str("&release=1");
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ClaimError::Other(e.to_string()))?;
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| ClaimError::Other(format!("device unreachable: {e}")))?;
    let code = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Ok((code, body))
}

fn parse_claim_response(
    ctx: &AppCtx,
    code: reqwest::StatusCode,
    body: String,
) -> Result<Option<Value>, ClaimError> {
    if code == reqwest::StatusCode::NOT_FOUND {
        return Err(ClaimError::Unsupported);
    }
    if code == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ClaimError::Unauthorized);
    }
    let doc: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    if code == reqwest::StatusCode::CONFLICT {
        return Err(ClaimError::Occupied(
            doc.get("owner").cloned().unwrap_or(doc),
        ));
    }
    if !code.is_success() {
        return Err(ClaimError::Other(format!("POST /claim -> HTTP {code} {body}")));
    }
    *ctx.last_claim_at.lock().unwrap() = Some(now_secs());
    ctx.activity.note_contact("claim");
    Ok(doc.get("owner").cloned().filter(|o| !o.is_null()))
}

/// Claim with the cached device token (no BLE scan; the push loop must stay
/// cheap). A missing/expired token surfaces as a user action prompt.
async fn post_claim(
    ctx: &AppCtx,
    force: bool,
    release: bool,
) -> Result<Option<Value>, ClaimError> {
    let cached = bridge_mcp::load_device_token(&mcp_config(ctx));
    let Some(token) = cached else {
        return Err(ClaimError::Unauthorized);
    };
    let (code, body) = post_claim_once(ctx, &token, force, release).await?;
    parse_claim_response(ctx, code, body)
}

/// Explicit user action: on a rejected/missing token, fetch a fresh one over
/// the bonded BLE link (needs a device BLE session: single BOOT click) once.
async fn post_claim_explicit(
    ctx: &AppCtx,
    force: bool,
    release: bool,
) -> Result<Option<Value>, ClaimError> {
    match post_claim(ctx, force, release).await {
        Err(ClaimError::Unauthorized) => {
            tracing::info!("claim token missing/rejected; requesting a fresh one over BLE");
            let cfg = mcp_config(ctx);
            let fresh = bridge_mcp::fetch_device_token(&cfg).await.map_err(|e| {
                ClaimError::Other(format!(
                    "device token unavailable ({e}); click BOOT to open the BLE session"
                ))
            })?;
            let (code, body) = post_claim_once(ctx, &fresh, force, release).await?;
            parse_claim_response(ctx, code, body)
        }
        other => other,
    }
}

enum Occupancy {
    Owned,
    Unsupported,
    Yielded,
    Other(Value),
    Failed(String),
}

/// Auto occupation decision (docs/power-state.md §9 / task-4): free/expired ->
/// claim; self -> renew (60 s throttle); other -> report. Never pushes while
/// another bridge owns the device.
async fn occupancy_gate(ctx: &AppCtx) -> Occupancy {
    if ctx.claim_unsupported.load(Ordering::SeqCst) {
        return Occupancy::Unsupported;
    }
    if ctx.yielded.load(Ordering::SeqCst) {
        return Occupancy::Yielded;
    }
    if let Some(owner) = cached_owner(ctx).filter(owner_valid) {
        if owner_id(&owner) == Some(ctx.bridge_id.as_str()) {
            let renew = ctx
                .last_claim_at
                .lock()
                .unwrap()
                .map(|at| now_secs() - at >= 60)
                .unwrap_or(true);
            if renew {
                match post_claim(ctx, false, false).await {
                    Ok(owner) => set_owner_cache(ctx, owner),
                    Err(ClaimError::Unsupported) => {
                        ctx.claim_unsupported.store(true, Ordering::SeqCst);
                        tracing::info!("/claim unavailable (firmware < 0.13.4); legacy push behavior");
                        return Occupancy::Unsupported;
                    }
                    Err(ClaimError::Occupied(owner)) => {
                        set_owner_cache(ctx, Some(owner.clone()));
                        return Occupancy::Other(owner);
                    }
                    Err(e) => return Occupancy::Failed(e.to_string()),
                }
            }
            return Occupancy::Owned;
        }
        return Occupancy::Other(owner);
    }
    match post_claim(ctx, false, false).await {
        Ok(owner) => {
            set_owner_cache(ctx, owner);
            Occupancy::Owned
        }
        Err(ClaimError::Unsupported) => {
            ctx.claim_unsupported.store(true, Ordering::SeqCst);
            tracing::info!("/claim unavailable (firmware < 0.13.4); legacy push behavior");
            Occupancy::Unsupported
        }
        Err(ClaimError::Occupied(owner)) => {
            set_owner_cache(ctx, Some(owner.clone()));
            Occupancy::Other(owner)
        }
        Err(e) => Occupancy::Failed(e.to_string()),
    }
}

/// Identity snapshot shared by the panel and MCP. All guards are dropped
/// before building the JSON (no nested locks: get_status holds `status`).
fn device_identity_json(ctx: &AppCtx) -> Value {
    let name = ctx.device_name.lock().unwrap().clone();
    let mac = ctx.device_mac.lock().unwrap().clone();
    let ip = ctx.device_ip.lock().unwrap().clone();
    let discover = ctx.discover.lock().unwrap().clone();
    let owner = ctx
        .owner_cache
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| c.owner.clone());
    let note = ctx.status.lock().unwrap().device_note.clone();
    json!({
        "name": name,
        "mac": mac,
        "ip": ip,
        "discover": discover.map(|d| json!({"via": d.via, "at": d.at})),
        "yielded": ctx.yielded.load(Ordering::SeqCst),
        "claim_unsupported": ctx.claim_unsupported.load(Ordering::SeqCst),
        "owner": owner,
        "note": note,
    })
}

async fn discover_arp(ctx: &AppCtx) -> Result<Value, String> {
    let mac = ctx
        .device_mac
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "device MAC unknown; click BOOT and use via=ble once".to_string())?;
    let local = lan_ip();
    let target = mac.clone();
    let found = tokio::task::spawn_blocking(move || {
        arp_scan_for_mac(&local, &target, Duration::from_secs(30))
    })
    .await
    .map_err(|e| e.to_string())?;
    match found {
        Some(ip) => {
            set_device_ip(ctx, &ip, "arp");
            Ok(device_identity_json(ctx))
        }
        None => Err(format!(
            "ARP scan found no host with MAC {mac} on the local /24"
        )),
    }
}

async fn discover_ble(ctx: &AppCtx) -> Result<Value, String> {
    let adapter = Pusher::adapter().await.map_err(|e| e.to_string())?;
    let info = Pusher::read_device_info(&adapter, "CodexStatus-", 30000)
        .await
        .map_err(|e| format!("{e:#}"))?;
    adopt_ble_info(ctx, &info)?;
    let mut out = device_identity_json(ctx);
    // Raw info (includes the firmware mac/ip/http_port) for panel/MCP diagnostics.
    out["ble_info"] = info;
    Ok(out)
}

/// Explicit `device_discover` action: auto (HTTP, then ARP), arp, or ble.
async fn discover_device(ctx: &AppCtx, via: &str) -> Result<Value, String> {
    match via {
        "arp" => discover_arp(ctx).await,
        "ble" => discover_ble(ctx).await,
        "auto" | "" => {
            let ip = ctx.device_ip.lock().unwrap().clone();
            let fetch_ip = ip.clone();
            let status = tokio::task::spawn_blocking(move || {
                bridge_core::device::fetch(&fetch_ip, Duration::from_secs(2))
            })
            .await;
            match status {
                Ok(Ok(status)) => {
                    if let Some(mac) = status.get("mac") {
                        if !learn_mac(ctx, mac, "http") {
                            return Err(format!("device at {ip} reports an unexpected MAC"));
                        }
                    }
                    set_device_ip(ctx, &ip, "http");
                    Ok(device_identity_json(ctx))
                }
                _ => {
                    if ctx.device_mac.lock().unwrap().is_some() {
                        discover_arp(ctx).await
                    } else {
                        Err("device unreachable and MAC unknown; click BOOT, then use via=ble"
                            .to_string())
                    }
                }
            }
        }
        other => Err(format!("unknown via: {other} (use auto|arp|ble)")),
    }
}

/// MCP device tools, handled in the app because they mutate live state.
async fn device_tool(ctx: &AppCtx, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "device_rename" => {
            let raw = args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing argument: name".to_string())?;
            let name = sanitize_display(raw, 24);
            if name.is_empty() {
                return Err("name must not be empty".to_string());
            }
            *ctx.device_name.lock().unwrap() = name.clone();
            save_identity(ctx);
            tracing::info!("device renamed to {name}");
            Ok(format!("device name -> {name}"))
        }
        "device_owner" => Ok(device_identity_json(ctx).to_string()),
        "device_discover" => {
            let via = args
                .get("via")
                .and_then(|v| v.as_str())
                .unwrap_or("auto");
            let result = discover_device(ctx, via).await?;
            Ok(result.to_string())
        }
        "device_claim" => {
            ctx.yielded.store(false, Ordering::SeqCst);
            let force = args
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // Explicit user action: let the device decide (no stale cache).
            let owner = post_claim_explicit(ctx, force, false)
                .await
                .map_err(claim_error_text)?;
            set_owner_cache(ctx, owner.clone());
            ctx.device_ip_dirty.store(true, Ordering::SeqCst);
            ctx.force_push.notify_one();
            Ok(json!({"owner": owner, "yielded": false}).to_string())
        }
        "device_release" => {
            let force = args
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            post_claim_explicit(ctx, force, true)
                .await
                .map_err(claim_error_text)?;
            ctx.yielded.store(true, Ordering::SeqCst);
            set_owner_cache(ctx, None);
            Ok("device released; auto-claim paused until device_claim".to_string())
        }
        // Debug helper: shortens the light -> deep loop for battery/deep tests.
        // `note_deep` makes the next envelope carry mode=deep (and next_contact
        // 60); `device_ip_dirty` + notify make the push loop send it even though
        // it now "expects deep". The firmware applies its 60 s grace and sleeps.
        "device_sleep" => {
            // Persist deep as well: the pull response follows the push within a
            // minute and would otherwise bring the device back to light while
            // the 10 min quiet window is not yet satisfied.
            ctx.activity.request_deep();
            ctx.activity.note_deep(60);
            ctx.device_ip_dirty.store(true, Ordering::SeqCst);
            ctx.force_push.notify_one();
            Ok("deep hint queued + pull responses forced deep; device sleeps after the 60 s grace (clear with device_mode auto)"
                .to_string())
        }
        // Debug: make the next pull answer light so the device wakes up and
        // stays online (readable /status.json, /log). The forced push carries
        // mode=light to cancel a pending deep hint when the device is awake.
        "device_wake" => {
            ctx.activity.request_light();
            ctx.device_ip_dirty.store(true, Ordering::SeqCst);
            ctx.force_push.notify_one();
            Ok("light requested: next pull answers light; push carries mode=light when reachable"
                .to_string())
        }
        // Debug: persistent mode override for pull responses/pushes so the
        // deep/light loop does not depend on the 10 min quiet hysteresis.
        "device_mode" => {
            let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("auto");
            let code = match mode {
                "auto" => 0u8,
                "deep" => 1,
                "light" => 2,
                other => return Err(format!("mode must be auto|deep|light (got {other})")),
            };
            ctx.activity.set_debug_mode(code);
            Ok(format!("debug mode = {mode} (pull responses and push envelopes)"))
        }
        // Debug: override the pull cadence (0 = auto) to speed up deep cycles.
        "device_contact_s" => {
            let s = args.get("s").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
            if s != 0 && !(30..=3600).contains(&s) {
                return Err("s must be 0 (auto) or 30..=3600 seconds".to_string());
            }
            ctx.activity.set_debug_contact_s(s);
            if s == 0 {
                Ok("debug pull cadence cleared (auto)".to_string())
            } else {
                Ok(format!("debug pull cadence = {s}s"))
            }
        }
        other => Err(format!("unknown device tool: {other}")),
    }
}

#[tauri::command]
async fn rename_device(state: State<'_, Arc<AppCtx>>, name: String) -> Result<Value, String> {
    device_tool(&state, "device_rename", &json!({"name": name})).await?;
    Ok(device_identity_json(&state))
}

#[tauri::command]
async fn device_discover(
    state: State<'_, Arc<AppCtx>>,
    via: Option<String>,
) -> Result<Value, String> {
    discover_device(&state, via.as_deref().unwrap_or("auto")).await
}

#[tauri::command]
async fn device_owner(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    Ok(device_identity_json(&state))
}

#[tauri::command]
async fn claim_device(state: State<'_, Arc<AppCtx>>, force: Option<bool>) -> Result<Value, String> {
    let text = device_tool(
        &state,
        "device_claim",
        &json!({"force": force.unwrap_or(false)}),
    )
    .await?;
    Ok(json!({"result": text, "device": device_identity_json(&state)}))
}

#[tauri::command]
async fn release_device(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    let text = device_tool(&state, "device_release", &json!({})).await?;
    Ok(json!({"result": text, "device": device_identity_json(&state)}))
}

// ---------------------------------------------------------------------------
// v2 platform commands: the four UI pages call exactly the same service the MCP
// tools use (no second business logic, no save-implies-publish).
// ---------------------------------------------------------------------------

#[tauri::command]
async fn platform_overview(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    Ok(platform::overview(&state))
}

#[tauri::command]
async fn platform_templates(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    Ok(platform::templates(&state))
}

#[tauri::command]
async fn platform_devices(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    Ok(platform::device_rows(&state))
}

#[tauri::command]
async fn platform_template_get(
    state: State<'_, Arc<AppCtx>>,
    id: String,
    render_target: Option<String>,
) -> Result<Value, String> {
    platform::template_get(&state, &id, render_target.as_deref())
}

#[tauri::command]
async fn platform_template_validate(
    state: State<'_, Arc<AppCtx>>,
    json: Value,
) -> Result<Value, String> {
    let _ = &state;
    Ok(platform::template_validate(&json))
}

#[tauri::command]
async fn platform_template_preview(
    state: State<'_, Arc<AppCtx>>,
    id: Option<String>,
    json: Option<Value>,
    usage: Option<String>,
) -> Result<Value, String> {
    platform::template_preview(&state, id.as_deref(), json.as_ref(), usage.as_deref())
}

#[tauri::command]
async fn platform_template_save(
    state: State<'_, Arc<AppCtx>>,
    id: String,
    render_target: String,
    json: Value,
) -> Result<Value, String> {
    let result = platform::template_save(&state, &id, &render_target, &json)?;
    let _ = state.app_handle.get().map(|app| {
        use tauri::Emitter;
        let _ = app.emit("templates-changed", ());
    });
    Ok(result)
}

#[tauri::command]
async fn platform_profile_get(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    Ok(platform::profile_get(&state))
}

#[tauri::command]
async fn platform_profile_save(
    state: State<'_, Arc<AppCtx>>,
    profile: Value,
) -> Result<Value, String> {
    let parsed: bridge_core::platform::model::Profile =
        serde_json::from_value(profile).map_err(|e| e.to_string())?;
    platform::profile_save(&state, parsed)
}

#[tauri::command]
async fn platform_publish(
    state: State<'_, Arc<AppCtx>>,
    mac: Option<String>,
) -> Result<Value, String> {
    let mac = match mac {
        Some(m) if !m.is_empty() => m,
        _ => state
            .device_mac
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "device MAC not learned yet".to_string())?,
    };
    platform::publish(&state, &mac).await
}

#[tauri::command]
async fn platform_publish_cancel(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    let mac = state.device_mac.lock().unwrap().clone().unwrap_or_default();
    Ok(platform::job_cancel(&state, &mac))
}

#[tauri::command]
async fn platform_activate(
    state: State<'_, Arc<AppCtx>>,
    id: String,
) -> Result<Value, String> {
    let mac = state
        .device_mac
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "device MAC not learned yet".to_string())?;
    platform::activate(&state, &mac, &id).await
}

#[tauri::command]
async fn platform_data_sources(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    Ok(platform::data_sources(&state))
}

#[tauri::command]
async fn platform_data_source_save(
    state: State<'_, Arc<AppCtx>>,
    source: Value,
) -> Result<Value, String> {
    platform::data_source_save(&state, source)
}

#[tauri::command]
async fn platform_data_probe(
    state: State<'_, Arc<AppCtx>>,
    source_id: String,
) -> Result<Value, String> {
    platform::data_probe(&state, &source_id)
}

#[tauri::command]
async fn platform_power(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    Ok(platform::power_view(&state))
}

#[tauri::command]
async fn platform_plan(
    state: State<'_, Arc<AppCtx>>,
    mode: String,
) -> Result<Value, String> {
    let mac = state
        .device_mac
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "device MAC not learned yet".to_string())?;
    match mode.as_str() {
        "light" => Ok(platform::send_plan(&state, &mac, "manual", 0).await),
        "sleep" => {
            let text = platform::tool(&state, "power_plan", &json!({"mode": "sleep"})).await?;
            Ok(serde_json::from_str(&text).unwrap_or_else(|_| json!({"result": text})))
        }
        other => Err(format!("mode must be light|sleep (got {other})")),
    }
}

#[tauri::command]
async fn platform_status_refresh(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    let mac = state.device_mac.lock().unwrap().clone().unwrap_or_default();
    Ok(platform::refresh_status(&state, &mac).await)
}

#[tauri::command]
async fn get_device_status(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    let cached = state.device_cache.lock().unwrap().clone();
    match cached {
        Some(cached) => {
            let map: serde_json::Map<String, Value> = cached
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            Ok(json!({
                "online": cached.online,
                "ip": cached.ip,
                "fetched_at": cached.fetched_at,
                "fields": map,
                "owner": cached.owner,
                "device": device_identity_json(&state),
            }))
        }
        None => Ok(json!({
            "online": false,
            "ip": state.device_ip.lock().unwrap().clone(),
            "pending": true,
            "fields": {},
            "owner": Value::Null,
            "device": device_identity_json(&state),
        })),
    }
}

/// Read `GET /pmstats` from the device. Cached for 10 s because each read
/// briefly wakes the device out of light sleep and would skew the counters.
#[tauri::command]
async fn get_pmstats(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    const TTL_SECS: i64 = 10;
    {
        let cache = state.pmstats_cache.lock().unwrap();
        if let Some(cached) = cache.as_ref() {
            if now_secs() - cached.fetched_at < TTL_SECS {
                return Ok(json!({
                    "online": cached.online,
                    "ip": cached.ip,
                    "fetched_at": cached.fetched_at,
                    "text": cached.text,
                }));
            }
        }
    }
    let ip = state.device_ip.lock().unwrap().clone();
    let fetch_ip = ip.clone();
    let result = tokio::task::spawn_blocking(move || {
        bridge_core::device::fetch_pmstats(&fetch_ip, Duration::from_secs(5))
    })
    .await;
    let (online, text) = match result {
        Ok(Ok(text)) => (true, text),
        Ok(Err(e)) => (false, e.to_string()),
        Err(e) => (false, e.to_string()),
    };
    *state.pmstats_cache.lock().unwrap() = Some(CachedPmStats {
        fetched_at: now_secs(),
        online,
        ip: ip.clone(),
        text: text.clone(),
    });
    Ok(json!({
        "online": online,
        "ip": ip,
        "fetched_at": now_secs(),
        "text": text,
    }))
}

#[tauri::command]
async fn get_mcp_info(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    let port = *state.mcp_port.lock().unwrap();
    let error = state.mcp_error.lock().unwrap().clone();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let tools = "bridge_status / template_get / template_validate / template_render / template_save / profiles_list / profile_save / profile_push / firmware_ota / pm_stats";

    let generic_prompt = format!(
        "本机已启动 Codex Status 的 MCP 服务（Streamable HTTP）：{url}\n\
         工具前缀 codex_status_（{tools}），用于读取、校验、渲染预览、保存模板并 HTTP 推送到墨水屏。\n\
         请把它加入你所在客户端的 MCP 配置（remote/http 类型）；配置写入后需要重开会话或客户端才能加载。\
         若你无法自行修改配置，请告诉我该在哪一步粘贴这一行。"
    );
    let opencode_snippet = serde_json::to_string_pretty(&json!({
        "mcp": {"codex_status": {"type": "remote", "url": url, "enabled": true}}
    }))
    .unwrap_or_default();
    let claude_snippet = serde_json::to_string_pretty(&json!({
        "mcpServers": {"codex-status": {"type": "http", "url": url}}
    }))
    .unwrap_or_default();
    let cursor_snippet = serde_json::to_string_pretty(&json!({
        "mcpServers": {"codex-status": {"url": url}}
    }))
    .unwrap_or_default();
    let vscode_snippet = serde_json::to_string_pretty(&json!({
        "servers": {"codex-status": {"type": "http", "url": url}}
    }))
    .unwrap_or_default();
    let codex_command = format!("codex mcp add codex-status --url {url}");
    let claude_command = format!("claude mcp add --transport http codex-status {url}");

    let prompts = json!({
        "generic": generic_prompt,
        "opencode": format!(
            "请把下面的 MCP 配置合并进项目根目录的 opencode.jsonc（没有就新建，保留 $schema: https://opencode.ai/config.json），保存后重开会话即可使用 codex_status_* 工具：\n\n{opencode_snippet}"
        ),
        "codex": format!("在终端运行：\n{codex_command}\n\n或在 ~/.codex/config.toml 中加入：\n[mcp_servers.codex-status]\nurl = \"{url}\""),
        "claude": format!(
            "Claude Code（CLI）：在终端运行\n{claude_command}\n\nClaude Desktop：把下面的配置合并进 claude_desktop_config.json 后重启客户端：\n\n{claude_snippet}"
        ),
        "cursor": format!(
            "请把下面的配置合并进 ~/.cursor/mcp.json（或项目内 .cursor/mcp.json），保存后在 Cursor 设置里确认该 MCP 已启用：\n\n{cursor_snippet}"
        ),
        "vscode": format!(
            "请把下面的配置合并进项目内的 .vscode/mcp.json，然后在 VS Code 中启用该 server：\n\n{vscode_snippet}"
        ),
    });

    Ok(json!({
        "port": port,
        "url": url,
        "error": error,
        "prompts": prompts,
    }))
}

#[tauri::command]
async fn set_mcp_port(state: State<'_, Arc<AppCtx>>, port: u16) -> Result<Value, String> {
    if port < 1024 {
        return Err("端口需 >= 1024".to_string());
    }
    *state.mcp_port.lock().unwrap() = port;
    let _ = state.mcp_tx.send(port);
    persist_mcp_port(&state.root, port);
    Ok(json!({"port": port}))
}

/// MCP tools and manual edits change template files behind the app's back, so
/// re-scan the directory before answering preview/status queries.
async fn refresh_library(state: &State<'_, Arc<AppCtx>>) {
    if let Ok(fresh) = Library::load(&state.config.templates) {
        *state.library.write().await = fresh;
    }
}

#[tauri::command]
async fn preview_template(
    state: State<'_, Arc<AppCtx>>,
    id: Option<String>,
) -> Result<Value, String> {
    refresh_library(&state).await;
    let (template, width, height) = {
        let library = state.library.read().await;
        let target = id
            .filter(|value| !value.is_empty())
            .or_else(|| library.ids().into_iter().next())
            .ok_or_else(|| "no templates loaded".to_string())?;
        let entry = library
            .get(&target)
            .ok_or_else(|| format!("template not found: {target}"))?;
        let text = String::from_utf8(entry.bytes.clone()).map_err(|e| e.to_string())?;
        (text, bridge_render::WIDTH, bridge_render::HEIGHT)
    };
    let usage = state
        .envelope
        .read()
        .await
        .clone()
        .unwrap_or_else(|| json!({}));
    let ip = lan_ip();
    let sync = local_hhmm();
    let env = bridge_render::Env {
        channel: "WIFI",
        ip: &ip,
        sync_hhmm: &sync,
        battery: bridge_render::DEFAULT_BATTERY,
        ..Default::default()
    };
    let bits = bridge_render::render_bits(&template, &usage.to_string(), &env)
        .map_err(|e| e.to_string())?;
    Ok(json!({"width": width, "height": height, "bits": bits}))
}

#[tauri::command]
async fn force_sync(state: State<'_, Arc<AppCtx>>) -> Result<(), String> {
    request_sync(&state);
    Ok(())
}

/// Explicit user action: push the envelope over HTTP immediately and run one
/// BLE handshake (endpoint/token refresh).
fn request_sync(ctx: &AppCtx) {
    ctx.activity.note_activity("sync");
    ctx.device_ip_dirty.store(true, Ordering::SeqCst);
    ctx.force_push.notify_one();
    ctx.udp_ble.store(true, Ordering::SeqCst);
    ctx.force_ble.notify_one();
}

fn profiles_path(ctx: &AppCtx) -> PathBuf {
    ctx.config.profiles.clone()
}

/// Seed the runtime directory from the repo copies. Existing files are never
/// overwritten: runtime edits always win.
fn ensure_runtime(config: &Config) -> std::io::Result<()> {
    std::fs::create_dir_all(&config.templates)?;
    if let Ok(entries) = std::fs::read_dir(&config.seeds) {
        for entry in entries.flatten() {
            let source = entry.path();
            if source.extension().map(|e| e != "json").unwrap_or(true) {
                continue;
            }
            let Some(name) = source.file_name() else { continue };
            let target = config.templates.join(name);
            if !target.exists() {
                let _ = std::fs::copy(&source, &target);
            }
        }
    }
    if !config.profiles.exists() && config.profile_seed.exists() {
        if let Some(parent) = config.profiles.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::copy(&config.profile_seed, &config.profiles);
    }
    Ok(())
}

#[tauri::command]
async fn get_profiles(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    refresh_library(&state).await;
    let templates: Vec<Value> = {
        let library = state.library.read().await;
        library
            .entries
            .values()
            .map(|e| json!({"id": e.id, "hash": e.hash}))
            .collect()
    };
    let profiles = bridge_core::profile::ProfilesFile::load(&profiles_path(&state))
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
    Ok(json!({"profiles": list, "templates": templates}))
}

#[tauri::command]
async fn save_profile(
    state: State<'_, Arc<AppCtx>>,
    id: String,
    name: Option<String>,
    templates: Vec<bridge_core::profile::ProfileEntry>,
) -> Result<(), String> {
    let known: Vec<String> = state.library.read().await.ids();
    let path = profiles_path(&state);
    let mut profiles = bridge_core::profile::ProfilesFile::load(&path).map_err(|e| e.to_string())?;
    let profile = bridge_core::profile::Profile {
        id,
        name: name.unwrap_or_default(),
        templates,
    };
    profiles
        .upsert(profile, &known)
        .map_err(|e| e.to_string())?;
    profiles.save(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_profile(state: State<'_, Arc<AppCtx>>, id: String) -> Result<(), String> {
    let path = profiles_path(&state);
    let mut profiles = bridge_core::profile::ProfilesFile::load(&path).map_err(|e| e.to_string())?;
    if !profiles.remove(&id) {
        return Err(format!("profile not found: {id}"));
    }
    profiles.save(&path).map_err(|e| e.to_string())
}

/// Explicit user action (docs/power-state.md §5/§9): templates are pushed over
/// HTTP (`POST /template`), not BLE. Returns the transfer summary for the UI.
#[tauri::command]
async fn push_profile(state: State<'_, Arc<AppCtx>>, id: String) -> Result<String, String> {
    let profiles = bridge_core::profile::ProfilesFile::load(&profiles_path(&state))
        .map_err(|e| e.to_string())?;
    let profile = profiles
        .get(&id)
        .ok_or_else(|| format!("profile not found: {id}"))?
        .clone();
    let enabled = profile.enabled_ids();
    if enabled.is_empty() {
        return Err("没有启用的模板，无法推送".to_string());
    }
    let activate = enabled.first().cloned();
    let cfg = mcp_config(&state);
    match bridge_mcp::push_templates_http(&cfg, &enabled, activate.as_deref()).await {
        Ok(summary) => {
            state.activity.note_activity("template-push");
            let mut status = state.status.lock().unwrap();
            status.last_push_at = Some(now_secs());
            status.last_push_error = None;
            Ok(summary)
        }
        Err(e) => {
            state.status.lock().unwrap().last_push_error = Some(e.clone());
            // Device asleep: queue the profile for the next pull contact.
            if state.activity.expects_deep() {
                state.activity.queue_templates(enabled, activate);
                return Ok(format!(
                    "设备在 deep 睡眠；已排队推送 profile '{id}'，下次联系窗口自动发送"
                ));
            }
            Err(e)
        }
    }
}

#[tauri::command]
async fn set_paused(state: State<'_, Arc<AppCtx>>, paused: bool) -> Result<(), String> {
    state.status.lock().unwrap().paused = paused;
    if !paused {
        state.force_ble.notify_one();
    }
    Ok(())
}

#[tauri::command]
async fn reload_templates(state: State<'_, Arc<AppCtx>>) -> Result<usize, String> {
    let library = Library::load(&state.config.templates).map_err(|e| e.to_string())?;
    let count = library.entries.len();
    *state.library.write().await = library;
    Ok(count)
}

/// UDP announce listener (docs/power-state.md §9): the device broadcasts
/// `{magic, mac, ip, port, proto, ble, fw}` to 255.255.255.255:8767 on IP
/// change, BLE session start and every ~5 min. Only a known MAC may move the
/// endpoint; before the first /status.json fetch only the configured address
/// is accepted (and its MAC learned).
async fn udp_listen(ctx: Arc<AppCtx>) {
    let socket = match tokio::net::UdpSocket::bind(("0.0.0.0", 8767)).await {
        Ok(socket) => socket,
        Err(e) => {
            tracing::warn!("udp announce bind :8767 failed: {e}");
            let mut status = ctx.status.lock().unwrap();
            status.last_error = Some(format!("udp: {e}"));
            status.last_error_at = Some(now_secs());
            return;
        }
    };
    let _ = socket.set_broadcast(true);
    tracing::info!("udp announce listener on :8767");
    let mut buf = [0u8; 1024];
    loop {
        let Ok((n, from)) = socket.recv_from(&mut buf).await else { continue };
        let Ok(doc) = serde_json::from_slice::<Value>(&buf[..n]) else { continue };
        if doc.get("magic").and_then(|v| v.as_str()) != Some("codex-status") {
            continue;
        }
        let mac = doc.get("mac").and_then(|v| v.as_str()).unwrap_or("");
        let ip = doc.get("ip").and_then(|v| v.as_str()).unwrap_or("");
        if mac.is_empty() || ip.is_empty() || from.ip().to_string() != ip {
            continue;
        }
        let known = ctx.device_mac.lock().unwrap().clone();
        if known
            .as_deref()
            .map(|existing| !existing.eq_ignore_ascii_case(mac))
            .unwrap_or(false)
        {
            tracing::debug!("udp announce ignored (mac mismatch)");
            continue;
        }
        if known.is_none() && ctx.device_ip.lock().unwrap().as_str() != ip {
            // First identification must match the configured address (the MAC
            // then becomes the identity key for all later announcements).
            tracing::debug!("udp announce ignored (unknown mac from {ip})");
            continue;
        }
        if !learn_mac(&ctx, mac, "udp") {
            continue;
        }
        set_device_ip(&ctx, ip, "udp");
        ctx.activity.note_contact("udp");
        if doc.get("ble").and_then(|v| v.as_i64()).unwrap_or(0) == 1 {
            tracing::info!("device requested a BLE handshake via UDP");
            ctx.udp_ble.store(true, Ordering::SeqCst);
            ctx.force_ble.notify_one();
        }
    }
}

/// ARP fallback after repeated HTTP failures: rescan the local /24 for the
/// known MAC and move the address attribute when found (task-2).
fn maybe_arp_fallback(ctx: &Arc<AppCtx>) {
    if ctx.arp_running.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(mac) = ctx.device_mac.lock().unwrap().clone() else {
        ctx.arp_running.store(false, Ordering::SeqCst);
        return;
    };
    let recently = {
        let discover = ctx.discover.lock().unwrap();
        discover
            .as_ref()
            .map(|d| d.via == "arp" && now_secs() - d.at < 60)
            .unwrap_or(false)
    };
    if recently {
        ctx.arp_running.store(false, Ordering::SeqCst);
        return;
    }
    let ctx = ctx.clone();
    tracing::info!("device HTTP unreachable; ARP fallback scan for {mac}");
    tokio::spawn(async move {
        let local = lan_ip();
        let found = tokio::task::spawn_blocking(move || {
            arp_scan_for_mac(&local, &mac, Duration::from_secs(20))
        })
        .await
        .ok()
        .flatten();
        if let Some(ip) = found {
            set_device_ip(&ctx, &ip, "arp");
            ctx.device_fail_streak.store(0, Ordering::SeqCst);
        }
        ctx.arp_running.store(false, Ordering::SeqCst);
    });
}

/// Refresh the cached device status every 10 s; the panel reads the cache.
/// Also learns the MAC identity, keeps the owner cache fresh and triggers the
/// ARP fallback after two consecutive failures.
async fn device_cache_loop(ctx: Arc<AppCtx>) {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        if ctx.status.lock().unwrap().paused {
            continue;
        }
        let ip = ctx.device_ip.lock().unwrap().clone();
        let fetch_ip = ip.clone();
        let result = tokio::task::spawn_blocking(move || {
            bridge_core::device::fetch(&fetch_ip, Duration::from_secs(3))
        })
        .await;
        let (mut online, fields, mac, owner, raw) = match result {
            Ok(Ok(status)) => {
                let mac = status.get("mac").map(str::to_string);
                let owner = status
                    .raw
                    .as_ref()
                    .and_then(|raw| raw.get("owner"))
                    .cloned()
                    .filter(|value| !value.is_null());
                (true, status.fields, mac, owner, status.raw)
            }
            Ok(Err(e)) => {
                tracing::debug!("device status fetch {ip}: {e}");
                (false, Vec::new(), None, None, None)
            }
            Err(e) => {
                tracing::debug!("device status task: {e}");
                (false, Vec::new(), None, None, None)
            }
        };
        if let Some(mac) = mac.as_deref().filter(|m| !m.is_empty()) {
            if !learn_mac(&ctx, mac, "http") {
                tracing::warn!("device status at {ip} reports a different MAC; treating as offline");
                online = false;
            }
        }
        if online {
            if let Some(raw) = raw.as_ref() {
                let (caps, legacy) = platform::caps_from_status(raw);
                platform::ensure_device(&ctx, caps, legacy);
                platform::note_status_json(&ctx, raw);
            }
        }
        if online {
            ctx.device_fail_streak.store(0, Ordering::SeqCst);
            ctx.activity.note_contact("http");
            set_owner_cache(&ctx, owner.clone());
            // Surface another bridge's occupancy immediately (the push itself
            // may not run for minutes); the push gate clears the note again.
            if let Some(o) = owner.as_ref().filter(|o| owner_valid(o)) {
                if owner_id(o) != Some(ctx.bridge_id.as_str()) {
                    set_device_note(&ctx, Some(format!("被 {} 占用", owner_line(o))));
                }
            }
        } else if ctx.device_fail_streak.fetch_add(1, Ordering::SeqCst) + 1 >= 2 {
            maybe_arp_fallback(&ctx);
        }
        let mut cache = ctx.device_cache.lock().unwrap();
        let fields = if online {
            fields
        } else {
            cache.as_ref().map(|c| c.fields.clone()).unwrap_or_default()
        };
        let owner = if online {
            owner
        } else {
            cache.as_ref().and_then(|c| c.owner.clone())
        };
        *cache = Some(CachedDevice {
            fetched_at: now_secs(),
            online,
            ip,
            fields,
            owner,
        });
    }
}

async fn run_services(ctx: Arc<AppCtx>) {
    // A missing codex.exe must not block HTTP/MCP/BLE: the poller re-discovers
    // the CLI itself (Codex auto-upgrades move the binary).
    let exe = match locate_codex(ctx.config.codex_path.as_deref()) {
        Ok(exe) => {
            tracing::info!("codex cli: {}", exe.display());
            Some(exe)
        }
        Err(e) => {
            tracing::error!("codex cli not found: {e}");
            let mut status = ctx.status.lock().unwrap();
            status.last_error = Some(format!("codex cli: {e}"));
            status.last_error_at = Some(now_secs());
            None
        }
    };

    let addr = format!("0.0.0.0:{}", ctx.config.port).parse().expect("addr");
    let http_state = AppState {
        token: Arc::new(ctx.config.token.clone()),
        envelope: ctx.envelope.clone(),
        library: ctx.library.clone(),
        activity: ctx.activity.clone(),
    };
    {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if let Err(e) = serve(addr, http_state).await {
                tracing::error!("http server: {e}");
                let mut status = ctx.status.lock().unwrap();
                status.last_error = Some(format!("http: {e}"));
                status.last_error_at = Some(now_secs());
            }
        });
    }

    let poller = PollerConfig {
        exe,
        codex_override: ctx.config.codex_path.clone(),
        host_id: ctx.bridge_id.clone(),
        bridge_host: lan_ip(),
        bridge_port: ctx.config.port,
        interval_secs: ctx.config.interval_secs,
        templates: ctx.library.clone(),
        active_hold_seconds: ctx.config.active_hold_seconds,
        activity: ctx.activity.clone(),
    };
    tokio::spawn(run_poller(poller, ctx.envelope.clone()));

    let udp_ctx = ctx.clone();
    tokio::spawn(async move { udp_listen(udp_ctx).await });
    let cache_ctx = ctx.clone();
    tokio::spawn(async move { device_cache_loop(cache_ctx).await });

    // v2 platform loop: feed the Codex envelope into the DataSource and run one
    // coordinator delivery per device on a bounded cadence. Never used to renew
    // a light window (only data/publish/render actions trigger work).
    {
        let ctx = ctx.clone();
        // Automatic delivery can be disabled for a debugging session
        // (explicit publish/activate still delivers). Automatic attempts are
        // throttled to one per minute so a device that hangs on install is not
        // hammered every cycle.
        let auto_deliver = std::env::var("CODEX_STATUS_PLATFORM_AUTODELIVER")
            .map(|v| v != "0")
            .unwrap_or(true);
        tokio::spawn(async move {
            let mut last_stamp = 0u64;
            let mut tick = 0u64;
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                tick += 1;
                if ctx.status.lock().unwrap().paused {
                    continue;
                }
                {
                    let envelope = ctx.envelope.read().await.clone();
                    if let Some(env) = envelope {
                        let stamp = env
                            .get("server_time")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        if stamp != last_stamp {
                            last_stamp = stamp;
                            platform::note_envelope(&ctx, &env);
                        }
                    }
                }
                if !platform::is_v2_device(&ctx) {
                    continue;
                }
                // Pending v2 work must keep the *pull* response light, otherwise
                // a timer-woken device bounces straight back to deep before the
                // coordinator can deliver. Reads/status never extend the light
                // lease; this only mirrors the legacy pending-work rule.
                let mac = ctx.device_mac.lock().unwrap().clone().unwrap_or_default();
                if !mac.is_empty() {
                    if let Some(summary) = platform::service(&ctx).coordinator_summary(&mac) {
                        let pending = summary["push_dirty"].as_bool().unwrap_or(false)
                            || summary["in_flight"].is_object()
                            || summary["pending_activate"].is_string()
                            || summary["job"]
                                .as_object()
                                .map(|j| {
                                    !matches!(
                                        j.get("state").and_then(|s| s.as_str()),
                                        Some("succeeded") | Some("failed") | Some("cancelled")
                                    )
                                })
                                .unwrap_or(false);
                        if pending {
                            ctx.activity.note_activity("platform");
                        }
                    }
                }
                let online = ctx
                    .device_cache
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|c| c.online)
                    .unwrap_or(false);
                if !online {
                    // Deep/unreachable: keep the intent; the next rendezvous
                    // (BOOT or a pull contact) picks it up. No radio wake.
                    continue;
                }
                platform::cycle(&ctx, tick % 3 == 0, auto_deliver && tick % 6 == 0).await;
            }
        });
    }

    // Usage push (docs/history/sleep-plan-v4.md §4.3/§4.6): POST the envelope to the device on
    // fingerprint change or 5 min heartbeat. Connection errors just mean the
    // device is in DEEP; the BLE/window path covers that.
    {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let client = match reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("usage push client: {e}");
                    return;
                }
            };
            let mut last_fp: u64 = 0;
            let mut last_ok: u64 = 0;
            let mut last_gate: u64 = 0;
            let mut last_contact_gen = ctx.activity.contact_generation();
            let mut last_mode: Option<&'static str> = None;
            let mut deep_skip_logged = false;
            loop {
                // Check the fingerprint every 3 s so an envelope change reaches
                // the device well inside the T9 ≤5 s budget. A UDP endpoint
                // update wakes the loop and skips the fingerprint gate.
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                    _ = ctx.force_push.notified() => {}
                }
                let forced = ctx.device_ip_dirty.swap(false, Ordering::SeqCst);
                let usage = ctx.envelope.try_read().ok().and_then(|g| g.clone());
                let Some(usage) = usage else { continue };
                let fp = usage_fingerprint(&usage);
                let now = now_secs() as u64;
                // v0.14: a pull or announce means the device is awake again;
                // treat it like a forced push so the fresh data goes out at once.
                let contact_gen = ctx.activity.contact_generation();
                let contact_new = contact_gen != last_contact_gen;
                last_contact_gen = contact_gen;
                // Bridge-controlled sleep: the desired mode is the same
                // quiet/dwell decision as the pull response. A light->deep
                // transition pushes immediately instead of waiting for the
                // 5-minute heartbeat, so the device sleeps on the bridge's
                // schedule (design §13.4).
                let mode_now = ctx.activity.mode_str();
                let mode_changed = last_mode != Some(mode_now);
                // Expected deep sleep: the device pulls on its own schedule and
                // cannot receive pushes; do not count failures or alert.
                if ctx.activity.expects_deep() && !forced && !contact_new {
                    if !deep_skip_logged {
                        tracing::info!("device expected deep; pushes paused");
                        deep_skip_logged = true;
                    }
                    continue;
                }
                deep_skip_logged = false;
                // v2 devices use the coordinator data path (`/v2/data`), not the
                // legacy Wi-Fi push: one device, one data channel.
                if platform::is_v2_device(&ctx) {
                    continue;
                }
                // While occupied/yielded, re-check every 15 s so a lease expiry
                // or another bridge's release is picked up before the 5 min
                // heartbeat (the gate itself only spends HTTP when claiming).
                let note_set = ctx.status.lock().unwrap().device_note.is_some();
                let recheck = note_set && now.saturating_sub(last_gate) >= 15;
                let self_owned = cached_owner(&ctx)
                    .filter(owner_valid)
                    .map(|o| owner_id(&o) == Some(ctx.bridge_id.as_str()))
                    .unwrap_or(false);
                let renew_due = self_owned
                    && ctx
                        .last_claim_at
                        .lock()
                        .unwrap()
                        .map(|at| now_secs() - at >= 60)
                        .unwrap_or(true);
                let push_needed = forced
                    || contact_new
                    || fp != last_fp
                    || now.saturating_sub(last_ok) >= 300
                    || mode_changed;
                if !push_needed && !recheck && !renew_due {
                    continue;
                }
                last_gate = now;
                // Auto occupation decision before every write (task-4): free ->
                // claim, self -> renew (60 s), other -> do not push at all.
                match occupancy_gate(&ctx).await {
                    Occupancy::Owned | Occupancy::Unsupported => set_device_note(&ctx, None),
                    Occupancy::Yielded => {
                        set_device_note(&ctx, Some("已释放（本地让步），等待手动恢复".to_string()));
                        continue;
                    }
                    Occupancy::Other(owner) => {
                        set_device_note(&ctx, Some(format!("被 {} 占用", owner_line(&owner))));
                        continue;
                    }
                    Occupancy::Failed(e) => {
                        set_device_note(&ctx, Some(format!("占用检查失败：{e}")));
                        continue;
                    }
                }
                if !push_needed {
                    // Renewal/recheck only: no data changed, keep the heartbeat.
                    continue;
                }
                last_mode = Some(mode_now);
                // Push envelope carries the mode decision (docs §13.4) so the
                // device follows the bridge's quiet/dwell sleep schedule.
                // `server_time`/`tz_offset_min` are stamped fresh here (the
                // cached envelope's timestamp can be minutes old) so the device
                // clock/timezone follow this PC on every push.
                let mut push_usage = usage.clone();
                if let Some(obj) = push_usage.as_object_mut() {
                    obj.insert("server_time".to_string(), json!(now));
                    obj.insert(
                        "tz_offset_min".to_string(),
                        json!(bridge_core::local_offset_minutes()),
                    );
                    obj.insert(
                        "mode".to_string(),
                        Value::String(mode_now.to_string()),
                    );
                    obj.insert(
                        "next_contact_s".to_string(),
                        json!(ctx.activity.next_contact_s()),
                    );
                    obj.insert("usage_rev".to_string(), json!(ctx.activity.usage_rev()));
                }
                let text = push_usage.to_string();
                let device_ip = ctx.device_ip.lock().unwrap().clone();
                let url = format!("http://{device_ip}/usage");
                match client
                    .post(&url)
                    .bearer_auth(&ctx.config.token)
                    .header("Content-Type", "application/json")
                    .body(text)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        last_fp = fp;
                        last_ok = now;
                        ctx.activity.note_contact("push");
                        tracing::info!("usage push -> {} ({})", resp.status(), url);
                        let mut status = ctx.status.lock().unwrap();
                        status.last_push_ok_at = Some(now as i64);
                        status.push_fail_streak = 0;
                        status.last_sync = Some(now as i64);
                        if status.last_error.as_deref().is_some_and(|e| e.starts_with("ble:")) {
                            status.last_error = None;
                            status.last_error_at = None;
                        }
                    }
                    Ok(resp) if resp.status() == reqwest::StatusCode::CONFLICT => {
                        // Lost a race with another bridge: show the new owner,
                        // count it as a protocol outcome, not a push failure.
                        let body = resp.text().await.unwrap_or_default();
                        let doc: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                        let owner = doc.get("owner").cloned().filter(|o| !o.is_null());
                        let text = owner
                            .as_ref()
                            .map(|o| format!("被 {} 占用", owner_line(o)))
                            .unwrap_or_else(|| "设备被其他桥占用".to_string());
                        set_owner_cache(&ctx, owner);
                        set_device_note(&ctx, Some(text));
                        tracing::warn!("usage push rejected (409) by {device_ip}");
                    }
                    Ok(resp) => {
                        tracing::warn!("usage push -> {} ({})", resp.status(), url);
                        let mut status = ctx.status.lock().unwrap();
                        status.last_error = Some(format!("push: HTTP {}", resp.status()));
                        status.last_error_at = Some(now as i64);
                        status.push_fail_streak = status.push_fail_streak.saturating_add(1);
                        if status.push_fail_streak >= 2 {
                            status.last_push_ok_at = None;
                        }
                    }
                    Err(e) => {
                        if ctx.activity.expects_deep() {
                            // Device went to sleep; silence is expected.
                            tracing::debug!("usage push skipped (device deep): {e}");
                        } else {
                            tracing::debug!("usage push skipped: {e}");
                            let mut status = ctx.status.lock().unwrap();
                            status.push_fail_streak = status.push_fail_streak.saturating_add(1);
                            if status.push_fail_streak >= 2 {
                                status.last_push_ok_at = None;
                            }
                        }
                    }
                }
            }
        });
    }

    // v0.14 queued pushes (docs §13.4): while the device is deep, template
    // pushes / OTA requests are queued; the first contact after that (a pull
    // keeps the device awake for `pending`) flushes them.
    {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let mut last_gen = ctx.activity.contact_generation();
            let mut last_flush = 0u64;
            let mut fails = 0u32;
            let mut last_pending: (Vec<String>, bool) = (Vec::new(), false);
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let (templates, activate) = ctx.activity.pending_templates();
                let pending_ota = ctx.activity.pending_ota();
                if templates.is_empty() && !pending_ota {
                    last_gen = ctx.activity.contact_generation();
                    fails = 0;
                    last_pending = (Vec::new(), false);
                    continue;
                }
                // A fresh request (different ROM/templates) is retried promptly
                // even after earlier failures instead of inheriting the backoff.
                let pending_now = (templates.clone(), pending_ota);
                if pending_now != last_pending {
                    last_pending = pending_now;
                    fails = 0;
                    last_flush = 0;
                }
                let gen = ctx.activity.contact_generation();
                let now = now_secs() as u64;
                // Backoff after failed attempts: 60s, 2m, 4m, 8m, ... max 30m.
                // A pull contact (gen change) still retries immediately.
                let wait = if fails == 0 {
                    60u64
                } else {
                    (60u64 << fails.min(5)).min(1800)
                };
                if gen == last_gen && now.saturating_sub(last_flush) < wait {
                    continue;
                }
                last_gen = gen;
                last_flush = now;
                let mut failed = false;
                if !templates.is_empty() {
                    let cfg = mcp_config(&ctx);
                    match bridge_mcp::push_templates_http(&cfg, &templates, activate.as_deref()).await {
                        Ok(summary) => {
                            tracing::info!("queued template push flushed: {summary}");
                            ctx.activity.clear_pending_templates();
                        }
                        Err(e) => {
                            failed = true;
                            tracing::warn!("queued template push: {e}");
                        }
                    }
                }
                if pending_ota {
                    let rom = ctx.pending_ota_rom.lock().unwrap().clone();
                    if let Some(rom) = rom {
                        let request = json!({
                            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                            "params": {"name": "firmware_ota",
                                       "arguments": {"rom": rom.display().to_string()}}
                        });
                        let cfg = mcp_config(&ctx);
                        let response = bridge_mcp::handle_request(&cfg, &request).await;
                        let failed_ota = response
                            .as_ref()
                            .and_then(|r| r.pointer("/result/isError"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        if failed_ota {
                            failed = true;
                            tracing::warn!("queued OTA attempt failed; backing off");
                        } else {
                            ctx.activity.clear_pending_ota();
                            *ctx.pending_ota_rom.lock().unwrap() = None;
                        }
                    } else {
                        ctx.activity.clear_pending_ota();
                    }
                }
                fails = if failed { fails.saturating_add(1) } else { 0 };
            }
        });
    }

    // v0.12 demand-driven BLE (docs/power-state.md §5/§9): no periodic scanning
    // and no template transfers. A cycle runs only when the device asks for a
    // handshake in its UDP announce (`ble=1`), which refreshes the endpoint
    // record (host/port/token) over the bonded link. Templates go over HTTP.
    //
    // Plan C: the v2 rendezvous window is a hard 3 s, so this loop retries the
    // opportunity every 250 ms (effectively continuous scan coverage on a
    // mains-powered PC) and remembers a successful connect so one 60 s
    // rendezvous period never gets a second connection. Scan misses while the
    // device is not advertising are expected and stay at debug level.
    let mut last_v2_ok: Option<i64> = None;
    loop {
        if ctx.status.lock().unwrap().paused {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        tokio::select! {
            _ = ctx.force_ble.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(250)) => {}
        }
        let udp_request = ctx.udp_ble.swap(false, Ordering::SeqCst);
        if !udp_request {
            if platform::is_v2_device(&ctx) {
                let now = now_secs();
                let already_connected = last_v2_ok.is_some_and(|at| now.saturating_sub(at) < 55);
                if !already_connected {
                    match platform::ble_cycle(&ctx).await {
                        Ok(platform::BleOpportunity::Attempted) => last_v2_ok = Some(now_secs()),
                        Ok(platform::BleOpportunity::NoDevice) => {}
                        Err(error) => {
                            tracing::debug!(%error, "v2 BLE opportunity unavailable");
                        }
                    }
                }
            }
            continue;
        }
        let adapter = match Pusher::adapter().await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("bluetooth adapter: {e}");
                {
                    let mut status = ctx.status.lock().unwrap();
                    status.last_error = Some(format!("ble adapter: {e}"));
                    status.last_error_at = Some(now_secs());
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }
        };
        let ble_cfg = BleConfig {
            name_prefix: "CodexStatus-".to_string(),
            host: lan_ip(),
            port: ctx.config.port,
            token: ctx.config.token.clone(),
            // Identity handshake only: `Some(vec![])` leaves templates untouched.
            template_ids: Some(Vec::new()),
            activate: None,
            scan_timeout_ms: 30000,
        };
        let pusher = Pusher::new(
            ble_cfg,
            ctx.library.clone(),
            format!("http://127.0.0.1:{}", ctx.config.port),
        );
        match pusher.cycle_once(&adapter).await {
            Ok(info) => {
                tracing::info!("ble handshake done (udp announce)");
                // The device announces `ble=1`; adopt its identity/address from
                // the info JSON while we are connected (task-2 fallback path).
                if let Err(e) = adopt_ble_info(&ctx, &info) {
                    tracing::debug!("ble info not adopted: {e}");
                }
                let mut status = ctx.status.lock().unwrap();
                status.last_sync = Some(now_secs());
                status.last_error = None;
                status.last_error_at = None;
            }
            Err(e) => {
                tracing::warn!("ble cycle: {e}");
                {
                    let mut status = ctx.status.lock().unwrap();
                    status.last_error = Some(format!("ble: {e}"));
                    status.last_error_at = Some(now_secs());
                }
                // Give an unreachable device time to re-open its BLE session.
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
        }
    }
}

fn main() {
    // Hidden watchdog mode: supervise the given pid, restart on abnormal exit.
    let argv: Vec<String> = std::env::args().collect();
    if let Some(pos) = argv.iter().position(|a| a == "--watchdog") {
        if let Some(pid) = argv.get(pos + 1).and_then(|v| v.parse::<u32>().ok()) {
            watchdog::run(pid);
        }
        std::process::exit(1);
    }

    let root = config::repo_root();
    let cfg_path = config::config_path(&root);
    let mut config = Config::load(&cfg_path);
    if config.templates.is_relative() {
        config.templates = root.join(&config.templates);
    }
    if config.seeds.is_relative() {
        config.seeds = root.join(&config.seeds);
    }
    if config.profiles.is_relative() {
        config.profiles = root.join(&config.profiles);
    }
    if config.profile_seed.is_relative() {
        config.profile_seed = root.join(&config.profile_seed);
    }
    if let Err(e) = ensure_runtime(&config) {
        tracing::warn!("runtime seeding: {e}");
    }

    let log_dir = bridge_core::paths::data_root().join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "bridge-app.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    Box::leak(Box::new(guard));
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_ansi(false)
        .with_writer(non_blocking)
        .init();
    tracing::info!(
        "starting bridge-app: config={} templates={}",
        cfg_path.display(),
        config.templates.display()
    );

    let library = Library::load(&config.templates).unwrap_or_default();
    tracing::info!("templates: {:?}", library.ids());
    let mcp_port = config.mcp_port;
    let device_ip = config.device_ip.clone();
    // Identity: MAC is the key (learned/persisted); the name is an editable
    // display label generated on first identification.
    let device_mac = config
        .device_mac
        .clone()
        .map(|mac| normalize_mac(&mac))
        .filter(|mac| mac.len() == 12);
    let mut device_name = sanitize_display(&config.device_name, 24);
    let mut generated_name = false;
    if device_name.is_empty() {
        if let Some(mac) = &device_mac {
            device_name = default_device_name(mac);
            generated_name = true;
        }
    }
    let bridge_name = {
        let configured = sanitize_display(&config.bridge_name, 24);
        if configured.is_empty() {
            pc_name()
        } else {
            configured
        }
    };
    let bridge_id = short_id(&host_label());
    let (mcp_tx, _mcp_rx) = tokio::sync::watch::channel(mcp_port);
    let ctx = Arc::new(AppCtx {
        config,
        root: root.clone(),
        app_handle: OnceLock::new(),
        envelope: Arc::new(RwLock::new(None)),
        library: Arc::new(RwLock::new(library)),
        force_ble: Arc::new(Notify::new()),
        v2_delivery: tokio::sync::Mutex::new(()),
        status: Mutex::new(RuntimeStatus {
            last_sync: None,
            last_error: None,
            last_error_at: None,
            paused: false,
            last_push_at: None,
            last_push_error: None,
            last_push_ok_at: None,
            push_fail_streak: 0,
            device_note: None,
        }),
        device_ip: Mutex::new(device_ip),
        device_mac: Mutex::new(device_mac),
        device_name: Mutex::new(device_name),
        bridge_name: Mutex::new(bridge_name),
        bridge_id,
        discover: Mutex::new(Some(Discovery {
            via: "config".to_string(),
            at: now_secs(),
        })),
        device_cache: Mutex::new(None),
        pmstats_cache: Mutex::new(None),
        owner_cache: Mutex::new(None),
        last_claim_at: Mutex::new(None),
        yielded: AtomicBool::new(false),
        claim_unsupported: AtomicBool::new(false),
        arp_running: AtomicBool::new(false),
        device_fail_streak: AtomicU32::new(0),
        udp_ble: AtomicBool::new(false),
        device_ip_dirty: AtomicBool::new(false),
        activity: Arc::new(Activity::new()),
        pending_ota_rom: Mutex::new(None),
        force_push: Arc::new(Notify::new()),
        mcp_port: Mutex::new(mcp_port),
        mcp_error: Mutex::new(None),
        mcp_tx,
        platform: Arc::new(
            bridge_core::platform::service::PlatformService::open(
                &bridge_core::paths::data_root(),
            )
            .expect("open platform state"),
        ),
    });
    if generated_name {
        save_identity(&ctx);
    }
    tracing::info!(
        "device identity: name='{}' mac={} ip={} bridge='{}' id={}",
        ctx.device_name.lock().unwrap(),
        ctx.device_mac.lock().unwrap().as_deref().unwrap_or("-"),
        ctx.device_ip.lock().unwrap(),
        ctx.bridge_name.lock().unwrap(),
        ctx.bridge_id
    );

    let ctx_setup = ctx.clone();
    watchdog::spawn(std::process::id());
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_panel(app);
        }))
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_device_status,
            get_pmstats,
            preview_template,
            force_sync,
            set_paused,
            push_profile,
            reload_templates,
            get_profiles,
            save_profile,
            delete_profile,
            get_mcp_info,
            set_mcp_port,
            rename_device,
            device_discover,
            device_owner,
            claim_device,
            release_device,
            platform_overview,
            platform_templates,
            platform_devices,
            platform_template_get,
            platform_template_validate,
            platform_template_preview,
            platform_template_save,
            platform_profile_get,
            platform_profile_save,
            platform_publish,
            platform_publish_cancel,
            platform_activate,
            platform_data_sources,
            platform_data_source_save,
            platform_data_probe,
            platform_power,
            platform_plan,
            platform_status_refresh
        ])
        .setup(move |app| {
            let _ = ctx_setup.app_handle.set(app.handle().clone());
            app.manage(ctx_setup.clone());
            tauri::async_runtime::spawn(run_services(ctx_setup.clone()));
            tauri::async_runtime::spawn(mcp_serve(ctx_setup.clone()));

            let open = MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?;
            let sync = MenuItem::with_id(app, "sync", "立即同步", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "暂停推送", true, None::<&str>)?;
            let upgrade =
                MenuItem::with_id(app, "upgrade", "升级固件…", false, None::<&str>)?;
            let device = MenuItem::with_id(app, "device", "打开设备页", true, None::<&str>)?;
            let logs = MenuItem::with_id(app, "logs", "打开日志", true, None::<&str>)?;
            let templates = MenuItem::with_id(app, "templates", "打开模板目录", true, None::<&str>)?;
            let autostart = CheckMenuItem::with_id(
                app,
                "autostart",
                "开机自启",
                true,
                autostart::matches_current_exe(),
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(
                app,
                &[
                    &open,
                    &sync,
                    &pause,
                    &upgrade,
                    &sep1,
                    &device,
                    &logs,
                    &templates,
                    &autostart,
                    &sep2,
                    &quit,
                ],
            )?;

            let tray = TrayIconBuilder::with_id("bridge-tray")
                .icon(icon::render(None, IconState::NoData))
                .tooltip("Codex Status 桥")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event({
                    let app_handle = app.handle().clone();
                    let ctx = ctx_setup.clone();
                    move |_app, event| match event.id.as_ref() {
                        "quit" => app_handle.exit(0),
                        "open" => show_panel(&app_handle),
                        "sync" => request_sync(&ctx),
                        "pause" => {
                            let paused = {
                                let mut status = ctx.status.lock().unwrap();
                                status.paused = !status.paused;
                                status.paused
                            };
                            if !paused {
                                ctx.force_ble.notify_one();
                            }
                            let pause_id = tauri::menu::MenuId::from("pause");
                            if let Some(MenuItemKind::MenuItem(item)) =
                                app_handle.menu().and_then(|m| m.get(&pause_id))
                            {
                                let _ = item.set_text(if paused {
                                    "恢复推送"
                                } else {
                                    "暂停推送"
                                });
                            }
                            refresh_tray(&app_handle, &ctx);
                        }
                        "device" => open_url(&format!("http://{}", lan_ip())),
                        "autostart" => {
                            let enable = !autostart::enabled();
                            match autostart::set(enable) {
                                Ok(()) => tracing::info!("autostart set to {enable}"),
                                Err(e) => tracing::warn!("autostart set failed: {e}"),
                            }
                            let autostart_id = tauri::menu::MenuId::from("autostart");
                            if let Some(MenuItemKind::Check(item)) =
                                app_handle.menu().and_then(|m| m.get(&autostart_id))
                            {
                                let _ = item.set_checked(autostart::matches_current_exe());
                            }
                        }
                        "logs" => open_path(&bridge_core::paths::data_root().join("logs")),
                        "templates" => open_path(&ctx.config.templates),
                        _ => {}
                    }
                })
                .on_tray_icon_event({
                    let app_handle = app.handle().clone();
                    move |_tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            show_panel(&app_handle);
                        }
                    }
                })
                .build(app)?;
            let _ = tray;
            spawn_tray_loop(app.handle().clone(), ctx_setup.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
