#![cfg_attr(windows, windows_subsystem = "windows")]

mod autostart;
mod config;
mod device_runtime;
mod discovery;
mod icon;
mod instance;
mod platform;
mod watchdog;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bridge_ble::{lan_ip, BleConfig, Pusher};
use bridge_core::activity::Activity;
use bridge_core::codex::locate_codex;
use bridge_core::http::serve;
use bridge_core::runtime::{run_poller, PollerConfig};
use bridge_core::template::Library;
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
    last_attempt_at: i64,
    last_error: Option<String>,
    ip: String,
    text: String,
}

/// How/when the current device IP was last learned (udp/arp/ble/http/config).
#[derive(Clone)]
struct Discovery {
    via: String,
    at: i64,
}

/// Last `owner` seen in `/status.json`.
#[derive(Clone)]
struct CachedOwner {
    owner: Option<Value>,
    observed_at: i64,
}

/// Per-device result of the authenticated device status endpoint. Offline updates
/// retain the last successful body for diagnostics.
#[derive(Clone)]
struct CachedDeviceStatus {
    fetched_at: i64,
    fetched_mono_ms: Option<u64>,
    online: bool,
    status: Option<Value>,
    owner: Option<Value>,
    owner_observed_at: Option<i64>,
    last_claim_at: Option<i64>,
    last_claim_mono_ms: Option<u64>,
    yielded: bool,
}

fn update_device_status_cache(
    cache: &mut HashMap<String, CachedDeviceStatus>,
    mac: &str,
    online: bool,
    status: Option<Value>,
    fetched_at: i64,
) {
    let Some(mac) = bridge_core::platform::model::DeviceIdentity::normalized_mac(mac) else {
        return;
    };
    let previous = cache.get(&mac);
    let previous_status = previous.and_then(|entry| entry.status.clone());
    let owner = if online {
        status
            .as_ref()
            .and_then(|status| status.get("owner"))
            .filter(|owner| !owner.is_null())
            .cloned()
    } else {
        previous.and_then(|entry| entry.owner.clone())
    };
    cache.insert(
        mac.clone(),
        CachedDeviceStatus {
            fetched_at,
            fetched_mono_ms: bridge_core::device_clock::monotonic_ms(&mac),
            online,
            status: if online { status } else { previous_status },
            owner,
            owner_observed_at: if online {
                Some(fetched_at)
            } else {
                previous.and_then(|entry| entry.owner_observed_at)
            },
            last_claim_at: previous.and_then(|entry| entry.last_claim_at),
            last_claim_mono_ms: previous.and_then(|entry| entry.last_claim_mono_ms),
            yielded: previous.is_some_and(|entry| entry.yielded),
        },
    );
}

fn update_device_claim_cache(
    cache: &mut HashMap<String, CachedDeviceStatus>,
    mac: &str,
    owner: Option<Value>,
    claimed_at: i64,
    yielded: bool,
) {
    let Some(mac) = bridge_core::platform::model::DeviceIdentity::normalized_mac(mac) else {
        return;
    };
    let entry = cache.entry(mac.clone()).or_insert(CachedDeviceStatus {
        fetched_at: 0,
        fetched_mono_ms: None,
        online: false,
        status: None,
        owner: None,
        owner_observed_at: None,
        last_claim_at: None,
        last_claim_mono_ms: None,
        yielded: false,
    });
    entry.owner = owner.filter(|owner| !owner.is_null());
    entry.owner_observed_at = Some(claimed_at);
    entry.last_claim_at = Some(claimed_at);
    entry.last_claim_mono_ms = bridge_core::device_clock::monotonic_ms(mac.as_str());
    entry.yielded = yielded;
}

fn update_device_owner_cache(cache: &mut HashMap<String, CachedDeviceStatus>, mac: &str, owner: Value, at: i64) {
    let Some(mac) = bridge_core::platform::model::DeviceIdentity::normalized_mac(mac) else {
        return;
    };
    let entry = cache.entry(mac).or_insert(CachedDeviceStatus {
        fetched_at: 0,
        fetched_mono_ms: None,
        online: false,
        status: None,
        owner: None,
        owner_observed_at: None,
        last_claim_at: None,
        last_claim_mono_ms: None,
        yielded: false,
    });
    entry.owner = (!owner.is_null()).then_some(owner);
    entry.owner_observed_at = Some(at);
}

fn device_cycle_targets(
    devices: &[Value],
    cache: &HashMap<String, CachedDeviceStatus>,
    now: i64,
) -> Vec<String> {
    devices
        .iter()
        .filter_map(|device| {
            let mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(
                device["device_mac"].as_str()?,
            )?;
            let endpoint = device["ip"].as_str()?.trim();
            if endpoint.is_empty() || endpoint == "0.0.0.0" {
                return None;
            }
            let status = cache.get(&mac)?;
            let now = scheduled_now(&mac, now);
            (status.online && device_status_fresh(&mac, status, now, 30))
                .then_some(mac)
        })
        .collect()
}

fn device_status_fresh(mac: &str, status: &CachedDeviceStatus, now: i64, ttl_s: u64) -> bool {
    if let Some(monotonic) = bridge_core::device_clock::monotonic_ms(mac) {
        return status.fetched_mono_ms.is_some_and(|at|
            monotonic >= at && monotonic - at <= ttl_s * 1000);
    }
    status.fetched_at <= now && now.saturating_sub(status.fetched_at) <= ttl_s as i64
}

fn device_claim_renew_due(mac: &str, status: &CachedDeviceStatus, now: i64) -> bool {
    if let Some(monotonic) = bridge_core::device_clock::monotonic_ms(mac) {
        return status.last_claim_mono_ms.is_none_or(|at|
            monotonic >= at && monotonic - at >= 60_000);
    }
    status.last_claim_at.is_none_or(|at| now.saturating_sub(at) >= 60)
}

enum DeviceOccupancyDecision {
    Offline,
    Yielded,
    Other(Value),
    Owned { renew: bool },
    Claim,
}

fn device_occupancy_decision(
    mac: &str,
    status: Option<&CachedDeviceStatus>,
    bridge_id: &str,
    now: i64,
) -> DeviceOccupancyDecision {
    let Some(status) =
        status.filter(|status| status.online && device_status_fresh(mac, status, now, 30))
    else {
        return DeviceOccupancyDecision::Offline;
    };
    if status.yielded {
        return DeviceOccupancyDecision::Yielded;
    }
    if let Some(owner) = status.owner.as_ref().filter(|owner| owner_valid(owner)) {
        if owner_id(owner) != Some(bridge_id) {
            return DeviceOccupancyDecision::Other(owner.clone());
        }
        let renew = device_claim_renew_due(mac, status, now);
        return DeviceOccupancyDecision::Owned { renew };
    }
    DeviceOccupancyDecision::Claim
}

