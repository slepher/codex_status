#![cfg_attr(windows, windows_subsystem = "windows")]

mod autostart;
mod config;
mod icon;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bridge_ble::{lan_ip, BleConfig, Pusher};
use bridge_core::codex::locate_codex;
use bridge_core::http::{serve, AppState};
use bridge_core::runtime::{run_poller, PollerConfig};
use bridge_core::template::Library;
use bridge_core::short_id;
use config::Config;
use icon::State as IconState;
use serde_json::{json, Value};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tokio::sync::{Notify, RwLock};

#[derive(Default)]
struct PendingPush {
    ids: Vec<String>,
    activate: Option<String>,
}

struct RuntimeStatus {
    last_sync: Option<i64>,
    last_error: Option<String>,
    last_error_at: Option<i64>,
    paused: bool,
    pending: PendingPush,
    last_push_at: Option<i64>,
    last_push_error: Option<String>,
    /// Last successful `POST /usage`; while fresh, the HTTP path is the healthy
    /// data route and BLE scanning stays off (sleep.md §4.6).
    last_push_ok_at: Option<i64>,
    /// Consecutive failed push attempts; a single timeout must not invalidate
    /// the HTTP path (the device's WebServer occasionally misses a request).
    push_fail_streak: u32,
}

struct AppCtx {
    config: Config,
    root: PathBuf,
    app_handle: OnceLock<AppHandle>,
    envelope: Arc<RwLock<Option<Value>>>,
    library: Arc<RwLock<Library>>,
    force_ble: Arc<Notify>,
    idle_template: Arc<RwLock<Option<String>>>,
    status: Mutex<RuntimeStatus>,
    ble_primed: AtomicBool,
    mcp_port: Mutex<u16>,
    mcp_error: Mutex<Option<String>>,
    mcp_tx: tokio::sync::watch::Sender<u16>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// How long a successful `POST /usage` keeps the HTTP path "healthy" (covers
/// the 5 min heartbeat plus jitter) before BLE scanning resumes.
const HTTP_PUSH_HEALTHY_SECS: u64 = 360;

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
        "pending": status.pending.ids.len(),
        "last_push_at": status.last_push_at,
        "last_push_error": status.last_push_error,
        "last_push_ok_at": status.last_push_ok_at,
        "last_sync": status.last_sync,
        "last_error": status.last_error,
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
    let mcp_cfg = bridge_mcp::McpConfig {
        port: ctx.config.port,
        token: ctx.config.token.clone(),
        templates: ctx.config.templates.clone(),
        profiles: ctx.config.profiles.clone(),
        data_root: bridge_core::paths::data_root(),
        seeds: ctx.config.seeds.clone(),
        profile_seed: ctx.config.profile_seed.clone(),
        device_ip: ctx.config.device_ip.clone(),
        idle_template: ctx.idle_template.read().await.clone(),
        root: ctx.root.clone(),
    };
    let tool = request
        .pointer("/params/name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    match bridge_mcp::handle_request(&mcp_cfg, &request).await {
        Some(response) => {
            let ok = response
                .pointer("/result/isError")
                .and_then(|v| v.as_bool())
                .map(|is_error| !is_error)
                .unwrap_or(false);
            if ok && matches!(tool.as_deref(), Some("template_save") | Some("profile_save")) {
                if let Some(handle) = ctx.app_handle.get() {
                    let _ = handle.emit("templates-changed", ());
                }
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

#[tauri::command]
async fn get_device_status(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    let ip = state.config.device_ip.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        bridge_core::device::fetch(&ip, std::time::Duration::from_secs(3))
    })
    .await
    .map_err(|e| e.to_string())?;
    match result {
        Ok(status) => {
            let map: serde_json::Map<String, Value> = status
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            Ok(json!({"online": true, "ip": state.config.device_ip, "fields": map}))
        }
        Err(e) => Ok(json!({
            "online": false,
            "ip": state.config.device_ip,
            "error": e.to_string(),
            "fields": {},
        })),
    }
}

#[tauri::command]
async fn get_mcp_info(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    let port = *state.mcp_port.lock().unwrap();
    let error = state.mcp_error.lock().unwrap().clone();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let tools = "bridge_status / template_get / template_validate / template_render / template_save / profiles_list / profile_save / profile_push";

    let generic_prompt = format!(
        "本机已启动 Codex Status 的 MCP 服务（Streamable HTTP）：{url}\n\
         工具前缀 codex_status_（{tools}），用于读取、校验、渲染预览、保存模板并 BLE 推送到墨水屏。\n\
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

fn persist_idle_template(_root: &Path, id: &Option<String>) {
    let path = bridge_core::paths::data_root().join("bridge-app.json");
    let mut doc: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    if let Some(object) = doc.as_object_mut() {
        object.insert("idle_template".to_string(), json!(id));
    }
    if let Ok(text) = serde_json::to_string_pretty(&doc) {
        let _ = std::fs::write(&path, text);
    }
}

#[tauri::command]
async fn get_idle_template(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    Ok(json!({"idle_template": state.idle_template.read().await.clone()}))
}

#[tauri::command]
async fn set_idle_template(
    state: State<'_, Arc<AppCtx>>,
    id: Option<String>,
) -> Result<Value, String> {
    let id = id.filter(|value| !value.is_empty());
    if let Some(value) = &id {
        let library = state.library.read().await;
        if library.get(value).is_none() {
            return Err(format!("template not found: {value}"));
        }
    }
    *state.idle_template.write().await = id.clone();
    persist_idle_template(&state.root, &id);
    state.force_ble.notify_one();
    Ok(json!({"idle_template": id}))
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
    state.force_ble.notify_one();
    Ok(())
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

#[tauri::command]
async fn push_profile(state: State<'_, Arc<AppCtx>>, id: String) -> Result<(), String> {
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
    {
        let mut status = state.status.lock().unwrap();
        status.pending = PendingPush {
            ids: enabled.clone(),
            activate: enabled.first().cloned(),
        };
    }
    state.force_ble.notify_one();
    Ok(())
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

/// Fingerprint of what the BLE cycle would push: usage envelope + pending
/// template work + the idle template. A successful cycle pauses scanning until
/// this changes or the 5 min heartbeat expires (sleep.md §4.6).
fn ble_fingerprint(ctx: &AppCtx) -> u64 {
    let usage = ctx
        .envelope
        .try_read()
        .ok()
        .and_then(|g| g.as_ref().map(|v| v.to_string()))
        .unwrap_or_default();
    let (ids, activate) = {
        let status = ctx.status.lock().unwrap();
        (
            status.pending.ids.join(","),
            status.pending.activate.clone().unwrap_or_default(),
        )
    };
    let idle = ctx
        .idle_template
        .try_read()
        .ok()
        .and_then(|v| v.clone())
        .unwrap_or_default();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in usage
        .bytes()
        .chain(ids.bytes())
        .chain(activate.bytes())
        .chain(idle.bytes())
    {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

async fn run_services(ctx: Arc<AppCtx>) {
    let exe = match locate_codex(ctx.config.codex_path.as_deref()) {
        Ok(exe) => exe,
        Err(e) => {
            tracing::error!("codex cli not found: {e}");
            ctx.status.lock().unwrap().last_error = Some(format!("codex cli: {e}"));
            ctx.status.lock().unwrap().last_error_at = Some(now_secs());
            return;
        }
    };
    tracing::info!("codex cli: {}", exe.display());

    let addr = format!("0.0.0.0:{}", ctx.config.port).parse().expect("addr");
    let http_state = AppState {
        token: Arc::new(ctx.config.token.clone()),
        envelope: ctx.envelope.clone(),
        library: ctx.library.clone(),
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
        host_id: short_id(&host_label()),
        interval_secs: ctx.config.interval_secs,
        templates: ctx.library.clone(),
        idle_template: ctx.idle_template.clone(),
        active_hold_seconds: ctx.config.active_hold_seconds,
    };
    tokio::spawn(run_poller(poller, ctx.envelope.clone()));

    // Usage push (sleep.md §4.3/§4.6): POST the envelope to the device on
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
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let usage = ctx.envelope.try_read().ok().and_then(|g| g.clone());
                let Some(usage) = usage else { continue };
                let text = usage.to_string();
                let fp = {
                    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                    for b in text.bytes() {
                        h ^= b as u64;
                        h = h.wrapping_mul(0x100_0000_01b3);
                    }
                    h
                };
                let now = now_secs() as u64;
                if fp == last_fp && now.saturating_sub(last_ok) < 300 {
                    continue;
                }
                let url = format!("http://{}/usage", ctx.config.device_ip);
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
                        tracing::debug!("usage push skipped: {e}");
                        let mut status = ctx.status.lock().unwrap();
                        status.push_fail_streak = status.push_fail_streak.saturating_add(1);
                        if status.push_fail_streak >= 2 {
                            status.last_push_ok_at = None;
                        }
                    }
                }
            }
        });
    }

        let mut misses: u32 = 0;
    let mut last_fp: u64 = 0;
    let mut pause_until: u64 = 0;
    loop {
        let paused = ctx.status.lock().unwrap().paused;
        if paused {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        let first = !ctx.ble_primed.swap(true, Ordering::SeqCst);
        let now = now_secs() as u64;
        let pending_waiting = !ctx.status.lock().unwrap().pending.ids.is_empty();
        // Demand-driven scanning (sleep.md §4.6): while `POST /usage` is
        // succeeding the device is LIVE and BLE is off on its side, so scan
        // only for explicit work, startup, or when the HTTP path goes quiet.
        let http_ok = {
            let status = ctx.status.lock().unwrap();
            status
                .last_push_ok_at
                .is_some_and(|t| now.saturating_sub(t as u64) < HTTP_PUSH_HEALTHY_SECS)
        };
        if !first && !pending_waiting && http_ok {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(15)) => {}
                _ = ctx.force_ble.notified() => {}
            }
            continue;
        }
        let fp = ble_fingerprint(&ctx);
        if !pending_waiting && fp == last_fp && now < pause_until {
            let wait = Duration::from_secs(pause_until - now);
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = ctx.force_ble.notified() => {}


            }
            continue;
        }
        let mut manual = pending_waiting;
        let wait = if first || pending_waiting {
            Duration::ZERO
        } else {
            Duration::from_secs(if misses >= 12 { 60 } else { 20 })
        };
        if !wait.is_zero() {
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = ctx.force_ble.notified() => { manual = true; }
            }
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
        // Templates are only pushed when explicitly requested (profile push in
        // the panel or MCP profile_push); the periodic cycle just refreshes
        // usage/endpoint. Explicit profile pushes also carry the idle template.
        let pending = std::mem::take(&mut ctx.status.lock().unwrap().pending);
        let mut push_ids = pending.ids.clone();
        if !push_ids.is_empty() {
            if let Some(idle) = ctx.idle_template.read().await.clone() {
                if ctx.library.read().await.get(&idle).is_some() && !push_ids.contains(&idle) {
                    push_ids.push(idle);
                }
            }
        }
        let ble_cfg = BleConfig {
            name_prefix: "CodexStatus-".to_string(),
            host: lan_ip(),
            port: ctx.config.port,
            token: ctx.config.token.clone(),
            template_ids: Some(push_ids),
            activate: pending.activate.clone(),
            // A user-triggered sync (panel/MCP/tray) waits longer for the
            // device's short DEEP window; periodic rounds stay low-duty.
            scan_timeout_ms: if manual { 30000 } else { 5000 },
        };
        let pusher = Pusher::new(
            ble_cfg,
            ctx.library.clone(),
            format!("http://127.0.0.1:{}", ctx.config.port),
        );
        match pusher.cycle_once(&adapter).await {
            Ok(()) => {
                tracing::info!("ble cycle done");
                misses = 0;
                last_fp = fp;
                let mut status = ctx.status.lock().unwrap();
                status.last_sync = Some(now_secs());
                status.last_error = None;
                status.last_error_at = None;
                if !pending.ids.is_empty() {
                    status.last_push_at = Some(now_secs());
                    status.last_push_error = None;
                }
                pause_until = now_secs() as u64 + 300;
            }
            Err(e) => {
                misses = misses.saturating_add(1);
                let mut status = ctx.status.lock().unwrap();
                if http_ok {
                    // Expected while the device is LIVE (BLE off by design); the
                    // HTTP push path is healthy, so don't surface a hard error.
                    tracing::debug!("ble cycle skipped: {e}");
                } else {
                    tracing::warn!("ble cycle: {e}");
                    status.last_error = Some(format!("ble: {e}"));
                    status.last_error_at = Some(now_secs());
                }
                if !pending.ids.is_empty() {
                    status.last_push_error = Some(format!("ble: {e}"));
                }
                for id in pending.ids {
                    if !status.pending.ids.contains(&id) {
                        status.pending.ids.push(id);
                    }
                }
                if status.pending.activate.is_none() {
                    status.pending.activate = pending.activate;
                }
            }
        }
    }
}

fn main() {
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
    let idle_template = Arc::new(RwLock::new(config.idle_template.clone()));
    let (mcp_tx, _mcp_rx) = tokio::sync::watch::channel(mcp_port);
    let ctx = Arc::new(AppCtx {
        config,
        root: root.clone(),
        app_handle: OnceLock::new(),
        envelope: Arc::new(RwLock::new(None)),
        library: Arc::new(RwLock::new(library)),
        force_ble: Arc::new(Notify::new()),
        idle_template,
        status: Mutex::new(RuntimeStatus {
            last_sync: None,
            last_error: None,
            last_error_at: None,
            paused: false,
            pending: PendingPush::default(),
            last_push_at: None,
            last_push_error: None,
            last_push_ok_at: None,
            push_fail_streak: 0,
        }),
        ble_primed: AtomicBool::new(false),
        mcp_port: Mutex::new(mcp_port),
        mcp_error: Mutex::new(None),
        mcp_tx,
    });

    let ctx_setup = ctx.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_panel(app);
        }))
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_device_status,
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
            get_idle_template,
            set_idle_template
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
                        "sync" => ctx.force_ble.notify_one(),
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