struct AppCtx {
    instance: instance::Instance,
    config: Config,
    root: PathBuf,
    app_handle: OnceLock<AppHandle>,
    envelope: Arc<RwLock<Option<Value>>>,
    library: Arc<RwLock<Library>>,
    force_ble: Arc<Notify>,
    device_delivery: tokio::sync::Mutex<()>,
    sim_control: tokio::sync::Mutex<()>,
    per_mac_delivery: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    status: Mutex<RuntimeStatus>,
    /// Per-MAC runtime records (address, name, caches, occupancy, queues).
    ///
    /// Replaces the old single global device: an announcement or a claim for one
    /// MAC can only ever touch that MAC's record (`design.md` §3 — no "switch
    /// the global device"). `bridge_name`/`bridge_id` stay bridge-wide.
    devices: Mutex<device_runtime::DeviceRegistry>,
    /// Bridge display name reported in `POST /claim` (defaults to the host name).
    bridge_name: Mutex<String>,
    /// Reused as the device-side owner id (envelope `bridge.hostId`).
    bridge_id: String,
    /// Authenticated device status and reachability, isolated by normalized MAC.
    device_status_cache: Mutex<HashMap<String, CachedDeviceStatus>>,
    /// An ARP fallback scan is already running.
    arp_running: AtomicBool,
    /// v0.14 mode/activity state shared with the HTTP pull path
    /// (usage_rev / last_change_at / pending queues; docs §13.4).
    activity: Arc<Activity>,
    /// The device asked for a BLE handshake in its UDP announce (`ble=1`).
    udp_ble: AtomicBool,
    mcp_port: Mutex<u16>,
    mcp_error: Mutex<Option<String>>,
    mcp_tx: tokio::sync::watch::Sender<u16>,
    /// platform application service: the single business model shared by the
    /// four UI pages and MCP (templates, devices, data sources, power, MCP).
    platform: Arc<bridge_core::platform::service::PlatformService>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn device_now_secs(mac: &str) -> i64 {
    bridge_core::device_clock::wall_secs(mac) as i64
}

fn scheduled_now(mac: &str, fallback: i64) -> i64 {
    if bridge_core::device_clock::snapshot(mac).is_some() {
        device_now_secs(mac)
    } else {
        fallback
    }
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
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
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

fn device_path() -> PathBuf {
    bridge_core::paths::data_root().join("bridge-app.json")
}

/// Persist `{device_name, device_mac, device_ip, bridge_name}` into
/// `<data>/bridge-app.json` (read-modify-write, like `persist_mcp_port`).
///
/// The device triple is the **selected** device's record, so the next start
/// adopts the same device. Other registered devices live in the platform state.
fn save_identity(ctx: &AppCtx) {
    let (name, mac, ip) = {
        let registry = ctx.devices.lock().unwrap();
        match registry.primary_mac().and_then(|mac| registry.get(mac)) {
            Some(device) => (device.name.clone(), Some(device.mac.clone()), device.ip.clone()),
            None => (String::new(), None, String::new()),
        }
    };
    let bridge_name = ctx.bridge_name.lock().unwrap().clone();
    let path = device_path();
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

// ---------------------------------------------------------------------------
// Per-MAC runtime records: the only way the bridge reads or writes a device's
// address, name, caches, occupancy or queues. Everything below takes an
// explicit MAC (or resolves one) — there is no global device to switch.
// ---------------------------------------------------------------------------

/// One device's runtime facts, cloned out of the registry so no lock is held
/// across an await point.
#[derive(Clone, Default)]
struct DeviceFacts {
    mac: String,
    /// Name the user chose; empty means "use the MAC-derived default".
    name: String,
    ip: String,
    discover: Option<Discovery>,
    status: Option<CachedDevice>,
    pmstats: Option<CachedPmStats>,
    owner: Option<CachedOwner>,
    yielded: bool,
    note: Option<String>,
}

impl DeviceFacts {
    /// Label for the panel: the user's name when set, else the MAC default.
    fn display_name(&self) -> String {
        if self.name.trim().is_empty() {
            default_device_name(&self.mac)
        } else {
            self.name.clone()
        }
    }

    /// Address as an HTTP endpoint; empty when this device has no address yet.
    fn endpoint(&self) -> String {
        self.ip.trim().to_string()
    }

    fn is_online(&self) -> bool {
        self.status.as_ref().is_some_and(|cache| cache.online)
    }

}

/// Registry lock helper.
fn devices(ctx: &AppCtx) -> std::sync::MutexGuard<'_, device_runtime::DeviceRegistry> {
    ctx.devices.lock().unwrap()
}

/// Facts of one specific MAC; `None` when that MAC is not registered.
fn device_facts_for(ctx: &AppCtx, mac: &str) -> Option<DeviceFacts> {
    let registry = devices(ctx);
    registry.get(mac).map(|device| DeviceFacts {
        mac: device.mac.clone(),
        name: device.name.clone(),
        ip: device.ip.clone(),
        discover: device.discover.clone(),
        status: device.status.clone(),
        pmstats: device.pmstats.clone(),
        owner: device.owner.clone(),
        yielded: device.yielded,
        note: device.note.clone(),
    })
}

/// Facts of the device this operation resolves to without an explicit MAC.
fn device_facts(ctx: &AppCtx) -> Option<DeviceFacts> {
    let mac = devices(ctx).primary_mac_owned()?;
    device_facts_for(ctx, &mac)
}

/// MAC of the device an operation without an explicit `mac` argument targets.
fn selected_mac(ctx: &AppCtx) -> Option<String> {
    devices(ctx).primary_mac_owned()
}

/// Runtime record for a MAC, created on demand from the platform store so a
/// registered-but-not-yet-seen device still has its own record (used by the
/// single-device paths that legitimately fall back to the selected device).
fn ensure_runtime_record(ctx: &AppCtx, mac: &str) -> Option<DeviceFacts> {
    let mac = normalize_mac(mac);
    if mac.len() != 12 {
        return None;
    }
    if devices(ctx).get(&mac).is_none() {
        let registered = platform::service(ctx).device_get(&mac);
        let registered = registered?;
        let ip = registered
            ["ip"].as_str().map(str::to_string);
        let name = registered["name"].as_str();
        devices(ctx).observe(&mac, ip.as_deref(), name, "platform", device_now_secs(&mac));
    }
    device_facts_for(ctx, &mac)
}

/// MAC a device-facing operation may use when the caller named none.
///
/// `design.md` §2: a compatibility default exists only while **exactly one**
/// device is registered. With several, the caller must name one — the panel's
/// current selection is what the device page shows, not a licence for an
/// operation that never said which device it means.
///
/// "Registered" means the platform store (the durable list the panel shows),
/// not merely the devices this process has heard from since it started: right
/// after a restart those two differ, and the rule counts what the user sees.
pub(crate) fn sole_registered_mac(ctx: &AppCtx) -> Result<String, String> {
    let registered = platform::service(ctx).devices();
    match registered.len() {
        0 => Err("no device registered yet; run device_discover first".to_string()),
        1 => registered[0]["device_mac"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "registered device has no MAC".to_string()),
        _ => {
            let macs: Vec<&str> = registered
                .iter()
                .filter_map(|device| device["device_mac"].as_str())
                .collect();
            Err(format!(
                "select a device: several are registered ({})",
                macs.join(", ")
            ))
        }
    }
}

/// MAC an operation with an optional `mac` argument must run against.
///
/// * an explicit MAC is used as given and must be registered;
/// * without one the registry's selection answers — which is the only
///   registered device while there is exactly one;
/// * two or more devices with none selected is an error, never "the first
///   entry" (`design.md` §2).
fn resolve_target_mac(ctx: &AppCtx, requested: Option<&str>) -> Result<String, String> {
    match requested.map(str::trim).filter(|mac| !mac.is_empty()) {
        Some(raw) => {
            let mac = normalize_mac(raw);
            if mac.len() != 12 {
                return Err(format!("invalid device MAC: {raw}"));
            }
            if !devices(ctx).get(&mac).is_some()
                && platform::service(ctx).device_get(&mac).is_none()
            {
                return Err(format!("device {mac} is not registered"));
            }
            Ok(mac)
        }
        None => sole_registered_mac(ctx),
    }
}

/// MAC a discovery/registration action must run against: an explicit MAC is
/// accepted even before it is registered (that is what discovery is for).
fn resolve_discovery_mac(ctx: &AppCtx, requested: Option<&str>) -> Result<String, String> {
    match requested.map(str::trim).filter(|mac| !mac.is_empty()) {
        Some(raw) => {
            let mac = normalize_mac(raw);
            if mac.len() != 12 {
                return Err(format!("invalid device MAC: {raw}"));
            }
            Ok(mac)
        }
        None => selected_mac(ctx).ok_or_else(|| {
            "no device selected; pass an explicit MAC or register a device first".to_string()
        }),
    }
}

/// Record an authenticated announcement for exactly this MAC.
///
/// Routing replaces the old `learn_mac` single-global-identity rule: this can
/// only ever create or update **this** MAC's record, so another device's
/// address, name and caches can never be written from here. An invalid/empty
/// MAC is ignored (never "the" device), and a name is only filled when empty so
/// a chosen label survives announcements.
fn observe_device(
    ctx: &AppCtx,
    mac: &str,
    ip: Option<&str>,
    name: Option<&str>,
    via: &str,
) -> bool {
    let normalized = normalize_mac(mac);
    if normalized.len() != 12 {
        return false;
    }
    let now = device_now_secs(&mac);
    let mut registry = devices(ctx);
    registry.observe(&normalized, ip, name, via, now);
    true
}

/// Update one MAC's address attribute (and nothing else's); `via` records the
/// discovery method shown in the panel/MCP. Returns true when it changed.
fn set_device_ip(ctx: &AppCtx, mac: &str, ip: &str, via: &str) -> bool {
    let ip = ip.trim();
    if ip.is_empty() {
        return false;
    }
    let now = device_now_secs(&mac);
    let changed = {
        let mut registry = devices(ctx);
        match registry.get_mut(mac) {
            Some(device) => device.set_ip(ip, via, now),
            None => return false,
        }
    };
    if changed {
        tracing::info!("device {mac} endpoint updated: {ip} (via {via})");
        let selected = selected_mac(ctx).as_deref() == Some(&normalize_mac(mac));
        if selected {
            save_identity(ctx);
        }
    }
    changed
}

fn set_device_note(ctx: &AppCtx, mac: &str, note: Option<String>) {
    if let Some(device) = devices(ctx).get_mut(mac) {
        device.note = note;
    }
}

fn device_note(ctx: &AppCtx, mac: &str) -> Option<String> {
    devices(ctx).get(mac).and_then(|device| device.note.clone())
}

fn note_failure(ctx: &AppCtx, mac: &str) -> u32 {
    devices(ctx)
        .get_mut(mac)
        .map(|device| device.note_failure())
        .unwrap_or(0)
}

fn note_contact(ctx: &AppCtx, mac: &str) {
    if let Some(device) = devices(ctx).get_mut(mac) {
        device.note_contact();
    }
}

fn set_yielded(ctx: &AppCtx, mac: &str, yielded: bool) {
    if let Some(device) = devices(ctx).get_mut(mac) {
        device.yielded = yielded;
    }
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

fn set_owner_cache(ctx: &AppCtx, mac: &str, owner: Option<Value>) {
    let owner = match owner {
        Some(value) if !value.is_null() => Some(value),
        _ => None,
    };
    if let Some(device) = devices(ctx).get_mut(mac) {
        device.owner = Some(CachedOwner {
            owner,
            observed_at: device_now_secs(mac),
        });
    }
}

/// Adopt identity from a BLE info JSON (`{mac, ip, http_port}`).
///
/// The MAC is authenticated by the bonded BLE link, so the record it feeds is
/// exactly that MAC's — never another device's.
fn adopt_ble_info(ctx: &AppCtx, info: &Value) -> Result<(), String> {
    let mac = info.get("mac").and_then(|v| v.as_str()).unwrap_or("");
    let ip = info.get("ip").and_then(|v| v.as_str()).unwrap_or("");
    if mac.is_empty() && ip.is_empty() {
        return Err("device info has no mac/ip (firmware < 0.13.4?)".to_string());
    }
    if !mac.is_empty() {
        let normalized = normalize_mac(mac);
        if normalized.len() != 12 {
            return Err(format!("device info reports an invalid MAC: {mac}"));
        }
        observe_device(ctx, &normalized, None, None, "ble");
        if !ip.is_empty() {
            set_device_ip(ctx, &normalized, ip, "ble");
        }
        return Ok(());
    }
    Err("device info has no MAC; refusing to guess a target".to_string())
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
    if let Some(name) = &ctx.instance.name {
        tip.insert_str(0, &format!("[{name}] "));
    }
    if status.paused {
        tip.push_str(" · 已暂停");
    } else {
        tip.push_str(&format!(" · 同步 {sync_text}"));
    }
    if let Some(err) = status.last_error.as_deref() {
        tip.push_str(&format!(" · {err}"));
    }
    if let Some(note) = current_note(ctx).as_deref() {
        tip.push_str(&format!(" · {note}"));
    }
    (percent, state, tip)
}

/// Informational note shown for the selected device (the panel's device page
/// note moved into the per-MAC record when a second device appeared).
fn current_note(ctx: &AppCtx) -> Option<String> {
    let registry = devices(ctx);
    registry
        .primary_mac()
        .and_then(|mac| registry.get(mac))
        .and_then(|device| device.note.clone())
        // No selection (several devices): the registry's stable order decides
        // which note the tray shows, never discovery order.
        .or_else(|| {
            registry
                .macs()
                .into_iter()
                .find_map(|mac| registry.get(&mac).and_then(|device| device.note.clone()))
        })
}

fn stale_or_ok(status: &RuntimeStatus, now: i64) -> IconState {
    match status.last_sync {
        Some(sync) if now - sync <= HTTP_PUSH_HEALTHY_SECS as i64 => IconState::Ok,
        _ => IconState::Stale,
    }
}

fn refresh_tray(app: &AppHandle, ctx: &Arc<AppCtx>) {
    let (percent, state, tip) = tray_snapshot(ctx);
    let shape = ctx.instance.shape;
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(tray) = app.tray_by_id("bridge-tray") {
            let _ = tray.set_icon(Some(icon::render(percent, state, shape)));
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
async fn get_status(
    state: State<'_, Arc<AppCtx>>,
    mac: Option<String>,
) -> Result<Value, String> {
    let usage = state.envelope.read().await.clone();
    refresh_library(&state).await;
    let library = state.library.read().await;
    let templates: Vec<Value> = library
        .entries
        .values()
        .map(|e| json!({"id": e.id, "hash": e.hash}))
        .collect();
    // The page requests one device's snapshot; a bridge with no device at all
    // still answers the bridge-level fields with `device: null` (the panel must
    // render before any device exists).
    let target = match mac.as_deref().map(str::trim).filter(|mac| !mac.is_empty()) {
        Some(requested) => {
            let normalized = normalize_mac(requested);
            if normalized.len() != 12 {
                return Err(format!("invalid device MAC: {requested}"));
            }
            Some(normalized)
        }
        None => selected_mac(&state),
    };
    // Compute the device snapshot before locking `status` (it takes the same
    // locks in the opposite order and would deadlock against a 3 s poll).
    let device = target
        .as_deref()
        .and_then(|mac| device_facts_for(&state, mac))
        .map(|facts| device_facts_json(&facts))
        .unwrap_or(Value::Null);
    let device_note = target
        .as_deref()
        .and_then(|mac| device_note(&state, mac))
        .or_else(|| current_note(&state));
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
        "last_sync": status.last_sync,
        "last_error": status.last_error,
        "device_note": device_note,
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
    let facts = device_facts(ctx);
    bridge_mcp::McpConfig {
        port: ctx.config.port,
        token: ctx.config.token.clone(),
        templates: ctx.config.templates.clone(),
        data_root: bridge_core::paths::data_root(),
        seeds: ctx.config.seeds.clone(),
        device_ip: facts.as_ref().map(|f| f.ip.clone()).unwrap_or_default(),
        device_name: facts.as_ref().map(|f| f.name.clone()).unwrap_or_default(),
        device_mac: facts.as_ref().map(|f| f.mac.clone()),
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
    let _sim_barrier = if std::env::var("CODEX_STATUS_SIM_COOPERATIVE").as_deref() == Ok("1") {
        Some(ctx.sim_control.lock().await)
    } else { None };
    let host_ok = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| h.starts_with("127.0.0.1") || h.starts_with("localhost") || h.starts_with("[::1]"))
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
        // platform tools share the exact application service the UI uses.
        if matches!(
            name,
            "platform_overview"
                | "platform_device_view"
                | "platform_device_detail"
                | "platform_device_register"
                | "platform_template_list"
                | "platform_template_get"
                | "platform_template_save"
                | "platform_template_validate"
                | "platform_profile_get"
                | "platform_profile_save"
                | "platform_data_sync_save"
                | "platform_firmware_release_publish"
                | "platform_family_profiles"
                | "family_platform_profile_save"
                | "platform_family_profile_delete"
                | "platform_family_profile_copy"
                | "platform_publish"
                | "platform_publish_preview"
                | "platform_font_list"
                | "platform_font_import"
                | "platform_publish_cancel"
                | "firmware_ota"
                | "firmware_ota_status"
                | "firmware_ota_cancel"
                | "platform_template_activate"
                | "platform_data_sources"
                | "platform_data_source_save"
                | "platform_data_probe"
                | "platform_power_view"
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
                Ok(text) => {
                    if name == "platform_template_save" {
                        if let Some(handle) = ctx.app_handle.get() {
                            let _ = handle.emit("templates-changed", ());
                        }
                    }
                    json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {"content": [{"type": "text", "text": text}], "isError": false}
                    })
                }
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
    let sim_clock_enabled = if std::env::var_os("CODEX_STATUS_BRIDGE_SIM_CONTROL_TOKEN").is_some() {
        let path = bridge_core::paths::data_root().join("sim-bridge-clocks.json");
        if path.exists() {
            let restored = std::fs::read_to_string(&path)
                .map_err(anyhow::Error::from)
                .and_then(|raw| serde_json::from_str::<Value>(&raw).map_err(anyhow::Error::from))
                .and_then(|state| bridge_core::device_clock::restore(&state));
            if let Err(error) = restored {
                tracing::error!("bridge experiment clock state rejected: {error:#}");
                false
            } else { true }
        } else { true }
    } else { false };
    let mut rx = ctx.mcp_tx.subscribe();
    loop {
        let port = *rx.borrow();
        match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                *ctx.mcp_error.lock().unwrap() = None;
                tracing::info!("mcp http listening on http://127.0.0.1:{port}/mcp");
                let mut app = axum::Router::new()
                    .route("/mcp", axum::routing::post(mcp_handler));
                if sim_clock_enabled {
                    app = app.route("/sim/clock", axum::routing::post(sim_clock_handler));
                    if std::env::var("CODEX_STATUS_SIM_COOPERATIVE").as_deref() == Ok("1") {
                        app = app.route("/sim/run", axum::routing::post(sim_run_handler));
                    }
                }
                let app = app.with_state(ctx.clone());
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

async fn sim_clock_handler(
    axum::extract::State(ctx): axum::extract::State<Arc<AppCtx>>,
    headers: axum::http::HeaderMap,
    axum::Json(command): axum::Json<Value>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let Ok(token) = std::env::var("CODEX_STATUS_BRIDGE_SIM_CONTROL_TOKEN") else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    if headers.get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok()) != Some(format!("Bearer {token}").as_str()) {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(mac) = command["mac"].as_str() else {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(_barrier) = ctx.sim_control.try_lock() else {
        return (axum::http::StatusCode::CONFLICT,
            axum::Json(json!({"state":"in_flight"}))).into_response();
    };
    let op = command["op"].as_str().unwrap_or("");
    if op == "step" && std::env::var("CODEX_STATUS_SIM_COOPERATIVE").as_deref() != Ok("1") {
        return (axum::http::StatusCode::CONFLICT,
            axum::Json(json!({"error":"cooperative_mode_required"}))).into_response();
    }
    let result = match op {
        "set" => match (command["monotonic_ms"].as_u64(),
            command["wall_ms"].as_i64(),command["rate_ppm"].as_u64()) {
            (Some(monotonic),Some(wall),Some(rate)) =>
                bridge_core::device_clock::configure(mac,monotonic,wall,rate),
            _ => Err(anyhow::anyhow!("invalid clock settings")),
        },
        "rate" => command["rate_ppm"].as_u64()
            .ok_or_else(|| anyhow::anyhow!("rate_ppm required"))
            .and_then(|rate| bridge_core::device_clock::change_rate(mac,rate)),
        "step" => command["delta_ms"].as_u64()
            .ok_or_else(|| anyhow::anyhow!("delta_ms required"))
            .and_then(|delta| bridge_core::device_clock::step(mac,delta)),
        "wall" => command["wall_ms"].as_i64()
            .ok_or_else(|| anyhow::anyhow!("wall_ms required"))
            .and_then(|wall| bridge_core::device_clock::set_wall(mac,wall)),
        "get" => bridge_core::device_clock::snapshot(mac)
            .ok_or_else(|| anyhow::anyhow!("no experiment clock")),
        "clear" => {
            bridge_core::device_clock::clear(mac);
            Ok(json!({"mac":mac,"cleared":true}))
        },
        _ => Err(anyhow::anyhow!("invalid clock operation")),
    };
    match result {
        Ok(value) => {
            if op != "get" {
                let path = bridge_core::paths::data_root().join("sim-bridge-clocks.json");
                let saved = bridge_core::device_clock::export()
                    .and_then(|state| Ok(serde_json::to_vec(&state)?))
                    .and_then(|bytes| bridge_core::platform::store::atomic_write(&path, &bytes));
                if let Err(error) = saved {
                    return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        axum::Json(json!({"error":format!("clock storage failed: {error}")})))
                        .into_response();
                }
            }
            axum::Json(value).into_response()
        },
        Err(error) => (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(json!({"error":error.to_string()}))).into_response(),
    }
}

async fn sim_run_handler(
    axum::extract::State(ctx): axum::extract::State<Arc<AppCtx>>,
    headers: axum::http::HeaderMap,
    axum::Json(command): axum::Json<Value>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let Ok(token) = std::env::var("CODEX_STATUS_BRIDGE_SIM_CONTROL_TOKEN") else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    if headers.get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok()) != Some(format!("Bearer {token}").as_str()) {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(mac) = command["mac"].as_str()
        .and_then(bridge_core::platform::model::DeviceIdentity::normalized_mac) else {
        return axum::http::StatusCode::BAD_REQUEST.into_response();
    };
    if bridge_core::device_clock::snapshot(&mac).is_none() {
        return axum::http::StatusCode::CONFLICT.into_response();
    }
    let _barrier = ctx.sim_control.lock().await;
    let outcome = match command["kind"].as_str().unwrap_or("") {
        "ble" => match platform::ble_cycle(&ctx, &[mac.clone()]).await {
            Ok(platform::BleOpportunity::NoDevice) => json!({"state":"idle","contact":false}),
            Ok(platform::BleOpportunity::Connected {result, ..}) =>
                json!({"state":"idle","contact":true,"result":result}),
            Err(error) => json!({"state":"idle","error":error}),
        },
        "http" => {
            platform::cycle(&ctx,&mac,true,true).await;
            json!({"state":"idle","cycle":"complete"})
        },
        _ => return axum::http::StatusCode::BAD_REQUEST.into_response(),
    };
    axum::Json(json!({"mac":mac,"outcome":outcome,
        "clock":bridge_core::device_clock::snapshot(&mac),
        "device":ctx.platform.device_get(&mac)})).into_response()
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
    endpoint: &str,
    target_mac: &str,
    token: &str,
    force: bool,
    release: bool,
) -> Result<(reqwest::StatusCode, String), ClaimError> {
    let bridge_id = ctx.bridge_id.clone();
    let name = ctx.bridge_name.lock().unwrap().clone();
    let host = lan_ip();
    let port = ctx.config.port;
    post_claim_to_endpoint(
        endpoint,
        target_mac,
        token,
        &ctx.config.token,
        &bridge_id,
        &name,
        &host,
        port,
        force,
        release,
    )
    .await
}

async fn post_claim_to_endpoint(
    endpoint: &str,
    target_mac: &str,
    claim_token: &str,
    device_token: &str,
    bridge_id: &str,
    name: &str,
    host: &str,
    port: u16,
    force: bool,
    release: bool,
) -> Result<(reqwest::StatusCode, String), ClaimError> {
    let started = std::time::Instant::now();
    let expected_mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(target_mac)
        .ok_or_else(|| ClaimError::Other("invalid target MAC; refusing claim".into()))?;
    tracing::info!(event = "preflight", device_mac = %expected_mac, operation = "/status.json",
        "claim identity preflight");
    let endpoint = endpoint.to_string();
    let status_endpoint = endpoint.clone();
    let status = tokio::task::spawn_blocking(move || {
        bridge_core::device::fetch(&status_endpoint, Duration::from_secs(2))
    })
    .await
    .map_err(|e| ClaimError::Other(format!("device identity check failed: {e}")))?;
    let actual_mac = status.ok().and_then(|status| {
        status
            .get("MAC")
            .and_then(bridge_core::platform::model::DeviceIdentity::normalized_mac)
    });
    let actual_mac = match actual_mac {
        Some(mac) => mac,
        None => {
            tracing::info!(event = "preflight_fallback", device_mac = %expected_mac,
                operation = "/api/status", "using authenticated device status for claim identity");
            let device_endpoint = endpoint.clone();
            let device_token = device_token.to_owned();
            let device_status = tokio::task::spawn_blocking(move || {
                bridge_core::device_client::status(
                    &device_endpoint,
                    &device_token,
                    Duration::from_secs(2),
                )
            })
            .await
            .map_err(|e| ClaimError::Other(format!("device identity check failed: {e}")))?
            .map_err(|e| {
                tracing::warn!(event = "preflight_result", device_mac = %expected_mac,
                    elapsed_ms = started.elapsed().as_millis() as u64, error_category = "http_or_status",
                    "claim identity preflight failed");
                ClaimError::Other(format!("device identity check failed: {e}"))
            })?;
            device_status
                .get("device_mac")
                .and_then(Value::as_str)
                .and_then(bridge_core::platform::model::DeviceIdentity::normalized_mac)
                .ok_or_else(|| {
                    tracing::warn!(event = "preflight_result", device_mac = %expected_mac,
                        elapsed_ms = started.elapsed().as_millis() as u64, error_category = "identity_mismatch",
                        "claim identity preflight rejected");
                    ClaimError::Other("authenticated device status has no valid MAC; refusing claim".into())
                })?
        }
    };
    if actual_mac != expected_mac {
        tracing::warn!(event = "preflight_result", device_mac = %expected_mac,
            reported_mac = %actual_mac, elapsed_ms = started.elapsed().as_millis() as u64,
            error_category = "identity_mismatch", "claim identity preflight rejected");
        return Err(ClaimError::Other(format!(
            "device at {endpoint} reports MAC {actual_mac}, expected {expected_mac}; refusing claim"
        )));
    }

    let mut url = format!(
        "http://{endpoint}/claim?id={}&name={}&host={}&port={}&lease=300",
        url_encode(bridge_id),
        url_encode(name),
        url_encode(host),
        port
    );
    if force {
        url.push_str("&force=1");
    }
    if release {
        url.push_str("&release=1");
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ClaimError::Other(e.to_string()))?;
    tracing::info!(event = "send", device_mac = %expected_mac, operation = "/claim",
        "claim request");
    let claim_started = std::time::Instant::now();
    let resp = match client.post(&url).bearer_auth(claim_token).send().await {
        Ok(resp) => resp,
        Err(error) => {
            tracing::warn!(event = "result", device_mac = %expected_mac, operation = "/claim",
                elapsed_ms = claim_started.elapsed().as_millis() as u64,
                error_category = if error.is_timeout() { "timeout" } else if error.is_connect() { "connection" } else { "transport" },
                "claim request failed");
            return Err(ClaimError::Other(format!("device unreachable: {error}")));
        }
    };
    let code = resp.status();
    let body = resp.text().await.unwrap_or_default();
    tracing::info!(event = "result", device_mac = %expected_mac, operation = "/claim",
        status = code.as_u16(), elapsed_ms = claim_started.elapsed().as_millis() as u64,
        error_category = if code.is_success() { "none" } else if code == reqwest::StatusCode::UNAUTHORIZED { "identity_rejected" }
            else if code == reqwest::StatusCode::CONFLICT { "claim_rejected" } else { "http_non_success" },
        "claim request result");
    Ok((code, body))
}

fn parse_claim_response(
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
        return Err(ClaimError::Other(format!(
            "POST /claim -> HTTP {code} {body}"
        )));
    }
    Ok(doc.get("owner").cloned().filter(|o| !o.is_null()))
}

fn claim_endpoint_for_mac(ctx: &AppCtx, mac: &str) -> Result<String, ClaimError> {
    let target_mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(mac)
        .ok_or_else(|| ClaimError::Other("invalid target MAC; refusing claim".into()))?;
    if let Some(endpoint) = platform::service(ctx)
        .device_get(&target_mac)
        .and_then(|device| device["ip"].as_str().map(str::to_owned))
        .map(|ip| ip.trim().to_owned())
        .filter(|ip| !ip.is_empty() && ip != "0.0.0.0")
    {
        return Ok(endpoint);
    }
    // Fall back to this MAC's own runtime record — never to another device's IP.
    if let Some(facts) = device_facts_for(ctx, &target_mac) {
        let endpoint = facts.endpoint();
        if !endpoint.is_empty() && endpoint != "0.0.0.0" {
            return Ok(endpoint);
        }
    }
    Err(ClaimError::Other(format!(
        "no IP is known for target device {target_mac}; refusing claim"
    )))
}

async fn post_claim_for_mac(
    ctx: &AppCtx,
    force: bool,
    release: bool,
    mac: &str,
) -> Result<Option<Value>, ClaimError> {
    let endpoint = claim_endpoint_for_mac(ctx, mac)?;
    let cached = bridge_mcp::load_device_token(&mcp_config(ctx), mac);
    let Some(token) = cached else {
        return Err(ClaimError::Unauthorized);
    };
    let (code, body) = post_claim_once(ctx, &endpoint, mac, &token, force, release).await?;
    parse_claim_response(code, body)
}

/// Explicit user action: on a rejected/missing token, fetch a fresh one over
/// the bonded BLE link (needs a device BLE session: single BOOT click) once.
/// The token is fetched **for this MAC** and never copied from another device.
async fn post_claim_explicit_for_mac(
    ctx: &AppCtx,
    force: bool,
    release: bool,
    mac: &str,
) -> Result<Option<Value>, ClaimError> {
    let Some(mac) = bridge_core::platform::model::DeviceIdentity::normalized_mac(mac) else {
        return Err(ClaimError::Other(
            "device MAC is not confirmed; refusing claim".into(),
        ));
    };
    match post_claim_for_mac(ctx, force, release, &mac).await {
        Err(ClaimError::Unauthorized) => {
            tracing::info!("claim token missing/rejected; requesting a fresh one over BLE");
            let cfg = mcp_config(ctx);
            let fresh = bridge_mcp::fetch_device_token(&cfg, &mac)
                .await
                .map_err(|e| {
                    ClaimError::Other(format!(
                        "device token unavailable ({e}); click BOOT to open the BLE session"
                    ))
                })?;
            let endpoint = claim_endpoint_for_mac(ctx, &mac)?;
            let (code, body) =
                post_claim_once(ctx, &endpoint, &mac, &fresh, force, release).await?;
            parse_claim_response(code, body)
        }
        other => other,
    }
}

enum Occupancy {
    Owned,
    Yielded,
    Other(Value),
    Failed(String),
}

async fn device_occupancy_gate(ctx: &AppCtx, mac: &str) -> Occupancy {
    let Some(mac) = bridge_core::platform::model::DeviceIdentity::normalized_mac(mac) else {
        return Occupancy::Failed("invalid target MAC; refusing claim".into());
    };
    let decision = {
        let cache = ctx.device_status_cache.lock().unwrap();
        device_occupancy_decision(&mac, cache.get(&mac), &ctx.bridge_id, device_now_secs(&mac))
    };
    match decision {
        DeviceOccupancyDecision::Offline => Occupancy::Failed(
            "target has no authenticated online status within the last 30 seconds".into(),
        ),
        DeviceOccupancyDecision::Yielded => Occupancy::Yielded,
        DeviceOccupancyDecision::Other(owner) => Occupancy::Other(owner),
        DeviceOccupancyDecision::Owned { renew: false } => Occupancy::Owned,
        DeviceOccupancyDecision::Owned { renew: true } | DeviceOccupancyDecision::Claim => {
            match post_claim_for_mac(ctx, false, false, &mac).await {
                Ok(owner) => {
                    let now = device_now_secs(&mac);
                    update_device_claim_cache(
                        &mut ctx.device_status_cache.lock().unwrap(),
                        &mac,
                        owner,
                        now,
                        false,
                    );
                    Occupancy::Owned
                }
                Err(ClaimError::Occupied(owner)) => {
                    let now = device_now_secs(&mac);
                    update_device_owner_cache(
                        &mut ctx.device_status_cache.lock().unwrap(),
                        &mac,
                        owner.clone(),
                        now,
                    );
                    Occupancy::Other(owner)
                }
                Err(error) => Occupancy::Failed(error.to_string()),
            }
        }
    }
}

/// One device's runtime identity snapshot, shared by the panel and MCP.
///
/// Built from a cloned record, so no registry lock is held while the caller
/// still needs other locks.
fn device_facts_json(facts: &DeviceFacts) -> Value {
    json!({
        "name": facts.display_name(),
        "mac": facts.mac,
        "ip": facts.ip,
        "discover": facts.discover.as_ref().map(|d| json!({"via": d.via, "at": d.at})),
        "yielded": facts.yielded,
        "owner": facts.owner.as_ref().and_then(|c| c.owner.clone()),
        "owner_known": facts.owner.is_some(),
        "owner_observed_at": facts.owner.as_ref().map(|c| c.observed_at),
        "owner_source": facts.owner.as_ref().map(|_| "public_status"),
        "note": facts.note,
    })
}

/// Identity snapshot of the device an operation resolves to; `None` when the
/// bridge has no registered device at all.
fn device_identity_json(ctx: &AppCtx) -> Option<Value> {
    device_facts(ctx).map(|facts| device_facts_json(&facts))
}

fn device_identity_for_mac(ctx: &AppCtx, mac: &str) -> Result<Value, String> {
    ensure_runtime_record(ctx, mac)
        .map(|facts| device_facts_json(&facts))
        .ok_or_else(|| format!("device {mac} has no runtime record"))
}

async fn discover_arp(ctx: &AppCtx, mac: &str) -> Result<Value, String> {
    let mac = normalize_mac(mac);
    if mac.len() != 12 {
        return Err("device MAC unknown; click BOOT and use via=ble once".to_string());
    }
    let local = lan_ip();
    let target = mac.clone();
    let found = tokio::task::spawn_blocking(move || {
        arp_scan_for_mac(&local, &target, Duration::from_secs(30))
    })
    .await
    .map_err(|e| e.to_string())?;
    match found {
        Some(ip) => {
            observe_device(ctx, &mac, None, None, "arp");
            set_device_ip(ctx, &mac, &ip, "arp");
            device_identity_for_mac(ctx, &mac)
        }
        None => Err(format!(
            "ARP scan found no host with MAC {mac} on the local /24"
        )),
    }
}

/// BLE discovery: the info read is authenticated, so the address lands in the
/// record of the MAC the device itself reported. When the caller named a MAC,
/// the answer must be that device.
async fn discover_ble(ctx: &AppCtx, expected_mac: Option<&str>) -> Result<Value, String> {
    let adapter = Pusher::adapter().await.map_err(|e| e.to_string())?;
    let info = Pusher::read_device_info(&adapter, "CodexStatus-", 30000)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let reported = info
        .get("mac")
        .and_then(|v| v.as_str())
        .map(normalize_mac)
        .filter(|mac| mac.len() == 12)
        .ok_or_else(|| "BLE info reports no valid MAC".to_string())?;
    if let Some(expected) = expected_mac {
        if normalize_mac(expected) != reported {
            return Err(format!(
                "BLE device reports MAC {reported}, expected {expected}; refusing to write another device's record"
            ));
        }
    }
    adopt_ble_info(ctx, &info)?;
    let mut out = device_identity_for_mac(ctx, &reported)?;
    // Raw info (includes the firmware mac/ip/http_port) for panel/MCP diagnostics.
    out["ble_info"] = info;
    Ok(out)
}

/// Explicit `device_discover` action: auto (HTTP, then ARP), arp, or ble.
///
/// Always operates on one MAC: the requested one, else the selected device.
async fn discover_device(
    ctx: &AppCtx,
    via: &str,
    requested_mac: Option<&str>,
) -> Result<Value, String> {
    match via {
        "ble" => {
            let expected = requested_mac
                .map(str::trim)
                .filter(|mac| !mac.is_empty())
                .map(normalize_mac);
            return discover_ble(ctx, expected.as_deref()).await;
        }
        _ => {}
    }
    let mac = resolve_discovery_mac(ctx, requested_mac)?;
    observe_device(ctx, &mac, None, None, "manual");
    match via {
        "arp" => discover_arp(ctx, &mac).await,
        "auto" | "" => {
            let Some(facts) = device_facts_for(ctx, &mac) else {
                return Err(format!("device {mac} has no runtime record"));
            };
            let ip = facts.endpoint();
            let fetch_ip = ip.clone();
            let status = tokio::task::spawn_blocking(move || {
                bridge_core::device::fetch(&fetch_ip, Duration::from_secs(2))
            })
            .await;
            match status {
                Ok(Ok(status)) => {
                    let reported = status
                        .raw
                        .as_ref()
                        .and_then(|raw| raw.get("mac"))
                        .and_then(Value::as_str)
                        .map(normalize_mac)
                        .unwrap_or_default();
                    if reported.is_empty() {
                        set_device_ip(ctx, &mac, &ip, "http");
                        return device_identity_for_mac(ctx, &mac);
                    }
                    if reported != mac {
                        // Never adopt another device's address into this record.
                        return Err(format!(
                            "device at {ip} reports MAC {reported}, expected {mac}"
                        ));
                    }
                    set_device_ip(ctx, &mac, &ip, "http");
                    device_identity_for_mac(ctx, &mac)
                }
                _ => {
                    if device_facts_for(ctx, &mac).is_some() {
                        discover_arp(ctx, &mac).await
                    } else {
                        Err(
                            "device unreachable and MAC unknown; click BOOT, then use via=ble"
                                .to_string(),
                        )
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
            let mac = resolve_target_mac(ctx, args.get("mac").and_then(Value::as_str))?;
            ensure_runtime_record(ctx, &mac)
                .ok_or_else(|| format!("device {mac} has no runtime record"))?;
            let selected = selected_mac(ctx).as_deref() == Some(mac.as_str());
            if platform::service(ctx).device_get(&mac).is_some() {
                platform::service(ctx).device_rename(&mac, &name).map_err(|e| e.to_string())?;
            }
            {
                let mut registry = devices(ctx);
                let device = registry
                    .get_mut(&mac)
                    .ok_or_else(|| format!("device {mac} is not registered"))?;
                device.name = name.clone();
            }
            // Only the selected device's triple is persisted; a rename of
            // another registered device stays in its own record.
            if selected {
                save_identity(ctx);
            }
            tracing::info!("device {mac} renamed to {name}");
            Ok(format!("device {mac} name -> {name}"))
        }
        "device_owner" => Ok(device_identity_json(ctx)
            .unwrap_or(Value::Null)
            .to_string()),
        "device_discover" => {
            let via = args.get("via").and_then(|v| v.as_str()).unwrap_or("auto");
            let result =
                discover_device(ctx, via, args.get("mac").and_then(Value::as_str)).await?;
            Ok(result.to_string())
        }
        "device_claim" => {
            let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
            let mac = resolve_target_mac(ctx, args.get("mac").and_then(Value::as_str))?;
            set_yielded(ctx, &mac, false);
            // Explicit user action: on a rejected/missing token the explicit
            // path refreshes it over BLE (needs a device BLE session).
            let owner = post_claim_explicit_for_mac(ctx, force, false, &mac)
                .await
                .map_err(claim_error_text)?;
            update_device_claim_cache(
                &mut ctx.device_status_cache.lock().unwrap(),
                &mac,
                owner.clone(),
                device_now_secs(&mac),
                false,
            );
            Ok(json!({"owner": owner, "mac": mac, "yielded": false}).to_string())
        }
        "device_release" => {
            let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
            let mac = resolve_target_mac(ctx, args.get("mac").and_then(Value::as_str))?;
            post_claim_explicit_for_mac(ctx, force, true, &mac)
                .await
                .map_err(claim_error_text)?;
            set_yielded(ctx, &mac, true);
            update_device_claim_cache(
                &mut ctx.device_status_cache.lock().unwrap(),
                &mac,
                None,
                device_now_secs(&mac),
                true,
            );
            Ok(format!(
                "device {mac} released; auto-claim paused until device_claim"
            ))
        }
        other => Err(format!("unknown device tool: {other}")),
    }
}

#[tauri::command]
async fn rename_device(
    state: State<'_, Arc<AppCtx>>,
    name: String,
    mac: Option<String>,
) -> Result<Value, String> {
    device_tool(&state, "device_rename", &json!({"name": name, "mac": mac})).await?;
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    device_identity_for_mac(&state, &mac)
}

#[tauri::command]
async fn device_discover(
    state: State<'_, Arc<AppCtx>>,
    via: Option<String>,
    mac: Option<String>,
) -> Result<Value, String> {
    discover_device(&state, via.as_deref().unwrap_or("auto"), mac.as_deref()).await
}

#[tauri::command]
async fn device_owner(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    device_identity_json(&state).ok_or_else(|| "no device registered yet".to_string())
}

#[tauri::command]
async fn claim_device(
    state: State<'_, Arc<AppCtx>>,
    force: Option<bool>,
    mac: Option<String>,
) -> Result<Value, String> {
    let text = device_tool(
        &state,
        "device_claim",
        &json!({"force": force.unwrap_or(false), "mac": mac}),
    )
    .await?;
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    Ok(json!({"result": text, "device": device_identity_for_mac(&state, &mac)?}))
}

#[tauri::command]
async fn release_device(
    state: State<'_, Arc<AppCtx>>,
    mac: Option<String>,
) -> Result<Value, String> {
    let text = device_tool(&state, "device_release", &json!({"mac": mac})).await?;
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    Ok(json!({"result": text, "device": device_identity_for_mac(&state, &mac)?}))
}

// ---------------------------------------------------------------------------
// platform commands: the four UI pages call exactly the same service the MCP
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
async fn platform_device_view(state: State<'_, Arc<AppCtx>>, mac: Option<String>) -> Result<Value, String> {
    platform::device_view(&state, mac.as_deref())
}

#[tauri::command]
async fn platform_device_detail(state: State<'_, Arc<AppCtx>>, mac: String, section: String) -> Result<Value, String> {
    platform::device_detail(&state, &mac, &section)
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
    font_ids: Option<std::collections::BTreeMap<String, String>>,
) -> Result<Value, String> {
    platform::template_preview(&state, id.as_deref(), json.as_ref(), usage.as_deref(), font_ids.as_ref())
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
async fn platform_data_sync_save(
    state: State<'_, Arc<AppCtx>>,
    mac: String,
    enabled: bool,
) -> Result<Value, String> {
    platform::data_sync_save(&state, &mac, enabled)
}

#[tauri::command]
async fn platform_family_profiles(
    state: State<'_, Arc<AppCtx>>,
    render_target: Option<String>,
) -> Result<Value, String> {
    Ok(platform::family_profiles(&state, render_target.as_deref()))
}

#[tauri::command]
async fn platform_family_profile_save(
    state: State<'_, Arc<AppCtx>>,
    profile: Value,
) -> Result<Value, String> {
    let parsed: bridge_core::platform::model::FamilyProfile =
        serde_json::from_value(profile).map_err(|e| e.to_string())?;
    platform::family_profile_save(&state, parsed)
}

#[tauri::command]
async fn platform_family_profile_delete(
    state: State<'_, Arc<AppCtx>>,
    render_target: String,
    id: String,
) -> Result<Value, String> {
    platform::family_profile_delete(&state, &render_target, &id)
}

#[tauri::command]
async fn platform_family_profile_copy_from_device(
    state: State<'_, Arc<AppCtx>>,
    mac: String,
    id: String,
    name: String,
) -> Result<Value, String> {
    platform::family_profile_copy_from_device(&state, &mac, &id, &name)
}

#[tauri::command]
async fn platform_publish(
    state: State<'_, Arc<AppCtx>>,
    mac: Option<String>,
    expected_target_id: Option<String>,
) -> Result<Value, String> {
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    platform::publish(&state, &mac, expected_target_id.as_deref()).await
}

#[tauri::command]
async fn platform_publish_preview(
    state: State<'_, Arc<AppCtx>>,
    mac: Option<String>,
) -> Result<Value, String> {
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    platform::publish_preview(&state, &mac)
}

#[tauri::command]
async fn platform_font_list(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    platform::font_list(&state)
}

#[tauri::command]
async fn platform_font_import(
    state: State<'_, Arc<AppCtx>>,
    path: String,
) -> Result<Value, String> {
    platform::font_import(&state, &path)
}

#[tauri::command]
async fn platform_publish_cancel(
    state: State<'_, Arc<AppCtx>>,
    mac: Option<String>,
) -> Result<Value, String> {
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    Ok(platform::job_cancel(&state, &mac))
}

#[tauri::command]
async fn platform_activate(
    state: State<'_, Arc<AppCtx>>,
    id: String,
    mac: Option<String>,
) -> Result<Value, String> {
    let mac = resolve_target_mac(&state, mac.as_deref())?;
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
async fn platform_power(state: State<'_, Arc<AppCtx>>, mac: Option<String>) -> Result<Value, String> {
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    Ok(platform::power_view(&state, &mac))
}

#[tauri::command]
async fn platform_plan(
    state: State<'_, Arc<AppCtx>>,
    mode: String,
    mac: Option<String>,
) -> Result<Value, String> {
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    match mode.as_str() {
        "light" => Ok(platform::request_light(&state, &mac).await),
        "sleep" => {
            let text = platform::tool(&state, "power_plan", &json!({"mode": "sleep", "mac": mac}))
                .await?;
            Ok(serde_json::from_str(&text).unwrap_or_else(|_| json!({"result": text})))
        }
        other => Err(format!("mode must be light|sleep (got {other})")),
    }
}

#[tauri::command]
async fn platform_status_refresh(
    state: State<'_, Arc<AppCtx>>,
    mac: Option<String>,
) -> Result<Value, String> {
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    Ok(platform::refresh_status(&state, &mac).await)
}

#[tauri::command]
async fn platform_recovery(
    state: State<'_, Arc<AppCtx>>,
    mac: Option<String>,
    digest: Value,
) -> Result<Value, String> {
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    platform::recovery(&state, &mac, &digest)
}

/// Last `/status.json` read for one device (the device page's cache).
#[tauri::command]
async fn get_device_status(
    state: State<'_, Arc<AppCtx>>,
    mac: Option<String>,
) -> Result<Value, String> {
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    let facts = ensure_runtime_record(&state, &mac)
        .ok_or_else(|| format!("device {mac} has no runtime record"))?;
    let device = device_facts_json(&facts);
    let registered = platform::service(&state).device_get(&mac).unwrap_or(Value::Null);
    let last = &registered["last_authenticated"];
    let observed_at = last["observed_at"].as_u64();
    let age_s = observed_at.map(|at| (device_now_secs(&mac) as u64).saturating_sub(at));
    let clock_anomaly = observed_at.is_some_and(|at| (device_now_secs(&mac) as u64) < at);
    let recent = !clock_anomaly && age_s.is_some_and(|age| age <= 30)
        && state.device_status_cache.lock().unwrap().get(&mac)
            .is_some_and(|v| v.online && v.fetched_at <= device_now_secs(&mac)
                && device_now_secs(&mac).saturating_sub(v.fetched_at) <= 30);
    let contact_at = registered["last_authenticated_contact_at"].as_u64();
    let recent_contact = contact_at.is_some_and(|at| at <= device_now_secs(&mac) as u64
        && (device_now_secs(&mac) as u64).saturating_sub(at) <= 30);
    let blocked = (registered["last_status_attempt"]["outcome"] == "blocked"
        || registered["last_status_attempt"]["outcome"] == "mac_mismatch")
        && registered["last_status_attempt"]["at"].as_u64().unwrap_or(0)
            >= contact_at.unwrap_or(0);
    let reachability = if blocked { "blocked" }
        else if recent || recent_contact { "recently_authenticated" }
        else if last["body"]["power"]["mode"] == "sleep" { "expected_sleep" }
        else { "unknown" };
    match facts.status.clone() {
        Some(cached) => {
            let map: serde_json::Map<String, Value> = cached
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            Ok(json!({
                "online": recent,
                "public_online": cached.online && cached.fetched_at <= device_now_secs(&mac)
                    && device_now_secs(&mac).saturating_sub(cached.fetched_at) <= 30,
                "public_sampled_at": cached.fetched_at,
                "ip": cached.ip,
                "fetched_at": observed_at,
                "last_success": last,
                "last_authenticated_contact_at": registered["last_authenticated_contact_at"],
                "last_authenticated_transport": registered["last_authenticated_transport"],
                "last_attempt": registered["last_status_attempt"],
                "age_s": age_s,
                "clock_anomaly": clock_anomaly,
                "reachability": reachability,
                "record_observed": registered["record_observed"],
                "fields": map,
                "owner": cached.owner,
                "device": device,
            }))
        }
        None => Ok(json!({
            "online": false,
            "public_online": false,
            "ip": facts.endpoint(),
            "pending": true,
            "last_success": last,
            "last_authenticated_contact_at": registered["last_authenticated_contact_at"],
            "last_authenticated_transport": registered["last_authenticated_transport"],
            "last_attempt": registered["last_status_attempt"],
            "age_s": age_s,
            "clock_anomaly": clock_anomaly,
            "reachability": reachability,
            "record_observed": registered["record_observed"],
            "fields": {},
            "owner": Value::Null,
            "device": device,
        })),
    }
}

/// Read `GET /pmstats` from one device after checking the endpoint's public
/// status MAC. Cached for 10 s because reads briefly wake light-sleep devices.
#[tauri::command]
async fn get_pmstats(state: State<'_, Arc<AppCtx>>, mac: Option<String>) -> Result<Value, String> {
    const TTL_SECS: i64 = 10;
    let mac = resolve_target_mac(&state, mac.as_deref())?;
    let facts = ensure_runtime_record(&state, &mac)
        .ok_or_else(|| format!("device {mac} has no runtime record"))?;
    let now = device_now_secs(&mac);
    if let Some(cached) = facts
        .pmstats
        .as_ref()
        .filter(|cached| now >= cached.last_attempt_at && now - cached.last_attempt_at < TTL_SECS)
    {
        return Ok(json!({
            "online": cached.last_error.is_none(),
            "device_mac": mac,
            "ip": cached.ip,
            "fetched_at": cached.fetched_at,
            "last_attempt_at": cached.last_attempt_at,
            "last_error": cached.last_error,
            "text": cached.text,
        }));
    }
    let ip = facts.endpoint();
    let fetch_ip = ip.clone();
    let expected_mac = mac.clone();
    let result = tokio::task::spawn_blocking(move || {
        let status = bridge_core::device::fetch(&fetch_ip, Duration::from_secs(5))?;
        let actual_mac = status.raw.as_ref().and_then(|raw| raw["mac"].as_str())
            .and_then(bridge_core::platform::model::DeviceIdentity::normalized_mac);
        if actual_mac.as_deref() != Some(expected_mac.as_str()) {
            return Err(anyhow::anyhow!("/pmstats endpoint MAC does not match {expected_mac}"));
        }
        bridge_core::device::fetch_pmstats(&fetch_ip, Duration::from_secs(5))
    })
    .await;
    let (text, error) = match result {
        Ok(Ok(text)) => (Some(text), None),
        Ok(Err(e)) => (None, Some(e.to_string())),
        Err(e) => (None, Some(e.to_string())),
    };
    let attempted_at = device_now_secs(&mac);
    let mut devices = devices(&state);
    let previous = devices.get(&mac).and_then(|device| device.pmstats.as_ref());
    let cached = CachedPmStats {
        fetched_at: if text.is_some() { attempted_at } else { previous.map_or(0, |p| p.fetched_at) },
        last_attempt_at: attempted_at,
        last_error: error,
        ip: if text.is_some() { ip.clone() } else { previous.map_or(ip.clone(), |p| p.ip.clone()) },
        text: text.unwrap_or_else(|| previous.map_or(String::new(), |p| p.text.clone())),
    };
    if let Some(device) = devices.get_mut(&mac) {
        device.pmstats = Some(cached.clone());
    }
    Ok(json!({
        "online": cached.last_error.is_none(),
        "device_mac": mac,
        "ip": cached.ip,
        "fetched_at": cached.fetched_at,
        "last_attempt_at": cached.last_attempt_at,
        "last_error": cached.last_error,
        "text": cached.text,
    }))
}

#[tauri::command]
async fn get_mcp_info(state: State<'_, Arc<AppCtx>>) -> Result<Value, String> {
    let port = *state.mcp_port.lock().unwrap();
    let error = state.mcp_error.lock().unwrap().clone();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let tools = "bridge_status / template_render / firmware_ota / pm_stats / device_* / platform_*";

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
        let (width, height) = bridge_render::canvas_size(&text).ok_or("unsupported canvas")?;
        (text, width, height)
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
    let mac = selected_mac(&state).ok_or("no device selected")?;
    if platform::service(&state).device_get(&mac).is_none() {
        return Err("device is not registered for the current protocol".into());
    }
    platform::cycle(&state, &mac, true, true).await;
    Ok(())
}

/// Tray action: ask the registered device coordinator for one immediate cycle.
fn request_sync(ctx: &Arc<AppCtx>) {
    let Some(mac) = selected_mac(ctx) else { return };
    if platform::service(ctx).device_get(&mac).is_none() { return }
    let ctx = ctx.clone();
    tokio::spawn(async move { platform::cycle(&ctx, &mac, true, true).await; });
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
            let Some(name) = source.file_name() else {
                continue;
            };
            let target = config.templates.join(name);
            if !target.exists() {
                let _ = std::fs::copy(&source, &target);
            }
        }
    }
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

/// UDP announce listener (docs/power-state.md §9). Announcements are per-MAC
/// address hints for registered devices that are verified over HTTP.
async fn udp_listen(ctx: Arc<AppCtx>) {
    // Device announces target the fixed UDP port. Only the default bridge
    // owns that socket; named instances use their exact MAC HTTP/BLE routes.
    if ctx.instance.name.is_some() {
        tracing::info!("UDP announce listener disabled for named instance");
        return;
    }
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
    let mut last_revalidation = HashMap::<String, i64>::new();
    let mut device_udp_sampler = DeviceUdpTraceSampler::default();
    loop {
        let Ok((n, from)) = socket.recv_from(&mut buf).await else {
            continue;
        };
        let Ok(doc) = serde_json::from_slice::<Value>(&buf[..n]) else {
            if let Some(suppressed) = device_udp_sampler.record(None, "malformed_json", now_secs()) {
                tracing::debug!(event = "udp_discard", category = "malformed_json", suppressed,
                    "unrelated UDP traffic sampled");
            }
            continue;
        };
        if doc.get("magic").and_then(|v| v.as_str()) != Some("codex-status") {
            if let Some(suppressed) = device_udp_sampler.record(None, "unrelated_magic", now_secs()) {
                tracing::debug!(event = "udp_discard", category = "unrelated_magic", suppressed,
                    "unrelated UDP traffic sampled");
            }
            continue;
        }
        let raw_mac = doc.get("mac").and_then(Value::as_str).unwrap_or("");
        let devices = ctx.platform.devices();
        if bridge_core::platform::model::DeviceIdentity::normalized_mac(raw_mac).is_none() {
            if let Some(suppressed) = device_udp_sampler.record(None, "format_invalid", now_secs()) {
                tracing::debug!(event = "udp_discard", category = "format_invalid", suppressed,
                    "UDP announce with invalid identity sampled");
            }
            continue;
        }
        let hint = classify_device_udp_hint(&doc, from.ip(), &devices);
        let registered_device = hint.is_registered();
        if registered_device {
            let (mac, endpoint) = match hint {
                DeviceUdpHint::Changed { mac, endpoint } => {
                    if let Some(suppressed) =
                        device_udp_sampler.record(Some(&mac), "endpoint_changed", now_secs())
                    {
                        tracing::info!(event = "udp_accept", device_mac = %mac, source = %from,
                            endpoint = %endpoint, category = "endpoint_changed", suppressed,
                            "registered device UDP announce accepted");
                    }
                    (mac, endpoint)
                }
                DeviceUdpHint::Unchanged { mac } => {
                    if let Some(suppressed) =
                        device_udp_sampler.record(Some(&mac), "unchanged", now_secs())
                    {
                        tracing::trace!(event = "udp_duplicate", device_mac = %mac, source = %from,
                            suppressed, "registered device UDP announce unchanged");
                    }
                    continue;
                }
                DeviceUdpHint::Rejected {
                    mac,
                    category,
                    registered,
                } => {
                    let mac_for_log = if registered { mac.as_deref() } else { None };
                    if let Some(suppressed) =
                        device_udp_sampler.record(mac_for_log, category, now_secs())
                    {
                        if registered {
                            tracing::warn!(event = "udp_reject", device_mac = mac.as_deref().unwrap_or(""),
                                source = %from, category, suppressed,
                                "registered device UDP announce rejected");
                        } else {
                            tracing::debug!(event = "udp_reject", source = %from, category, suppressed,
                                "unregistered UDP announce ignored");
                        }
                    }
                    continue;
                }
            };
            if last_revalidation
                .get(&mac)
                .is_none_or(|last| device_now_secs(&mac).saturating_sub(*last) >= 10)
            {
                last_revalidation.insert(mac.clone(), device_now_secs(&mac));
                let ctx = ctx.clone();
                let mac_for_task = mac.clone();
                let endpoint_for_task = endpoint.clone();
                tokio::spawn(async move {
                    match platform::revalidate_registered_device_endpoint(
                        &ctx,
                        &mac_for_task,
                        &endpoint_for_task,
                    )
                    .await
                    {
                        Ok(true) => {
                            tracing::info!(event = "udp_verified", device_mac = %mac_for_task,
                                endpoint = %endpoint_for_task, "UDP endpoint verified and updated");
                        }
                        Ok(false) => {}
                        Err(error) => {
                            tracing::warn!(event = "udp_reject", device_mac = %mac_for_task,
                                source = %endpoint_for_task, category = safe_device_error_category(&error),
                                "registered device UDP endpoint verification failed")
                        }
                    }
                });
            }
            // The per-MAC BLE scheduler remains independent of UDP announces.
            continue;
        }
    }
}

enum DeviceUdpHint {
    Changed {
        mac: String,
        endpoint: String,
    },
    Unchanged {
        mac: String,
    },
    Rejected {
        mac: Option<String>,
        category: &'static str,
        registered: bool,
    },
}

impl DeviceUdpHint {
    fn is_registered(&self) -> bool {
        match self {
            Self::Changed { .. } | Self::Unchanged { .. } => true,
            Self::Rejected { registered, .. } => *registered,
        }
    }
}

fn classify_device_udp_hint(doc: &Value, source: std::net::IpAddr, devices: &[Value]) -> DeviceUdpHint {
    use bridge_core::platform::model::DeviceIdentity;

    if doc.get("magic").and_then(Value::as_str) != Some("codex-status") {
        return DeviceUdpHint::Rejected {
            mac: None,
            category: "format_invalid",
            registered: false,
        };
    }
    let Some(mac) = doc
        .get("mac")
        .and_then(Value::as_str)
        .and_then(DeviceIdentity::normalized_mac)
    else {
        return DeviceUdpHint::Rejected {
            mac: None,
            category: "format_invalid",
            registered: false,
        };
    };
    let Some(ip) = doc
        .get("ip")
        .and_then(Value::as_str)
        .and_then(|ip| ip.parse::<std::net::Ipv4Addr>().ok())
    else {
        let registered = is_registered_device_mac(&mac, devices);
        return DeviceUdpHint::Rejected {
            mac: Some(mac),
            category: "format_invalid",
            registered,
        };
    };
    if source != std::net::IpAddr::V4(ip) {
        let registered = is_registered_device_mac(&mac, devices);
        return DeviceUdpHint::Rejected {
            mac: Some(mac),
            category: "source_mismatch",
            registered,
        };
    }
    let port = if let Some(value) = doc.get("port") {
        let Some(port) = value
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .filter(|port| *port != 0)
        else {
            let registered = is_registered_device_mac(&mac, devices);
            return DeviceUdpHint::Rejected {
                mac: Some(mac),
                category: "format_invalid",
                registered,
            };
        };
        Some(port)
    } else {
        None
    };
    let registered_device = devices.iter().find(|device| {
        device["device_mac"]
            .as_str()
            .and_then(DeviceIdentity::normalized_mac)
            .as_deref()
            == Some(mac.as_str())
    });
    let Some(device) = registered_device else {
        return DeviceUdpHint::Rejected {
            mac: None,
            category: "identity_unregistered",
            registered: false,
        };
    };
    let Some(previous_endpoint) = device["ip"].as_str() else {
        return DeviceUdpHint::Rejected {
            mac: Some(mac),
            category: "identity_mismatch",
            registered: true,
        };
    };
    let endpoint = port
        .filter(|port| *port != 80)
        .map(|port| format!("{ip}:{port}"))
        .unwrap_or_else(|| ip.to_string());
    if previous_endpoint == endpoint {
        DeviceUdpHint::Unchanged { mac }
    } else {
        DeviceUdpHint::Changed { mac, endpoint }
    }
}

fn is_registered_device_mac(mac: &str, devices: &[Value]) -> bool {
    use bridge_core::platform::model::DeviceIdentity;
    devices.iter().any(|device| {
        device["device_mac"]
            .as_str()
            .and_then(DeviceIdentity::normalized_mac)
            .as_deref()
            == Some(mac)
    })
}

#[derive(Default)]
struct DeviceUdpTraceSampler {
    last_logged: HashMap<(String, &'static str), i64>,
    suppressed: HashMap<(String, &'static str), u64>,
}

impl DeviceUdpTraceSampler {
    fn record(&mut self, mac: Option<&str>, category: &'static str, now: i64) -> Option<u64> {
        let key = (mac.unwrap_or_default().to_owned(), category);
        if !self.last_logged.contains_key(&key) && self.last_logged.len() >= 256 {
            if let Some(oldest) = self
                .last_logged
                .iter()
                .min_by_key(|(_, last)| **last)
                .map(|(key, _)| key.clone())
            {
                self.last_logged.remove(&oldest);
                self.suppressed.remove(&oldest);
            }
        }
        if let Some(last) = self.last_logged.get(&key) {
            if now.saturating_sub(*last) < 10 {
                *self.suppressed.entry(key).or_default() += 1;
                return None;
            }
        }
        self.last_logged.insert(key.clone(), now);
        Some(self.suppressed.remove(&key).unwrap_or(0))
    }
}

fn safe_device_error_category(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("does not match") || text.contains("expected") || text.contains("identity") {
        "identity_mismatch"
    } else if text.contains("timeout") || text.contains("timed out") {
        "timeout"
    } else {
        "http_or_transport"
    }
}

#[cfg(test)]
mod device_udp_hint_tests {
    use super::{classify_device_udp_hint, DeviceUdpHint, DeviceUdpTraceSampler};
    use serde_json::json;
    use std::net::IpAddr;

    fn device(mac: &str, ip: &str) -> serde_json::Value {
        json!({"device_mac": mac, "ip": ip})
    }

    #[test]
    fn endpoint_hint_is_registered_per_mac_and_source_bound() {
        let mac_a = "AA:BB:CC:DD:EE:01";
        let mac_b = "AA:BB:CC:DD:EE:02";
        let devices = [device(mac_a, "192.168.1.10"), device(mac_b, "192.168.1.20")];
        let announce = json!({
            "magic": "codex-status",
            "mac": "aa-bb-cc-dd-ee-01",
            "ip": "192.168.1.11"
        });

        assert!(matches!(classify_device_udp_hint(&announce,
            "192.168.1.11".parse::<IpAddr>().unwrap(), &devices),
            DeviceUdpHint::Changed { mac, endpoint } if mac == "AABBCCDDEE01" && endpoint == "192.168.1.11"));
        assert_eq!(devices[1]["ip"], "192.168.1.20");
        assert!(matches!(
            classify_device_udp_hint(
                &announce,
                "192.168.1.12".parse::<IpAddr>().unwrap(),
                &devices
            ),
            DeviceUdpHint::Rejected {
                category: "source_mismatch",
                registered: true,
                ..
            }
        ));
    }

    #[test]
    fn unknown_mac_unchanged_endpoint_and_invalid_port_are_ignored() {
        let known = [device("AA:BB:CC:DD:EE:01", "192.168.1.10")];
        let unknown = json!({
            "magic": "codex-status",
            "mac": "AA:BB:CC:DD:EE:02",
            "ip": "192.168.1.11"
        });
        assert!(matches!(
            classify_device_udp_hint(&unknown, "192.168.1.11".parse::<IpAddr>().unwrap(), &known),
            DeviceUdpHint::Rejected {
                category: "identity_unregistered",
                registered: false,
                mac: None
            }
        ));

        let same = json!({
            "magic": "codex-status",
            "mac": "AA:BB:CC:DD:EE:01",
            "ip": "192.168.1.10"
        });
        assert!(
            matches!(classify_device_udp_hint(&same, "192.168.1.10".parse::<IpAddr>().unwrap(), &known),
            DeviceUdpHint::Unchanged { mac } if mac == "AABBCCDDEE01")
        );

        let same_with_port_80 = json!({
            "magic": "codex-status",
            "mac": "AA:BB:CC:DD:EE:01",
            "ip": "192.168.1.10",
            "port": 80
        });
        assert!(matches!(
            classify_device_udp_hint(
                &same_with_port_80,
                "192.168.1.10".parse::<IpAddr>().unwrap(),
                &known
            ),
            DeviceUdpHint::Unchanged { .. }
        ));

        for port in [json!(0), json!(65536), json!("80")] {
            let announce = json!({
                "magic": "codex-status",
                "mac": "AA:BB:CC:DD:EE:01",
                "ip": "192.168.1.11",
                "port": port
            });
            assert!(matches!(
                classify_device_udp_hint(&announce, "192.168.1.11".parse::<IpAddr>().unwrap(), &known),
                DeviceUdpHint::Rejected {
                    category: "format_invalid",
                    registered: true,
                    ..
                }
            ));
        }
    }

    #[test]
    fn udp_trace_classifies_identity_source_and_duplicate_without_side_effects() {
        let known = [device("AA:BB:CC:DD:EE:01", "192.168.1.10")];
        let source = "192.168.1.10".parse::<IpAddr>().unwrap();
        let base = json!({"magic":"codex-status", "mac":"AA:BB:CC:DD:EE:01", "ip":"192.168.1.10"});
        assert!(matches!(
            classify_device_udp_hint(&base, source, &known),
            DeviceUdpHint::Unchanged { .. }
        ));
        assert!(matches!(
            classify_device_udp_hint(&base, "192.168.1.11".parse().unwrap(), &known),
            DeviceUdpHint::Rejected {
                category: "source_mismatch",
                registered: true,
                ..
            }
        ));
        let unknown =
            json!({"magic":"codex-status", "mac":"AA:BB:CC:DD:EE:02", "ip":"192.168.1.10"});
        assert!(matches!(
            classify_device_udp_hint(&unknown, source, &known),
            DeviceUdpHint::Rejected {
                category: "identity_unregistered",
                registered: false,
                mac: None
            }
        ));
        let moved = json!({"magic":"codex-status", "mac":"AA:BB:CC:DD:EE:01", "ip":"192.168.1.12"});
        assert!(matches!(
            classify_device_udp_hint(&moved, "192.168.1.12".parse().unwrap(), &known),
            DeviceUdpHint::Changed { .. }
        ));
    }

    #[test]
    fn udp_trace_sampling_is_bounded_and_counts_suppressed_events() {
        let mut sampler = DeviceUdpTraceSampler::default();
        assert_eq!(
            sampler.record(Some("AABBCCDDEE01"), "source_mismatch", 100),
            Some(0)
        );
        assert_eq!(
            sampler.record(Some("AABBCCDDEE01"), "source_mismatch", 109),
            None
        );
        assert_eq!(
            sampler.record(Some("AABBCCDDEE01"), "source_mismatch", 110),
            Some(1)
        );
        assert_eq!(sampler.record(None, "identity_unregistered", 110), Some(0));
        assert_eq!(sampler.last_logged.len(), 2);
        for n in 0..300 {
            let mac = format!("AA:BB:CC:DD:{:02X}:{:02X}", n / 256, n % 256);
            let _ = sampler.record(Some(&mac), "endpoint_changed", 200 + n);
        }
        assert!(sampler.last_logged.len() <= 256);
    }
}

/// ARP fallback after repeated HTTP failures: rescan the local /24 for the
/// selected device's MAC and move that record's address when found (task-2).
/// The MAC comes from the record's own identity, so no other device can be
/// moved by this scan.
fn maybe_arp_fallback(ctx: &Arc<AppCtx>) {
    if ctx.arp_running.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(mac) = selected_mac(ctx) else {
        ctx.arp_running.store(false, Ordering::SeqCst);
        return;
    };
    let recently = device_facts_for(ctx, &mac)
        .and_then(|facts| facts.discover)
        .map(|d| d.via == "arp" && device_now_secs(&mac) - d.at < 60)
        .unwrap_or(false);
    if recently {
        ctx.arp_running.store(false, Ordering::SeqCst);
        return;
    }
    let ctx = ctx.clone();
    tracing::info!("device HTTP unreachable; ARP fallback scan for {mac}");
    tokio::spawn(async move {
        let local = lan_ip();
        let target = mac.clone();
        let found = tokio::task::spawn_blocking(move || {
            arp_scan_for_mac(&local, &target, Duration::from_secs(20))
        })
        .await
        .ok()
        .flatten();
        if let Some(ip) = found {
            set_device_ip(&ctx, &mac, &ip, "arp");
            note_contact(&ctx, &mac);
        }
        ctx.arp_running.store(false, Ordering::SeqCst);
    });
}

/// Refresh the selected device's plain-HTTP status cache every 10 s; the panel
/// reads that cache. The loop stays single-device (as before) but every read and
/// write is keyed by the MAC it was aimed at, so another device's record is
/// never touched; a registered device is polled over its authenticated
/// `/api/status` instead (`device_status_poll_loop`).
async fn device_cache_loop(ctx: Arc<AppCtx>) {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        if ctx.status.lock().unwrap().paused {
            continue;
        }
        let Some(mac) = selected_mac(&ctx) else {
            continue;
        };
        let Some(facts) = device_facts_for(&ctx, &mac) else {
            continue;
        };
        let ip = facts.endpoint();
        if ip.is_empty() || ip == "0.0.0.0" {
            continue;
        }
        refresh_http_status(&ctx, &mac, &ip).await;
    }
}

/// One plain-HTTP `/status.json` refresh for exactly this MAC.
async fn refresh_http_status(ctx: &Arc<AppCtx>, mac: &str, ip: &str) {
    let fetch_ip = ip.to_string();
    let result = tokio::task::spawn_blocking(move || {
        bridge_core::device::fetch(&fetch_ip, Duration::from_secs(3))
    })
    .await;
    let (mut online, fields, reported_mac, owner, raw) = match result {
        Ok(Ok(status)) => {
            let reported = status
                .raw
                .as_ref()
                .and_then(|raw| raw.get("mac"))
                .and_then(Value::as_str)
                .map(normalize_mac)
                .filter(|mac| mac.len() == 12);
            let owner = status
                .raw
                .as_ref()
                .and_then(|raw| raw.get("owner"))
                .cloned()
                .filter(|value| !value.is_null());
            (true, status.fields, reported, owner, status.raw)
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
    if let Some(reported) = reported_mac.as_deref() {
        if reported != mac {
            // The address now answers for a different device: never adopt it
            // into this MAC's record.
            tracing::warn!(
                "device status at {ip} reports MAC {reported}, expected {mac}; treating as offline"
            );
            online = false;
        }
    }
    if online {
        if let Some(raw) = raw.as_ref() {
            match platform::caps_from_status(raw) {
                Ok(caps) => {
                    platform::ensure_device(ctx, mac, ip, caps);
                }
                Err(e) => tracing::warn!("device capability contract rejected: {e}"),
            }
        }
    }
    if online {
        note_contact(ctx, mac);
        ctx.activity.note_contact("http");
        set_owner_cache(ctx, mac, owner.clone());
        // Surface another bridge's occupancy immediately (the push itself may
        // not run for minutes); the push gate clears the note again.
        if let Some(o) = owner.as_ref().filter(|o| owner_valid(o)) {
            if owner_id(o) != Some(ctx.bridge_id.as_str()) {
                set_device_note(ctx, mac, Some(format!("被 {} 占用", owner_line(o))));
            }
        }
    } else if note_failure(ctx, mac) >= 2 {
        // Two consecutive failures: try to relocate this MAC (never another).
        maybe_arp_fallback(ctx);
    }
    let previous = device_facts_for(ctx, mac).and_then(|facts| facts.status);
    let fields = if online {
        fields
    } else {
        previous
            .as_ref()
            .map(|c| c.fields.clone())
            .unwrap_or_default()
    };
    let owner = if online {
        owner
    } else {
        previous.as_ref().and_then(|c| c.owner.clone())
    };
    if let Some(device) = devices(ctx).get_mut(mac) {
        device.status = Some(CachedDevice {
            fetched_at: device_now_secs(&mac),
            online,
            ip: ip.to_string(),
            fields,
            owner,
        });
    }
}

/// Poll registered devices independently of the single-device cache.
/// This path is read-only and records each result under the target MAC.
async fn device_status_poll_loop(ctx: Arc<AppCtx>) {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        if ctx.status.lock().unwrap().paused {
            continue;
        }
        for device in platform::service(&ctx).devices() {
            if ctx.status.lock().unwrap().paused {
                break;
            }
            let Some(mac) = device["device_mac"]
                .as_str()
                .and_then(bridge_core::platform::model::DeviceIdentity::normalized_mac)
            else {
                continue;
            };
            // No inferred endpoint: the registered identity must carry an IP,
            // and the shared endpoint token must be present.
            if platform::device_link_for_mac(&ctx, &mac).is_none() {
                continue;
            }
            let pending = device["ota_job"]["state"] == "queued"
                || device["job"]["state"] == "waiting";
            if pending {
                platform::cycle(&ctx, &mac, true, true).await;
            } else {
                let _ = platform::refresh_status(&ctx, &mac).await;
            }
        }
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

    let addr = format!("0.0.0.0:{}", ctx.config.port)
        .parse()
        .expect("addr");
    {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if let Err(e) = serve(addr).await {
                tracing::error!("http server: {e}");
                let mut status = ctx.status.lock().unwrap();
                status.last_error = Some(format!("http: {e}"));
                status.last_error_at = Some(now_secs());
            }
        });
    }

    if std::env::var("CODEX_STATUS_SIM_COOPERATIVE").as_deref() == Ok("1") {
        return;
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
    if std::env::var("CODEX_STATUS_PLATFORM_STATUS_POLL").as_deref() != Ok("0") {
        let status_ctx = ctx.clone();
        tokio::spawn(async move { device_status_poll_loop(status_ctx).await });
    }

    // platform loop: feed the Codex envelope into the DataSource and run one
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
                        let stamp = env.get("server_time").and_then(|v| v.as_u64()).unwrap_or(0);
                        if stamp != last_stamp {
                            last_stamp = stamp;
                            platform::note_envelope(&ctx, &env);
                        }
                    }
                }
                let devices = platform::service(&ctx).devices();
                let targets = {
                    let cache = ctx.device_status_cache.lock().unwrap();
                    device_cycle_targets(&devices, &cache, now_secs())
                };
                for target_mac in targets {
                    if platform::device_link_for_mac(&ctx, &target_mac).is_none() {
                        continue;
                    }
                    if ctx.status.lock().unwrap().paused {
                        break;
                    }
                    platform::cycle(
                        &ctx,
                        &target_mac,
                        tick % 3 == 0,
                        auto_deliver && tick % 6 == 0,
                    )
                    .await;
                }
            }
        });
    }

    // v0.12 demand-driven BLE (docs/power-state.md §5/§9): no periodic scanning
    // and no template transfers. A cycle runs only when the device asks for a
    // handshake in its UDP announce (`ble=1`), which refreshes the endpoint
    // record (host/port/token) over the bonded link. Templates go over HTTP.
    //
    // Plan C: keep the 250 ms wake cadence and throttle actual GATT attempts
    // independently for each registered device.
    let mut last_device_attempt = HashMap::<String, i64>::new();
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
        let current_is_registered_device = platform::is_registered_device(&ctx);
        let devices = ctx.platform.devices();
        let registered_device = device_ble_candidates(&devices, &HashMap::new(), now_secs());
        if !udp_request || current_is_registered_device || !registered_device.is_empty() {
            let candidates = device_ble_candidates(&devices, &last_device_attempt, now_secs());
            if !candidates.is_empty() {
                match platform::ble_cycle(&ctx, &candidates).await {
                    Ok(platform::BleOpportunity::NoDevice) => {}
                    Ok(platform::BleOpportunity::Connected { mac, result }) => {
                        last_device_attempt.insert(mac.clone(), device_now_secs(&mac));
                        match result {
                            Ok(()) => tracing::info!(event = "result", device_mac = %mac,
                                transport = "ble", "device rendezvous complete"),
                            Err(error) => tracing::warn!(event = "result", device_mac = %mac,
                                transport = "ble", error_category = safe_device_error_category(&error),
                                "device rendezvous failed"),
                        }
                    }
                    Err(error) => {
                        tracing::debug!(
                            event = "result",
                            error_category = safe_device_error_category(&error),
                            "device BLE opportunity unavailable"
                        );
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
            scan_timeout_ms: 30000,
        };
        let pusher = Pusher::new(ble_cfg);
        match pusher.cycle_once(&adapter).await {
            Ok(info) => {
                tracing::info!("ble handshake done (udp announce)");
                // The device announces `ble=1`; adopt its identity/address from
                // the info JSON while we are connected (task-2 fallback path).
                // The write lands in the record of the MAC the info names — and
                // if that is not the device this handshake was for, it is not
                // adopted at all rather than rewriting another device.
                let reported = info
                    .get("mac")
                    .and_then(|v| v.as_str())
                    .map(normalize_mac)
                    .unwrap_or_default();
                let selected = selected_mac(&ctx);
                if let (Some(selected), false) = (selected.as_deref(), reported.is_empty()) {
                    if selected != reported {
                        tracing::warn!(
                            "ble handshake returned {reported}, expected {selected}; not adopting"
                        );
                    } else if let Err(e) = adopt_ble_info(&ctx, &info) {
                        tracing::debug!("ble info not adopted: {e}");
                    }
                } else if let Err(e) = adopt_ble_info(&ctx, &info) {
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

fn device_ble_candidates(devices: &[Value], last_attempt: &HashMap<String, i64>, now: i64) -> Vec<String> {
    devices
        .iter()
        .filter_map(|device| {
            device["device_mac"]
                .as_str()
                .and_then(bridge_core::platform::model::DeviceIdentity::normalized_mac)
        })
        .filter(|mac| {
            let now = scheduled_now(mac, now);
            last_attempt
                .get(mac)
                .map_or(true, |at| now.saturating_sub(*at) >= 55)
        })
        .collect()
}

#[cfg(test)]
mod device_ble_candidate_tests {
    use super::*;

    fn device(mac: &str) -> Value {
        json!({"device_mac": mac})
    }

    #[test]
    fn throttles_each_registered_device_mac_independently() {
        let devices = [device("AA:BB:CC:DD:EE:01"), device("aabbccddee02")];
        let attempts = HashMap::from([("AABBCCDDEE01".to_string(), 100)]);

        let due = device_ble_candidates(&devices, &attempts, 120);
        assert_eq!(due, ["AABBCCDDEE02"]);

        let due = device_ble_candidates(&devices, &attempts, 155);
        assert!(due.contains(&"AABBCCDDEE01".to_string()));
        assert!(due.contains(&"AABBCCDDEE02".to_string()));
    }

    #[test]
    fn excludes_invalid_and_unregistered_macs() {
        let devices = [
            device("not-a-mac"),
        ];
        let attempts = HashMap::from([("AABBCCDDEE03".to_string(), 100)]);
        let due = device_ble_candidates(&devices, &attempts, 120);
        assert!(due.is_empty());
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

    let instance = instance::Instance::from_env().unwrap_or_else(|error| panic!("{error}"));
    #[cfg(windows)]
    let _instance_lock = instance::acquire(&instance).unwrap_or_else(|error| panic!("{error}"));

    let root = config::repo_root();
    let cfg_path = config::config_path(&root);
    let mut config = Config::load(&cfg_path);
    assert_ne!(config.port, config.mcp_port, "HTTP and MCP ports must differ");
    if config.templates.is_relative() {
        config.templates = root.join(&config.templates);
    }
    if config.seeds.is_relative() {
        config.seeds = root.join(&config.seeds);
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
    let bridge_name = {
        let configured = sanitize_display(&config.bridge_name, 24);
        if configured.is_empty() {
            pc_name()
        } else {
            configured
        }
    };
    let bridge_id = instance.bridge_id(&host_label());

    // Seed the per-MAC registry from the pre-multi-device single record: a
    // one-device bridge then behaves exactly as before, and the next start
    // adopts the same device. Other registered devices keep their platform
    // records and are picked up by their own MAC.
    let mut registry = device_runtime::DeviceRegistry::default();
    let seeded = config
        .device_mac
        .as_deref()
        .map(normalize_mac)
        .filter(|mac| mac.len() == 12);
    if let Some(mac) = seeded.as_deref() {
        let ip = config.device_ip.trim();
        let name = sanitize_display(&config.device_name, 24);
        let observed = registry.observe(
            mac,
            (!ip.is_empty()).then_some(ip),
            (!name.is_empty()).then_some(name.as_str()),
            "config",
            now_secs(),
        );
        if observed.is_none() {
            tracing::warn!("configured device MAC {mac} is invalid; ignoring it");
        }
        registry.select(mac);
    }
    let seed_default_name = seeded
        .as_deref()
        .filter(|_| sanitize_display(&config.device_name, 24).is_empty());

    let (mcp_tx, _mcp_rx) = tokio::sync::watch::channel(mcp_port);
    let ctx = Arc::new(AppCtx {
        instance,
        config,
        root: root.clone(),
        app_handle: OnceLock::new(),
        envelope: Arc::new(RwLock::new(None)),
        library: Arc::new(RwLock::new(library)),
        force_ble: Arc::new(Notify::new()),
        device_delivery: tokio::sync::Mutex::new(()),
        sim_control: tokio::sync::Mutex::new(()),
        per_mac_delivery: Mutex::new(HashMap::new()),
        status: Mutex::new(RuntimeStatus {
            last_sync: None,
            last_error: None,
            last_error_at: None,
            paused: false,
        }),
        devices: Mutex::new(registry),
        bridge_name: Mutex::new(bridge_name),
        bridge_id,
        device_status_cache: Mutex::new(HashMap::new()),
        arp_running: AtomicBool::new(false),
        udp_ble: AtomicBool::new(false),
        activity: Arc::new(Activity::new()),
        mcp_port: Mutex::new(mcp_port),
        mcp_error: Mutex::new(None),
        mcp_tx,
        platform: Arc::new(
            bridge_core::platform::service::PlatformService::open(&bridge_core::paths::data_root())
                .expect("open platform state"),
        ),
    });
    if let Some(mac) = seed_default_name {
        // Persist the generated label, as the single-device bridge did.
        let name = default_device_name(mac);
        if let Some(device) = devices(&ctx).get_mut(mac) {
            device.name = name;
        }
        save_identity(&ctx);
    }
    {
        let facts = device_facts(&ctx);
        tracing::info!(
            "device identity: name='{}' mac={} ip={} bridge='{}' id={} registered={}",
            facts
                .as_ref()
                .map(|f| f.display_name())
                .unwrap_or_default(),
            facts.as_ref().map(|f| f.mac.clone()).unwrap_or_default(),
            facts.as_ref().map(|f| f.ip.clone()).unwrap_or_default(),
            ctx.bridge_name.lock().unwrap(),
            ctx.bridge_id,
            devices(&ctx).len(),
        );
    }

    let ctx_setup = ctx.clone();
    watchdog::spawn(std::process::id());
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_device_status,
            get_pmstats,
            preview_template,
            force_sync,
            set_paused,
            reload_templates,
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
            platform_device_view,
            platform_device_detail,
            platform_template_get,
            platform_template_validate,
            platform_template_preview,
            platform_template_save,
            platform_profile_get,
            platform_profile_save,
            platform_data_sync_save,
            platform_family_profiles,
            platform_family_profile_save,
            platform_family_profile_delete,
            platform_family_profile_copy_from_device,
            platform_publish,
            platform_publish_preview,
            platform_font_list,
            platform_font_import,
            platform_publish_cancel,
            platform_activate,
            platform_data_sources,
            platform_data_source_save,
            platform_data_probe,
            platform_power,
            platform_plan,
            platform_status_refresh,
            platform_recovery
        ])
        .setup(move |app| {
            let _ = ctx_setup.app_handle.set(app.handle().clone());
            app.manage(ctx_setup.clone());
            tauri::async_runtime::spawn(run_services(ctx_setup.clone()));
            tauri::async_runtime::spawn(mcp_serve(ctx_setup.clone()));

            let open = MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?;
            let sync = MenuItem::with_id(app, "sync", "立即同步", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "暂停推送", true, None::<&str>)?;
            let upgrade = MenuItem::with_id(app, "upgrade", "升级固件…", false, None::<&str>)?;
            let device = MenuItem::with_id(app, "device", "打开设备页", true, None::<&str>)?;
            let logs = MenuItem::with_id(app, "logs", "打开日志", true, None::<&str>)?;
            let templates =
                MenuItem::with_id(app, "templates", "打开模板目录", true, None::<&str>)?;
            let autostart = CheckMenuItem::with_id(
                app,
                "autostart",
                "开机自启",
                ctx_setup.instance.name.is_none(),
                ctx_setup.instance.name.is_none() && autostart::matches_current_exe(),
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(
                app,
                &[
                    &open, &sync, &pause, &upgrade, &sep1, &device, &logs, &templates, &autostart,
                    &sep2, &quit,
                ],
            )?;

            let tray = TrayIconBuilder::with_id("bridge-tray")
                .icon(icon::render(None, IconState::NoData, ctx_setup.instance.shape))
                .tooltip(match &ctx_setup.instance.name {
                    Some(name) => format!("Codex Status 桥 [{name}]"),
                    None => "Codex Status 桥".into(),
                })
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
                            if ctx.instance.name.is_some() { return; }
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

#[cfg(test)]
mod claim_target_tests {
    use super::post_claim_to_endpoint;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    fn read_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut chunk = [0; 1024];
        loop {
            let count = stream.read(&mut chunk).unwrap();
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
            if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn status_server(mac: &'static str, expect_claim: bool) -> (String, thread::JoinHandle<bool>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            assert!(read_request(&mut stream).starts_with("GET /status.json "));
            let body = format!(r#"{{"fw":"test","mac":"{mac}"}}"#);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            drop(stream);

            if expect_claim {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                let claimed = request.starts_with("POST /claim?");
                let body = "{}";
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{body}"
                )
                .unwrap();
                claimed
            } else {
                listener.set_nonblocking(true).unwrap();
                thread::sleep(Duration::from_millis(100));
                matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
            }
        });
        (endpoint, worker)
    }

    fn device_fallback_server(
        mac: &'static str,
        expect_claim: bool,
    ) -> (String, thread::JoinHandle<bool>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let worker = thread::spawn(move || {
            // The HTML status page is no longer a fallback: `/status.json` is the
            // only status source, so the 503 here is the whole first attempt.
            let (mut stream, _) = listener.accept().unwrap();
            assert!(read_request(&mut stream).starts_with("GET /status.json "));
            let body = "unavailable";
            write!(
                stream,
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();

            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /api/status "));
            assert!(request.contains("Authorization: Bearer endpoint-token\r\n"));
            let body = format!(
                r#"{{"device_mac":"{mac}","session_nonce":"0123456789abcdef0123456789abcdef"}}"#
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            drop(stream);

            if expect_claim {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                let claimed = request.starts_with("POST /claim?");
                let body = "{}";
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{body}"
                )
                .unwrap();
                claimed
            } else {
                listener.set_nonblocking(true).unwrap();
                thread::sleep(Duration::from_millis(100));
                matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
            }
        });
        (endpoint, worker)
    }

    #[tokio::test]
    async fn claim_refuses_wrong_target_mac_without_posting() {
        let (endpoint, server) = status_server("AA:BB:CC:DD:EE:02", false);
        let result = post_claim_to_endpoint(
            &endpoint,
            "AA:BB:CC:DD:EE:01",
            "token",
            "endpoint-token",
            "bridge",
            "name",
            "127.0.0.1",
            8765,
            false,
            false,
        )
        .await;

        assert!(result.is_err());
        assert!(server.join().unwrap());
    }

    #[tokio::test]
    async fn claim_posts_after_matching_target_mac() {
        let (endpoint, server) = status_server("AA:BB:CC:DD:EE:01", true);
        let result = post_claim_to_endpoint(
            &endpoint,
            "aa-bb-cc-dd-ee-01",
            "token",
            "endpoint-token",
            "bridge",
            "name",
            "127.0.0.1",
            8765,
            false,
            false,
        )
        .await;

        assert!(matches!(result, Ok((code, _)) if code == reqwest::StatusCode::OK));
        assert!(server.join().unwrap());
    }

    #[tokio::test]
    async fn claim_falls_back_to_authenticated_device_status_before_posting() {
        let (endpoint, server) = device_fallback_server("AA:BB:CC:DD:EE:01", true);
        let result = post_claim_to_endpoint(
            &endpoint,
            "aa-bb-cc-dd-ee-01",
            "token",
            "endpoint-token",
            "bridge",
            "name",
            "127.0.0.1",
            8765,
            false,
            false,
        )
        .await;

        assert!(matches!(result, Ok((code, _)) if code == reqwest::StatusCode::OK));
        assert!(server.join().unwrap());
    }

    #[tokio::test]
    async fn claim_refuses_wrong_device_target_mac_without_posting() {
        let (endpoint, server) = device_fallback_server("AA:BB:CC:DD:EE:02", false);
        let result = post_claim_to_endpoint(
            &endpoint,
            "AA:BB:CC:DD:EE:01",
            "token",
            "endpoint-token",
            "bridge",
            "name",
            "127.0.0.1",
            8765,
            true,
            false,
        )
        .await;

        assert!(result.is_err());
        assert!(server.join().unwrap());
    }
}

#[cfg(test)]
mod device_status_cache_tests {
    use super::{
        update_device_claim_cache, update_device_owner_cache, update_device_status_cache,
        device_cycle_targets, device_occupancy_decision, CachedDeviceStatus, DeviceOccupancyDecision,
    };
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn cycle_targets_use_registered_mac_status_and_ignore_current_selection() {
        let mac_a = "AA:BB:CC:DD:EE:01";
        let mac_b = "AA:BB:CC:DD:EE:02";
        let mac_unregistered = "AA:BB:CC:DD:EE:03";
        let devices = vec![
            json!({"device_mac": mac_a, "ip": "192.168.3.10"}),
            json!({"device_mac": mac_b, "ip": "192.168.3.11"}),
            json!({"device_mac": "", "ip": "192.168.3.12"}),
        ];
        let mut cache = HashMap::new();
        update_device_status_cache(
            &mut cache,
            mac_a,
            true,
            Some(json!({"device_mac": mac_a})),
            100,
        );
        update_device_status_cache(
            &mut cache,
            mac_b,
            true,
            Some(json!({"device_mac": mac_b})),
            69,
        );
        update_device_status_cache(
            &mut cache,
            mac_unregistered,
            true,
            Some(json!({"device_mac": mac_unregistered})),
            100,
        );

        assert_eq!(
            device_cycle_targets(&devices, &cache, 100),
            vec!["AABBCCDDEE01"]
        );
    }

    #[test]
    fn online_offline_and_mismatched_updates_stay_with_their_mac() {
        let mut cache: HashMap<String, CachedDeviceStatus> = HashMap::new();
        let mac_a = "AA:BB:CC:DD:EE:01";
        let mac_b = "AA:BB:CC:DD:EE:02";
        let status_a = json!({"device_mac": mac_a, "fw": "a"});
        let status_b = json!({"device_mac": mac_b, "fw": "b"});

        update_device_status_cache(&mut cache, mac_a, true, Some(status_a.clone()), 10);
        update_device_status_cache(&mut cache, mac_b, true, Some(status_b.clone()), 11);
        // A failed/mismatched response for A changes only A's reachability.
        update_device_status_cache(&mut cache, "aa-bb-cc-dd-ee-01", false, None, 12);

        let a = &cache["AABBCCDDEE01"];
        let b = &cache["AABBCCDDEE02"];
        assert!(!a.online);
        assert_eq!(a.status, Some(status_a));
        assert!(b.online);
        assert_eq!(b.status, Some(status_b));
        assert_eq!(a.fetched_at, 12);
        assert_eq!(b.fetched_at, 11);
    }

    #[test]
    fn occupancy_and_claim_updates_are_isolated_by_mac() {
        let mac_a = "AA:BB:CC:DD:EE:01";
        let mac_b = "AA:BB:CC:DD:EE:02";
        let mut cache = HashMap::new();
        update_device_status_cache(
            &mut cache,
            mac_a,
            true,
            Some(json!({"owner": {"id": "bridge-a", "expires_in_s": 100}})),
            100,
        );
        update_device_status_cache(
            &mut cache,
            mac_b,
            true,
            Some(json!({"owner": {"id": "bridge-b", "expires_in_s": 100}})),
            100,
        );
        update_device_claim_cache(&mut cache, mac_a, None, 40, false);
        update_device_claim_cache(
            &mut cache,
            mac_b,
            Some(json!({"id": "bridge-b", "expires_in_s": 100})),
            60,
            true,
        );
        let b_before = cache["AABBCCDDEE02"].clone();

        update_device_status_cache(&mut cache, mac_a, false, None, 101);
        assert!(matches!(
            device_occupancy_decision("AABBCCDDEE01", cache.get("AABBCCDDEE01"), "bridge-a", 101),
            DeviceOccupancyDecision::Offline
        ));
        assert_eq!(cache["AABBCCDDEE02"].owner, b_before.owner);
        assert_eq!(cache["AABBCCDDEE02"].last_claim_at, b_before.last_claim_at);
        assert_eq!(cache["AABBCCDDEE02"].yielded, b_before.yielded);

        // A fresh status reporting another owner blocks A only.
        update_device_status_cache(
            &mut cache,
            mac_a,
            true,
            Some(json!({"owner": {"id": "bridge-c", "expires_in_s": 100}})),
            102,
        );
        update_device_owner_cache(
            &mut cache,
            mac_a,
            json!({"id": "bridge-c", "expires_in_s": 100}),
            102,
        );
        assert_eq!(cache["AABBCCDDEE01"].last_claim_at, Some(40));
        assert_eq!(cache["AABBCCDDEE01"].owner_observed_at, Some(102));
        assert!(matches!(
            device_occupancy_decision("AABBCCDDEE01", cache.get("AABBCCDDEE01"), "bridge-a", 102),
            DeviceOccupancyDecision::Other(_)
        ));

        // A successful per-MAC renewal updates A without changing B.
        update_device_status_cache(
            &mut cache,
            mac_a,
            true,
            Some(json!({"owner": {"id": "bridge-a", "expires_in_s": 100}})),
            102,
        );
        update_device_claim_cache(
            &mut cache,
            mac_a,
            Some(json!({"id": "bridge-a", "expires_in_s": 100})),
            103,
            false,
        );
        assert!(matches!(
            device_occupancy_decision("AABBCCDDEE01", cache.get("AABBCCDDEE01"), "bridge-a", 103),
            DeviceOccupancyDecision::Owned { renew: false }
        ));
        assert_eq!(cache["AABBCCDDEE02"].owner, b_before.owner);
        assert_eq!(
            cache["AABBCCDDEE02"].owner_observed_at,
            b_before.owner_observed_at
        );
        assert_eq!(cache["AABBCCDDEE02"].last_claim_at, b_before.last_claim_at);
        assert_eq!(cache["AABBCCDDEE02"].yielded, b_before.yielded);
        assert!(matches!(
            device_occupancy_decision("AABBCCDDEE02", cache.get("AABBCCDDEE02"), "bridge-b", 103),
            DeviceOccupancyDecision::Yielded
        ));
    }

    #[test]
    fn fake_wall_jump_does_not_extend_status_or_claim_freshness() {
        let mac = "0200000000D1";
        let other = "0200000000D2";
        bridge_core::device_clock::configure(mac, 1000, 1_000_000, 0).unwrap();
        bridge_core::device_clock::configure(other, 7000, 2_000_000, 0).unwrap();
        let mut cache = HashMap::new();
        update_device_status_cache(&mut cache, mac, true,
            Some(json!({"owner":{"id":"bridge-a","expires_in_s":120}})), 1000);
        update_device_claim_cache(&mut cache, mac,
            Some(json!({"id":"bridge-a","expires_in_s":120})),1000,false);
        bridge_core::device_clock::set_wall(mac, 9_000_000).unwrap();
        let status = &cache[mac];
        assert!(super::device_status_fresh(mac,status,9000,30));
        assert!(!super::device_claim_renew_due(mac,status,9000));
        bridge_core::device_clock::step(mac,30_001).unwrap();
        assert!(!super::device_status_fresh(mac,status,9000,30));
        assert!(!super::device_claim_renew_due(mac,status,9000));
        bridge_core::device_clock::step(mac,29_999).unwrap();
        assert!(super::device_claim_renew_due(mac,status,9000));
        assert_eq!(bridge_core::device_clock::monotonic_ms(other),Some(7000));
        bridge_core::device_clock::clear(mac);
        bridge_core::device_clock::clear(other);
    }
}
