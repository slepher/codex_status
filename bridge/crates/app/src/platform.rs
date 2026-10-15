//! Application-service front end for the four UI pages and MCP (platform design §2/§11).
//!
//! Both interfaces call these functions; there is no second business model.
//! Saving never publishes, publishing never auto-enables sync, and every device
//! write is a user-visible action that goes through the coordinator.

use std::sync::Arc;
use base64::Engine;

use bridge_core::coordinator::DeliveryKind;
use bridge_core::platform::model::{DeviceCapabilities, DeviceIdentity, FamilyProfile, Profile};
use bridge_core::platform::service::PlatformService;
use bridge_core::device_client;
use serde_json::{json, Value};
use std::time::Duration;

use crate::AppCtx;

#[path = "wake_history.rs"]
mod wake_history;
#[path = "sync_diagnostics.rs"]
mod sync_diagnostics;
#[path = "rendezvous.rs"]
mod rendezvous;
pub use rendezvous::ble_cycle;

/// Resolved per-device transport facts (token from the cached device token).
#[derive(Clone)]
pub struct DeviceLink {
    pub mac: String,
    pub ip: String,
    pub token: String,
    pub bridge_id: String,
}

pub fn service(ctx: &AppCtx) -> &Arc<PlatformService> {
    &ctx.platform
}

/// Blocking socket workers retain the same device permit as their caller.
/// Dropping an async waiter cannot release a permit while its worker can write.
#[derive(Default)]
pub(crate) struct DeliveryLock {
    mutex: Arc<tokio::sync::Mutex<()>>,
    active: std::sync::Mutex<std::sync::Weak<tokio::sync::OwnedMutexGuard<()>>>,
}

impl DeliveryLock {
    fn remember(&self, guard: tokio::sync::OwnedMutexGuard<()>) -> Arc<tokio::sync::OwnedMutexGuard<()>> {
        let guard = Arc::new(guard);
        *self.active.lock().unwrap() = Arc::downgrade(&guard);
        guard
    }

    pub(crate) async fn lock(&self) -> Arc<tokio::sync::OwnedMutexGuard<()>> {
        self.remember(self.mutex.clone().lock_owned().await)
    }

    pub(crate) fn try_lock(&self) -> Result<Arc<tokio::sync::OwnedMutexGuard<()>>, tokio::sync::TryLockError> {
        self.mutex.clone().try_lock_owned().map(|guard| self.remember(guard))
    }

    pub(crate) fn spawn_blocking<T: Send + 'static>(&self, f: impl FnOnce() -> T + Send + 'static)
        -> Result<tokio::task::JoinHandle<T>, String>
    {
        let permit = self.active_permit()?;
        Ok(tokio::task::spawn_blocking(move || {
            let _permit = permit;
            f()
        }))
    }

    pub(crate) fn active_permit(&self) -> Result<Arc<tokio::sync::OwnedMutexGuard<()>>, String> {
        self.active.lock().unwrap().upgrade().ok_or_else(|| "device_execution_permit_missing".into())
    }
}

pub(super) fn delivery_lock(ctx: &AppCtx, mac: &str) -> Arc<DeliveryLock> {
    ctx.per_mac_delivery.lock().unwrap().entry(mac.to_uppercase())
        .or_insert_with(|| Arc::new(DeliveryLock::default())).clone()
}

#[cfg(test)]
mod delivery_worker_tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_http_waiter_keeps_device_exclusive_until_socket_deadline() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let (sent, received) = tokio::sync::oneshot::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut bytes = [0; 4096];
            assert!(socket.read(&mut bytes).unwrap() > 0);
            sent.send(()).unwrap();
            while socket.read(&mut bytes).is_ok_and(|n| n > 0) {}
        });
        let lock = Arc::new(DeliveryLock::default());
        assert!(lock.spawn_blocking(|| ()).is_err());
        let owner = lock.clone();
        let waiter = tokio::spawn(async move {
            let _guard = owner.lock().await;
            owner.spawn_blocking(move || {
                device_client::status(&address, "worker-test", Duration::from_millis(500))
            }).unwrap().await.unwrap()
        });
        tokio::time::timeout(Duration::from_secs(2), received).await.unwrap().unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        assert!(lock.try_lock().is_err(), "the live socket still owns this device");
        assert!(tokio::time::timeout(Duration::from_millis(25), lock.lock()).await.is_err());
        let other = DeliveryLock::default();
        let other_guard = other.lock().await;
        assert_eq!(other.spawn_blocking(|| 7).unwrap().await.unwrap(), 7);
        drop(other_guard);
        let next = tokio::time::timeout(Duration::from_secs(2), lock.lock()).await.unwrap();
        assert_eq!(lock.spawn_blocking(|| 9).unwrap().await.unwrap(), 9);
        drop(next);
        assert!(lock.try_lock().is_ok());
        server.join().unwrap();
    }
}

/// Device link from the selected device's runtime record + cached device token;
/// `None` when the token has not been negotiated yet (click BOOT to open the
/// BLE session) or no device is selected.
pub fn device_link(ctx: &AppCtx) -> Option<DeviceLink> {
    let mac = crate::selected_mac(ctx)?;
    device_link_for_mac(ctx, &mac)
}

pub(super) fn device_link_for_mac(ctx: &AppCtx, requested_mac: &str) -> Option<DeviceLink> {
    let mac = DeviceIdentity::normalized_mac(requested_mac)?;
    let device = service(ctx).device_get(&mac)?;
    device_link_from_identity(&mac, Some(&device), &ctx.config.token, &ctx.bridge_id)
}

fn device_link_from_identity(
    mac: &str,
    device: Option<&Value>,
    token: &str,
    bridge_id: &str,
) -> Option<DeviceLink> {
    let device = device?;
    if device["device_mac"].as_str()? != mac {
        return None;
    }
    let ip = device["ip"].as_str()?.to_string();
    if ip.is_empty() || ip == "0.0.0.0" || token.is_empty() {
        return None;
    }
    Some(DeviceLink {
        mac: mac.to_string(),
        ip,
        token: token.to_string(),
        bridge_id: bridge_id.to_string(),
    })
}

fn timeout() -> Duration {
    Duration::from_secs(6)
}

fn parse_device_endpoint(value: &str) -> Result<String, String> {
    if value.is_empty() || value.trim() != value {
        return Err("endpoint must be a bare IPv4 address or IPv4:port".into());
    }
    let (ip, port) = match value.split_once(':') {
        Some((ip, port)) => {
            let port = port
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or("endpoint port must be 1–65535")?;
            (
                ip.parse::<std::net::Ipv4Addr>()
                    .map_err(|_| "endpoint must be IPv4")?,
                Some(port),
            )
        }
        None => (
            value
                .parse::<std::net::Ipv4Addr>()
                .map_err(|_| "endpoint must be IPv4")?,
            None,
        ),
    };
    if ip.is_unspecified() {
        return Err("endpoint cannot be 0.0.0.0".into());
    }
    Ok(port
        .map(|port| format!("{ip}:{port}"))
        .unwrap_or_else(|| ip.to_string()))
}

fn registered_device_facts(
    requested_mac: &str,
    status: &Value,
    device_status: &Value,
) -> Result<(DeviceCapabilities, String), String> {
    let requested_mac = DeviceIdentity::normalized_mac(requested_mac)
        .ok_or_else(|| "invalid device MAC".to_string())?;
    let status_mac = status
        .get("mac")
        .and_then(Value::as_str)
        .and_then(DeviceIdentity::normalized_mac)
        .ok_or_else(|| "status.json missing valid mac".to_string())?;
    if status_mac != requested_mac {
        return Err(format!(
            "status.json reports MAC {status_mac}, not requested {requested_mac}"
        ));
    }
    let capabilities = caps_from_status(status)?;
    let authenticated_mac = device_status
        .get("device_mac")
        .and_then(Value::as_str)
        .and_then(DeviceIdentity::normalized_mac)
        .ok_or_else(|| "authenticated /api/status missing valid device_mac".to_string())?;
    if authenticated_mac != requested_mac || authenticated_mac != status_mac {
        return Err(format!(
            "authenticated /api/status reports MAC {authenticated_mac}, expected {requested_mac}"
        ));
    }
    Ok((capabilities, requested_mac))
}

fn registration_identity(mac: &str, endpoint: &str, name: &str) -> Result<DeviceIdentity, String> {
    let mut identity = DeviceIdentity::new(mac, name).map_err(err_text)?;
    identity.ip = Some(endpoint.to_string());
    identity.discovered_via = "manual".into();
    identity.last_seen_at = device_now(&mac);
    Ok(identity)
}

async fn register_device(ctx: &AppCtx, args: &Value) -> Result<Value, String> {
    let mac = args
        .get("mac")
        .and_then(Value::as_str)
        .ok_or("missing mac")?;
    let mac = DeviceIdentity::normalized_mac(mac).ok_or("invalid device MAC")?;
    let endpoint = parse_device_endpoint(
        args.get("endpoint")
            .and_then(Value::as_str)
            .ok_or("missing endpoint")?,
    )?;
    let (endpoint_status, endpoint_device_status, token) =
        (endpoint.clone(), endpoint.clone(), ctx.config.token.clone());
    let (status, device_status) = blocking(move || {
        let status = bridge_core::device::fetch(&endpoint_status, timeout())
            .map_err(err_text)?
            .raw
            .ok_or_else(|| "endpoint did not return structured /status.json".to_string())?;
        let device_status =
            device_client::status(&endpoint_device_status, &token, timeout()).map_err(err_text)?;
        Ok((status, device_status))
    })
    .await?;
    let (capabilities, mac) = registered_device_facts(&mac, &status, &device_status)?;
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            service(ctx)
                .device_get(&mac)
                .and_then(|d| d["name"].as_str().map(str::to_string))
        })
        .unwrap_or_else(|| format!("CodexStatus-{}", &mac[6..]));
    let identity = registration_identity(&mac, &endpoint, &name)?;
    service(ctx)
        .device_upsert(identity, capabilities)
        .map_err(err_text)?;
    service(ctx)
        .device_get(&mac)
        .ok_or_else(|| "registered device record unavailable".into())
}

/// Verify and update one already-registered device's endpoint from UDP.
/// The service upsert keeps its Profile, sync setting, and observations.
pub(super) async fn revalidate_registered_device_endpoint(
    ctx: &AppCtx,
    requested_mac: &str,
    endpoint: &str,
) -> Result<bool, String> {
    let mac = DeviceIdentity::normalized_mac(requested_mac).ok_or("invalid device MAC")?;
    let endpoint = parse_device_endpoint(endpoint)?;
    let Some(existing) = service(ctx).device_get(&mac) else {
        return Ok(false);
    };
    let previous_endpoint = existing["ip"].as_str().map(str::to_string);
    if previous_endpoint.as_deref() == Some(&endpoint) {
        return Ok(false);
    }

    let (status_endpoint, device_endpoint, token) =
        (endpoint.clone(), endpoint.clone(), ctx.config.token.clone());
    let (status, device_status) = blocking(move || {
        let status = bridge_core::device::fetch(&status_endpoint, timeout())
            .map_err(err_text)?
            .raw
            .ok_or_else(|| "endpoint did not return structured /status.json".to_string())?;
        let device_status = device_client::status(&device_endpoint, &token, timeout()).map_err(err_text)?;
        Ok((status, device_status))
    })
    .await?;
    let (capabilities, mac) = registered_device_facts(&mac, &status, &device_status)?;

    // Another address update or rename may have landed while HTTP was in flight.
    let Some(current) = service(ctx).device_get(&mac) else {
        return Ok(false);
    };
    if current["ip"].as_str() != previous_endpoint.as_deref() {
        return Ok(false);
    }
    let name = current["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .unwrap_or("CodexStatus")
        .to_string();
    let mut identity = registration_identity(&mac, &endpoint, &name)?;
    identity.discovered_via = "udp".into();
    service(ctx)
        .device_upsert(identity, capabilities)
        .map_err(err_text)?;
    // Keep this MAC's runtime record in step with the verified endpoint. The
    // write is keyed by the MAC the authenticated status confirmed, so no other
    // device's address can be touched here.
    crate::observe_device(ctx, &mac, Some(&endpoint), None, "udp");
    Ok(true)
}

#[cfg(test)]
mod registration_tests {
    use super::*;

    fn status() -> Value {
        json!({
            "mac": "AA:BB:CC:DD:EE:FF",
            "fw_target": "codex-status-154g",
            "render_target": "epd-ssd1681-200x200-1bpp",
            "max_templates": 8
        })
    }

    fn device_status(mac: &str) -> Value {
        json!({"device_mac": mac})
    }

    #[test]
    fn endpoint_accepts_only_ipv4_and_valid_optional_port() {
        assert_eq!(
            parse_device_endpoint("192.168.1.50").unwrap(),
            "192.168.1.50"
        );
        assert_eq!(
            parse_device_endpoint("192.168.1.50:8765").unwrap(),
            "192.168.1.50:8765"
        );
        for invalid in [
            "",
            " http://192.168.1.50",
            "device.local",
            "[::1]",
            "0.0.0.0",
            "192.168.1.50:0",
            "192.168.1.50:abc",
        ] {
            assert!(
                parse_device_endpoint(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn registration_requires_both_status_documents_to_match_requested_mac() {
        let (caps, mac) =
            registered_device_facts("aa-bb-cc-dd-ee-ff", &status(), &device_status("AABBCCDDEEFF"))
                .unwrap();
        assert_eq!(mac, "AABBCCDDEEFF");
        assert_eq!(caps.max_templates, 8);

        assert!(registered_device_facts(
            "00:00:00:00:00:01",
            &status(),
            &device_status("AABBCCDDEEFF")
        )
        .is_err());
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &json!({"fw_target":"codex-status-154g"}),
            &device_status("AABBCCDDEEFF")
        )
        .is_err());
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &json!({
                "mac": "00:00:00:00:00:01",
                "fw_target": "codex-status-154g",
                "render_target": "epd-ssd1681-200x200-1bpp"
            }),
            &device_status("AABBCCDDEEFF")
        )
        .is_err());
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &status(),
            &device_status("00:00:00:00:00:01")
        )
        .is_err());
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &json!({"mac":"AABBCCDDEEFF"}),
            &device_status("AABBCCDDEEFF")
        )
        .unwrap_err()
        .contains("device capabilities missing"));
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &json!({"mac":"AABBCCDDEEFF", "fw_target":"zectrix-note4-400x300", "render_target":"epd-ssd2683-400x300-1bpp"}),
            &device_status("AABBCCDDEEFF")
        )
        .unwrap_err()
        .contains("missing width"));
    }

    #[test]
    fn manual_registration_identity_uses_endpoint_and_name() {
        let identity = registration_identity("AABBCCDDEEFF", "127.0.0.1:8123", "test").unwrap();
        assert_eq!(identity.device_mac, "AABBCCDDEEFF");
        assert_eq!(identity.ip.as_deref(), Some("127.0.0.1:8123"));
        assert_eq!(identity.discovered_via, "manual");
        assert!(identity.last_seen_at > 0);
    }
}

/// Address of one device's runtime record (empty when unknown).
fn endpoint_for(ctx: &AppCtx, mac: &str) -> String {
    crate::device_facts_for(ctx, mac)
        .map(|facts| facts.endpoint())
        .unwrap_or_default()
}

fn err_text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Run the blocking std-socket client off the async runtime.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

async fn blocking_device<T: Send + 'static>(ctx: &AppCtx, mac: &str,
    f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String>
{
    delivery_lock(ctx, mac).spawn_blocking(f)?.await.map_err(|e| e.to_string())?
}

async fn sync_http_inner(ctx: &AppCtx, mac: &str, may_continue: bool) {
    sync_http_inner_budget(ctx, mac, may_continue, bridge_core::rendezvous::Budget::new(Duration::from_secs(90))).await;
}

async fn sync_http_inner_budget(ctx: &AppCtx, mac: &str, may_continue: bool, budget: bridge_core::rendezvous::Budget) {
    let Some(link) = device_link_for_mac(ctx, mac) else { return; };
    let root = service(ctx).data_root().to_path_buf();
    match blocking_device(ctx, mac, move || {
        let mut last = json!({"result":"idle"});
        for attempt in 0..3 {
            last = sync_diagnostics::transfer_with_budget(&root, &link, budget).map_err(err_text)?;
            if last["result"] != "complete" { break; }
            if !may_continue || attempt == 2 { break; }
            // A completed one-shot batch can close Wi-Fi immediately. Probe
            // briefly before attempting another batch; a full retry here
            // waits through association timeouts after every normal sync.
            let next = match device_client::status(&link.ip, &link.token, budget.remaining().min(Duration::from_secs(1))) {
                Ok(status) => status,
                Err(_) => break,
            };
            if next["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac)
                .as_deref() != Some(link.mac.as_str()) {
                return Err("sync continuation status MAC mismatch".to_string());
            }
            let sync = &next["sync"];
            if sync["due"] != true && sync["pending_batch"].is_null() &&
                sync["reasons"].as_array().is_none_or(Vec::is_empty) { break; }
        }
        Ok(last)
    }).await {
        Ok(value) if value["result"] == "complete" =>
            tracing::info!(device_mac = %mac, result = %value["result"], "sync-v1 transfer complete"),
        Ok(_) => {},
        Err(error) => tracing::warn!(device_mac = %mac, error = %error, "sync-v1 transfer pending"),
    }
}

async fn sync_http(ctx: &AppCtx, mac: &str, may_continue: bool) {
    let lock = delivery_lock(ctx, mac);
    let _delivery = lock.lock().await;
    sync_http_inner(ctx, mac, may_continue).await;
}

// ---------------------------------------------------------------------------
// Overview / templates / profiles
// ---------------------------------------------------------------------------

pub fn overview(ctx: &AppCtx) -> Value {
    let mut value = service(ctx).overview();
    value["device"] = device_summary(ctx);
    value["link"] = match device_link(ctx) {
        Some(link) => json!({"mac": link.mac, "ip": link.ip, "token_cached": true}),
        None => match crate::selected_mac(ctx) {
            Some(mac) => json!({"mac": mac, "ip": endpoint_for(ctx, &mac), "token_cached": false}),
            None => json!({"mac": Value::Null, "ip": "", "token_cached": false}),
        },
    };
    value
}

/// Summary of the device an operation without an explicit MAC resolves to.
fn device_summary(ctx: &AppCtx) -> Value {
    match crate::device_facts(ctx) {
        Some(facts) => json!({
            "device_mac": facts.mac,
            "name": facts.display_name(),
            "ip": facts.endpoint(),
        }),
        None => json!({"device_mac": Value::Null, "name": "", "ip": ""}),
    }
}

pub fn device_rows(ctx: &AppCtx) -> Value {
    let root = crate::mcp_config(ctx).data_root;
    let devices: Vec<Value> = service(ctx).devices().into_iter().map(|mut device| {
        if let Some(mac) = device["device_mac"].as_str() {
            device["diagnostics"] = sync_diagnostics::summary(&root, mac);
        }
        device
    }).collect();
    json!({
        "selected_device_mac": crate::selected_mac(ctx),
        "devices": devices,
        "templates": service(ctx).templates(),
    })
}

fn reading(body: &Value, key: &str, group: &str) -> Value {
    let value = body.pointer(&format!("/{}", key.replace('.', "/"))).cloned().unwrap_or(Value::Null);
    let sample = &body["field_samples"][key];
    let group_sample = &body["groups"][group];
    let observed_at = sample["received_at"].as_u64()
        .or_else(|| group_sample["received_at"].as_u64());
    let exception_at = observed_at.filter(|at| Some(*at) != body["received_at"].as_u64());
    let with_time = |mut reading: Value| {
        if let Some(at) = exception_at {
            reading["observed_at"] = json!(at);
            reading["source"] = json!(sample["transport"].as_str()
                .or_else(|| group_sample["transport"].as_str()).unwrap_or("unknown"));
        }
        reading
    };
    let sampled_boot = sample["sampled_boot_id"].as_str()
        .or_else(|| group_sample["sampled_boot_id"].as_str());
    let current_boot = body["boot_id"].as_str();
    if sampled_boot.is_some() && current_boot.is_some() && sampled_boot != current_boot {
        return with_time(json!({"value":null,"reason":"stale_boot","previous":value}));
    }
    if value.is_null() {
        return with_time(json!({"value":null,"reason":"not_sampled"}));
    }
    if value.get("reason").is_some() && value["value"].is_null() {
        return with_time(json!({"value":null,"reason":value["reason"],"previous":value["previous"]}));
    }
    with_time(json!({"value":value}))
}

fn prefer_observed(wifi: Value, observed: &Value, key: &str, wifi_at: u64) -> Value {
    let observed_at = observed["observed_at"].as_u64();
    if observed["transport"] == "ble" && !observed[key].is_null()
        && observed_at.is_some_and(|at| at >= wifi_at) {
        json!({"value":observed[key],"observed_at":observed_at,"source":"ble_digest"})
    } else { wifi }
}

fn upgrade_failure_label(error: &str) -> &'static str {
    let error = error.to_ascii_lowercase();
    if error.contains("401") || error.contains("unauthorized") { "设备认证失败" }
    else if error.contains("occupied") || error.contains("claim") { "设备被其他 Bridge 占用" }
    else if error.contains("target") || error.contains("mac mismatch") { "固件与设备身份不匹配" }
    else if error.contains("timeout") || error.contains("connect") { "联系设备超时，等待下次唤醒" }
    else { "升级任务失败，详情见历史归档" }
}

fn firmware_version_order(current: &str, registered: &str) -> Option<std::cmp::Ordering> {
    let parts = |text: &str| {
        let (number, family) = text.split_once('-').unwrap_or((text, ""));
        let numbers = number.split('.').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>().ok()?;
        ((2..=3).contains(&numbers.len())).then(|| (numbers, family.to_owned()))
    };
    let (current, current_family) = parts(current)?;
    let (registered, registered_family) = parts(registered)?;
    (current.len() == registered.len() && current_family == registered_family)
        .then(|| current.cmp(&registered))
}

fn project_device(raw: &Value, identity: &Value, now: u64) -> Value {
    let mac = &raw["device_mac"];
    let observed = &raw["record_observed"];
    let mut status_body = raw["last_authenticated"]["body"].clone();
    status_body["received_at"] = raw["last_authenticated"]["observed_at"].clone();
    if !observed["boot_id"].is_null() && observed["observed_at"].as_u64().unwrap_or(0)
        >= raw["last_authenticated"]["observed_at"].as_u64().unwrap_or(0) {
        status_body["boot_id"] = observed["boot_id"].clone();
    }
    let body = &status_body;
    let wifi_at = raw["last_authenticated"]["observed_at"].as_u64().unwrap_or(0);
    let contact_at = raw["last_authenticated_contact_at"].as_u64();
    let attempt = &raw["last_status_attempt"];
    let blocked = matches!(attempt["outcome"].as_str(), Some("blocked" | "mac_mismatch"))
        && attempt["at"].as_u64().unwrap_or(0) >= contact_at.unwrap_or(0);
    let recent = contact_at.is_some_and(|at| at <= now && now - at <= 30);
    let expected_sleep = body["power"]["mode"] == "sleep"
        || raw["record_observed"]["power"]["mode"] == "sleep";
    let contact_state = if blocked { "blocked" } else if recent { "recently_authenticated" }
        else if expected_sleep { "expected_sleep" } else { "waiting" };
    let contact_label = match contact_state {
        "blocked" => "认证或占用受阻", "recently_authenticated" => "最近已认证",
        "expected_sleep" => "预计休眠", _ => "等待下次联系",
    };
    let current = reading(body, "fw", "firmware");
    let target = raw["capabilities"]["firmware_target"].as_str().unwrap_or("");
    let release = &raw["latest_firmware"];
    let latest = if release.is_null() { json!({"value":null,"reason":"no_release"}) }
        else { json!({"value":release["version"],"published_at":release["published_at"]}) };
    let image = &raw["running_image"];
    let current_boot = &body["boot_id"];
    let comparable = image["fw_target"] == target && image["fw"] == current["value"]
        && image["image_bytes"] == release["size"]
        && image["boot_id"] == *current_boot && !image["sha256"].is_null();
    let verdict = if current["value"].is_null() || release.is_null() { "unknown" }
        else if current["value"] != latest["value"] {
            match firmware_version_order(current["value"].as_str().unwrap_or(""), latest["value"].as_str().unwrap_or("")) {
                Some(std::cmp::Ordering::Less) => "upgrade_available",
                Some(std::cmp::Ordering::Greater) => "newer_than_release",
                _ => "version_mismatch",
            }
        }
        else if comparable && image["sha256"] != release["sha256"] { "different_image" }
        else { "current" };
    let verdict_label = match verdict {
        "current" => "已是最新版", "upgrade_available" => "可升级",
        "different_image" => "同版本但运行镜像不同", "newer_than_release" => "运行版本高于登记版本",
        "version_mismatch" => "版本不同，需核对", _ => "无法判断",
    };
    let mut related = Vec::new();
    if !release.is_null() {
        if let Some(job) = raw.get("ota_job").filter(|v| !v.is_null()) {
            related.push((job["updated_at"].as_u64().unwrap_or(0), job.clone()));
        }
        if let Some(entries) = raw["ota_history"].as_array() {
            related.extend(entries.iter().map(|entry| (entry["closed_at"].as_u64().unwrap_or(0),
                entry["job"].clone())));
        }
    }
    let latest_job = related.into_iter().filter(|(_, job)|
        job["firmware_target"] == target && job["sha256"] == release["sha256"])
        .max_by_key(|(at, _)| *at);
    let upgrade_failure = if matches!(verdict, "upgrade_available" | "different_image") {
        latest_job.and_then(|(at, job)| (job["state"] == "failed").then(|| json!({
            "at":at,"reason":upgrade_failure_label(job["last_error"].as_str().unwrap_or("")),
            "label":"最近升级失败"})))
    } else { None };
    let mut installed = reading(body, "template_ids", "display");
    if let Some(ids) = installed["value"].as_array() { installed["value"] = json!(ids.len()); }
    if let Some(ids) = installed["previous"].as_array() { installed["previous"] = json!(ids.len()); }
    let mut active_template = prefer_observed(reading(body, "active_template_id", "display"), observed, "active_template_id", wifi_at);
    if let Some(id) = active_template["value"].as_str() { active_template["value"] = json!(!id.is_empty()); }
    if let Some(id) = active_template["previous"].as_str() { active_template["previous"] = json!(!id.is_empty()); }
    let mut applied = prefer_observed(reading(body, "applied_seq", "jobs"), observed, "applied_seq", wifi_at);
    if let Some(seq) = applied["value"].as_u64() { applied["value"] = json!(seq > 0); }
    if let Some(seq) = applied["previous"].as_u64() { applied["previous"] = json!(seq > 0); }
    let wifi_display = reading(body, "display_state", "display");
    let display_state = if observed["display_state"] == "unchanged" &&
        !wifi_display["value"].is_null() { wifi_display }
        else { prefer_observed(wifi_display, observed, "display_state", wifi_at) };
    let screen_label = match display_state["value"].as_str() {
        Some("displayed") => "屏幕已显示", Some("failed") => "显示失败",
        Some("pending") => "等待屏幕显示", Some("unchanged") => "显示状态未改变",
        _ => "尚无屏幕结果",
    };
    let owner_state = if identity["yielded"] == true { "yielded" }
        else if identity["owner_known"] != true { "unknown" }
        else if identity["owner"].is_null() { "free" } else { "occupied" };
    let owner_label = match owner_state {
        "yielded" => "已释放并在本地让步", "free" => "上次观察为空闲",
        "occupied" => "上次观察为已占用", _ => "尚未读取占用状态",
    };
    let mut attention = Vec::new();
    if matches!(verdict, "upgrade_available" | "different_image") { attention.push("firmware_upgrade"); }
    if verdict == "version_mismatch" { attention.push("firmware_version"); }
    if upgrade_failure.is_some() { attention.push("upgrade_failed"); }
    if display_state["value"] == "failed" { attention.push("display_failed"); }
    if blocked { attention.push("contact_blocked"); }
    json!({
        "device_mac":mac,
        "identity":{"name":identity["name"].as_str().filter(|name| !name.is_empty())
            .unwrap_or_else(|| raw["name"].as_str().unwrap_or("")),"model":match target {
            "zectrix-note4-400x300" => "Note4 400×300",
            "codex-status-154g" => "书桌屏 200×200", _ => target},
            "source":"bridge_registration"},
        "contact":{"state":contact_state,"label":contact_label,"last_authenticated_at":contact_at,
            "transport":raw["last_authenticated_transport"],
            "wifi_sampled_at":raw["last_authenticated"]["observed_at"],
            "last_attempt_at":attempt["at"]},
        "firmware":{"current":current,"latest":latest,"verdict":verdict,"label":verdict_label,
            "image_observed_at":image["observed_at"]},
        "upgrade_failure":upgrade_failure,
        "display":{"installed":installed,"active_template":active_template,
            "data_applied":applied,"screen_state":display_state,"label":screen_label},
        "battery":reading(body,"power.battery","power"),
        "delivery":{"enabled":raw["sync_enabled"],"source":"bridge_setting",
            "required_fields":raw["data_contract"]["requirements"].as_array().map(|fields|
                fields.iter().filter(|field| field["local"] == false).count())},
        "occupancy":{"state":owner_state,"label":owner_label,"observed_at":identity["owner_observed_at"],
            "owner_name":identity["owner"]["name"],"source":identity["owner_source"]},
        "attention":attention,
    })
}

#[cfg(test)]
mod device_view_tests {
    use super::*;

    #[test]
    fn firmware_and_screen_conclusions_follow_device_evidence() {
        let identity = json!({"owner_known":false});
        let mut raw = json!({
            "device_mac":"0200000000A1", "name":"A", "sync_enabled":false,
            "capabilities":{"firmware_target":"codex-status-154g"},
            "last_authenticated_contact_at":80,"last_authenticated_transport":"ble",
            "last_authenticated":{"observed_at":50,"body":{
                "boot_id":"boot-a", "fw":"1.0", "display_state":"failed",
                "template_ids":["quad"], "active_template_id":"quad", "applied_seq":0,
                "power":{"battery":0,"mode":"sleep"},
                "groups":{"firmware":{"received_at":50,"sampled_boot_id":"boot-a"},
                    "display":{"received_at":50,"sampled_boot_id":"boot-a"},
                    "power":{"received_at":50,"sampled_boot_id":"boot-a"},
                    "jobs":{"received_at":50,"sampled_boot_id":"boot-a"}}}},
            "latest_firmware":{"version":"1.0","sha256":"expected","size":1000,"published_at":70},
            "running_image":{"fw_target":"codex-status-154g","fw":"1.0",
                "boot_id":"boot-a","image_bytes":1000,"sha256":"other"},
            "ota_history":[{"closed_at":30,"job":{"firmware_target":"codex-status-154g",
                "sha256":"expected","state":"failed","last_error":"old failure"}}]
        });
        let view = project_device(&raw, &identity, 100);
        assert_eq!(view["firmware"]["verdict"], "different_image");
        assert_eq!(view["upgrade_failure"]["reason"], "升级任务失败，详情见历史归档");
        assert_eq!(view["display"]["label"], "显示失败");
        assert_eq!(view["display"]["installed"]["value"], 1);
        assert_eq!(view["display"]["active_template"]["value"], true);
        assert_eq!(view["display"]["data_applied"]["value"], false);
        assert!(!view.to_string().contains("quad"));
        assert_eq!(view["battery"]["value"], 0);
        assert_eq!(view["contact"]["wifi_sampled_at"], 50);
        raw["running_image"]["sha256"] = json!("expected");
        let current = project_device(&raw, &identity, 100);
        assert_eq!(current["firmware"]["verdict"], "current");
        assert!(current["upgrade_failure"].is_null());
        raw["latest_firmware"] = Value::Null;
        assert_eq!(project_device(&raw, &identity, 100)["firmware"]["verdict"], "unknown");
        raw["latest_firmware"] = json!({"version":"2.0","sha256":"new","size":1000});
        assert_eq!(project_device(&raw, &identity, 100)["firmware"]["verdict"], "upgrade_available");
        raw["last_authenticated"]["body"]["boot_id"] = json!("boot-b");
        assert_eq!(project_device(&raw, &identity, 100)["firmware"]["verdict"], "unknown");
        raw["record_observed"] = json!({"observed_at":100,"transport":"ble","boot_id":"boot-c","fw":"2.0",
            "display_state":"failed"});
        let fresh_ble = project_device(&raw, &identity, 100);
        assert_eq!(fresh_ble["firmware"]["verdict"], "unknown");
        assert_eq!(fresh_ble["battery"]["reason"], "stale_boot");
        assert_eq!(fresh_ble["display"]["installed"]["reason"], "stale_boot");
    }

    #[test]
    fn newer_running_firmware_does_not_create_a_pending_upgrade_notice() {
        let mut raw=json!({"device_mac":"7C4FADB93408","capabilities":{"firmware_target":"zectrix-note4-400x300"},
            "last_authenticated":{"observed_at":100,"body":{"fw":"0.18.102-note4","boot_id":"boot","display_state":"displayed"}},
            "latest_firmware":{"version":"0.18.61-note4"}});
        let view=project_device(&raw,&Value::Null,100);
        assert_eq!(view["firmware"]["verdict"],"newer_than_release");
        assert_eq!(view["attention"],json!([]));
        raw["latest_firmware"]["version"]=json!("0.18.103-note4");
        assert_eq!(project_device(&raw,&Value::Null,100)["attention"],json!(["firmware_upgrade"]));
        raw["latest_firmware"]["version"]=json!("0.18.103-other");
        assert_eq!(project_device(&raw,&Value::Null,100)["firmware"]["verdict"],"version_mismatch");
        assert_eq!(firmware_version_order("0.18.9-note4","0.18.10-note4"),Some(std::cmp::Ordering::Less));
        assert_eq!(firmware_version_order("dev-note4","0.18.10-note4"),None);
    }

    #[test]
    fn wifi_receipt_is_shared_and_ble_only_updates_carried_fields() {
        let identity = json!({"owner_known":true,"owner":null,
            "owner_observed_at":90,"owner_source":"public_status"});
        let mut raw = json!({"device_mac":"0200000000A1","name":"A",
            "capabilities":{"firmware_target":"codex-status-154g"},
            "last_authenticated_contact_at":50,"last_authenticated_transport":"http",
            "last_authenticated":{"observed_at":50,"body":{"boot_id":"a","fw":"1.0",
                "power":{"battery":0},"template_ids":[],"active_template_id":"quad",
                "applied_seq":0,"display_state":"displayed",
                "groups":{"firmware":{"received_at":50},"power":{"received_at":50},
                    "display":{"received_at":50},"jobs":{"received_at":50}}}},
            "latest_firmware":{"version":"1.0","published_at":40}});
        let wifi = project_device(&raw, &identity, 100);
        assert!(wifi["firmware"]["current"]["observed_at"].is_null());
        assert!(wifi["battery"]["observed_at"].is_null());
        assert!(wifi["battery"].get("observed_at").is_none());
        assert!(wifi["display"]["screen_state"]["observed_at"].is_null());
        assert_eq!(wifi["display"]["installed"]["value"], 0);
        raw["last_authenticated_contact_at"] = json!(90);
        raw["last_authenticated_transport"] = json!("ble");
        raw["record_observed"] = json!({"observed_at":90,"transport":"ble",
            "active_template_id":"other","applied_seq":1});
        let ble = project_device(&raw, &identity, 100);
        assert_eq!(ble["contact"]["wifi_sampled_at"], 50);
        assert_eq!(ble["firmware"]["current"]["value"], "1.0");
        assert!(ble["firmware"]["current"]["observed_at"].is_null());
        assert!(ble["battery"]["observed_at"].is_null());
        assert!(ble["display"]["screen_state"]["observed_at"].is_null());
        assert_eq!(ble["display"]["active_template"]["source"], "ble_digest");
        assert_eq!(ble["display"]["data_applied"]["observed_at"], 90);
        assert_eq!(ble["occupancy"]["source"], "public_status");
    }
}

/// Passive, MAC-scoped projection shared by the panel and MCP.
pub fn device_view(ctx: &AppCtx, requested_mac: Option<&str>) -> Result<Value, String> {
    let mac = requested_mac.map(|mac| DeviceIdentity::normalized_mac(mac).ok_or("invalid MAC"))
        .transpose()?;
    let rows = service(ctx).devices();
    if mac.as_ref().is_some_and(|mac| !rows.iter().any(|row| row["device_mac"] == *mac)) {
        return Err("unknown device MAC".into());
    }
    let views: Vec<Value> = rows.iter().map(|row| {
        let device_mac = row["device_mac"].as_str().unwrap_or("");
        let identity = crate::device_facts_for(ctx, device_mac)
            .map(|facts| crate::device_facts_json(&facts)).unwrap_or(Value::Null);
        project_device(row, &identity, device_now(device_mac))
    }).collect();
    let devices: Vec<Value> = views.iter().map(|view| json!({
        "device_mac":view["device_mac"],"name":view["identity"]["name"],
        "model":view["identity"]["model"],"status":view["contact"]["label"],
        "attention":view["attention"]})).collect();
    let device = mac.as_ref().and_then(|mac| views.into_iter()
        .find(|view| view["device_mac"] == *mac));
    Ok(json!({"devices":devices,"device":device}))
}

/// One collapsed section from local state. No device contact or implicit action.
pub fn device_detail(ctx: &AppCtx, requested_mac: &str, section: &str) -> Result<Value, String> {
    let mac = DeviceIdentity::normalized_mac(requested_mac).ok_or("invalid MAC")?;
    let row = service(ctx).device_get(&mac).ok_or("unknown device MAC")?;
    let mut status_body = row["last_authenticated"]["body"].clone();
    status_body["received_at"] = row["last_authenticated"]["observed_at"].clone();
    let observed = &row["record_observed"];
    if !observed["boot_id"].is_null() && observed["observed_at"].as_u64().unwrap_or(0)
        >= row["last_authenticated"]["observed_at"].as_u64().unwrap_or(0) {
        status_body["boot_id"] = observed["boot_id"].clone();
    }
    let body = &status_body;
    let identity = crate::device_facts_for(ctx, &mac)
        .map(|facts| crate::device_facts_json(&facts)).unwrap_or(Value::Null);
    let data = match section {
        "firmware" => json!({"target":row["capabilities"]["firmware_target"],
            "render_target":row["capabilities"]["render_target"],
            "compiler_abi":row["capabilities"]["compiler_abi"],
            "slot":reading(body,"running_slot","firmware"),
            "reset_reason":reading(body,"reset_reason","firmware"),
            "running_image":if row["running_image"].is_null() {
                json!({"value":null,"reason":"not_sampled"})
            } else { json!({"value":row["running_image"],
                "observed_at":row["running_image"]["observed_at"]}) }}),
        "connection" => json!({"device_mac":mac,"ip":row["ip"],
            "discovered_via":row["discovered_via"],"last_seen_at":row["last_seen_at"],
            "wifi_connected":reading(body,"radio.wifi_connected","radio"),
            "rssi":reading(body,"radio.rssi","radio"),
            "ble_connected":reading(body,"radio.ble_connected","radio"),
            "owner":identity["owner"],
            "owner_known":identity["owner_known"],"owner_observed_at":identity["owner_observed_at"],
            "owner_source":identity["owner_source"]}),
        "runtime_display" => json!({"heap_free":reading(body,"heap_free","runtime"),
            "heap_min":reading(body,"heap_min","runtime"),
            "uptime_ms":reading(body,"uptime_ms","runtime"),
            "epd_writes":reading(body,"display.epd_writes","display"),
            "epd_busy_fails":reading(body,"display.epd_busy_fails","display"),
            "template_ids":reading(body,"template_ids","display"),
            "active_template_id":reading(body,"active_template_id","display"),
            "active_context_id":reading(body,"active_context_id","display"),
            "commit_seq":reading(body,"commit_seq","jobs"),
            "display_state":reading(body,"display_state","display")}),
        "sync_diagnostics" => json!({"sync":reading(body,"sync","power"),
            "archive":sync_diagnostics::summary(&crate::mcp_config(ctx).data_root,&mac)}),
        "power" => {
            let pm = crate::device_facts_for(ctx, &mac).and_then(|facts| facts.pmstats)
                .map(|sample| json!({"fetched_at":sample.fetched_at,"text":sample.text,
                    "last_attempt_at":sample.last_attempt_at,"last_error":sample.last_error}))
                .unwrap_or_else(|| json!({"fetched_at":null,"last_attempt_at":null,
                    "last_error":null,"reason":"not_sampled"}));
            json!({"plan":row["power"],"device_power":reading(body,"power","power"),
                "pm_stats":pm,"pm_sampling":"explicit get_pmstats only",
                "wifi_sampled_at":row["last_authenticated"]["observed_at"]})
        },
        "upgrade_history" => json!({"ota_current":row["ota_job"],
            "ota_history":row["ota_history"],
            "bundle_current":row["job"],
            "bundle_history":service(ctx).bundle_history(&mac)}),
        "maintenance" => json!({"profile_present":!row["profile"].is_null(),
            "last_authenticated_at":row["last_authenticated_contact_at"],
            "recovery_requires_explicit_refresh":true}),
        _ => return Err("unknown device section".into()),
    };
    Ok(json!({"device_mac":mac,"section":section,"data":data}))
}

pub fn templates(ctx: &AppCtx) -> Value {
    json!({
        "templates": service(ctx).templates(),
        "targets": [target_contract("codex-status-154g", "epd-ssd1681-200x200-1bpp", true),
                    target_contract("codex-status-154g-gray4", "epd-200x200-2bpp-gray4", false)],
    })
}

fn target_contract(firmware_target: &str, render_target: &str, verified: bool) -> Value {
    json!({
        "firmware_target": firmware_target,
        "render_target": render_target,
        "hardware_verified": verified,
        "note": if verified { "hardware-verified" } else { "blocked_by_hardware_arrival: compile/host tested only" },
    })
}

pub fn template_get(ctx: &AppCtx, id: &str, render_target: Option<&str>) -> Result<Value, String> {
    service(ctx)
        .template_get(id, render_target)
        .ok_or_else(|| format!("unknown template {id}"))
}

pub fn template_save(
    ctx: &AppCtx,
    id: &str,
    render_target: &str,
    source: &Value,
) -> Result<Value, String> {
    let saved = service(ctx)
        .template_save(id, render_target, source, now_secs())
        .map_err(err_text)?;
    Ok(json!({
        "saved": true,
        "published": false,
        "template_id": saved.key.template_id,
        "render_target": saved.key.render_target,
        "source_crc": saved.source_crc,
        "compiled_crc": bridge_core::compile::compiled_crc(&saved.compiled),
        "requirements": saved.compiled.requirements,
        "note": "saved only; use publish on a device to push",
    }))
}

pub fn template_validate(source: &Value) -> Value {
    let target = source
        .get("render_target")
        .and_then(|v| v.as_str())
        .unwrap_or("epd-ssd1681-200x200-1bpp");
    match bridge_core::compile::compile(source, target) {
        Ok(compiled) => json!({
            "valid": true,
            "compiler_abi": compiled.compiler_abi,
            "requirements": compiled.requirements,
            "resources": compiled.resources.len(),
        }),
        Err(e) => json!({"valid": false, "error": e.to_string()}),
    }
}

pub fn template_preview(
    ctx: &AppCtx,
    id: Option<&str>,
    source: Option<&Value>,
    usage: Option<&str>,
    font_ids: Option<&std::collections::BTreeMap<String, String>>,
) -> Result<Value, String> {
    let source = match source {
        Some(s) => s.clone(),
        None => {
            let id = id.ok_or("template_preview needs id or json")?;
            let stored = service(ctx)
                .template_get(id, None)
                .ok_or_else(|| format!("unknown template {id}"))?;
            stored["source"].clone()
        }
    };
    let text = serde_json::to_string(&source).map_err(err_text)?;
    let usage = match usage {
        Some(u) => u.to_string(),
        None => ctx
            .envelope
            .try_read()
            .ok()
            .and_then(|envelope| envelope.clone())
            .and_then(|envelope| serde_json::to_string(&envelope).ok())
            .unwrap_or_default(),
    };
    let fonts = match font_ids {
        Some(ids) => service(ctx).preview_fonts_by_ids(ids).map_err(err_text)?,
        None => match crate::selected_mac(ctx) {
            Some(mac) => service(ctx).profile_preview_fonts(&mac).map_err(err_text)?,
            None => Vec::new(),
        },
    };
    let bits = bridge_render::render_bits_with_fonts(&text, &usage, &bridge_render::Env::default(), fonts)
        .map_err(err_text)?;
    let (width, height) = bridge_render::canvas_size(&text).ok_or("unsupported canvas")?;
    let png = bridge_render::bits_to_png_size(&bits, width, height).map_err(err_text)?;
    Ok(json!({
        "render_target": source.get("render_target").and_then(|v| v.as_str()).unwrap_or("epd-ssd1681-200x200-1bpp"),
        "width": width,
        "height": height,
        "png_len": png.len(),
        "png_base64": base64::engine::general_purpose::STANDARD.encode(&png),
    }))
}

pub fn profile_get(ctx: &AppCtx) -> Value {
    let mac = crate::selected_mac(ctx).unwrap_or_default();
    json!({
        "device_mac": mac,
        "profile": service(ctx).profile_get(&mac),
        "draft": Profile::draft(&mac),
    })
}

pub fn profile_save(ctx: &AppCtx, profile: Profile) -> Result<Value, String> {
    let saved = service(ctx)
        .profile_save(profile, now_secs())
        .map_err(err_text)?;
    Ok(json!({
        "saved": true,
        "published": false,
        "device_mac": saved.device_mac,
        "template_ids": saved.template_ids,
        "initial_active_id": saved.initial_active_id,
        "sync_enabled": saved.sync_enabled,
        "note": "profile saved only; publish is a separate explicit action",
    }))
}

pub fn data_sync_save(ctx: &AppCtx, mac: &str, enabled: bool) -> Result<Value, String> {
    service(ctx).set_data_sync(mac, enabled).map_err(err_text)?;
    Ok(json!({"device_mac":mac.to_uppercase(), "sync_enabled":enabled, "published":false}))
}

pub fn family_profiles(ctx: &AppCtx, render_target: Option<&str>) -> Value {
    use bridge_core::platform::model::{
        RENDER_TARGET_154G, RENDER_TARGET_GRAY4, RENDER_TARGET_NOTE4,
    };

    let mut families = vec![
        (RENDER_TARGET_154G, "200×200 黑白"),
        (RENDER_TARGET_NOTE4, "Note4 400×300 黑白"),
        (RENDER_TARGET_GRAY4, "200×200 灰阶"),
    ];
    families.sort_unstable_by_key(|(render_target, _)| *render_target);
    json!({
        "families": families.into_iter().map(|(render_target, label)| json!({
            "render_target": render_target,
            "label": label,
        })).collect::<Vec<_>>(),
        "profiles": service(ctx).family_profiles(render_target),
    })
}

pub fn family_profile_save(ctx: &AppCtx, profile: FamilyProfile) -> Result<Value, String> {
    let saved = service(ctx)
        .family_profile_save(profile, now_secs())
        .map_err(err_text)?;
    Ok(json!({"saved": saved, "published": false}))
}

pub fn family_profile_delete(ctx: &AppCtx, render_target: &str, id: &str) -> Result<Value, String> {
    let deleted = service(ctx)
        .family_profile_delete(render_target, id)
        .map_err(err_text)?;
    Ok(json!({"deleted": deleted}))
}

pub fn family_profile_copy_from_device(
    ctx: &AppCtx,
    mac: &str,
    id: &str,
    name: &str,
) -> Result<Value, String> {
    let saved = service(ctx)
        .family_profile_copy_from_device(mac, id, name, device_now(&mac))
        .map_err(err_text)?;
    Ok(json!({"saved": saved}))
}

// ---------------------------------------------------------------------------
// Publish / delivery
// ---------------------------------------------------------------------------

pub async fn publish(
    ctx: &AppCtx,
    mac: &str,
    expected_target_id: Option<&str>,
) -> Result<Value, String> {
    let job = service(ctx)
        .publish_checked(mac, device_now(&mac), expected_target_id, Some(&ctx.bridge_id))
        .map_err(err_text)?;
    Ok(json!({
        "job": job,
        "delivery": {"result": "waiting_for_authenticated_contact"},
        "state": service(ctx).job(mac),
    }))
}

pub fn publish_preview(ctx: &AppCtx, mac: &str) -> Result<Value, String> {
    service(ctx).publish_preview(mac).map_err(err_text)
}

pub fn font_list(ctx: &AppCtx) -> Result<Value, String> {
    service(ctx).font_list().map_err(err_text)
}

pub fn font_import(ctx: &AppCtx, path: &str) -> Result<Value, String> {
    service(ctx)
        .font_import(std::path::Path::new(path))
        .map_err(err_text)
}

pub fn job_cancel(ctx: &AppCtx, mac: &str) -> Value {
    service(ctx).cancel_job(mac, device_now(&mac));
    let job = service(ctx).job(mac);
    json!({"cancelled": job.as_ref().is_some_and(|j| j["state"] == "cancelled"), "job": job})
}

pub async fn activate(ctx: &AppCtx, mac: &str, template_id: &str) -> Result<Value, String> {
    service(ctx).activate(mac, template_id).map_err(err_text)?;
    Ok(deliver(ctx, mac).await)
}

/// Execute at most one pending coordinator action against the device.
pub async fn deliver(ctx: &AppCtx, mac: &str) -> Value {
    let lock = delivery_lock(ctx, mac);
    let _delivery = lock.lock().await;
    if service(ctx).ota_job(mac).is_some_and(|j| j.blocks_following_work()) {
        return json!({"result": "waiting_for_ota_confirmation"});
    }
    if service(ctx).asset_job_pending(mac) {
        return json!({"result": "waiting_for_device_protocol", "reason": "versioned manifest/object endpoints are pending joint device confirmation"});
    }
    let Some(link) = device_link_for_mac(ctx, mac) else {
        let _ = service(ctx).note_status_attempt(mac, "waiting_for_link", None);
        return json!({"result": "waiting_for_link", "reason": "device token/ip not available; open a BOOT session"});
    };
    let reachable = true;
    let decision = service(ctx).next_http_delivery(mac, reachable, device_now(&mac));
    match decision["decision"].as_str().unwrap_or("none") {
        "bundle" => {
            if let Err(e) = service(ctx).bind_pending_bundle_owner(mac, &link.bridge_id) {
                return json!({"result": "failed", "error": e.to_string()});
            }
            let payload = match service(ctx).bundle_payload(mac) {
                Ok(p) => p,
                Err(e) => return json!({"result": "failed", "error": e.to_string()}),
            };
            let (ip, token, bridge_id) =
                (link.ip.clone(), link.token.clone(), link.bridge_id.clone());
            let expected_mac = link.mac.clone();
            match blocking_device(ctx, mac, move || {
                device_client::install_bundle(
                    &ip,
                    &token,
                    &expected_mac,
                    &bridge_id,
                    &payload,
                    device_client::BUNDLE_CHUNK_BYTES,
                    Duration::from_secs(60),
                )
                .map_err(|e| format!("{e:#}"))
            })
            .await
            {
                Ok(ack) => {
                    let job_id = service(ctx)
                        .job(mac)
                        .and_then(|j| j["job_id"].as_str().map(str::to_string))
                        .unwrap_or_default();
                    if ack["result"] == "applied" {
                        let ctx_id = ack["active_context_id"].as_str().unwrap_or("");
                        service(ctx).retry_job(mac, &job_id, true, device_now(&mac));
                        if !ctx_id.is_empty() {
                            let _ = service(ctx).adopt_activation_context(mac, ctx_id, device_now(&mac));
                            // The next Data must use the newly installed indexed contract.
                            let observed = refresh_status(ctx, mac).await;
                            if observed["result"] != "ok" {
                                return json!({"result":"bundle","ack":ack,"next_stage":"read_device_contract","status":observed});
                            }
                        }
                    } else {
                        let _ = service(ctx).retry_job(mac, &job_id, false, device_now(&mac));
                    }
                    json!({"result": "bundle", "ack": ack})
                }
                Err(e) => json!({"result": "deferred", "error": e}),
            }
        }
        "activate" => {
            let template_id = decision["template_id"].as_str().unwrap_or("").to_string();
            let (ip, token, bridge_id) =
                (link.ip.clone(), link.token.clone(), link.bridge_id.clone());
            let id = template_id.clone();
            let expected_context = decision["expected_active_context_id"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            let expected_mac = link.mac.clone();
            match blocking_device(ctx, mac, move || {
                device_client::activate(
                    &ip,
                    &token,
                    &expected_mac,
                    &bridge_id,
                    &id,
                    &expected_context,
                    timeout(),
                )
                .map_err(err_text)
            })
            .await
            {
                Ok(ack) => {
                    if ack["result"] == "applied" {
                        if let Some(ctx_id) = ack["active_context_id"].as_str() {
                            let _ = service(ctx).note_activate_done(mac, ctx_id, device_now(&mac));
                            let observed = refresh_status(ctx, mac).await;
                            if observed["result"] != "ok" {
                                return json!({"result":"activate","ack":ack,"next_stage":"read_device_contract","status":observed});
                            }
                        }
                    }
                    json!({"result": "activate", "ack": ack})
                }
                Err(e) => json!({"result": "deferred", "error": err_text(e)}),
            }
        }
        "light_data" => {
            let body = service(ctx).data_message_body(mac);
            let Some(body) = body else {
                return json!({"result": "idle", "note": "no in-flight snapshot"});
            };
            let mut payload = body.clone();
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("bridge_id".into(), json!(link.bridge_id));
            }
            let (ip, token, expected_mac) = (link.ip.clone(), link.token.clone(), link.mac.clone());
            let sent = payload.clone();
            tracing::info!(event = "send", device_mac = %mac, operation = "/api/data",
                seq = body["seq"].as_u64().unwrap_or(0), "device data send");
            match blocking_device(ctx, mac, move || {
                device_client::data(&ip, &token, &expected_mac, &sent, timeout()).map_err(err_text)
            })
            .await
            {
                Ok(ack) => {
                    let applied = ack["result"] == "applied";
                    let ack_category = if !applied {
                        "ack_rejected"
                    } else if ack["data_seq"] != body["seq"]
                        || ack["active_context_id"] != body["active_context_id"]
                    {
                        "ack_mismatch"
                    } else {
                        "none"
                    };
                    let seq = body["seq"].as_u64().unwrap_or(0);
                    let crc = body["crc"].as_str().unwrap_or("").to_string();
                    let kind = DeliveryKind::LightData;
                    let outcome = service(ctx).note_ack(
                        mac,
                        kind,
                        seq,
                        &crc,
                        applied,
                        ack["display_state"].as_str().unwrap_or("unchanged"),
                    );
                    tracing::info!(
                        event = "ack",
                        device = mac,
                        seq,
                        data_seq = ack["data_seq"].as_u64().unwrap_or(0),
                        result = ack["result"].as_str().unwrap_or("unknown"),
                        display_state = ack["display_state"].as_str().unwrap_or("unknown"),
                        error_category = ack_category,
                        device_error = ack["error"].as_str().unwrap_or(""),
                        outcome = outcome["outcome"].as_str().unwrap_or("unknown"),
                        transport = "http",
                        "device data acknowledgement"
                    );
                    json!({"result": "data", "transport": "http", "ack": ack, "confirmation": outcome})
                }
                Err(e) => {
                    tracing::warn!(event = "result", device_mac = %mac, operation = "/api/data",
                        seq = body["seq"].as_u64().unwrap_or(0), error_category = safe_protocol_error_category(&e),
                        "device data send failed");
                    json!({"result": "deferred", "error": e})
                }
            }
        }
        "waiting_for_rendezvous" => json!(decision),
        other => json!({"result": other}),
    }
}

/// Formal plan decision for the current rendezvous (Bridge is the only decider).
pub async fn send_plan(
    ctx: &AppCtx,
    mac: &str,
    wake_reason: &str,
    provisional_remaining_s: u32,
) -> Value {
    let lock = delivery_lock(ctx, mac);
    let _delivery = lock.lock().await;
    let Some(link) = device_link_for_mac(ctx, mac) else {
        return json!({"result": "waiting_for_link"});
    };
    let plan = match service(ctx).plan_for_rendezvous(
        mac,
        device_now(&mac),
        wake_reason,
        provisional_remaining_s,
    ) {
        Ok(p) => p,
        Err(e) => return json!({"result": "failed", "error": e.to_string()}),
    };
    let mut body = serde_json::to_value(&plan).unwrap_or(Value::Null);
    if let Some(obj) = body.as_object_mut() {
        obj.insert("bridge_id".into(), json!(link.bridge_id));
    }
    let (ip, token, expected_mac) = (link.ip.clone(), link.token.clone(), link.mac.clone());
    let sent = body.clone();
    tracing::info!(event = "send", device_mac = %mac, operation = "/api/plan",
        plan_id = plan.plan_id, "device PowerPlan send");
    match blocking_device(ctx, mac, move || {
        device_client::plan(&ip, &token, &expected_mac, &sent, timeout()).map_err(err_text)
    })
    .await
    {
        Ok(ack) => {
            let accepted = ack["result"] == "applied";
            if let Some(remaining) = ack["accepted_remaining_s"].as_u64() {
                let _ = service(ctx).note_plan_ack(
                    mac,
                    plan.plan_id,
                    remaining as u32,
                    plan.mode == bridge_core::platform::model::PlanMode::Light
                        && provisional_remaining_s > 0,
                    device_now(&mac),
                );
            }
            let ack_category = if !accepted {
                "ack_rejected"
            } else if ack["plan_id"].as_u64().is_some_and(|id| id != plan.plan_id) {
                "ack_mismatch"
            } else {
                "none"
            };
            tracing::info!(event = "ack", device_mac = %mac, plan_id = plan.plan_id,
                result = ack["result"].as_str().unwrap_or("unknown"),
                ack_plan_id = ack["plan_id"].as_u64().unwrap_or(0),
                accepted_remaining_s = ack["accepted_remaining_s"].as_u64().unwrap_or(0),
                error_category = ack_category,
                transport = "http", "device PowerPlan acknowledgement");
            json!({"result": "plan", "plan": plan, "ack": ack, "accepted": accepted})
        }
        Err(e) => {
            tracing::warn!(event = "result", device_mac = %mac, operation = "/api/plan",
                plan_id = plan.plan_id, error_category = safe_protocol_error_category(&e),
                "device PowerPlan send failed");
            json!({"result": "deferred", "error": e})
        }
    }
}

/// User-requested light window. Freeze one formal plan before attempting any
/// transport so a deep-sleeping device receives the same ID over BLE later.
pub async fn request_light(ctx: &AppCtx, mac: &str) -> Value {
    let lock = delivery_lock(ctx, mac);
    let _delivery = lock.lock().await;
    let (plan, already_pending) = match service(ctx).queue_explicit_light(mac, device_now(&mac)) {
        Ok(value) => value,
        Err(e) => return json!({"result": "failed", "error": e.to_string()}),
    };
    ctx.force_ble.notify_one();
    // "Online" means this MAC's own cached status is fresh — never another
    // device's cache.
    let online = crate::device_facts_for(ctx, mac)
        .map(|facts| facts.is_online())
        .unwrap_or(false);
    if !online {
        return json!({"result": "queued", "transport": "ble_rendezvous",
            "plan": plan, "already_pending": already_pending, "ack": null});
    }
    let Some(link) = device_link_for_mac(ctx, mac) else {
        return json!({"result": "queued", "transport": "waiting_for_link",
            "plan": plan, "already_pending": already_pending, "ack": null});
    };
    let mut body = serde_json::to_value(&plan).unwrap_or(Value::Null);
    body["bridge_id"] = json!(link.bridge_id);
    let (ip, token, expected_mac) = (link.ip, link.token, link.mac);
    tracing::info!(event = "send", device_mac = %mac, operation = "/api/plan",
        plan_id = plan.plan_id, "device PowerPlan send");
    match blocking_device(ctx, mac, move || {
        device_client::plan(&ip, &token, &expected_mac, &body, timeout()).map_err(err_text)
    })
    .await
    {
        Ok(ack) if ack["result"] == "applied" => {
            let remaining = ack["accepted_remaining_s"].as_u64().unwrap_or(0) as u32;
            let confirmation =
                service(ctx).note_plan_ack(mac, plan.plan_id, remaining, false, device_now(&mac));
            tracing::info!(event = "ack", device_mac = %mac, plan_id = plan.plan_id,
                result = ack["result"].as_str().unwrap_or("unknown"),
                ack_plan_id = ack["plan_id"].as_u64().unwrap_or(0),
                accepted_remaining_s = remaining,
                error_category = if ack["plan_id"].as_u64().is_some_and(|id| id != plan.plan_id) { "ack_mismatch" } else { "none" },
                transport = "http",
                "device PowerPlan acknowledgement");
            json!({"result": "applied", "transport": "http", "plan": plan,
                "already_pending": already_pending, "ack": ack, "confirmation": confirmation})
        }
        Ok(ack) => {
            tracing::warn!(event = "ack", device_mac = %mac, plan_id = plan.plan_id,
                result = ack["result"].as_str().unwrap_or("unknown"),
                ack_plan_id = ack["plan_id"].as_u64().unwrap_or(0),
                accepted_remaining_s = ack["accepted_remaining_s"].as_u64().unwrap_or(0),
                error_category = if ack["result"] == "applied" { "none" } else { "ack_rejected" },
                error_category = "ack_rejected", transport = "http", "device PowerPlan rejected");
            json!({"result": "queued", "transport": "ble_rendezvous",
                "plan": plan, "already_pending": already_pending, "ack": ack})
        }
        Err(error) => {
            tracing::warn!(event = "result", device_mac = %mac, operation = "/api/plan",
                plan_id = plan.plan_id, error_category = safe_protocol_error_category(&error),
                "device PowerPlan send failed");
            json!({"result": "queued", "transport": "ble_rendezvous",
                "plan": plan, "already_pending": already_pending, "ack": null, "http_error": error})
        }
    }
}

fn safe_protocol_error_category(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("does not match") || text.contains("identity") || text.contains("expected") {
        "identity_mismatch"
    } else if text.contains("timeout") || text.contains("timed out") {
        "timeout"
    } else if text.contains("http") {
        "http_non_success"
    } else {
        "transport_or_protocol"
    }
}

/// Refresh the bridge-side view from the device's authenticated status.
pub async fn refresh_status(ctx: &AppCtx, mac: &str) -> Value {
    refresh_status_mode(ctx, mac, false).await
}

pub async fn refresh_compact_status(ctx: &AppCtx, mac: &str) -> Value {
    refresh_status_mode(ctx, mac, true).await
}

fn ota_online_window(status: &Value) -> bool {
    status["presence"]["mode"] == "continuous" ||
        (status["power"]["mode"] == "light" && status["power"]["remaining_s"].as_u64().unwrap_or(0) >= 120)
}

fn ota_old_boot(job: &bridge_core::platform::service::OtaJob, status: &Value) -> bool {
    job.before_boot_id.as_deref().is_some_and(|before| status["boot_id"].as_str() == Some(before))
}

fn ota_confirmation_expired(job: &bridge_core::platform::service::OtaJob, now: u64) -> bool {
    now.saturating_sub(job.confirmation_started_at.or(job.last_attempt_at).unwrap_or(job.created_at)) >= 120
}

async fn refresh_status_mode(ctx: &AppCtx, mac: &str, compact: bool) -> Value {
    let Some(link) = device_link_for_mac(ctx, mac) else {
        crate::update_device_status_cache(
            &mut ctx.device_status_cache.lock().unwrap(),
            mac,
            false,
            None,
            device_now(&mac) as i64,
        );
        return json!({"result": "waiting_for_link"});
    };
    let (ip, token, expected_mac) = (link.ip.clone(), link.token.clone(), link.mac.clone());
    match blocking(move || {
        let read = if compact { device_client::status_compact } else { device_client::status };
        read(&ip, &token, timeout()).map_err(err_text)
    }).await {
        Ok(status) => {
            let matches_target = status["device_mac"]
                .as_str()
                .and_then(DeviceIdentity::normalized_mac)
                .zip(DeviceIdentity::normalized_mac(mac))
                .is_some_and(|(actual, expected)| actual == expected);
            if !matches_target {
                let _ = service(ctx).note_status_attempt(mac, "mac_mismatch", Some("authenticated response named another MAC"));
                crate::update_device_status_cache(
                    &mut ctx.device_status_cache.lock().unwrap(),
                    mac,
                    false,
                    None,
                    device_now(&mac) as i64,
                );
                return json!({"result": "error", "error": format!("authenticated status MAC {} does not match target MAC {}", status["device_mac"].as_str().unwrap_or("<missing>"), expected_mac)});
            }
            let received_at = device_now(mac);
            if let Err(e) = service(ctx).note_device_status_at(mac, &status, "http", received_at) {
                return json!({"result":"error", "error":format!("device status contract: {e}")});
            }
            if compact && service(ctx).device_get(mac)
                .is_none_or(|device|device["data_contract"].is_null()) {
                return Box::pin(refresh_status_mode(ctx, mac, false)).await;
            }
            crate::wifi::http_ready(ctx, mac);
            if let Err(e) = service(ctx).note_authenticated_status_at(mac, &status, received_at) {
                return json!({"result": "error", "error": format!("persist authenticated status: {e}")});
            }
            if let Some(job) = service(ctx).ota_job(mac).filter(|j|j.state == "awaiting_confirmation" && ota_old_boot(j,&status)) {
                if ota_confirmation_expired(&job, device_now(mac)) {
                    let _ = service(ctx).ota_note_confirmation_miss(mac);
                }
            } else {
                let _ = service(ctx).ota_note_authenticated_version(mac, &status);
            }
            if let Some(job) = service(ctx).ota_job(mac).filter(|job|
                job.state == "awaiting_confirmation" && !ota_old_boot(job,&status)) {
                let can_check_image = job.upload_ack &&
                    status["image_identity"].as_array().is_some_and(|items|
                        items.iter().any(|item| item == "sha256-running-prefix-v1"));
                let mut checked_image = false;
                if can_check_image && status["ota_auth"] == "token" {
                    let image_link = link.clone();
                    let image_bytes = job.size;
                    if let Some(operation_token) = bridge_mcp::load_device_token_at(
                        &crate::mcp_config(ctx).data_root, mac) {
                        let result = blocking(move || device_client::ota_image(
                            &image_link.ip, &operation_token, &image_link.mac,
                            image_bytes, Duration::from_secs(15)).map_err(err_text)).await;
                        if let Ok(image) = result {
                            checked_image = true;
                            let _ = service(ctx).note_running_image(mac, &status, &image);
                            if let Err(error) = service(ctx).ota_note_running_image(mac, &image) {
                                tracing::warn!(device_mac = %mac, error = %error, "OTA image proof rejected");
                            }
                        }
                    }
                } else if let Some(nonce) = status["session_nonce"].as_str().filter(|_| can_check_image) {
                    for attempt in 0..3 {
                        let image_link = link.clone();
                        let nonce = nonce.to_owned();
                        let fields = json!({"image_bytes": job.size});
                        let id = format!("ota-image-{}-{attempt}", job.job_id);
                        match blocking(move || device_client::sync(&image_link.ip, &image_link.token,
                            &image_link.mac, &image_link.bridge_id, &nonce, "image", &id,
                            &fields, Duration::from_secs(15)).map_err(err_text)).await {
                            Ok(image) => {
                                checked_image = true;
                                let _ = service(ctx).note_running_image(mac, &status, &image);
                                if let Err(error) = service(ctx).ota_note_running_image(mac, &image) {
                                    tracing::warn!(device_mac = %mac, error = %error, "OTA image proof rejected");
                                }
                                break;
                            }
                            Err(error) if error.contains("401") || error.contains("409") => break,
                            Err(_) => continue,
                        }
                    }
                }
                if !checked_image {
                    let _ = service(ctx).ota_note_confirmation_miss(mac);
                }
            }
            if let Some(release) = status["firmware_target"].as_str()
                .and_then(|target| service(ctx).latest_firmware(target)) {
                let observed = service(ctx).running_image(mac);
                let current_boot = status["boot_id"].as_str();
                let needs_check = observed.as_ref().is_none_or(|image|
                    image["image_bytes"] != release.size ||
                    image["boot_id"].as_str() != current_boot ||
                    image["fw_target"] != release.firmware_target);
                if needs_check && status["ota_auth"] == "token" &&
                    status["image_identity"].as_array().is_some_and(|items|
                        items.iter().any(|item| item == "sha256-running-prefix-v1")) {
                    if let Some(token) = bridge_mcp::load_device_token_at(
                        &crate::mcp_config(ctx).data_root, mac) {
                        let image_link = link.clone();
                        if let Ok(image) = blocking(move || device_client::ota_image(
                            &image_link.ip, &token, &image_link.mac,
                            release.size, Duration::from_secs(15)).map_err(err_text)).await {
                            let _ = service(ctx).note_running_image(mac, &status, &image);
                        }
                    }
                }
            }
            crate::update_device_status_cache(
                &mut ctx.device_status_cache.lock().unwrap(),
                mac,
                true,
                Some(status.clone()),
                device_now(&mac) as i64,
            );
            json!({"result": "ok", "status": status})
        }
        Err(e) => {
            let outcome = if e.contains("401") || e.contains("403") || e.contains("409") {
                "blocked"
            } else { "offline" };
            if service(ctx).ota_job(mac).is_some_and(|j|
                j.state == "awaiting_confirmation" && ota_confirmation_expired(&j,device_now(mac))) {
                let _ = service(ctx).ota_note_confirmation_miss(mac);
            }
            let _ = service(ctx).note_status_attempt(mac, outcome, Some(&e));
            crate::update_device_status_cache(
                &mut ctx.device_status_cache.lock().unwrap(),
                mac,
                false,
                None,
                device_now(&mac) as i64,
            );
            json!({"result": "offline", "error": e})
        }
    }
}

// ---------------------------------------------------------------------------
// Data sources
// ---------------------------------------------------------------------------

pub fn data_sources(ctx: &AppCtx) -> Value {
    json!({
        "sources": service(ctx).data_sources(),
        "codex_envelope_available": service(ctx).codex_envelope_available(),
    })
}

pub fn data_source_save(ctx: &AppCtx, source: Value) -> Result<Value, String> {
    let parsed = bridge_core::datasource::parse_data_source(&source).map_err(err_text)?;
    service(ctx).data_source_save(parsed).map_err(err_text)?;
    Ok(json!({"saved": true, "sources": service(ctx).data_sources()}))
}

pub fn data_probe(ctx: &AppCtx, source_id: &str) -> Result<Value, String> {
    let snapshot = service(ctx).data_probe(source_id).map_err(err_text)?;
    Ok(json!({
        "source_id": snapshot.source_id,
        "quality": snapshot.quality,
        "observed_at": snapshot.observed_at,
        "valid_until": snapshot.valid_until,
        "last_success_at": snapshot.last_success_at,
        "error": snapshot.error,
        "fields": snapshot.fields.iter().map(|(k, v)| json!({
            "field": k, "value": v.value, "quality": v.quality,
        })).collect::<Vec<_>>(),
    }))
}

/// Feed the Codex envelope into the platform (called by the poller/pull path).
pub fn note_envelope(ctx: &AppCtx, envelope: &Value) {
    let _ = service(ctx).note_codex_envelope(envelope);
}

// ---------------------------------------------------------------------------
// Power (device submenu)
// ---------------------------------------------------------------------------

pub fn power_view(ctx: &AppCtx, requested_mac: &str) -> Value {
    let mac = DeviceIdentity::normalized_mac(requested_mac)
        .or_else(|| crate::selected_mac(ctx))
        .unwrap_or_default();
    let summary = service(ctx).coordinator_summary(&mac, device_now(&mac));
    let explicit = summary.as_ref().map(|s| &s["plan"]);
    let pending = explicit.map(|p| &p["pending_explicit_light"]);
    let ack = explicit.map(|p| &p["last_explicit_light_ack"]);
    let hold_until = explicit
        .and_then(|p| p["light_hold_until"].as_u64())
        .unwrap_or(0);
    json!({
        "device_mac": mac,
        "coordinator": summary,
        "explicit_light": {"state": if pending.is_some_and(|p| !p.is_null()) { "queued" }
            else if ack.is_some_and(|a| !a.is_null()) {
                if device_now(&mac) < hold_until { "applied" } else { "expired" }
            } else { "none" },
            "pending_plan": pending, "ack": ack},
        "note": "reads never extend the light deadline; only a formal PowerPlan does",
    })
}

pub fn recovery(ctx: &AppCtx, requested_mac: &str, digest: &Value) -> Result<Value, String> {
    let mac = DeviceIdentity::normalized_mac(requested_mac)
        .ok_or_else(|| "invalid device MAC".to_string())?;
    if digest["device_mac"]
        .as_str()
        .and_then(DeviceIdentity::normalized_mac)
        .as_deref()
        != Some(mac.as_str())
    {
        return Err("recovery digest MAC does not match requested device".into());
    }
    let profile = service(ctx)
        .recovery_import(&mac, digest, device_now(&mac))
        .map_err(err_text)?;
    Ok(json!({
        "imported": true,
        "sync_enabled": false,
        "profile": profile,
        "note": "recovered profiles start with data sync disabled",
    }))
}

// ---------------------------------------------------------------------------
// Device registration / MCP helper
// ---------------------------------------------------------------------------

/// Capabilities from a `/status.json` document. Firmware that does not report
/// `fw_target` is refused because the device contract cannot be verified.
pub fn caps_from_status(raw: &Value) -> Result<DeviceCapabilities, String> {
    let fw_target = raw.get("fw_target").and_then(|v| v.as_str());
    let Some(fw_target) = fw_target else {
        return Err("device capabilities missing: device reports a legacy protocol".into());
    };
    let render_target = raw
        .get("render_target")
        .and_then(|v| v.as_str())
        .unwrap_or("epd-ssd1681-200x200-1bpp");
    let note4 = render_target == bridge_core::platform::model::RENDER_TARGET_NOTE4;
    let verified = render_target == "epd-ssd1681-200x200-1bpp";
    let required = |key: &str| -> Result<u64, String> {
        raw.get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("Note4 status missing {key}"))
    };
    let checked_u32 = |value: u64, key: &str| -> Result<u32, String> {
        u32::try_from(value).map_err(|_| format!("device {key} exceeds u32"))
    };
    let (width, height, pixel_format, colors, partial) = if note4 {
        (
            checked_u32(required("width")?, "width")?,
            checked_u32(required("height")?, "height")?,
            raw.get("pixel_format")
                .and_then(Value::as_str)
                .ok_or("Note4 status missing pixel_format")?
                .to_string(),
            raw.get("colors")
                .and_then(Value::as_str)
                .ok_or("Note4 status missing colors")?
                .to_string(),
            raw.get("partial")
                .and_then(Value::as_bool)
                .ok_or("Note4 status missing partial")?,
        )
    } else if render_target == bridge_core::platform::model::RENDER_TARGET_GRAY4 {
        (200, 200, "2bpp".into(), "gray4".into(), false)
    } else {
        (200, 200, "1bpp".into(), "bw".into(), verified)
    };
    let max_templates = checked_u32(
        if note4 {
            required("max_templates")?
        } else {
            raw.get("max_templates")
                .and_then(Value::as_u64)
                .unwrap_or(8)
        },
        "max_templates",
    )?;
    let max_bundle_bytes = if note4 {
        required("max_bundle_bytes")?
    } else {
        raw.get("max_bundle_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(262_144)
    };
    let caps = DeviceCapabilities {
        firmware_target: fw_target.to_string(),
        render_target: render_target.to_string(),
        width,
        height,
        pixel_format,
        colors,
        compiler_abi: checked_u32(
            raw.get("compiler_abi")
                .and_then(|v| v.as_u64())
                .unwrap_or(bridge_core::compile::COMPILER_ABI as u64),
            "compiler_abi",
        )?,
        max_templates,
        max_bundle_bytes,
        bundle_compression: checked_u32(raw.get("bundle_compression").and_then(Value::as_u64).unwrap_or(0), "bundle_compression")?,
        max_bundle_decoded_bytes: raw.get("max_bundle_decoded_bytes").and_then(Value::as_u64).unwrap_or(0),
        bundle_storage_protocol: checked_u32(
            raw.get("bundle_storage_protocol").and_then(Value::as_u64).unwrap_or(0),
            "bundle_storage_protocol",
        )?,
        bundle_font_protocol: checked_u32(
            raw.get("bundle_font_protocol").and_then(Value::as_u64).unwrap_or(0),
            "bundle_font_protocol",
        )?,
        asset_publish_protocol: checked_u32(
            raw.get("asset_publish_protocol")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "asset_publish_protocol",
        )?,
        max_object_bytes: raw
            .get("max_object_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        max_manifest_bytes: raw
            .get("max_manifest_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        install_peak_bytes: raw
            .get("install_peak_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        free_bytes: raw.get("free_bytes").and_then(Value::as_u64).unwrap_or(0),
        filesystem_overhead_bytes: raw
            .get("filesystem_overhead_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        partial,
        hardware_verified: raw
            .get("hardware_verified")
            .and_then(Value::as_bool)
            .unwrap_or(verified),
        ..DeviceCapabilities::ssd1681_154g()
    };
    caps.validate().map_err(|e| e.to_string())?;
    Ok(caps)
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    #[test]
    fn note4_requires_reported_geometry_and_never_infers_hardware_verification() {
        let status = json!({"fw_target":"zectrix-note4-400x300",
            "render_target":"epd-ssd2683-400x300-1bpp", "compiler_abi":1});
        assert!(caps_from_status(&status)
            .unwrap_err()
            .contains("missing width"));
        let mut complete = status;
        complete["width"] = json!(400);
        complete["height"] = json!(300);
        complete["pixel_format"] = json!("1bpp");
        complete["colors"] = json!("bw");
        complete["partial"] = json!(false);
        complete["max_templates"] = json!(8);
        complete["max_bundle_bytes"] = json!(262_144);
        let caps = caps_from_status(&complete).unwrap();
        assert_eq!((caps.width, caps.height), (400, 300));
        assert!(!caps.hardware_verified);
        complete["width"] = json!(200);
        assert!(caps_from_status(&complete)
            .unwrap_err()
            .contains("disagrees"));
    }

    #[test]
    fn note4_delivery_uses_its_saved_ip_when_global_selection_is_154() {
        let global_selected_ip = "192.168.1.50";
        let note4 = json!({"device_mac": "7C4FADB93408", "ip": "192.168.3.177"});
        let link =
            device_link_from_identity("7C4FADB93408", Some(&note4), "endpoint-token", "bridge-id")
                .unwrap();

        assert_eq!(global_selected_ip, "192.168.1.50");
        assert_eq!(link.ip, "192.168.3.177");
        assert_ne!(link.ip, global_selected_ip);
    }

    #[test]
    fn unknown_device_cannot_borrow_a_saved_or_global_link() {
        assert!(
            device_link_from_identity("AAAAAAAAAAAA", None, "endpoint-token", "bridge-id",)
                .is_none()
        );

        let other_device = json!({"device_mac": "70041DD7A340", "ip": "192.168.1.50"});
        assert!(device_link_from_identity(
            "AAAAAAAAAAAA",
            Some(&other_device),
            "endpoint-token",
            "bridge-id",
        )
        .is_none());

        let device_without_ip = json!({"device_mac": "AAAAAAAAAAAA", "ip": null});
        assert!(device_link_from_identity(
            "AAAAAAAAAAAA",
            Some(&device_without_ip),
            "endpoint-token",
            "bridge-id",
        )
        .is_none());
    }

    #[test]
    fn explicit_target_mac_is_normalized_and_invalid_values_are_rejected() {
        assert_eq!(
            requested_mac(&json!({"mac": "70:04:1d:aa:bb:cc"})).unwrap(),
            Some("70041DAABBCC".to_string())
        );
        assert!(requested_mac(&json!({"mac": "not-a-mac"})).is_err());
        assert!(requested_mac(&json!({"mac": 42})).is_err());
        assert_eq!(requested_mac(&json!({"device_mac": "70:04:1d:aa:bb:cc"})).unwrap(), Some("70041DAABBCC".into()));
        assert!(requested_mac(&json!({"mac": "70041DAABBCC", "device_mac": "112233445566"})).is_err());
        assert_eq!(requested_mac(&json!({})).unwrap(), None);
    }
}

/// Register/refresh one device in the platform service. Only device firmware
/// reaches this point; the capability contract is enforced upstream. The
/// identity written is exactly the `mac` the authenticated status reported.
pub fn ensure_device(
    ctx: &AppCtx,
    requested_mac: &str,
    endpoint: &str,
    capabilities: DeviceCapabilities,
) {
    let Some(mac) = DeviceIdentity::normalized_mac(requested_mac) else {
        return;
    };
    let name = crate::device_facts_for(ctx, &mac)
        .map(|facts| facts.display_name())
        .unwrap_or_default();
    if let Ok(identity) = DeviceIdentity::new(&mac, &name) {
        let mut identity = identity;
        identity.ip = Some(endpoint.to_string());
        let _ = service(ctx).device_upsert(identity, capabilities);
    }
}

/// Outcome of one device BLE scan and, when connected, its rendezvous.
#[derive(Debug)]
pub enum BleOpportunity {
    /// None of the registered candidates was advertising.
    NoDevice,
    /// A verified MAC connected; the rendezvous may still have failed.
    Connected { mac: String, result: Result<(), String> },
}

/// Pull completed wake records only after the normal rendezvous work has been
/// acknowledged. The short budget keeps diagnostics from extending the wake
/// window or changing the result of a healthy plan/data exchange.
async fn sync_wake_history(
    link: &mut bridge_ble::DeviceConnection,
    mac: &str,
    status: &Value,
) {
    let Some(generation) = status.get("wake_generation").and_then(Value::as_u64) else {
        // Older ROMs have no generation and therefore cannot be safely joined
        // to a durable Bridge stream.
        return;
    };
    let mut store = match wake_history::Store::open(mac, generation) {
        Ok(store) => store,
        Err(error) => {
            tracing::debug!(device_mac = %mac, wake_generation = generation, error = %error,
                "wake history store unavailable");
            return;
        }
    };
    let now = device_now(&mac) as i64;
    if !store.due(now) {
        return;
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut pages = 0;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() || pages >= 16 {
            tracing::debug!(device_mac = %mac, wake_generation = generation,
                cursor = store.cursor(), pages, "wake history sync budget exhausted");
            return;
        }
        pages += 1;
        let before = store.cursor();
        let reply = match tokio::time::timeout(
            remaining,
            link.command(
                "history",
                json!({
                    "since": before.min(u32::MAX as u64),
                    "limit": wake_history::PAGE_LIMIT,
                }),
            ),
        )
        .await
        {
            Ok(Ok(reply)) => reply,
            Ok(Err(error)) => {
                if is_history_unsupported(&error.to_string()) {
                    let _ = store.mark_unsupported();
                }
                tracing::debug!(device_mac = %mac, wake_generation = generation, error = %error,
                    "wake history sync skipped");
                return;
            }
            Err(_) => {
                tracing::debug!(device_mac = %mac, wake_generation = generation,
                    cursor = before, "wake history sync timed out");
                return;
            }
        };
        if is_history_unsupported(
            reply
                .get("result")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ) || is_history_unsupported(
            reply
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ) {
            let _ = store.mark_unsupported();
            return;
        }
        if reply.get("wake_generation").and_then(Value::as_u64) != Some(generation) {
            tracing::debug!(device_mac = %mac, wake_generation = generation,
                "wake history generation mismatch");
            return;
        }
        let progress = match store.append_page(&reply, device_now(&mac) as i64) {
            Ok(progress) => progress,
            Err(error) => {
                tracing::debug!(device_mac = %mac, wake_generation = generation, error = %error,
                    "wake history page rejected");
                return;
            }
        };
        if progress.more && progress.received == 0 && progress.cursor <= before {
            tracing::debug!(device_mac = %mac, wake_generation = generation,
                cursor = before, "wake history page made no progress");
            return;
        }
        if !progress.more {
            tracing::info!(event = "wake_history_sync", device_mac = %mac,
                wake_generation = generation, records = progress.received,
                cursor = progress.cursor, pages, "wake history synchronized");
            return;
        }
    }
}

fn wake_history_sync_due(mac: &str, status: &Value) -> bool {
    let Some(generation) = status.get("wake_generation").and_then(Value::as_u64) else {
        return false;
    };
    wake_history::Store::open(mac, generation)
        .map(|store| store.due(device_now(&mac) as i64))
        .unwrap_or(false)
}

fn is_history_unsupported(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("unsupported")
        || text.contains("unknown op")
        || text.contains("unknown command")
        || text.contains("not found")
}

async fn cache_device_token_if_missing(ctx: &AppCtx, link: &bridge_ble::DeviceConnection, mac: &str) {
    if bridge_mcp::load_device_token(&crate::mcp_config(ctx), mac).is_some() {
        return;
    }
    match tokio::time::timeout(Duration::from_secs(1), link.request_device_token()).await {
        Ok(Ok(token)) => match bridge_mcp::save_device_token(&crate::mcp_config(ctx), mac, &token) {
            Ok(()) => tracing::info!(device_mac = %mac, "device token cached after BLE rendezvous"),
            Err(error) => tracing::warn!(device_mac = %mac, error = %error,
                "device token cache write failed; continuing rendezvous"),
        },
        Ok(Err(error)) => tracing::warn!(device_mac = %mac, error = %error,
            "device token request failed; continuing rendezvous"),
        Err(_) => tracing::warn!(device_mac = %mac, reason = "timeout",
            "device token request timed out; continuing rendezvous"),
    }
}

/// Upper bound for the link cleanup at the end of `ble_cycle`. The cleanup
/// runs *after* both delivery locks are released, so this value no longer
/// gates HTTP delivery or the next rendezvous; it only bounds how long the
/// rendezvous task waits on a possibly-stuck `peripheral.disconnect()`.
/// Run one coordinator cycle for a registered device.
pub async fn cycle(ctx: &AppCtx, mac: &str, refresh: bool, deliver_now: bool) {
    let Some(mac) = DeviceIdentity::normalized_mac(mac) else {
        return;
    };
    if service(ctx).device_get(&mac).is_none() {
        return;
    }
    let refreshed = if refresh {
        let reply = refresh_status(ctx, &mac).await;
        if reply["result"] != "ok" { return; }
        reply["status"].clone()
    } else { Value::Null };
    match crate::device_occupancy_gate(ctx, &mac).await {
        crate::Occupancy::Owned => {}
        crate::Occupancy::Yielded => return,
        crate::Occupancy::Other(owner) => {
            if refresh { crate::wifi::opportunity(ctx, &mac).await; }
            tracing::debug!(device_mac = %mac, owner_id = owner["id"].as_str().unwrap_or("?"),
                "device is occupied");
            return;
        }
        crate::Occupancy::Failed(error) => {
            tracing::debug!(device_mac = %mac,
                error_category = safe_protocol_error_category(&error), "device claim unavailable");
            return;
        }
    }
    let ota = service(ctx).ota_job(&mac);
    let publish = service(ctx).job(&mac);
    if ota.as_ref().is_some_and(|j| j.blocks_following_work()) {
        return;
    }
    if publish.as_ref().is_some_and(|j| j["state"] == "unknown") {
        return; // authenticated status already reconciled; do not resubmit an unknown install
    }
    let ota_first = ota.as_ref().is_some_and(|j| j.state == "queued")
        && publish.as_ref().filter(|j| matches!(j["state"].as_str(), Some("waiting" | "sending" | "unknown")))
            .and_then(|j| j["created_at"].as_u64())
            .is_none_or(|created| ota.as_ref().unwrap().created_at <= created);
    // A queued OTA has a bounded online window. A slow diagnostics transfer
    // must not consume the status freshness budget before its claim check.
    if refresh && !(ota_first && deliver_now) { sync_http(ctx, &mac, true).await; }
    if ota_first && bridge_mcp::load_device_token_at(&crate::mcp_config(ctx).data_root, &mac).is_none() {
        return;
    }
    let mut ota_window = ota_online_window(&refreshed);
    if refresh && !(ota_first && ota_window) {
        // Formal plan for the current rendezvous: the Bridge is the only source
        // of light/sleep decisions. A BOOT wake is answered with the *remaining*
        // provisional window; everything else is a fresh decision. Repeats are
        // idempotent on the device; a new id is only generated when the decision
        // really changes or the window has expired.
        let status = service(ctx)
            .coordinator_summary(&mac, device_now(&mac))
            .unwrap_or(Value::Null);
        let provisional = status["session"]["power"]["provisional"]
            .as_bool()
            .unwrap_or(false);
        let was_formal_light = status["session"]["power"]["mode"] == "light"
            && status["session"]["power"]["plan_id"].as_u64().unwrap_or(0) > 0
            && !provisional;
        let remaining = status["session"]["power"]["provisional_remaining_s"]
            .as_u64()
            .unwrap_or(0) as u32;
        let plan_result = if provisional {
            send_plan(ctx, &mac, "manual", remaining).await
        } else {
            send_plan(ctx, &mac, "rendezvous", 0).await
        };
        ota_window = plan_result["accepted"] == true
            && plan_result["ack"]["accepted_remaining_s"].as_u64().unwrap_or(0) >= 120;
        if plan_result["accepted"] == true && !(ota_first && deliver_now) &&
            (plan_result["ack"]["accepted_remaining_s"].as_u64().unwrap_or(0) > 0 ||
                was_formal_light) {
            sync_http(ctx, &mac,
                plan_result["ack"]["accepted_remaining_s"].as_u64().unwrap_or(0) > 0).await;
        }
    }
    if deliver_now {
        if ota_first {
            if !ota_window { return; }
            let _ = deliver_ota(ctx, &mac).await;
            return;
        }
        let _ = deliver(ctx, &mac).await;
    }
    if refresh { crate::wifi::opportunity(ctx, &mac).await; }
}

pub fn enqueue_ota(ctx: &AppCtx, mac: &str, args: &Value) -> Result<Value, String> {
    if args.get("device_ip").is_some() {
        return Err("device_ip override is not allowed for queued OTA; target by registered MAC".into());
    }
    let rom = args.get("rom").and_then(Value::as_str).ok_or("missing rom")?;
    let path = std::path::Path::new(rom);
    let path = if path.is_absolute() { path.to_path_buf() } else { ctx.root.join(path) };
    let version = args.get("expected_version").and_then(Value::as_str).ok_or("missing expected_version")?;
    let target = args.get("firmware_target").and_then(Value::as_str).ok_or("missing firmware_target")?;
    let request_id = args.get("request_id").and_then(Value::as_str).ok_or("missing request_id")?;
    let job = service(ctx).queue_ota(mac, &ctx.bridge_id, request_id, &path, version, target)
        .map_err(err_text)?;
    ctx.ota_work.notify_one();
    Ok(json!({"accepted": true, "job_id": job.job_id, "state": job.state, "device_mac": mac,
        "sha256": job.sha256, "size": job.size, "expected_version": job.expected_version}))
}

async fn deliver_ota(ctx: &AppCtx, mac: &str) -> Value {
    let lock = delivery_lock(ctx, mac);
    let _delivery = lock.lock().await;
    if !matches!(crate::device_occupancy_gate(ctx, mac).await, crate::Occupancy::Owned) {
        return json!({"result": "blocked", "reason": "not authenticated owner"});
    }
    let Some(job) = service(ctx).ota_job(mac) else { return json!({"result": "idle"}); };
    if job.state != "queued" { return json!({"result": job.state}); }
    let Some(link) = device_link_for_mac(ctx, mac) else { return json!({"result": "waiting_for_link"}); };
    let cfg = crate::mcp_config(ctx);
    if bridge_mcp::load_device_token_at(&cfg.data_root, mac).is_none() {
        return json!({"result": "blocked", "reason": "device operation token unavailable"});
    }
    let path = match service(ctx).ota_begin(mac) {
        Ok(Some(path)) => path,
        Ok(None) => return json!({"result": "idle"}),
        Err(e) => return json!({"result": "failed", "error": e.to_string()}),
    };
    let arm_link = link.clone();
    let arm_job = job.clone();
    let ticket = match blocking_device(ctx, mac, move || {
        let status = device_client::status(&arm_link.ip, &arm_link.token, timeout()).map_err(err_text)?;
        if status["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac).as_deref()
            != Some(arm_link.mac.as_str()) { return Err("OTA preflight MAC mismatch".into()); }
        if status["ota_auth"] == "token" || status["sync_v1"] != 1 { return Ok(None); }
        if status["sync"]["enabled"] != true { return Err("sync-v1 is not configured".into()); }
        let nonce = status["session_nonce"].as_str().ok_or("OTA session nonce missing")?;
        let fields = json!({"job_id": arm_job.job_id, "kind":"ota",
            "image_bytes": arm_job.size, "file_sha256": arm_job.sha256});
        let reply = device_client::sync(&arm_link.ip, &arm_link.token, &arm_link.mac,
            &arm_link.bridge_id, nonce, "arm", &format!("ota-arm-{}", arm_job.job_id),
            &fields, timeout()).map_err(err_text)?;
        let ticket = reply["ticket"].as_str().ok_or("OTA arm ticket missing")?;
        if ticket.len() != 32 || !ticket.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("OTA arm ticket invalid".into());
        }
        Ok(Some(ticket.to_owned()))
    }).await {
        Ok(ticket) => ticket,
        Err(error) => {
            let _ = service(ctx).ota_preflight_failed(mac, &error, false);
            return json!({"result":"queued","error":error});
        }
    };
    let result = if let Some(ticket) = ticket {
        bridge_mcp::firmware_upload_with_ticket(&cfg, &path, &link.ip, mac,
            &job.firmware_target, &ticket).await
    } else {
        bridge_mcp::firmware_upload(&cfg, &path, &link.ip, mac, &job.firmware_target).await
    };
    if let Err(error) = &result {
        let terminal = error.contains("UPDATE FAILED") || error.contains("ROM") || error.contains("read ");
        let before_upload = terminal || error.contains("not reachable")
            || error.contains("reports MAC") || error.contains("HTTP 401")
            || error.contains("HTTP 409") || error.contains("token unavailable");
        if before_upload {
            let _ = service(ctx).ota_preflight_failed(mac, error, terminal);
            return json!({"result": if terminal { "failed" } else { "queued" }, "error": error});
        }
    }
    let _ = service(ctx).ota_await_confirmation(mac, result.is_ok(), result.as_ref().err().map(String::as_str));
    ctx.ota_work.notify_one(); // Confirm independently; a reboot never renews the plan.
    json!({"result": "awaiting_confirmation", "upload_ack": result.is_ok(), "error": result.err()})
}

fn device_now(mac: &str) -> u64 {
    bridge_core::device_clock::wall_secs(mac)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `json` tool argument: accept either an embedded object or a JSON string.
fn json_arg(args: &Value) -> Result<Value, String> {
    match args.get("json") {
        Some(Value::String(text)) => serde_json::from_str(text).map_err(|e| format!("json: {e}")),
        Some(value) => Ok(value.clone()),
        None => Err("missing json".to_string()),
    }
}

/// MCP-tool adapter: same service calls as the UI commands.
pub async fn tool(ctx: &AppCtx, name: &str, args: &Value) -> Result<String, String> {
    let value = match name {
        "platform_device_register" => register_device(ctx, args).await?,
        "platform_wifi_action" => {
            let body=args.get("body").cloned().unwrap_or(json!({}));
            if ["credentials","password","token"].iter().any(|key| body.get(*key).is_some()) { return Err("credentials_use_device_ui".into()); }
            let op=args["op"].as_str().ok_or("missing op")?;
            // The current-environment post-state guard belongs to the page's
            // manual preview, not programmatic or automatic configuration.
            crate::wifi::action(ctx,args["mac"].as_str().ok_or("missing mac")?,if op=="sync" {"sync_cached"} else {op},body).await?
        },
        "platform_wifi_view" => crate::wifi::view(ctx,
            args["mac"].as_str().ok_or("missing mac")?, args["refresh"].as_bool().unwrap_or(false)).await?,
        "platform_overview" => overview(ctx),
        "platform_device_view" => device_view(ctx, args.get("mac").and_then(Value::as_str))?,
        "platform_device_detail" => device_detail(ctx,
            args.get("mac").and_then(Value::as_str).ok_or("missing mac")?,
            args.get("section").and_then(Value::as_str).ok_or("missing section")?)?,
        "platform_template_list" => templates(ctx),
        "platform_template_get" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("missing id")?;
            let target = args.get("render_target").and_then(|v| v.as_str());
            template_get(ctx, id, target)?
        }
        "platform_template_save" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("missing id")?;
            let target = args
                .get("render_target")
                .and_then(|v| v.as_str())
                .unwrap_or("epd-ssd1681-200x200-1bpp");
            let source = json_arg(args)?;
            template_save(ctx, id, target, &source)?
        }
        "platform_template_validate" => template_validate(&json_arg(args)?),
        "platform_profile_get" => profile_get(ctx),
        "platform_profile_save" => {
            let profile: Profile =
                serde_json::from_value(args.get("profile").cloned().ok_or("missing profile")?)
                    .map_err(err_text)?;
            profile_save(ctx, profile)?
        }
        "platform_data_sync_save" => {
            let mac = args.get("mac").and_then(Value::as_str).ok_or("missing mac")?;
            let enabled = args.get("enabled").and_then(Value::as_bool).ok_or("missing enabled")?;
            data_sync_save(ctx, mac, enabled)?
        }
        "platform_firmware_release_publish" => {
            let rom = args.get("rom").and_then(Value::as_str).ok_or("missing rom")?;
            let version = args.get("version").and_then(Value::as_str).ok_or("missing version")?;
            let target = args.get("firmware_target").and_then(Value::as_str)
                .ok_or("missing firmware_target")?;
            let path = std::path::Path::new(rom);
            let path = if path.is_absolute() { path.to_path_buf() } else { ctx.root.join(path) };
            json!({"release": service(ctx).publish_firmware_release(&path, version, target)
                .map_err(err_text)?})
        }
        "platform_family_profiles" => {
            family_profiles(ctx, args.get("render_target").and_then(Value::as_str))
        }
        "family_platform_profile_save" => {
            let profile: FamilyProfile =
                serde_json::from_value(args.get("profile").cloned().ok_or("missing profile")?)
                    .map_err(err_text)?;
            family_profile_save(ctx, profile)?
        }
        "platform_family_profile_delete" => {
            let render_target = args
                .get("render_target")
                .and_then(Value::as_str)
                .ok_or("missing render_target")?;
            let id = args.get("id").and_then(Value::as_str).ok_or("missing id")?;
            family_profile_delete(ctx, render_target, id)?
        }
        "platform_family_profile_copy" => {
            let mac = args
                .get("mac")
                .and_then(Value::as_str)
                .ok_or("missing mac")?;
            let id = args.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .ok_or("missing name")?;
            family_profile_copy_from_device(ctx, mac, id, name)?
        }
        "platform_publish" => {
            let mac = target_mac(ctx, args)?;
            publish(
                ctx,
                &mac,
                args.get("expected_target_id").and_then(Value::as_str),
            )
            .await?
        }
        "platform_publish_preview" => {
            let mac = target_mac(ctx, args)?;
            publish_preview(ctx, &mac)?
        }
        "platform_font_list" => font_list(ctx)?,
        "platform_font_import" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or("missing path")?;
            font_import(ctx, path)?
        }
        "platform_publish_cancel" => {
            let mac = target_mac(ctx, args)?;
            job_cancel(ctx, &mac)
        }
        "firmware_ota" => {
            let mac = target_mac(ctx, args)?;
            enqueue_ota(ctx, &mac, args)?
        }
        "firmware_ota_status" => {
            let mac = target_mac(ctx, args)?;
            json!({"job": service(ctx).ota_job(&mac)})
        }
        "firmware_ota_cancel" => {
            let mac = target_mac(ctx, args)?;
            service(ctx).ota_cancel(&mac).map_err(err_text)?
        }
        "platform_template_activate" => {
            let mac = target_mac(ctx, args)?;
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("missing id")?;
            activate(ctx, &mac, id).await?
        }
        "platform_data_sources" => data_sources(ctx),
        "platform_data_source_save" => {
            let source = args.get("source").cloned().ok_or("missing source")?;
            data_source_save(ctx, source)?
        }
        "platform_data_probe" => {
            let id = args
                .get("source_id")
                .and_then(|v| v.as_str())
                .ok_or("missing source_id")?;
            data_probe(ctx, id)?
        }
        "platform_power_view" => {
            let mac = target_mac(ctx, args)?;
            power_view(ctx, &mac)
        }
        "power_plan" => {
            let mac = target_mac(ctx, args)?;
            let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("auto");
            match mode {
                "light" => request_light(ctx, &mac).await,
                "sleep" => {
                    let lock = delivery_lock(ctx, &mac);
                    let _delivery = lock.lock().await;
                    // A fresh id is required: reusing an old id (or 0) would be
                    // rejected as stale by the device.
                    let plan = service(ctx)
                        .explicit_plan(
                            &mac,
                            bridge_core::platform::model::PlanMode::Sleep,
                            0,
                            "explicit",
                        )
                        .map_err(err_text)?;
                    let mut body = serde_json::to_value(&plan).unwrap();
                    body["bridge_id"] = json!(ctx.bridge_id);
                    let link = device_link_for_mac(ctx, &mac).ok_or("device token not cached")?;
                    let (ip, token, expected_mac) =
                        (link.ip.clone(), link.token.clone(), link.mac.clone());
                    let sent = body.clone();
                    let ack = blocking_device(ctx, &mac, move || {
                        device_client::plan(&ip, &token, &expected_mac, &sent, timeout())
                            .map_err(err_text)
                    })
                    .await?;
                    let confirmation = if ack["result"] == "applied"
                        && ack["plan_id"].as_u64() == Some(plan.plan_id)
                    {
                        service(ctx).note_plan_ack(
                            &mac,
                            plan.plan_id,
                            ack["accepted_remaining_s"].as_u64().unwrap_or(0) as u32,
                            false,
                            device_now(&mac),
                        )
                    } else {
                        json!({"outcome": "unconfirmed"})
                    };
                    json!({"plan": body, "ack": ack, "confirmation": confirmation})
                }
                other => return Err(format!("mode must be light|sleep (got {other})")),
            }
        }
        "platform_status_refresh" => {
            let mac = target_mac(ctx, args)?;
            refresh_status(ctx, &mac).await
        }
        // Explicit user/agent action: deliver pending data now instead of
        // waiting for the automatic cadence.
        "platform_push_now" => {
            let mac = target_mac(ctx, args)?;
            deliver(ctx, &mac).await
        }
        "platform_recovery" => {
            let digest = args.get("digest").cloned().ok_or("missing digest")?;
            let mac = target_mac(ctx, args)?;
            recovery(ctx, &mac, &digest)?
        }
        other => return Err(format!("unknown platform tool: {other}")),
    };
    serde_json::to_string_pretty(&value).map_err(err_text)
}

fn requested_mac(args: &Value) -> Result<Option<String>, String> {
    if let (Some(a), Some(b)) = (args.get("mac"), args.get("device_mac")) {
        if a.as_str().and_then(DeviceIdentity::normalized_mac)
            != b.as_str().and_then(DeviceIdentity::normalized_mac) {
            return Err("mac and device_mac disagree".into());
        }
    }
    let Some(value) = args.get("mac").or_else(|| args.get("device_mac")) else {
        return Ok(None);
    };
    let raw = value
        .as_str()
        .map(str::trim)
        .filter(|mac| !mac.is_empty())
        .ok_or("mac must be a non-empty string")?;
    DeviceIdentity::normalized_mac(raw)
        .map(Some)
        .ok_or_else(|| "invalid device MAC".to_string())
}

/// Resolve the MAC an MCP tool acts on: explicit `mac`, else the only
/// registered device. Two or more registered devices without a MAC is an error
/// (`design.md` §2) — never "the first entry", and never merely whichever
/// device the process has selected.
fn target_mac(ctx: &AppCtx, args: &Value) -> Result<String, String> {
    let Some(mac) = requested_mac(args)? else {
        return crate::sole_registered_mac(ctx);
    };
    if service(ctx).device_get(&mac).is_none() {
        return Err(format!("device {mac} is not registered"));
    }
    Ok(mac)
}

/// task-1 evidence: production `ble_cycle` holds `ctx.device_delivery` (global)
/// and the per-MAC delivery lock across the *unbounded* `link.close()` at the
/// end of the function. This module injects a hanging link close through the
/// existing fake-BLE seam (`CODEX_STATUS_SIM_BLE_ENDPOINTS`, the same seam
/// device-sim uses) and checks whether the next production delivery can run.
///
/// The injected fault matches the reported macOS failure: btleplug 0.11.8's
/// `disconnect()` future for a peripheral already removed by a natural link
/// loss can stay pending forever, and `link.close()` has no timeout.
#[cfg(test)]
mod ble_close_lock_tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, AtomicUsize, AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    use axum::body::Bytes;
    use axum::extract::{Query, State};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde::Deserialize;

    #[test]
    fn ota_usb_window_and_reboot_confirmation_guard() {
        assert!(ota_online_window(&json!({"presence":{"mode":"continuous"},"power":{"mode":"sleep","remaining_s":0}})));
        assert!(ota_online_window(&json!({"power":{"mode":"light","remaining_s":120}})));
        assert!(!ota_online_window(&json!({"power":{"mode":"light","remaining_s":119}})));
        let mut job: bridge_core::platform::service::OtaJob=serde_json::from_value(json!({
            "job_id":"job","request_id":"request","device_mac":"AABBCCDDEEFF","bridge_id":"bridge",
            "created_at":100,"state":"awaiting_confirmation","firmware_target":"note4","expected_version":"next",
            "sha256":"hash","size":1024,"blob":"blob","upload_ack":true,"cancel_requested":false,
            "before_boot_id":"old-boot","confirmation_started_at":110
        })).unwrap();
        assert!(ota_old_boot(&job,&json!({"boot_id":"old-boot","fw":"next"})));
        assert!(!ota_old_boot(&job,&json!({"boot_id":"new-boot","fw":"next"})));
        assert!(!ota_confirmation_expired(&job,229));
        assert!(ota_confirmation_expired(&job,230));
        job.confirmation_started_at=None; job.last_attempt_at=Some(105);
        assert!(!ota_confirmation_expired(&job,224));
        assert!(ota_confirmation_expired(&job,225));
    }

    /// `CODEX_STATUS_SIM_BLE_ENDPOINTS` is process-wide; keep the two tests apart.
    static SIM_ENV: Mutex<()> = Mutex::new(());
    const HUNG_MAC: &str = "7C4FADB93408";
    const OTHER_MAC: &str = "70041DD7A340";
    const TOKEN: &str = "ble-close-lock-token";
    const BLOCK_LIMIT: Duration = Duration::from_millis(500);

    /// Minimal device side of the fake-BLE seam. Every command is answered
    /// immediately; only `/sim/ble/close` can be made to never answer.
    struct FakeBle {
        mac: String,
        hang_close: bool,
        protocol: AtomicBool,
        ble_claim: AtomicBool,
        omit_free_owner: AtomicBool,
        claim_owner: Mutex<String>,
        claim_rejected: AtomicBool,
        lost_plan: AtomicBool,
        lost_network_start: AtomicBool,
        applied_plan: AtomicU64,
        rf_initial_ms: AtomicU64,
        rf_after_plan_ms: AtomicU64,
        rf_epoch: Mutex<std::time::Instant>,
        apply_plan: AtomicBool,
        network_started: AtomicBool,
        sync_opened: AtomicBool,
        blocked_op: Mutex<String>,
        wifi_match_write_delay_ms: AtomicU64,
        wifi_match_reply: Mutex<Value>,
        /// Device status reports `sync.due=true` / `enabled=true`, so a real
        /// `ble_cycle` takes its `sync_open` path (`sync_opened == true`).
        sync_due: bool,
        provisional_remaining_s: AtomicU64,
        status_extra: Mutex<Value>,
        closes: AtomicUsize,
        status_reads: AtomicUsize,
        fragments: Mutex<Vec<u8>>,
        ack: Mutex<Option<Vec<u8>>>,
        commands: Mutex<Vec<Value>>,
        /// Shared ordered observation log (`ble:close`, `http:/api/status`, ...).
        events: Arc<Mutex<Vec<String>>>,
    }

    #[derive(Deserialize)]
    struct FragmentQuery {
        offset: usize,
        #[serde(rename = "final")]
        last: u8,
    }

    fn command_ack(mac: &str, command: &Value, sync_due: bool) -> Value {
        let id = command["request_id"].as_str().unwrap_or("");
        match command["op"].as_str().unwrap_or("") {
            "status" => json!({
                "ack": "command", "request_id": id, "op": "status", "result": "applied",
                "device_mac": mac,
                "session_nonce": "00112233445566778899aabbccddeeff",
                "active_context_id": "lock-repro-context",
                "active_template_id": "",
                "power": {"mode": "sleep", "plan_id": 0, "remaining_s": 0,
                    "provisional_remaining_s": 0},
                "sync": {"v": 1, "enabled": sync_due, "rounds": if sync_due { 15 } else { 0 },
                    "due": sync_due, "retry_skip": 0, "pending": false}
            }),
            "plan" => json!({
                "ack": "command", "request_id": id, "op": "plan", "result": "applied",
                "device_mac": mac, "plan_id": command["plan_id"], "accepted_remaining_s": 0
            }),
            "network_start" => json!({
                "ack": "command", "request_id": id, "op": "network_start", "result": "accepted",
                "device_mac": mac, "plan_id": command["plan_id"]
            }),
            op => json!({"ack": "command", "request_id": id, "op": op,
                "result": "applied", "device_mac": mac}),
        }
    }

    async fn info(State(fake): State<Arc<FakeBle>>) -> impl IntoResponse {
        let initial = fake.rf_initial_ms.load(Ordering::SeqCst);
        let after = fake.rf_after_plan_ms.load(Ordering::SeqCst);
        let limit = if fake.applied_plan.load(Ordering::SeqCst) > 0 && after > 0 { after } else { initial };
        let remaining = limit.saturating_sub(fake.rf_epoch.lock().unwrap().elapsed().as_millis() as u64);
        (
            [("x-sim-ble-generation", "1")],
            Json(json!({"mac": fake.mac, "rendezvous": true, "fw": "lock-repro",
                "peerBonded":true,"peerEncrypted":true,"endpoint_registration":1,
                "wifi_management": 3,
                "rf_remaining_ms": if initial > 0 {json!(remaining)} else {Value::Null},
                "wc":u8::from(fake.protocol.load(Ordering::SeqCst)),
                "ble_claim":u8::from(fake.ble_claim.load(Ordering::SeqCst)),
                "wake_generation":if fake.protocol.load(Ordering::SeqCst) {json!(2)} else {Value::Null},
                "wake_seq":if fake.protocol.load(Ordering::SeqCst) {json!(9)} else {Value::Null}})),
        )
    }

    async fn token(State(fake): State<Arc<FakeBle>>) -> impl IntoResponse {
        *fake.ack.lock().unwrap() = Some(serde_json::to_vec(&json!({"ack":"auth","ok":true,"token":"a".repeat(32)})).unwrap());
        Json(json!({"result":"accepted"}))
    }

    async fn endpoint(State(fake): State<Arc<FakeBle>>, Json(body): Json<Value>) -> impl IntoResponse {
        let error = fake.status_extra.lock().unwrap()["_endpoint_error"].as_str().unwrap_or("").to_owned();
        *fake.ack.lock().unwrap() = Some(serde_json::to_vec(&json!({"ack":"endpoint",
            "request_id":body["request_id"],"device_mac":fake.mac,"bridge_id":body["bridge_id"],
            "ok":error.is_empty(),"result":if error.is_empty(){"applied"}else if error=="storage_unknown"{"unknown"}else{"rejected"},
            "stage":if error.is_empty(){if body["check_only"]==true {"verified"}else{"persisted"}}else{if body["check_only"]==true {"verify"}else{"persist"}},"error":error})).unwrap());
        Json(json!({"result": "accepted"}))
    }

    async fn fragment(
        State(fake): State<Arc<FakeBle>>,
        Query(query): Query<FragmentQuery>,
        body: Bytes,
    ) -> impl IntoResponse {
        {
            let mut buffer = fake.fragments.lock().unwrap();
            if buffer.len() < query.offset {
                buffer.resize(query.offset, 0);
            }
            if buffer.len() == query.offset {
                buffer.extend_from_slice(&body);
            } else {
                let end = query.offset + body.len();
                if end > buffer.len() {
                    buffer.resize(end, 0);
                }
                buffer[query.offset..end].copy_from_slice(&body);
            }
        }
        if query.last != 1 {
            return Json(json!({"result": "more"}));
        }
        let bytes = std::mem::take(&mut *fake.fragments.lock().unwrap());
        let Ok((frame,payload))=bridge_core::compact_frame::Header::decode(&bytes,8192) else {
            return Json(json!({"result":"rejected","error":"frame"}));
        };
        let command: Value = serde_json::from_slice(payload).unwrap_or(Value::Null);
        fake.commands.lock().unwrap().push(command.clone());
        let op = command["op"].as_str().unwrap_or("");
        if op == "wifi_match" {
            tokio::time::sleep(Duration::from_millis(fake.wifi_match_write_delay_ms.load(Ordering::SeqCst))).await;
        }
        let blocked_write = *fake.blocked_op.lock().unwrap() == format!("write:{op}");
        if blocked_write {
            std::future::pending::<()>().await;
        }
        let mut reply = command_ack(&fake.mac, &command, fake.sync_due);
        if op == "status" {
            reply["power"]["provisional_remaining_s"] = json!(fake.provisional_remaining_s.load(Ordering::SeqCst));
            for (key,value) in fake.status_extra.lock().unwrap().as_object().unwrap() { reply[key]=value.clone(); }
        }
        if fake.protocol.load(Ordering::SeqCst) {
            fake.events.lock().unwrap().push(format!("ble:{op}"));
            reply["wake_generation"] = json!(2); reply["wake_seq"] = json!(9);
            if op == "status" { reply["rendezvous_v"] = json!(1); }
            if op == "status" && fake.applied_plan.load(Ordering::SeqCst) > 0 {
                reply["power"]["plan_id"] = json!(fake.applied_plan.load(Ordering::SeqCst));
                reply["power"]["mode"] = json!("light");
                reply["power"]["remaining_s"] = json!(110);
            }
            if op == "plan" {
                if fake.apply_plan.load(Ordering::SeqCst) { fake.applied_plan.store(command["plan_id"].as_u64().unwrap(), Ordering::SeqCst); }
                if fake.rf_after_plan_ms.load(Ordering::SeqCst) > 0 { *fake.rf_epoch.lock().unwrap() = std::time::Instant::now(); }
                reply["accepted_remaining_s"] = command["light_duration_s"].clone();
            }
            if op == "network_start" { fake.network_started.store(true, Ordering::SeqCst); }
            if op == "sync_open" { fake.sync_opened.store(true, Ordering::SeqCst); }
        }
        if op == "sync_open" && fake.rf_initial_ms.load(Ordering::SeqCst) > 0 {
            tokio::time::sleep(Duration::from_millis(900)).await;
        }
        if op.starts_with("wifi_") {
            reply["ack"] = json!("wifi"); reply["ok"] = json!(true);
            match op {
                "wifi_active" => { reply["revision"] = json!(1); },
                "wifi_match" => {reply["network_id"]=json!("home");reply["version"]=json!(2);
                    reply["matches"]=json!(true);reply["connected"]=json!(true);
                    for (key,value) in fake.wifi_match_reply.lock().unwrap().as_object().unwrap() {reply[key]=value.clone();}},
                "wifi_status" => {
                    // A paged status read can outlive the original short exchange.
                    if fake.rf_initial_ms.load(Ordering::SeqCst) > 0 {tokio::time::sleep(Duration::from_millis(450)).await;}
                    let text=serde_json::to_vec(&json!({"device_mac":fake.mac,
                        "boot_id":"00112233445566778899aabbccddeeff",
                        "wifi_control":{"principal":"bridge-lock-repro","namespace":"own","read_allowed":true,"write_allowed":true}})).unwrap();
                    let offset=command["offset"].as_u64().unwrap_or(0) as usize;
                    let end=(offset+100).min(text.len());
                    reply["snapshot"]=json!(1);reply["total"]=json!(text.len());reply["next_offset"]=json!(end);
                    reply["chunk"]=json!(text[offset..end].iter().map(|b|format!("{b:02x}")).collect::<String>());
                },
                "wifi_snapshot" if fake.rf_initial_ms.load(Ordering::SeqCst) > 0 => {
                    let text=serde_json::to_string(&json!({"schema":2,"device_mac":fake.mac,
                        "revision":8,"snapshot_id":"rf-read","complete":true,"observed_at":0,
                        "capacity":{"total":12},"networks":[]})).unwrap();
                    reply["schema"]=json!(2);reply["revision"]=json!(8);reply["snapshot_id"]=json!("rf-read");
                    reply["offset"]=json!(0);reply["total"]=json!(text.len());reply["next_offset"]=json!(text.len());
                    reply["chunk"]=json!(text);reply["complete"]=json!(true);
                },
                "wifi_save" | "wifi_result" => { reply["outcome"] = json!("committed"); reply["revision"] = json!(2); reply["bond"]=json!("own"); },
                "wifi_result_ack" => {reply["outcome"]=json!("retired");},
                "wifi_scheme_get" => {
                    let text=serde_json::to_vec(&json!({"networks":[{"network_id":"home","host_ssid":"Host","ssid":"Device","version":2,"credential":"open"}]})).unwrap();
                    let offset=command["offset"].as_u64().unwrap_or(0) as usize;
                    let end=(offset+100).min(text.len());
                    reply["snapshot"]=json!(1); reply["total"]=json!(text.len()); reply["next_offset"]=json!(end);
                    reply["chunk"]=json!(text[offset..end].iter().map(|b|format!("{b:02x}")).collect::<String>());
                },
                _ => {},
            }
        }
        if fake.ble_claim.load(Ordering::SeqCst) && op == "wifi_owner" {
            let id=fake.claim_owner.lock().unwrap();
            if !id.is_empty() || !fake.omit_free_owner.load(Ordering::SeqCst) { reply["owner_id"] = json!(*id); }
        }
        if fake.ble_claim.load(Ordering::SeqCst) && op == "claim" {
            if fake.claim_rejected.load(Ordering::SeqCst) {
                reply["result"]=json!("rejected");reply["error"]=json!("storage");
            } else {
                *fake.claim_owner.lock().unwrap()=command["bridge_id"].as_str().unwrap().into();
            }
            reply["owner_id"]=json!(*fake.claim_owner.lock().unwrap());reply["expires_in_s"]=json!(300);
        }
        let blocked = (op == "plan" && fake.lost_plan.load(Ordering::SeqCst)) ||
            (op == "network_start" && fake.lost_network_start.load(Ordering::SeqCst)) ||
            *fake.blocked_op.lock().unwrap() == op;
        if !blocked {
            let payload=if op=="status" && fake.protocol.load(Ordering::SeqCst) {
                let mut wire=reply.clone();
                wire["device_mac"]=json!(fake.mac.as_bytes().chunks(2)
                    .map(|part|std::str::from_utf8(part).unwrap()).collect::<Vec<_>>().join(":"));
                bridge_render::simulator_ble_wire("status", &wire).unwrap()
            } else { serde_json::to_vec(&reply).unwrap() };
            let response=bridge_core::compact_frame::Header{kind:1,op:frame.op,
                result:if reply["result"]=="accepted" {1} else if reply["result"]=="rejected" {2}else{0},
                payload_len:payload.len() as u16,
                session:[0x00,0x11,0x22,0x33,0x44,0x55,0x66,0x77,0x88,0x99,0xaa,0xbb,0xcc,0xdd,0xee,0xff],
                request_no:frame.request_no,context:frame.context};
            let mut bytes=response.encode().unwrap().to_vec();bytes.extend_from_slice(&payload);
            *fake.ack.lock().unwrap() = Some(bytes);
        }

        Json(json!({"result": "accepted"}))
    }

    async fn status(State(fake): State<Arc<FakeBle>>) -> Response {
        fake.status_reads.fetch_add(1, Ordering::SeqCst);
        let reply = fake
            .ack
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| b"{}".to_vec());
        (
            [("content-type", bridge_core::compact_frame::CONTENT_TYPE)],
            axum::body::Body::from(reply),
        )
            .into_response()
    }

    async fn close(State(fake): State<Arc<FakeBle>>) -> Response {
        fake.closes.fetch_add(1, Ordering::SeqCst);
        fake.events.lock().unwrap().push("ble:close".to_owned());
        if fake.hang_close {
            // Accepted, never answered: the caller's disconnect never completes.
            std::future::pending::<()>().await;
        }
        Json(json!({"result": "closed"})).into_response()
    }

    async fn start_fake_ble(mac: &str, hang_close: bool) -> (Arc<FakeBle>, String) {
        start_fake_ble_with(mac, hang_close, false, Arc::new(Mutex::new(Vec::new()))).await
    }

    async fn start_fake_ble_with(
        mac: &str,
        hang_close: bool,
        sync_due: bool,
        events: Arc<Mutex<Vec<String>>>,
    ) -> (Arc<FakeBle>, String) {
        let fake = Arc::new(FakeBle {
            mac: mac.to_owned(),
            hang_close,
            protocol: AtomicBool::new(false),
            ble_claim: AtomicBool::new(false),
            omit_free_owner: AtomicBool::new(false),
            claim_owner: Mutex::new(String::new()),
            claim_rejected: AtomicBool::new(false), lost_plan: AtomicBool::new(false), lost_network_start: AtomicBool::new(false),
            applied_plan: AtomicU64::new(0), apply_plan: AtomicBool::new(true), network_started: AtomicBool::new(false), sync_opened: AtomicBool::new(false), blocked_op: Mutex::new(String::new()), wifi_match_write_delay_ms: AtomicU64::new(0), wifi_match_reply: Mutex::new(json!({})),
            rf_initial_ms: AtomicU64::new(0), rf_after_plan_ms: AtomicU64::new(0), rf_epoch: Mutex::new(std::time::Instant::now()),
            sync_due,
            provisional_remaining_s: AtomicU64::new(0),
            status_extra: Mutex::new(json!({})),
            closes: AtomicUsize::new(0),
            status_reads: AtomicUsize::new(0),
            fragments: Mutex::new(Vec::new()),
            ack: Mutex::new(None),
            commands: Mutex::new(Vec::new()),
            events,
        });
        let app = Router::new()
            .route("/sim/ble/info", get(info))
            .route("/sim/ble/discover", post(|| async {Json(json!({"connected":true,"generation":1}))}))
            .route("/sim/ble/device", get(|| async {Json(json!({"connected":true,"generation":1,"close_callback":true}))}))
            .route("/sim/ble/endpoint", post(endpoint))
            .route("/sim/ble/token", post(token))
            .route("/sim/ble/fragment", post(fragment))
            .route("/sim/ble/status", get(status))
            .route("/sim/ble/close", post(close))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (fake, format!("http://{address}"))
    }

    async fn wait_for_close(fake: &FakeBle, limit: Duration) -> usize {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let observed = fake.closes.load(Ordering::SeqCst);
            if observed > 0 || tokio::time::Instant::now() >= deadline {
                return observed;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// The smallest production `AppCtx` that can run `ble_cycle`: same lock
    /// fields, same PlatformService, no Tauri handle, no tray, no server.
    fn test_ctx(root: &std::path::Path, macs: &[&str]) -> AppCtx {
        let platform = Arc::new(PlatformService::open(root).expect("open platform state"));
        for mac in macs {
            platform
                .device_upsert(
                    DeviceIdentity::new(mac, "BLE lock repro").unwrap(),
                    DeviceCapabilities::ssd1681_154g(),
                )
                .unwrap();
        }
        let config = crate::config::Config {
            token: TOKEN.to_owned(),
            port: 18765,
            mcp_port: 18766,
            ..Default::default()
        };
        let mcp_port = config.mcp_port;
        let (mcp_tx, _mcp_rx) = tokio::sync::watch::channel(mcp_port);
        AppCtx {
            instance: crate::instance::Instance {
                name: None,
                shape: crate::icon::Shape::Square,
            },
            config,
            root: root.to_path_buf(),
            app_handle: OnceLock::new(),
            envelope: Arc::new(tokio::sync::RwLock::new(None)),
            library: Arc::new(tokio::sync::RwLock::new(
                bridge_core::template::Library::default(),
            )),
            force_ble: Arc::new(tokio::sync::Notify::new()),
            ota_work: Arc::new(tokio::sync::Notify::new()),
            device_delivery: tokio::sync::Mutex::new(()),
            sim_control: tokio::sync::Mutex::new(()),
            per_mac_delivery: Mutex::new(HashMap::new()),
            report_sessions: Mutex::new(HashMap::new()),
            status: Mutex::new(crate::RuntimeStatus {
                last_sync: None,
                last_error: None,
                last_error_at: None,
                paused: false,
            }),
            devices: Mutex::new(crate::device_runtime::DeviceRegistry::default()),
            bridge_name: Mutex::new("BLE lock repro".to_owned()),
            bridge_id: "bridge-lock-repro".to_owned(),
            device_status_cache: Mutex::new(HashMap::new()),
            arp_running: AtomicBool::new(false),
            activity: Arc::new(bridge_core::activity::Activity::new()),
            mcp_port: Mutex::new(mcp_port),
            mcp_error: Mutex::new(None),
            mcp_tx,
            platform,
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn endpoint_failure_reaches_authorize_and_rendezvous_without_business_writes() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let dir=tempfile::tempdir().unwrap();
        let (fake,url)=start_fake_ble(HUNG_MAC,false).await;
        let ctx=test_ctx(dir.path(),&[HUNG_MAC]);
        bridge_mcp::save_device_token_at(dir.path(),HUNG_MAC,&"a".repeat(32)).unwrap();
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        for error in ["storage","storage_unknown","capacity","physical_required"] {
            fake.status_extra.lock().unwrap()["_endpoint_error"]=json!(error);
            let feedback=crate::wifi::action(&ctx,HUNG_MAC,"authorize",json!({})).await.unwrap();
            assert_eq!(feedback["error"],error);
            assert_eq!(feedback["stage"],"verify");
            assert_eq!(feedback["outcome"],if error=="storage_unknown"{"unknown"}else{"rejected"});
            assert!(feedback["action"].is_string());
            let result=ble_cycle(&ctx,&[HUNG_MAC.into()]).await;
            assert!(matches!(&result,Ok(BleOpportunity::Connected{result:Err(e),..}) if e.contains(error) && e.contains("verify")),"{result:?}");
            assert!(fake.commands.lock().unwrap().is_empty());
        }
        assert_eq!(fake.closes.load(Ordering::SeqCst),8);
        fake.status_extra.lock().unwrap().as_object_mut().unwrap().remove("_endpoint_error");
        let result=ble_cycle(&ctx,&[HUNG_MAC.into()]).await;
        assert!(matches!(result,Ok(BleOpportunity::Connected{result:Ok(()),..})),"{result:?}");
        assert!(fake.commands.lock().unwrap().iter().any(|c|c["op"]=="status"));
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn ble_first_claim_precedes_plan_and_never_forces_or_retries_unknown() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for case in ["free","free_legacy","occupied","storage","yielded","lost_ack"] {
            let dir=tempfile::tempdir().unwrap();
            let (fake,url)=start_fake_ble(HUNG_MAC,false).await;
            fake.ble_claim.store(true,Ordering::SeqCst);
            if case=="free_legacy" { fake.omit_free_owner.store(true,Ordering::SeqCst); }
            if case=="occupied" { *fake.claim_owner.lock().unwrap()="other-host".into(); }
            if case=="storage" { fake.claim_rejected.store(true,Ordering::SeqCst); }
            if case=="lost_ack" { *fake.blocked_op.lock().unwrap()="claim".into(); }
            let ctx=test_ctx(dir.path(),&[HUNG_MAC]);
            if case=="yielded" { crate::update_device_claim_cache(&mut ctx.device_status_cache.lock().unwrap(),HUNG_MAC,None,device_now(HUNG_MAC) as i64,true); }
            bridge_mcp::save_device_token_at(dir.path(),HUNG_MAC,&"a".repeat(32)).unwrap();
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            let result=ble_cycle(&ctx,&[HUNG_MAC.into()]).await;
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
            let commands=fake.commands.lock().unwrap();
            let ops=commands.iter().map(|c|c["op"].as_str().unwrap()).collect::<Vec<_>>();
            if case.starts_with("free") {
                assert!(matches!(result,Ok(BleOpportunity::Connected{result:Ok(()),..})),"{result:?}");
                assert!(ops.iter().position(|op|*op=="claim").unwrap()<ops.iter().position(|op|*op=="plan").unwrap());
            } else {
                let expected=match case {"occupied"=>"occupied","storage"=>"storage","yielded"=>"owner_yielded",_=>"claim_result_unknown"};
                assert!(matches!(&result,Ok(BleOpportunity::Connected{result:Err(e),..}) if e==expected),"{case}: {result:?}");
                assert!(!ops.contains(&"plan"));
            }
            let claims=commands.iter().filter(|c|c["op"]=="claim").collect::<Vec<_>>();
            assert!(claims.len()<=1);
            for command in claims {
                assert_eq!(command["operation_token"],"a".repeat(32));
                assert!(command["force"].is_null()&&command["release"].is_null());
            }
        }
    }

    /// task-1 RED case, now a regression: with `link.close_bounded` + locks
    /// released before cleanup, a hanging `/sim/ble/close` no longer blocks
    /// HTTP delivery or the next rendezvous. (task-4 recovery un-ignored it.)
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn link_close_hang_must_not_block_http_delivery_or_next_rendezvous() {
        let _serial = SIM_ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let (fake, url) = start_fake_ble(HUNG_MAC, true).await;
        let ctx = Arc::new(test_ctx(dir.path(), &[HUNG_MAC, OTHER_MAC]));
        std::env::set_var(
            "CODEX_STATUS_SIM_BLE_ENDPOINTS",
            json!({HUNG_MAC: url}).to_string(),
        );

        let rendezvous = tokio::spawn({
            let ctx = ctx.clone();
            async move { ble_cycle(&ctx, &[HUNG_MAC.to_owned()]).await }
        });
        let closes = wait_for_close(&fake, Duration::from_secs(15)).await;
        assert!(
            closes > 0,
            "production ble_cycle never reached link.close(): the fault injection did not engage"
        );

        // The reported symptom: after the link close stops returning, normal
        // HTTP data/bundle delivery for the same device never starts.
        let http_started = tokio::time::Instant::now();
        let http_blocked = tokio::time::timeout(BLOCK_LIMIT, deliver(&ctx, HUNG_MAC))
            .await
            .is_err();
        let http_elapsed = http_started.elapsed();
        // And the next rendezvous, for any device, never starts either.
        let next_blocked =
            tokio::time::timeout(BLOCK_LIMIT, ble_cycle(&ctx, &[OTHER_MAC.to_owned()]))
                .await
                .is_err();
        // Direct lock-level observation for the report (not an assertion: after
        // the fix the locks are released and this must report `false`).
        let global_held = ctx.device_delivery.try_lock().is_err();
        let per_mac_held = delivery_lock(&ctx, HUNG_MAC).try_lock().is_err();

        eprintln!(
            "[task-1 red] /sim/ble/close observed={closes}; HTTP deliver({HUNG_MAC}) \
             blocked={http_blocked} after {}ms; next ble_cycle({OTHER_MAC}) blocked={next_blocked}; \
             global device_delivery held={global_held}; per-MAC lock held={per_mac_held}",
            http_elapsed.as_millis()
        );

        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
        // The hanging close must not leak either: `close_bounded` gives up at
        // BLE_CLOSE_LIMIT and the rendezvous task still returns (task-4 added
        // this, so the test proves both "locks released early" and "the close
        // itself is bounded"). 5 s = BLE_CLOSE_LIMIT + generous margin.
        let rendezvous_started = tokio::time::Instant::now();
        let rendezvous_finished = tokio::time::timeout(Duration::from_secs(5), rendezvous).await;
        let rendezvous_elapsed_ms = rendezvous_started.elapsed().as_millis();

        assert!(
            !http_blocked && !next_blocked,
            "RED(task-1): while link.close() hung (/sim/ble/close unanswered, observed={closes}), \
             production HTTP delivery blocked={http_blocked} (per-MAC lock held={per_mac_held}) and \
             the next production rendezvous blocked={next_blocked} (global device_delivery lock \
             held={global_held}); the {BLOCK_LIMIT:?} budget was fully spent"
        );
        assert!(
            rendezvous_finished.is_ok(),
            "bounded close did not return: production ble_cycle still stuck after {}ms",
            rendezvous_elapsed_ms
        );
        let outcome = rendezvous_finished.unwrap().expect("rendezvous task panicked");
        assert!(
            outcome.is_ok(),
            "ble_cycle returned an error after the bounded close: {outcome:?}"
        );
    }

    /// Causality control: the exact same sequence with a bounded close must not
    /// block anything. This passes before and after the fix, so a failure of the
    /// RED test above can only come from the hanging close.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn bounded_link_close_control_does_not_block_delivery_or_next_rendezvous() {
        let _serial = SIM_ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let (fake, url) = start_fake_ble(HUNG_MAC, false).await;
        let ctx = Arc::new(test_ctx(dir.path(), &[HUNG_MAC, OTHER_MAC]));
        std::env::set_var(
            "CODEX_STATUS_SIM_BLE_ENDPOINTS",
            json!({HUNG_MAC: url}).to_string(),
        );

        let rendezvous = tokio::spawn({
            let ctx = ctx.clone();
            async move { ble_cycle(&ctx, &[HUNG_MAC.to_owned()]).await }
        });
        assert_eq!(wait_for_close(&fake, Duration::from_secs(15)).await, 1);
        let finished = tokio::time::timeout(Duration::from_secs(10), rendezvous).await;
        let http = tokio::time::timeout(BLOCK_LIMIT, deliver(&ctx, HUNG_MAC)).await;
        let next = tokio::time::timeout(BLOCK_LIMIT, ble_cycle(&ctx, &[OTHER_MAC.to_owned()])).await;
        let commands = fake.commands.lock().unwrap().len();

        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");

        assert!(
            finished.is_ok(),
            "controlled rendezvous did not finish after a bounded close"
        );
        assert!(
            http.is_ok(),
            "control: HTTP delivery blocked although the close returned (commands={commands})"
        );
        assert!(
            next.is_ok(),
            "control: next rendezvous blocked although the close returned (commands={commands})"
        );
    }

    // -----------------------------------------------------------------------
    // W6 combo regression: production-app order (C13) and slow onboarding (C01)
    // -----------------------------------------------------------------------

    /// Minimal std-socket device HTTP surface. Every request is answered with
    /// the same JSON body; the Nth (`delay_status_at`) `/api/status` reply can
    /// be delayed to model slow Wi-Fi association/DHCP (C01). Both fake servers
    /// append to one shared ordered log so the production step order (C13) can
    /// be asserted from the device side instead of inferred.
    struct FakeHttp {
        events: Arc<Mutex<Vec<String>>>,
        paths: Mutex<Vec<String>>,
        status_calls: AtomicUsize,
        delay_status_at: usize,
        delay: Duration,
        slow_reply_sent: AtomicBool,
        status: Value,
    }
    fn framed_test_reply(frame:bridge_core::compact_frame::Header,
        body:Value) -> Response {
        let payload=serde_json::to_vec(&body).unwrap();
        let header=bridge_core::compact_frame::Header{kind:1,op:frame.op,result:0,
            payload_len:payload.len() as u16,session:[9;16],request_no:frame.request_no,
            context:frame.context};
        let mut bytes=header.encode().unwrap().to_vec();bytes.extend_from_slice(&payload);
        ([(axum::http::header::CONTENT_TYPE,bridge_core::compact_frame::CONTENT_TYPE)],bytes).into_response()
    }

    fn start_fake_http(
        events: Arc<Mutex<Vec<String>>>,
        status: Value,
        delay_status_at: usize,
        delay: Duration,
    ) -> (Arc<FakeHttp>, String) {
        let fake = Arc::new(FakeHttp {
            events,
            paths: Mutex::new(Vec::new()),
            status_calls: AtomicUsize::new(0),
            delay_status_at,
            delay,
            slow_reply_sent: AtomicBool::new(false),
            status,
        });
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = fake.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue; };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let server = server.clone();
                std::thread::spawn(move || {
                    let mut raw = Vec::new();
                    let mut chunk = [0u8; 1024];
                    loop {
                        match stream.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(read) => {
                                raw.extend_from_slice(&chunk[..read]);
                                if let Some(end)=raw.windows(4).position(|w|w==b"\r\n\r\n") {
                                    let head=String::from_utf8_lossy(&raw[..end]);
                                    let len=head.lines().find_map(|line| {
                                        let (name,value)=line.split_once(':')?;
                                        name.eq_ignore_ascii_case("Content-Length")
                                            .then(||value.trim().parse::<usize>().ok()).flatten()
                                    }).unwrap_or(0);
                                    if raw.len()>=end+4+len {break;}
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    let head = String::from_utf8_lossy(&raw).into_owned();
                    let path = head
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/")
                        .to_owned();
                    server.paths.lock().unwrap().push(path.clone());
                    server.events.lock().unwrap().push(format!("http:{path}"));
                    if path.starts_with("/api/status") {
                        let index = server.status_calls.fetch_add(1, Ordering::SeqCst) + 1;
                        if server.delay_status_at > 0 && index >= server.delay_status_at {
                            std::thread::sleep(server.delay);
                            // Recorded before the write: the point is that the
                            // device *did* answer, only later than the budget.
                            server.slow_reply_sent.store(true, Ordering::SeqCst);
                        }
                    }
                    let start=raw.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;
                    let Ok((frame,_))=bridge_core::compact_frame::Header::decode(&raw[start..],4096) else {return;};
                    let payload = serde_json::to_vec(&server.status).unwrap();
                    let header=bridge_core::compact_frame::Header{kind:1,op:frame.op,result:0,
                        payload_len:payload.len() as u16,session:[9;16],request_no:frame.request_no,context:frame.context};
                    let mut body=header.encode().unwrap().to_vec();body.extend_from_slice(&payload);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.codex-status.frame\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.write_all(&body);
                    let _ = stream.flush();
                });
            }
        });
        (fake, address.to_string())
    }

    /// Same as [`test_ctx`] but with a reachable device address, which the
    /// production HTTP steps need (`device_link_for_mac` requires `ip`).
    fn test_ctx_with_ip(root: &std::path::Path, mac: &str, ip: &str) -> AppCtx {
        let ctx = test_ctx(root, &[mac]);
        let mut identity = DeviceIdentity::new(mac, "Combo regression").unwrap();
        identity.ip = Some(ip.to_owned());
        service(&ctx)
            .device_upsert(identity, DeviceCapabilities::ssd1681_154g())
            .unwrap();
        ctx
    }

    fn queue_read_job(ctx: &AppCtx, mac: &str) -> String {
        service(ctx)
            .wifi_queue_fields(mac, &ctx.bridge_id, "read", vec![], BTreeMap::new(), device_now(mac))
            .expect("queue read job")
            .job_id
    }

    #[tokio::test(flavor="multi_thread", worker_threads=4)]
    async fn ble_only_cycle_does_not_probe_http() {
        let _serial = SIM_ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let (ble, url) = start_fake_ble_with(HUNG_MAC, false, false, events.clone()).await;
        let (http, ip) = start_fake_http(events, plain_status_body(HUNG_MAC), 0, Duration::ZERO);
        let ctx = test_ctx_with_ip(dir.path(), HUNG_MAC, &ip);
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS", json!({HUNG_MAC:url}).to_string());
        let result = tokio::time::timeout(Duration::from_secs(12), ble_cycle(&ctx, &[HUNG_MAC.to_owned()])).await;
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
        assert!(result.is_ok_and(|r| r.is_ok()));
        assert_eq!(http.status_calls.load(Ordering::SeqCst), 0);
        let plans:Vec<_> = ble.commands.lock().unwrap().iter().filter(|c| c["op"] == "plan")
            .map(|c| c["need_http"].clone()).collect();
        assert_eq!(plans, vec![json!(false)]);
    }

    #[tokio::test]
    async fn ready_report_ack_is_only_a_hint_and_rejects_wrong_binding() {
        use axum::{extract::{State,ConnectInfo}, http::{HeaderMap, HeaderValue, header}, Json};
        async fn send(ctx:Arc<crate::AppCtx>,peer:ConnectInfo<std::net::SocketAddr>,
            mut auth:HeaderMap,mut report:Value) -> (axum::http::StatusCode,Value) {
            let session=report["session_id"].as_str().unwrap().to_owned();
            let number=report["report_request_no"].as_u64().unwrap();
            report.as_object_mut().unwrap().remove("session_id");
            report.as_object_mut().unwrap().remove("report_request_no");
            let mut bytes=[0u8;16];
            for (index,byte) in bytes.iter_mut().enumerate() {
                *byte=u8::from_str_radix(&session[index*2..index*2+2],16).unwrap();
            }
            let payload=serde_json::to_vec(&report).unwrap();
            let header=bridge_core::compact_frame::Header{kind:2,op:20,result:0,
                payload_len:payload.len() as u16,session:bytes,request_no:number,context:[0;16]};
            let mut frame=header.encode().unwrap().to_vec();frame.extend_from_slice(&payload);
            auth.insert(header::CONTENT_TYPE,HeaderValue::from_static(bridge_core::compact_frame::CONTENT_TYPE));
            let response=crate::framed_ready_report(State(ctx),peer,auth,axum::body::Bytes::from(frame)).await;
            let status=response.status();
            let body=axum::body::to_bytes(response.into_body(),4096).await.unwrap();
            let (_,payload)=bridge_core::compact_frame::Header::decode(&body,4096).unwrap();
            (status,serde_json::from_slice(payload).unwrap())
        }
        let dir=tempfile::tempdir().unwrap();
        let ctx=Arc::new(test_ctx_with_ip(dir.path(),HUNG_MAC,"192.0.2.10"));
        let plan=service(&ctx).plan_for_rendezvous(HUNG_MAC,device_now(HUNG_MAC),"rendezvous",0).unwrap();
        let peer=ConnectInfo("192.0.2.10:49152".parse().unwrap());
        let mut auth=HeaderMap::new();
        auth.insert(header::AUTHORIZATION,HeaderValue::from_static("Bearer fake-endpoint-token"));
        let hello=json!({"device_mac":HUNG_MAC,"boot_id":"0123456789abcdef0123456789abcdef",
            "principal":ctx.bridge_id,"instance_id":ctx.bridge_id});
        let (bad,_) = crate::report_session(State(ctx.clone()),peer,HeaderMap::new(),Json(hello.clone())).await;
        assert_eq!(bad,axum::http::StatusCode::UNAUTHORIZED);
        auth.insert(header::AUTHORIZATION,HeaderValue::from_str(&format!("Bearer {}",ctx.config.token)).unwrap());
        let (ok,Json(session)) = crate::report_session(State(ctx.clone()),peer,auth.clone(),Json(hello.clone())).await;
        assert_eq!(ok,axum::http::StatusCode::OK);
        let mut report=json!({"device_mac":HUNG_MAC,"boot_id":hello["boot_id"],
            "principal":ctx.bridge_id,"instance_id":ctx.bridge_id,
            "session_id":session["session_id"],"origin_session":hello["boot_id"],
            "origin_request_no":1,"report_request_no":1,"business_id":plan.plan_id.to_string(),"context_id":"",
            "ip":"192.0.2.10","port":80,"http_ready":true,"remaining_ms":100,
            "plan_result":"applied","data_result":"applied","network_result":"ready"});
        report["instance_id"]="wrong".into();
        let (bad,_) = send(ctx.clone(),peer,auth.clone(),report.clone()).await;
        assert_eq!(bad,axum::http::StatusCode::CONFLICT);
        report["instance_id"]=json!(ctx.bridge_id);
        report["business_id"]="999999".into();
        let (wrong_plan,_) = send(ctx.clone(),peer,auth.clone(),report.clone()).await;
        assert_eq!(wrong_plan,axum::http::StatusCode::CONFLICT);
        report["business_id"]=json!(plan.plan_id.to_string());
        let (received,reply)=send(ctx.clone(),peer,auth.clone(),report.clone()).await;
        assert_eq!(received,axum::http::StatusCode::ACCEPTED);
        assert_eq!(reply["result"],"report_received");
        let accepted_report=report.clone();
        let (replay,_) = send(ctx.clone(),peer,auth.clone(),report.clone()).await;
        assert_eq!(replay,axum::http::StatusCode::ACCEPTED);
        let mut late=report.clone();
        late["report_request_no"]=json!(2);late["remaining_ms"]=json!(0);
        let (expired,_) = send(ctx.clone(),peer,auth.clone(),late).await;
        assert_eq!(expired,axum::http::StatusCode::CONFLICT);
        report["network_result"]="different".into();
        let (conflict,_) = send(ctx.clone(),peer,auth.clone(),report.clone()).await;
        assert_eq!(conflict,axum::http::StatusCode::CONFLICT);
        report["ip"]="192.0.2.11".into();
        let (wrong_ip,_) = send(ctx.clone(),peer,auth,report).await;
        assert_eq!(wrong_ip,axum::http::StatusCode::CONFLICT);
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(service(&ctx).coordinator_summary(HUNG_MAC,device_now(HUNG_MAC)).unwrap()["plan"]["last_accepted_id"],0);
        // A process restart has a fresh reverse-direction session; a delayed
        // report from the old process cannot establish business completion.
        let restarted=Arc::new(test_ctx_with_ip(dir.path(),HUNG_MAC,"192.0.2.10"));
        let mut old_auth=HeaderMap::new();
        old_auth.insert(header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}",restarted.config.token)).unwrap());
        let (late,_) = send(restarted,peer,old_auth,accepted_report).await;
        assert_eq!(late,axum::http::StatusCode::CONFLICT);
    }

    #[test]
    fn ready_report_requires_current_authenticated_light_owner_fact() {
        let mut status=json!({"device_mac":HUNG_MAC,"boot_id":"same-boot",
            "power":{"plan_id":7,"mode":"light","remaining_s":25},
            "transports":{"http":{"ready":true}},"owner_id":"bridge-lock-repro"});
        let verified=|body:&Value|crate::report_fact_verified(body,HUNG_MAC,
            "same-boot",7,"bridge-lock-repro");
        assert!(verified(&status));
        for (path,value) in [
            ("device_mac",json!(OTHER_MAC)),
            ("boot_id",json!("restarted")),
            ("power.plan_id",json!(8)),
            ("power.mode",json!("sleep")),
            ("power.remaining_s",json!(0)),
            ("transports.http.ready",json!(false)),
            ("owner_id",json!("other")),
            ("owner_id",Value::Null),
        ] {
            let mut changed=status.clone();
            let mut slot=&mut changed;
            for segment in path.split('.') {slot=&mut slot[segment];}
            *slot=value;
            assert!(!verified(&changed),"report must remain a hint for {path}");
        }
        status["power"]["remaining_s"]=json!(0);
        assert!(!verified(&status));
    }

    fn observed_job(ctx: &AppCtx, mac: &str, job_id: &str) -> (String, Option<String>) {
        let job = service(ctx)
            .wifi_state(mac)
            .jobs
            .into_iter()
            .find(|j| j.job_id == job_id)
            .expect("queued job disappeared");
        (job.state, job.error_code)
    }

    fn command_ops(fake: &FakeBle) -> Vec<String> {
        fake.commands
            .lock()
            .unwrap()
            .iter()
            .map(|c| c["op"].as_str().unwrap_or("").to_owned())
            .collect()
    }

    /// A status body that makes the production HTTP steps terminate in one
    /// request each without writing anything: the diagnostic drain stops at
    /// `sync_v1 != 1`, and the Wi-Fi opportunity job is refused as
    /// `update_required` (`wifi_management != 3`).
    fn plain_status_body(mac: &str) -> Value {
        json!({"device_mac": mac, "sync_v1": 0, "sync": {"enabled": false},
            "wifi_management": 0})
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn completed_wifi_receipt_retires_on_ble_cycle_without_a_runnable_job() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for case in ["success","lost_ack","status_timeout","wrong_namespace"] {
            let dir=tempfile::tempdir().unwrap();
            let (ble,url)=start_fake_ble(HUNG_MAC,false).await;
            ble.protocol.store(true,Ordering::SeqCst);
            *ble.status_extra.lock().unwrap()=json!({"automatic_contact":false});
            let ctx=test_ctx(dir.path(),&[HUNG_MAC]);let svc=service(&ctx);let now=device_now(HUNG_MAC);
            let job=svc.wifi_queue_control(HUNG_MAC,&ctx.bridge_id,"activate",json!({"network_id":"home"}),now).unwrap();
            let namespace=if case=="wrong_namespace" {"old-bond"}else{"own"};
            let done=svc.wifi_attempt(HUNG_MAC,&job.job_id,&ctx.bridge_id,namespace,true,&BTreeMap::new(),now,|op,_| {
                assert_eq!(op,"activate");Ok(json!({"ok":true,"outcome":"completed","revision":1}))
            }).unwrap().unwrap();
            assert!(done.receipt_ack_pending);assert!(!done.runnable(now));
            drop(ctx);
            let ctx=test_ctx(dir.path(),&[HUNG_MAC]);
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            let retry=matches!(case,"lost_ack"|"status_timeout");
            for round in 0..if retry {2}else{1} {
                *ble.blocked_op.lock().unwrap()=if round==0 && retry {
                    if case=="lost_ack" {"wifi_result_ack".into()}else{"wifi_status".into()}
                }else{String::new()};
                let outcome=tokio::time::timeout(Duration::from_secs(5),ble_cycle(&ctx,&[HUNG_MAC.into()])).await;
                assert!(matches!(outcome,Ok(Ok(BleOpportunity::Connected{result:Ok(()),..}))),"{case}/{round}: {outcome:?}");
                let state=service(&ctx).wifi_state(HUNG_MAC);let restored=state.jobs.iter().find(|j|j.job_id==job.job_id).unwrap();
                assert_eq!(restored.state,"completed");assert_eq!(restored.request_id,job.request_id);
                assert_eq!(restored.receipt_ack_pending,case=="wrong_namespace" || (retry && round==0));
                assert!(!state.jobs.iter().any(|j|j.runnable(now)));
            }
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
            let commands=ble.commands.lock().unwrap();
            let acks:Vec<_>=commands.iter().filter(|c|c["op"]=="wifi_result_ack").collect();
            assert_eq!(acks.len(),match case {"lost_ack"=>2,"wrong_namespace"=>0,_=>1});
            assert!(acks.iter().all(|c|c["query_id"]==job.request_id));
            assert!(!commands.iter().any(|c|matches!(c["op"].as_str(),Some("wifi_activate"|"wifi_trial"|"wifi_save"|"wifi_result"|"plan"|"network_start"))));
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn boot_network_manual_gatt_and_failure_report_have_distinct_admission() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for mode in ["manual", "gatt", "failure"] {
            let dir=tempfile::tempdir().unwrap();
            let events=Arc::new(Mutex::new(Vec::new()));
            let (ble,url)=start_fake_ble_with(HUNG_MAC,false,mode=="failure",events.clone()).await;
            ble.protocol.store(true,Ordering::SeqCst);
            ble.provisional_remaining_s.store(295,Ordering::SeqCst);
            *ble.status_extra.lock().unwrap()=match mode {
                "gatt"=>json!({"automatic_contact":false}),
                "failure"=>json!({"network_error":"bridge_http_unreachable"}),
                _=>json!({}),
            };
            // Deliberately incomplete HTTP identity: admission may attempt it,
            // but must never convert this into a successful handover.
            let (_http,ip)=start_fake_http(events.clone(),plain_status_body(HUNG_MAC),0,Duration::ZERO);
            let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            let outcome=tokio::time::timeout(Duration::from_secs(5),ble_cycle(&ctx,&[HUNG_MAC.into()])).await;
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
            let ops=command_ops(&ble);
            let used_http=events.lock().unwrap().iter().any(|e|e.starts_with("http:"));
            if mode=="manual" {
                assert!(used_http && ops.iter().any(|op|op=="plan"), "{outcome:?}, ops={ops:?}");
                assert!(matches!(outcome,Ok(Ok(BleOpportunity::Connected{result:Err(_),..}))));
            } else {
                assert!(matches!(outcome,Ok(Ok(BleOpportunity::Connected{result:Ok(()),..}))),"{mode}: {outcome:?}");
                assert!(!used_http && !ops.iter().any(|op|matches!(op.as_str(),"network_start"|"sync_open"|"wifi_activate")));
                if mode=="gatt" { assert!(!ops.iter().any(|op|op=="plan")); }
            }
        }
    }

    /// C13 — production order over one real `ble_cycle`:
    /// `link cleanup -> sync_http_inner (diagnostic drain) -> opportunity_locked`.
    ///
    /// This is a **PASS-existing-order** characterisation, not a fix: it pins
    /// the order the deployed code actually uses (`platform.rs:2238-2249`) from
    /// the device side. The fake BLE server logs its `/sim/ble/close` hit and
    /// the fake device HTTP server logs each `/api/status` read, so the two
    /// production HTTP steps after the close are visible and distinguishable.
    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn wifi_idle_repeated_contacts_do_not_probe_http_or_activate_network() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let dir=tempfile::tempdir().unwrap();let events=Arc::new(Mutex::new(Vec::new()));
        let (ble,url)=start_fake_ble_with(HUNG_MAC,false,false,events.clone()).await;
        let (_http,ip)=start_fake_http(events.clone(),plain_status_body(HUNG_MAC),0,Duration::ZERO);
        let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);let svc=service(&ctx);let now=device_now(HUNG_MAC);
        svc.wifi_save(HUNG_MAC,0,serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}])).unwrap(),now).unwrap();
        svc.wifi_note_connection(HUNG_MAC,serde_json::from_value(json!({"host_ssid":"Host","network_id":"home","local_revision":1,
            "version":8,"state":"waiting_for_window","error":null,"reason":0,"observed_at":now,"attempts":0,"next_attempt_at":0,"http_ready":false})).unwrap()).unwrap();
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        for _ in 0..4 {
            let result=ble_cycle(&ctx,&[HUNG_MAC.to_owned()]).await;
            assert!(matches!(result,Ok(BleOpportunity::Connected{result:Ok(()),..})),"{result:?}");
        }
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
        let commands=ble.commands.lock().unwrap();
        assert_eq!(commands.iter().filter(|c|c["op"]=="plan").count(),4);
        assert!(commands.iter().filter(|c|c["op"]=="plan").all(|c|c["mode"]=="sleep"));
        assert!(!commands.iter().any(|c|matches!(c["op"].as_str(),Some("wifi_activate"|"wifi_match"|"handoff"|"sync_open"))));
        assert!(!events.lock().unwrap().iter().any(|e|e.starts_with("http:")),"idle BLE contacts must not probe HTTP");
        assert!(svc.wifi_state(HUNG_MAC).jobs.is_empty());assert_eq!(svc.wifi_state(HUNG_MAC).connection.unwrap().attempts,0);
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn protocol_ble_only_light_does_not_enter_diagnostic_http_tail() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let dir=tempfile::tempdir().unwrap();let events=Arc::new(Mutex::new(Vec::new()));
        let (ble,url)=start_fake_ble_with(HUNG_MAC,false,false,events.clone()).await;
        ble.protocol.store(true,Ordering::SeqCst);
        let (_http,ip)=start_fake_http(events.clone(),plain_status_body(HUNG_MAC),0,Duration::ZERO);
        let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);
        service(&ctx).hold_light(HUNG_MAC,device_now(HUNG_MAC)+120).unwrap();
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        let result=tokio::time::timeout(Duration::from_secs(5),ble_cycle(&ctx,&[HUNG_MAC.into()])).await;
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
        assert!(matches!(result,Ok(Ok(BleOpportunity::Connected{result:Ok(()),..}))),"{result:?}");
        let commands=ble.commands.lock().unwrap();
        assert!(commands.iter().any(|c|c["op"]=="sync_config"));
        let plan=commands.iter().find(|c|c["op"]=="plan").unwrap();
        assert_eq!(plan["mode"],"light");assert_eq!(plan["need_http"],false);
        assert!(!commands.iter().any(|c|matches!(c["op"].as_str(),Some("network_start"|"sync_open"))));
        assert!(!events.lock().unwrap().iter().any(|e|e.starts_with("http:")),
            "BLE-only Light must preserve its successful ACK without diagnostic HTTP: {:?}",events.lock().unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn combo_cycle_order_close_then_drain_then_wifi_opportunity() {
        let _serial = SIM_ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let (fake_ble, ble_url) = start_fake_ble_with(HUNG_MAC, false, true, events.clone()).await;
        let (fake_http, ip) = start_fake_http(
            events.clone(),
            plain_status_body(HUNG_MAC),
            0,
            Duration::ZERO,
        );
        let ctx = Arc::new(test_ctx_with_ip(dir.path(), HUNG_MAC, &ip));
        let job_id = queue_read_job(&ctx, HUNG_MAC);
        std::env::set_var(
            "CODEX_STATUS_SIM_BLE_ENDPOINTS",
            json!({HUNG_MAC: ble_url}).to_string(),
        );

        let outcome = tokio::time::timeout(Duration::from_secs(15), ble_cycle(&ctx, &[HUNG_MAC.to_owned()]))
            .await
            .expect("production ble_cycle did not finish")
            .expect("production ble_cycle failed");
        // Reset the process-wide seam before asserting, whatever happens next.
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");

        let ops = command_ops(&fake_ble);
        let log = events.lock().unwrap().clone();
        let (state, error) = observed_job(&ctx, HUNG_MAC, &job_id);

        assert!(
            ops.iter().any(|op| op == "sync_open"),
            "the production sync branch was not exercised (sync_opened must be true): ops={ops:?}"
        );
        assert_eq!(
            fake_ble.closes.load(Ordering::SeqCst),
            1,
            "production code did not perform exactly one link cleanup: log={log:?}"
        );
        let close_at = log
            .iter()
            .position(|event| event == "ble:close")
            .unwrap_or_else(|| panic!("no BLE close observed: {log:?}"));
        let statuses: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(index, event)| *index > close_at && event.as_str() == "http:/api/status")
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            statuses.len(),
            1,
            "an unsupported queued read must fail without a duplicate HTTP status fetch: log={log:?}"
        );
        assert!(
            close_at < statuses[0],
            "authenticated HTTP verification must follow BLE close: log={log:?}"
        );
        assert!(
            matches!(outcome, BleOpportunity::Connected { .. }),
            "production ble_cycle did not report a connected rendezvous: {outcome:?}"
        );
        assert_eq!(
            (state.as_str(), error.as_deref()),
            ("waiting_rendezvous", Some("http_session_mismatch")),
            "the queued read must not execute without authenticated confirmation of the original Plan: \
             status_calls={}",
            fake_http.status_calls.load(Ordering::SeqCst)
        );
    }

    /// C13 control that pins the identity of the post-close status read: with
    /// **no queued Wi-Fi job** `opportunity_locked` returns at its job lookup
    /// (`wifi.rs:307`) and issues no HTTP at all, so the single `/api/status`
    /// read observed after `ble:close` can only be `sync_http_inner`'s
    /// diagnostic drain. Combined with the sibling test (a second, 2s-budgeted
    /// read whose failure uniquely drives the job to `waiting_rendezvous`), the
    /// production order `close -> drain -> opportunity` is pinned end to end.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn combo_cycle_drain_read_is_the_single_post_close_http_step_without_a_job() {
        let _serial = SIM_ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let (fake_ble, ble_url) = start_fake_ble_with(HUNG_MAC, false, true, events.clone()).await;
        let (fake_http, ip) = start_fake_http(
            events.clone(),
            plain_status_body(HUNG_MAC),
            0,
            Duration::ZERO,
        );
        let ctx = Arc::new(test_ctx_with_ip(dir.path(), HUNG_MAC, &ip));
        std::env::set_var(
            "CODEX_STATUS_SIM_BLE_ENDPOINTS",
            json!({HUNG_MAC: ble_url}).to_string(),
        );

        let outcome = tokio::time::timeout(Duration::from_secs(15), ble_cycle(&ctx, &[HUNG_MAC.to_owned()]))
            .await
            .expect("production ble_cycle did not finish")
            .expect("production ble_cycle failed");
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");

        let log = events.lock().unwrap().clone();
        let close_at = log
            .iter()
            .position(|event| event == "ble:close")
            .unwrap_or_else(|| panic!("no BLE close observed: {log:?}"));
        let statuses: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(index, event)| *index > close_at && event.as_str() == "http:/api/status")
            .map(|(index, _)| index)
            .collect();
        assert!(
            matches!(outcome, BleOpportunity::Connected { .. }),
            "production ble_cycle did not report a connected rendezvous: {outcome:?}"
        );
        assert_eq!(fake_ble.closes.load(Ordering::SeqCst), 1);
        assert_eq!(
            statuses.len(),
            1,
            "without a queued job only sync_http_inner's drain may read status: log={log:?}"
        );
        assert!(
            close_at < statuses[0],
            "the drain read must still follow the link cleanup: log={log:?}"
        );
        assert_eq!(
            fake_http.status_calls.load(Ordering::SeqCst),
            2,
            "a job-less opportunity step must not read status"
        );
    }

    /// C13 + task-4 regression guard: with `/sim/ble/close` accepted but never
    /// answered, the bounded close must give up and production must **still**
    /// run `sync_http_inner` (drain) and `opportunity_locked` afterwards. This
    /// is the executable protection for the task-4 fix: before it, both steps
    /// were unreachable behind an unbounded `link.close()`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn combo_cycle_hung_close_still_runs_drain_and_wifi_opportunity() {
        let _serial = SIM_ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let (fake_ble, ble_url) = start_fake_ble_with(HUNG_MAC, true, true, events.clone()).await;
        let (fake_http, ip) = start_fake_http(
            events.clone(),
            plain_status_body(HUNG_MAC),
            0,
            Duration::ZERO,
        );
        let ctx = Arc::new(test_ctx_with_ip(dir.path(), HUNG_MAC, &ip));
        let job_id = queue_read_job(&ctx, HUNG_MAC);
        std::env::set_var(
            "CODEX_STATUS_SIM_BLE_ENDPOINTS",
            json!({HUNG_MAC: ble_url}).to_string(),
        );

        let started = tokio::time::Instant::now();
        let rendezvous = tokio::spawn({
            let ctx = ctx.clone();
            async move { ble_cycle(&ctx, &[HUNG_MAC.to_owned()]).await }
        });
        let closes = wait_for_close(&fake_ble, Duration::from_secs(15)).await;
        assert!(
            closes > 0,
            "the hanging-close fault injection never engaged: ble_cycle did not reach link cleanup"
        );
        let finished = tokio::time::timeout(Duration::from_secs(10), rendezvous)
            .await
            .expect("ble_cycle did not return after the bounded close gave up");
        let elapsed_ms = started.elapsed().as_millis();
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");

        let log = events.lock().unwrap().clone();
        let (state, error) = observed_job(&ctx, HUNG_MAC, &job_id);
        let close_at = log
            .iter()
            .position(|event| event == "ble:close")
            .unwrap_or_else(|| panic!("no BLE close observed: {log:?}"));
        let statuses: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(index, event)| *index > close_at && event.as_str() == "http:/api/status")
            .map(|(index, _)| index)
            .collect();

        assert!(
            finished.expect("rendezvous task panicked").is_ok(),
            "ble_cycle returned an error after the bounded close"
        );
        assert!(
            statuses.len() == 1 && close_at < statuses[0],
            "a hanging close must not skip authenticated HTTP verification: \
             elapsed={elapsed_ms}ms log={log:?}"
        );
        assert_eq!(
            (state.as_str(), error.as_deref()),
            ("waiting_rendezvous", Some("http_session_mismatch")),
            "the queued read must remain pending when post-close Plan verification fails: \
             status_calls={}",
            fake_http.status_calls.load(Ordering::SeqCst)
        );
        assert!(
            elapsed_ms < 8_000,
            "bounded close plus delayed steps took {elapsed_ms}ms, expected roughly \
             BLE_CLOSE_LIMIT (2s) plus the one authenticated status read"
        );
    }

    /// A delayed authenticated status cannot confirm the accepted Plan within
    /// the direct-HTTP proof budget. The read stays queued and the original
    /// Plan is not resent or renewed. The fake records the late response.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn combo_slow_onboarding_reply_after_budget_keeps_read_job_waiting() {
        let _serial = SIM_ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let (fake_ble, ble_url) = start_fake_ble_with(HUNG_MAC, false, true, events.clone()).await;
        // The admitted Wi-Fi read comes first and answers 3 s late.
        // Diagnostics must remain pending when this temporary-window read fails.
        let (fake_http, ip) = start_fake_http(
            events.clone(),
            plain_status_body(HUNG_MAC),
            2,
            Duration::from_secs(3),
        );
        let ctx = Arc::new(test_ctx_with_ip(dir.path(), HUNG_MAC, &ip));
        let job_id = queue_read_job(&ctx, HUNG_MAC);
        std::env::set_var(
            "CODEX_STATUS_SIM_BLE_ENDPOINTS",
            json!({HUNG_MAC: ble_url}).to_string(),
        );

        let started = tokio::time::Instant::now();
        let outcome = tokio::time::timeout(Duration::from_secs(15), ble_cycle(&ctx, &[HUNG_MAC.to_owned()]))
            .await
            .expect("production ble_cycle did not finish")
            .expect("production ble_cycle failed");
        let elapsed_ms = started.elapsed().as_millis();
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");

        // Give the slowed fake device time to produce its late reply.
        let reply_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !fake_http.slow_reply_sent.load(Ordering::SeqCst)
            && tokio::time::Instant::now() < reply_deadline
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let slow_reply_sent = fake_http.slow_reply_sent.load(Ordering::SeqCst);
        let (state, error) = observed_job(&ctx, HUNG_MAC, &job_id);
        let paths = fake_http.paths.lock().unwrap().clone();
        let plan_commands = command_ops(&fake_ble).iter().filter(|op| *op == "plan").count();
        eprintln!(
            "[C01 baseline] slow onboarding (3s reply vs 2s opportunity budget): \
             ble_cycle={elapsed_ms}ms; job={state}/{error:?}; \
             status_calls={}; slow_reply_sent={slow_reply_sent}; \
             plan_commands={plan_commands}; paths={paths:?}",
            fake_http.status_calls.load(Ordering::SeqCst)
        );

        assert!(
            matches!(outcome, BleOpportunity::Connected { .. }),
            "production ble_cycle did not report a connected rendezvous: {outcome:?}"
        );
        assert!(
            slow_reply_sent,
            "the fake device never produced its delayed reply; the slow path was not exercised"
        );
        assert_eq!(
            (state.as_str(), error.as_deref()),
            ("waiting_rendezvous", Some("http_proof_budget_exhausted")),
            "unconfirmed Plan must preserve the read and report its exhausted proof budget"
        );
        assert!(
            !paths.iter().any(|path| path == "/api/claim"),
            "a read attempt must never claim device ownership: paths={paths:?}"
        );
        assert_eq!(
            plan_commands, 1,
            "reading must not send another plan or renew the sleep window: ops={:?}",
            command_ops(&fake_ble)
        );
        assert!(
            (2..=4).contains(&fake_http.status_calls.load(Ordering::SeqCst)) && elapsed_ms < 5000,
            "unconfirmed Plan must stop after the bounded HTTP proof window"
        );
    }
    struct FullHttp {
        ble: Arc<FakeBle>,
        snapshot: String,
        diagnostic: Vec<u8>,
        proof_at: AtomicU64,
        owner: Mutex<String>,
        pages: AtomicUsize,
        complete: AtomicBool,
    }
    async fn full_status(State(f): State<Arc<FullHttp>>) -> Response {
        f.ble.events.lock().unwrap().push("http:proof".into());
        f.ble.events.lock().unwrap().push(format!("http:network_started:{}",
            f.ble.network_started.load(Ordering::SeqCst)));
        if !f.ble.network_started.load(Ordering::SeqCst) {
            return (axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"result":"rejected","stage":"http","error":"not_ready"}))).into_response();
        }
        let _ = f.proof_at.compare_exchange(0, device_now(HUNG_MAC), Ordering::SeqCst, Ordering::SeqCst);
        Json(json!({"result":"applied","device_mac":HUNG_MAC,"session_nonce":"00112233445566778899aabbccddeeff",
            "wake_generation":2,"wake_seq":9,"owner_id":*f.owner.lock().unwrap(),
            "power":{"plan_id":f.ble.applied_plan.load(Ordering::SeqCst),"mode":"light","remaining_s":110},
            "wifi_management":3,"wifi_control":{"mode":"window","remaining_ms":80000,"read_allowed":true,"write_allowed":true},"rendezvous":{"v":1,"id":"this-wake","challenge":"http-proof",
            // A ready-report ACK can finish rendezvous before this HTTP read.
            "ready":false,"remaining_ms":0},"transports":{"http":{"ready":true}},
            "sync_v1":1,"sync":{"enabled":true,"phase":if f.ble.sync_opened.load(Ordering::SeqCst) {"WIFI_SYNC_ONCE"} else {"WIFI_LIGHT"},"remaining_ms":80000,"due":!f.complete.load(Ordering::SeqCst),"reasons":[]}})).into_response()
    }
    async fn full_claim(State(f): State<Arc<FullHttp>>, headers: axum::http::HeaderMap) -> Response {
        assert_eq!(headers.get("x-device-token").unwrap(), "a".repeat(32).as_str());
        f.ble.events.lock().unwrap().push("http:claim".into());
        let mut owner=f.owner.lock().unwrap();
        if !owner.is_empty() && *owner != "bridge-lock-repro" { return (axum::http::StatusCode::CONFLICT,Json(json!({"owner":{"id":*owner}}))).into_response(); }
        *owner="bridge-lock-repro".into(); Json(json!({"owner":{"id":*owner}})).into_response()
    }
    async fn full_wifi(State(f): State<Arc<FullHttp>>, Json(body): Json<Value>) -> impl IntoResponse {
        assert!(f.ble.network_started.load(Ordering::SeqCst));
        assert!(f.ble.closes.load(Ordering::SeqCst)>0);
        assert!(!f.complete.load(Ordering::SeqCst), "diagnostic complete prematurely closed the read window");
        tokio::time::sleep(Duration::from_millis(220)).await;
        f.pages.fetch_add(1, Ordering::SeqCst);
        f.ble.events.lock().unwrap().push("http:snapshot-page".into());
        let offset=body["offset"].as_u64().unwrap() as usize;
        let end=(offset+130).min(f.snapshot.len());
        Json(json!({"ok":true,"device_mac":HUNG_MAC,"schema":2,"snapshot_id":"full-read","revision":1,
            "offset":offset,"next_offset":end,"total":f.snapshot.len(),"complete":end==f.snapshot.len(),"chunk":&f.snapshot[offset..end]}))
    }
    async fn full_sync(State(f): State<Arc<FullHttp>>, uri: axum::http::Uri, Json(body): Json<Value>) -> impl IntoResponse {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        let hash=format!("{:x}", Sha256::digest(&f.diagnostic));
        let mut reply=json!({"result":"applied","device_mac":HUNG_MAC,"batch_id":"1-audit","bytes":f.diagnostic.len(),"sha256":hash,
            "session_nonce":body["session_nonce"],"request_id":body["request_id"],"sync_version":1,
            "op":format!("sync_{}",uri.path().rsplit('/').next().unwrap())});
        match uri.path() {
            "/api/sync/page" => {
                reply["offset"]=body["offset"].clone();reply["next_offset"]=json!(f.diagnostic.len());
                reply["data_b64"]=json!(base64::engine::general_purpose::STANDARD.encode(&f.diagnostic));
                reply["chunk_sha256"]=json!(hash);
            },
            "/api/sync/ack" => { reply["acked_offset"]=body["offset"].clone(); },
            "/api/sync/complete" => {
                assert!(f.pages.load(Ordering::SeqCst)>1);
                f.ble.events.lock().unwrap().push("http:diagnostic-complete".into());
                f.complete.store(true, Ordering::SeqCst); reply["receipt"]=json!({"batch_id":"1-audit"});
            },
            _ => {},
        }
        Json(reply)
    }
    async fn full_framed(State(f): State<Arc<FullHttp>>, uri: axum::http::Uri,
        headers: axum::http::HeaderMap, body: Bytes) -> Response {
        let (frame,payload)=bridge_core::compact_frame::Header::decode(&body,4096).unwrap();
        let input=serde_json::from_slice::<Value>(payload).unwrap_or(Value::Null);
        let response=match uri.path() {
            "/api/status"=>full_status(State(f)).await,
            "/claim"=>full_claim(State(f),headers).await,
            "/api/wifi/snapshot"=>full_wifi(State(f),Json(input)).await.into_response(),
            _=>full_sync(State(f),uri,Json(input)).await.into_response(),
        };
        let status=response.status();
        let payload=axum::body::to_bytes(response.into_body(),4096).await.unwrap();
        let result=if status.is_success(){0}else{2};
        let header=bridge_core::compact_frame::Header{kind:1,op:frame.op,result,
            payload_len:payload.len() as u16,session:[9;16],request_no:frame.request_no,context:frame.context};
        let mut bytes=header.encode().unwrap().to_vec();bytes.extend_from_slice(&payload);
        (status,[(axum::http::header::CONTENT_TYPE,bridge_core::compact_frame::CONTENT_TYPE)],bytes).into_response()
    }
    async fn start_full_http(ble: Arc<FakeBle>, owner: &str) -> (Arc<FullHttp>, String, tokio::task::JoinHandle<()>) {
        let snapshot=serde_json::to_string(&json!({"schema":2,"device_mac":HUNG_MAC,"revision":1,"snapshot_id":"full-read",
            "complete":true,"observed_at":0,"capacity":{"total":12},"networks":[{"bond":"own","writable":true,
                "network":{"network_id":"home","host_ssid":"Host","ssid":"Device","version":1}}]})).unwrap();
        let diagnostic=serde_json::to_vec(&json!({"format":"device-sync-1","device_mac":HUNG_MAC,
            "bridge_id":"bridge-lock-repro","batch_id":"1-audit","diag":{"generation":"00112233445566778899aabbccddeeff",
                "from_seq":"1","through_seq":"0","records_b64":"","gaps":[]}})).unwrap();
        let f=Arc::new(FullHttp { ble,snapshot,diagnostic,owner:Mutex::new(owner.into()),
            proof_at:AtomicU64::new(0),pages:AtomicUsize::new(0),complete:AtomicBool::new(false) });
        let app=Router::new().route("/api/status",post(full_framed)).route("/status.json",get(||async {Json(json!({"MAC":HUNG_MAC}))}))
            .route("/claim",post(full_framed)).route("/api/wifi/snapshot",post(full_framed))
            .route("/api/sync/begin",post(full_framed)).route("/api/sync/page",post(full_framed))
            .route("/api/sync/ack",post(full_framed)).route("/api/sync/complete",post(full_framed)).with_state(f.clone());
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ip=listener.local_addr().unwrap().to_string();
        let server=tokio::spawn(async move {let _=axum::serve(listener,app).await;});
        (f,ip,server)
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn rendezvous_full_read_precedes_diagnostic_complete_and_recovers_lost_acks() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for (lost_plan,lost_network_start,lost_sync,owner) in [(false,false,false,"bridge-lock-repro"),(true,false,false,"bridge-lock-repro"),
            (false,true,false,"bridge-lock-repro"),(false,false,false,""),(false,false,true,"bridge-lock-repro"),
            (true,true,false,"bridge-lock-repro")] {
            let dir=tempfile::tempdir().unwrap();
            let events=Arc::new(Mutex::new(Vec::new()));
            let (ble,url)=start_fake_ble_with(HUNG_MAC,false,true,events.clone()).await;
            ble.protocol.store(true,Ordering::SeqCst);ble.lost_plan.store(lost_plan,Ordering::SeqCst);
            ble.ble_claim.store(true,Ordering::SeqCst);
            *ble.claim_owner.lock().unwrap()=owner.to_owned();
            ble.lost_network_start.store(lost_network_start,Ordering::SeqCst);
            if lost_sync { *ble.blocked_op.lock().unwrap() = "sync_open".into(); }
            let (http,ip,server)=start_full_http(ble.clone(),owner).await;
            let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);
            service(&ctx).wifi_save(HUNG_MAC,0,serde_json::from_value(json!([
                {"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}])).unwrap(),device_now(HUNG_MAC)).unwrap();
            bridge_mcp::save_device_token_at(dir.path(),HUNG_MAC,&"a".repeat(32)).unwrap();
            let job=queue_read_job(&ctx,HUNG_MAC);
            std::env::set_var("CODEX_STATUS_TEST_HOST_WIFI",json!({"state":"connected","networks":[{"ssid":"Host"}]}).to_string());
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            let result=tokio::time::timeout(Duration::from_secs(35),ble_cycle(&ctx,&[HUNG_MAC.into()])).await;
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");std::env::remove_var("CODEX_STATUS_TEST_HOST_WIFI");server.abort();
            assert!(matches!(result,Ok(Ok(BleOpportunity::Connected { result:Ok(()),.. }))),"case=({lost_plan},{lost_network_start},{lost_sync},{owner}) {result:?} ops={:?}",command_ops(&ble));
            assert_eq!(observed_job(&ctx,HUNG_MAC,&job),("completed".into(),None),
                "case=({lost_plan},{lost_network_start},{lost_sync},{owner}) events={:?}",events.lock().unwrap());
            assert!(http.pages.load(Ordering::SeqCst)>1);assert!(http.complete.load(Ordering::SeqCst));
            let log=events.lock().unwrap();
            let network_start=log.iter().position(|e|e=="ble:network_start").unwrap();
            let close=log.iter().position(|e|e=="ble:close").unwrap();
            let page=log.iter().rposition(|e|e=="http:snapshot-page").unwrap();
            let complete=log.iter().position(|e|e=="http:diagnostic-complete").unwrap();
            assert!(log[..network_start].iter().any(|e|e=="http:proof"));
            assert!(network_start<close && close<page && page<complete,"{log:?}");
            if owner.is_empty() { assert!(log.iter().position(|e|e=="ble:claim").unwrap()<network_start); }
            assert_eq!(command_ops(&ble).iter().filter(|op|*op=="plan").count(),1);
            assert!(ble.status_reads.load(Ordering::SeqCst) < 1200,
                "lost ACKs must not spin on a stale BLE status attribute");
            if lost_plan {
                let reopened=PlatformService::open(dir.path()).unwrap();
                let plan=reopened.coordinator_summary(HUNG_MAC,device_now(HUNG_MAC)).unwrap()["plan"].clone();
                let deadline=plan["last_accepted_at"].as_u64().unwrap()+plan["last_accepted_remaining_s"].as_u64().unwrap();
                let sampled_deadline=http.proof_at.load(Ordering::SeqCst)+110;
                assert!(deadline<=sampled_deadline,"first confirmation renewed sampled deadline: {plan}");
                assert!(plan["light_hold_until"].as_u64().unwrap()<=sampled_deadline,"persisted hold renewed: {plan}");
            }
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn rendezvous_other_owner_blocks_network_start_and_http_work() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let dir=tempfile::tempdir().unwrap();let (ble,url)=start_fake_ble(HUNG_MAC,false).await;
        ble.protocol.store(true,Ordering::SeqCst);
        ble.ble_claim.store(true,Ordering::SeqCst);
        *ble.claim_owner.lock().unwrap()="other-bridge".into();
        let (http,ip,server)=start_full_http(ble.clone(),"other-bridge").await;
        let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);queue_read_job(&ctx,HUNG_MAC);
        bridge_mcp::save_device_token_at(dir.path(),HUNG_MAC,&"a".repeat(32)).unwrap();
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        let result=ble_cycle(&ctx,&[HUNG_MAC.into()]).await;
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");server.abort();
        assert!(matches!(result,Ok(BleOpportunity::Connected { result:Err(_),.. })));
        assert!(!ble.network_started.load(Ordering::SeqCst));assert_eq!(http.pages.load(Ordering::SeqCst),0);
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn rendezvous_unconfirmed_plan_keeps_read_pending_and_releases_locks() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let dir=tempfile::tempdir().unwrap();let (ble,url)=start_fake_ble(HUNG_MAC,false).await;
        ble.protocol.store(true,Ordering::SeqCst);ble.lost_plan.store(true,Ordering::SeqCst);
        ble.apply_plan.store(false,Ordering::SeqCst);
        let (http,ip,server)=start_full_http(ble.clone(),"bridge-lock-repro").await;
        let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);let job=queue_read_job(&ctx,HUNG_MAC);
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        let result=tokio::time::timeout(Duration::from_secs(32),ble_cycle(&ctx,&[HUNG_MAC.into()])).await;
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");server.abort();
        assert!(matches!(result,Ok(Ok(BleOpportunity::Connected { result:Err(_),.. }))),"{result:?}");
        assert!(!ble.network_started.load(Ordering::SeqCst));assert_eq!(http.pages.load(Ordering::SeqCst),0);
        assert!(!http.complete.load(Ordering::SeqCst));
        assert_ne!(observed_job(&ctx,HUNG_MAC,&job).0,"completed");
        let plan=service(&ctx).coordinator_summary(HUNG_MAC,device_now(HUNG_MAC)).unwrap()["plan"].clone();
        assert_eq!(plan["last_accepted_id"],0);
        assert!(ctx.device_delivery.try_lock().is_ok());
        assert!(delivery_lock(&ctx,HUNG_MAC).try_lock().is_ok());
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn explicit_wifi_read_gets_own_online_opportunity_and_explains_waits() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let dir=tempfile::tempdir().unwrap();
        let (ble,_)=start_fake_ble(HUNG_MAC,false).await;
        ble.network_started.store(true,Ordering::SeqCst);
        ble.closes.store(1,Ordering::SeqCst);
        let (http,ip,server)=start_full_http(ble.clone(),"bridge-lock-repro").await;
        let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);let svc=service(&ctx);let now=device_now(HUNG_MAC);
        let old=svc.wifi_queue_control(HUNG_MAC,&ctx.bridge_id,"activate",json!({"network_id":"home","version":1}),now).unwrap();
        svc.wifi_bind_transaction(HUNG_MAC,&old.job_id,&ctx.bridge_id,"own").unwrap();
        svc.wifi_mark_sent(HUNG_MAC,&old.job_id,now).unwrap();
        svc.wifi_finish(HUNG_MAC,&old.job_id,"awaiting_confirmation",Some("result_unknown"),now-60).unwrap();
        // Even a runnable older transaction cannot steal this explicit request.
        let result=crate::wifi::action(&ctx,HUNG_MAC,"get",json!({})).await.unwrap();
        assert_eq!(result["state"],"completed","an online explicit read must finish in its own call: {result}");
        assert!(http.pages.load(Ordering::SeqCst)>1);
        assert!(!ble.events.lock().unwrap().iter().any(|e|e=="http:claim"));
        let old=svc.wifi_state(HUNG_MAC).jobs.into_iter().find(|j|j.job_id==old.job_id).unwrap();
        assert!(old.sent && old.may_have_committed && old.transaction_namespace.as_deref()==Some("own"));
        assert_eq!(old.error_code.as_deref(),Some("result_unknown"));
        svc.wifi_finish(HUNG_MAC,&old.job_id,"cancelled",None,now).unwrap();

        let lock=delivery_lock(&ctx,HUNG_MAC);let guard=lock.lock().await;
        let waiting=crate::wifi::action(&ctx,HUNG_MAC,"get",json!({})).await.unwrap();
        assert_eq!(waiting["state"],"waiting_rendezvous");
        assert_eq!(waiting["error_code"],"device_channel_busy");
        drop(guard);
        tokio::time::sleep(Duration::from_secs(6)).await;
        crate::wifi::opportunity(&ctx,HUNG_MAC).await;
        let resumed=svc.wifi_state(HUNG_MAC).jobs.into_iter().find(|j|j.job_id==waiting["job_id"]).unwrap();
        assert_eq!(resumed.state,"completed","channel-busy read must resume with its original ID");
        server.abort();
        let offline_dir=tempfile::tempdir().unwrap();let offline=test_ctx_with_ip(offline_dir.path(),HUNG_MAC,&ip);
        let result=crate::wifi::action(&offline,HUNG_MAC,"get",json!({})).await.unwrap();
        assert_eq!(result["state"],"waiting_rendezvous");
        assert_eq!(result["error_code"],"http_status_unavailable");
    }

    #[tokio::test(flavor="multi_thread",worker_threads=2)]
    async fn fresh_sync_from_stale_cache_ends_as_conflict_before_sending() {
        let calls=Arc::new(Mutex::new(Vec::<String>::new()));
        let app=Router::new().route("/api/status",post(|body:Bytes|async move {
            let (frame,_)=bridge_core::compact_frame::Header::decode(&body,4096).unwrap();
            framed_test_reply(frame,json!({"device_mac":HUNG_MAC,"wifi_management":3,"boot_id":"stale-cache-test","uptime_ms":100,
            "session_nonce":"00112233445566778899aabbccddeeff","status_seq":1,"owner_id":"bridge-lock-repro",
            "wifi_control":{"principal":"bridge-lock-repro","namespace":"own","mode":"continuous","read_allowed":true,"write_allowed":true},"wifi_storage":{"revision":3},
            "power":{"remaining_s":0},"sync":{"remaining_ms":0}}))}))
            .route("/api/wifi/command",post(|State(calls):State<Arc<Mutex<Vec<String>>>>,raw:Bytes|async move {
                let (frame,payload)=bridge_core::compact_frame::Header::decode(&raw,4096).unwrap();
                let body:Value=serde_json::from_slice(payload).unwrap();
                let op=body["op"].as_str().unwrap().to_owned();calls.lock().unwrap().push(op.clone());
                framed_test_reply(frame,if op=="match" {json!({"ok":true,"device_mac":HUNG_MAC,"network_id":body["network_id"],"matches":false,"version":3})}
                    else {json!({"ok":false,"device_mac":HUNG_MAC,"error":"revision_conflict"})})
            })).with_state(calls.clone());
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let ip=listener.local_addr().unwrap().to_string();
        let server=tokio::spawn(async move {let _=axum::serve(listener,app).await;});
        let dir=tempfile::tempdir().unwrap();let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);let svc=service(&ctx);let now=device_now(HUNG_MAC);
        let read=svc.wifi_queue(HUNG_MAC,&ctx.bridge_id,"read",vec![],now).unwrap();
        svc.wifi_complete_read(HUNG_MAC,&read.job_id,serde_json::from_value(json!({"schema":2,"device_mac":HUNG_MAC,"revision":1,"snapshot_id":"old-cache",
            "complete":true,"observed_at":now,"capacity":{"total":12},"networks":[]})).unwrap(),now).unwrap();
        let targets=serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Desired","credential":"open"}])).unwrap();
        svc.wifi_save(HUNG_MAC,0,targets,now).unwrap();let job=svc.wifi_queue(HUNG_MAC,&ctx.bridge_id,"sync",vec!["home".into()],now).unwrap();
        assert!(!job.sent);assert_eq!(job.expected_revision,Some(1));
        crate::wifi::opportunity(&ctx,HUNG_MAC).await;
        let result=svc.wifi_state(HUNG_MAC).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap();
        assert_eq!(result.state,"conflict");assert_eq!(result.error_code.as_deref(),Some("revision_conflict"));assert!(!result.sent && !result.may_have_committed);
        assert_eq!(*calls.lock().unwrap(),vec!["match"],"known stale cache must stop before mutation");
        assert_eq!(svc.wifi_state(HUNG_MAC).snapshot.unwrap().revision,1,"no implicit full config refresh");
        assert_eq!(svc.wifi_state(HUNG_MAC).targets[0].ssid,"Desired");
        assert_eq!(result.request_id,job.request_id);assert!(!result.runnable(now+600));server.abort();
    }

    #[tokio::test]
    async fn wifi_cached_import_is_local_and_checks_the_selected_snapshot() {
        for op in ["import", "read_import"] {
            let dir = tempfile::tempdir().unwrap();
            let ctx = test_ctx(dir.path(), &[HUNG_MAC]); // no reachable device
            let svc = service(&ctx);
            let now = bridge_core::now_secs();
            let targets = serde_json::from_value(json!([{"network_id":"home","host_ssid":"LocalHost","ssid":"LocalSSID","label":"Keep label"}])).unwrap();
            svc.wifi_save(HUNG_MAC, 0, targets, now).unwrap();
            let read = svc.wifi_queue(HUNG_MAC, &ctx.bridge_id, "read", vec![], now).unwrap();
            let snapshot = serde_json::from_value(json!({"schema":2,"device_mac":HUNG_MAC,"revision":3,
                "snapshot_id":"cached-choice","complete":true,"observed_at":now,
                "networks":[{"bond":"own","writable":true,"network":{"network_id":"home","host_ssid":"RemoteHost","ssid":"CachedSSID","label":"Remote label"}}],
                "capacity":{"total":12,"per_bond":4}})).unwrap();
            svc.wifi_complete_read(HUNG_MAC, &read.job_id, snapshot, now).unwrap();
            let before = svc.wifi_state(HUNG_MAC);
            let plan_before = svc.coordinator_summary(HUNG_MAC, device_now(HUNG_MAC)).unwrap()["plan"].clone();
            let mut body = json!({"selected":["home"],"fields":{"home":["ssid"]},"local_revision":1,"snapshot_id":"replaced-snapshot"});
            // Held delivery locks would block any accidental radio operation.
            let _global = ctx.device_delivery.lock().await;
            let lock = delivery_lock(&ctx, HUNG_MAC); let _device = lock.lock().await;
            assert_eq!(crate::wifi::action(&ctx, HUNG_MAC, op, body.clone()).await.unwrap_err(), "snapshot_conflict");
            assert_eq!(svc.wifi_state(HUNG_MAC).local_revision, 1);
            body["snapshot_id"] = json!("cached-choice");
            let result = tokio::time::timeout(Duration::from_secs(1), crate::wifi::action(&ctx, HUNG_MAC, op, body.clone())).await.unwrap().unwrap();
            assert!(result.get("job_id").is_none());
            assert_eq!(result["local_revision"], 2);
            let after = svc.wifi_state(HUNG_MAC);
            assert_eq!(after.targets[0].ssid, "CachedSSID");
            assert_eq!(after.targets[0].host_ssid, "LocalHost");
            assert_eq!(after.targets[0].label, "Keep label");
            assert_eq!(after.jobs.len(), before.jobs.len());
            assert_eq!(after.snapshot.unwrap().snapshot_id, "cached-choice");
            assert_eq!(svc.coordinator_summary(HUNG_MAC, device_now(HUNG_MAC)).unwrap()["plan"], plan_before);
            assert!(crate::wifi::action(&ctx, HUNG_MAC, op, body).await.is_err()); // local revision changed
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn wifi_prepare_slow_write_uses_carrier_budget_and_next_native_cycle_recovers() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let dir=tempfile::tempdir().unwrap();
        let (ble,url)=start_fake_ble_with(HUNG_MAC,false,true,Arc::new(Mutex::new(Vec::new()))).await;
        ble.protocol.store(true,Ordering::SeqCst);
        ble.wifi_match_write_delay_ms.store(900,Ordering::SeqCst);
        let (_http,ip,server)=start_full_http(ble.clone(),"bridge-lock-repro").await;
        let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);
        let svc=service(&ctx);
        svc.wifi_save(HUNG_MAC,0,serde_json::from_value(json!([
            {"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}
        ])).unwrap(),device_now(HUNG_MAC)).unwrap();
        bridge_mcp::save_device_token_at(dir.path(),HUNG_MAC,&"a".repeat(32)).unwrap();
        queue_read_job(&ctx,HUNG_MAC);
        // Also exercise the native radio lease: a cancelled write quarantines
        // this lease even when the device confirms disconnect immediately.
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        std::env::set_var("CODEX_STATUS_SIM_BLE_LIFECYCLE","1");
        std::env::set_var("CODEX_STATUS_TEST_HOST_WIFI",json!({"state":"connected","networks":[{"ssid":"Host"}]}).to_string());
        let first=ble_cycle(&ctx,&[HUNG_MAC.into()]).await;
        let second=ble_cycle(&ctx,&[HUNG_MAC.into()]).await;
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
        std::env::remove_var("CODEX_STATUS_SIM_BLE_LIFECYCLE");
        std::env::remove_var("CODEX_STATUS_TEST_HOST_WIFI");server.abort();
        for result in [first,second] {
            assert!(matches!(result,Ok(BleOpportunity::Connected { result:Ok(()),.. })),"{result:?} ops={:?}",command_ops(&ble));
        }
        let ops=command_ops(&ble);
        assert!(ops.iter().position(|op|op=="wifi_match").unwrap()<ops.iter().position(|op|op=="plan").unwrap());
        assert_eq!(ops.iter().filter(|op|*op=="plan").count(),2);
        assert_eq!(ble.closes.load(Ordering::SeqCst),2);
        assert!(ble.network_started.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn wifi_prepare_unknown_result_stops_before_plan_and_keeps_original_failure() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for blocked in ["wifi_match","write:wifi_match"] {
            let dir=tempfile::tempdir().unwrap();
            let (ble,url)=start_fake_ble_with(HUNG_MAC,false,true,Arc::new(Mutex::new(Vec::new()))).await;
            ble.protocol.store(true,Ordering::SeqCst);
            ble.rf_initial_ms.store(1400,Ordering::SeqCst);
            *ble.blocked_op.lock().unwrap()=blocked.into();
            let ctx=test_ctx(dir.path(),&[HUNG_MAC]);
            service(&ctx).wifi_save(HUNG_MAC,0,serde_json::from_value(json!([
                {"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}
            ])).unwrap(),device_now(HUNG_MAC)).unwrap();
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            std::env::set_var("CODEX_STATUS_TEST_HOST_WIFI",json!({"state":"connected","networks":[{"ssid":"Host"}]}).to_string());
            let started=std::time::Instant::now();
            let result=ble_cycle(&ctx,&[HUNG_MAC.into()]).await;
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
            std::env::remove_var("CODEX_STATUS_TEST_HOST_WIFI");
            assert!(matches!(result,Ok(BleOpportunity::Connected { result:Err(ref e),.. }) if e=="result_unknown"),"{blocked}: {result:?}");
            assert!(started.elapsed()<Duration::from_secs(4));
            let ops=command_ops(&ble);
            assert_eq!(ops.last().map(String::as_str),Some("wifi_match"),"{blocked}: {ops:?}");
            assert_eq!(ble.closes.load(Ordering::SeqCst),1);
            assert!(ctx.device_delivery.try_lock().is_ok());
            assert!(delivery_lock(&ctx,HUNG_MAC).try_lock().is_ok());
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn existing_active_network_reconnects_without_replaying_unknown_history() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let dir=tempfile::tempdir().unwrap();
        let (ble,url)=start_fake_ble_with(HUNG_MAC,false,true,Arc::new(Mutex::new(Vec::new()))).await;
        ble.protocol.store(true,Ordering::SeqCst);
        *ble.wifi_match_reply.lock().unwrap()=json!({"active":true,"connected":false,"stage":"saved"});
        let (http,ip,server)=start_full_http(ble.clone(),"bridge-lock-repro").await;
        let ctx=test_ctx_with_ip(dir.path(),HUNG_MAC,&ip);let svc=service(&ctx);let now=device_now(HUNG_MAC);
        svc.wifi_save(HUNG_MAC,0,serde_json::from_value(json!([
            {"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open","version":2}
        ])).unwrap(),now).unwrap();
        bridge_mcp::save_device_token_at(dir.path(),HUNG_MAC,&"a".repeat(32)).unwrap();
        let job=svc.wifi_queue_control(HUNG_MAC,&ctx.bridge_id,"activate",json!({"network_id":"home","version":2}),now).unwrap();
        svc.wifi_mark_sent(HUNG_MAC,&job.job_id,now).unwrap();
        svc.wifi_finish(HUNG_MAC,&job.job_id,"cancelled",Some("transaction_identity_unknown"),now).unwrap();
        // Keep the same fence that rejects true conflicting writes.
        assert!(svc.wifi_queue_control(HUNG_MAC,&ctx.bridge_id,"activate",json!({"network_id":"home","version":3}),now).unwrap_err().to_string().contains("wifi_confirmation_pending"));
        let history=serde_json::to_value(&svc.wifi_state(HUNG_MAC).jobs).unwrap();
        // Reproduce the stale local error inherited by later wifi_match replies.
        let mut connection:bridge_core::platform::wifi::Connection=serde_json::from_value(json!({
            "host_ssid":"Host","network_id":"home","local_revision":1,"version":2,
            "state":"waiting_for_window","error":"wifi_confirmation_pending","reason":0,"observed_at":now,
            "attempts":0,"next_attempt_at":0})).unwrap();
        svc.wifi_note_connection(HUNG_MAC,connection.clone()).unwrap();
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        std::env::set_var("CODEX_STATUS_TEST_HOST_WIFI",json!({"state":"connected","networks":[{"ssid":"Host"}]}).to_string());
        let mut results=Vec::new();
        for _ in 0..2 {
            // Lose the carrier again while preserving the same configuration
            // and unresolved historical result, then take another real cycle.
            ble.network_started.store(false,Ordering::SeqCst);
            http.complete.store(false,Ordering::SeqCst);
            queue_read_job(&ctx,HUNG_MAC);
            results.push(ble_cycle(&ctx,&[HUNG_MAC.into()]).await);
        }
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");std::env::remove_var("CODEX_STATUS_TEST_HOST_WIFI");server.abort();
        for result in results {
            assert!(matches!(result,Ok(BleOpportunity::Connected {result:Ok(()),..})),"{result:?} ops={:?}",command_ops(&ble));
        }
        assert_eq!(command_ops(&ble).iter().filter(|op|*op=="network_start").count(),2);
        assert!(ble.network_started.load(Ordering::SeqCst));assert!(http.pages.load(Ordering::SeqCst)>1);
        let after=svc.wifi_state(HUNG_MAC);assert_eq!(after.jobs.len(),history.as_array().unwrap().len()+2);
        assert_eq!(serde_json::to_value(&after.jobs[..1]).unwrap(),history,"unknown historical submission must remain unchanged");
        assert!(!command_ops(&ble).iter().any(|op|matches!(op.as_str(),"wifi_activate"|"wifi_save"|"wifi_apply"|"wifi_trial")));
        connection=after.connection.unwrap();assert!(connection.http_ready);assert_eq!(connection.error,None);
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn historical_unknown_submission_still_blocks_network_switch_or_configuration_change() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for (label,reply,credential) in [
            ("inactive",json!({"active":false,"connected":false,"stage":"saved"}),"keep"),
            ("active_unknown",json!({"connected":false,"stage":"saved"}),"keep"),
            ("metadata",json!({"active":true,"connected":false,"matches":false,"stage":"saved"}),"keep"),
            ("version_unknown",json!({"active":true,"version":0,"connected":false,"stage":"saved"}),"keep"),
            ("authentication",json!({"active":true,"connected":false,"stage":"failed","error":"authentication_failed"}),"keep"),
            ("password",json!({"active":true,"connected":false,"stage":"saved"}),"replace")
        ] {
            let dir=tempfile::tempdir().unwrap();let (ble,url)=start_fake_ble(HUNG_MAC,false).await;
            ble.protocol.store(true,Ordering::SeqCst);*ble.wifi_match_reply.lock().unwrap()=reply;
            let ctx=test_ctx(dir.path(),&[HUNG_MAC]);let svc=service(&ctx);let now=device_now(HUNG_MAC);
            svc.wifi_save(HUNG_MAC,0,serde_json::from_value(json!([
                {"network_id":"home","host_ssid":"Host","ssid":"Device","credential":credential,"password":"synthetic-password","version":2}
            ])).unwrap(),now).unwrap();
            let old=now.saturating_sub(172800);
            let job=svc.wifi_queue_control(HUNG_MAC,&ctx.bridge_id,"activate",json!({"network_id":"home","version":2}),old).unwrap();
            svc.wifi_mark_sent(HUNG_MAC,&job.job_id,old).unwrap();svc.wifi_expire(HUNG_MAC,now).unwrap();
            let before=serde_json::to_value(svc.wifi_state(HUNG_MAC)).unwrap();
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            std::env::set_var("CODEX_STATUS_TEST_HOST_WIFI",json!({"state":"connected","networks":[{"ssid":"Host"}]}).to_string());
            let mut link=bridge_ble::DeviceConnection::connect(HUNG_MAC,TOKEN,&ctx.bridge_id).await.unwrap().unwrap();
            link.command("status",json!({})).await.unwrap();
            let result=crate::wifi::automatic_on_link(&ctx,HUNG_MAC,&mut link,bridge_core::rendezvous::Budget::new(Duration::from_secs(3)),false).await;
            link.close_bounded(Duration::from_millis(100)).await;
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");std::env::remove_var("CODEX_STATUS_TEST_HOST_WIFI");
            let expected=if label=="authentication" {"authentication_failed"} else {"wifi_confirmation_pending"};
            assert_eq!(result,Err(expected.into()),"{label}");
            let after=serde_json::to_value(svc.wifi_state(HUNG_MAC)).unwrap();
            assert_eq!(after["jobs"],before["jobs"],"{label}");assert_eq!(after["targets"],before["targets"],"{label}");
            assert!(!command_ops(&ble).iter().any(|op|matches!(op.as_str(),"wifi_activate"|"wifi_save"|"network_start")),"{label}");
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn network_start_requires_refresh_after_plan_extends_rf_phase() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let (ble,url)=start_fake_ble(HUNG_MAC,false).await;
        ble.protocol.store(true,Ordering::SeqCst);
        ble.rf_initial_ms.store(700,Ordering::SeqCst);
        ble.rf_after_plan_ms.store(30000,Ordering::SeqCst);
        *ble.rf_epoch.lock().unwrap()=std::time::Instant::now();
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        let mut link=bridge_ble::DeviceConnection::connect(HUNG_MAC,TOKEN,"bridge-lock-repro").await.unwrap().unwrap();
        assert_eq!(link.command("status",json!({})).await.unwrap()["result"],"applied");
        let plan=link.command("plan",json!({"plan_id":1})).await.unwrap();
        assert_eq!(plan["result"],"applied");
        // Reproduce the old caller: keep the initial INFO deadline after the
        // device changes RF phase, then attempt startup on that expired lease.
        tokio::time::sleep(Duration::from_millis(800)).await;
        let error=link.command("network_start",json!({"plan_id":1})).await.unwrap_err();
        let failure=bridge_core::rendezvous::command_failure(&error).unwrap();
        assert_eq!(failure.outcome,bridge_core::rendezvous::DispatchOutcome::NotSent);
        assert_eq!(failure.stage,"before_write");
        assert!(!ble.network_started.load(Ordering::SeqCst));
        assert!(!command_ops(&ble).iter().any(|op|op=="network_start"));
        // The same authenticated MAC/wake reports its actual extended window.
        link.refresh_info(Duration::from_secs(1)).await.unwrap();
        assert_eq!(link.command("network_start",json!({"plan_id":1})).await.unwrap()["result"],"accepted");
        assert!(ble.network_started.load(Ordering::SeqCst));
        assert_eq!(command_ops(&ble).iter().filter(|op|*op=="network_start").count(),1);
        link.close_bounded(Duration::from_millis(100)).await;
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
    }

    #[tokio::test]
    async fn http_proof_exhaustion_and_network_error_remain_distinct() {
        for unknown in [false,true] {
            let dir=tempfile::tempdir().unwrap();let ctx=test_ctx(dir.path(),&[HUNG_MAC]);
            let error=super::rendezvous::wait_for_device_http(&ctx,HUNG_MAC,
                &json!({"_network_start_unknown":unknown}),1,true,
                bridge_core::rendezvous::Budget::new(Duration::ZERO),
                bridge_core::rendezvous::Budget::new(Duration::ZERO)).await.err().unwrap();
            assert_eq!(error,if unknown {"result_unknown"}else{"http_proof_budget_exhausted"});
            let targets=serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}])).unwrap();
            service(&ctx).wifi_save(HUNG_MAC,0,targets,100).unwrap();
            let c=serde_json::from_value(json!({"host_ssid":"Host","network_id":"home","local_revision":1,
                "version":1,"state":"connecting","error":null,"reason":0,"observed_at":100,"attempts":1,"next_attempt_at":0})).unwrap();
            service(&ctx).wifi_note_connection(HUNG_MAC,c).unwrap();
            crate::wifi::network_failed(&ctx,HUNG_MAC,"network_not_found");
            let c=service(&ctx).wifi_state(HUNG_MAC).connection.unwrap();
            assert_eq!(c.state,"connection_failed");assert_eq!(c.error.as_deref(),Some("network_not_found"));
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn network_start_reports_dispatch_without_waiting_for_http() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for case in ["not_sent","ack_unknown","write_unknown","accepted"] {
            let (ble,url)=start_fake_ble(HUNG_MAC,false).await;
            ble.protocol.store(true,Ordering::SeqCst);
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            let mut link=bridge_ble::DeviceConnection::connect(HUNG_MAC,TOKEN,"bridge-lock-repro").await.unwrap().unwrap();
            link.command("status",json!({})).await.unwrap();
            if case=="ack_unknown" {ble.lost_network_start.store(true,Ordering::SeqCst);}
            if case=="write_unknown" {*ble.blocked_op.lock().unwrap()="write:network_start".into();}
            let started=std::time::Instant::now();
            let result=super::rendezvous::network_start_on_link(&mut link,json!({"plan_id":1}),
                bridge_core::rendezvous::Budget::new(if case=="not_sent" {Duration::ZERO}else{Duration::from_millis(200)})).await;
            assert!(started.elapsed()<Duration::from_secs(1),"{case} waited for HTTP");
            match case {
                "not_sent" => assert_eq!(result,Err("not_sent".into())),
                "ack_unknown" => assert_eq!(result,Ok(false)),
                "write_unknown" => assert_eq!(result,Err("result_unknown".into())),
                _ => assert_eq!(result,Ok(true)),
            }
            assert_eq!(command_ops(&ble).iter().filter(|op|*op=="network_start").count(),usize::from(case!="not_sent"));
            if case=="write_unknown" {
                let error=link.command("status",json!({})).await.unwrap_err();
                let failure=bridge_core::rendezvous::command_failure(&error).unwrap();
                assert_eq!(failure.reason,"previous_write_unknown");
                assert_eq!(command_ops(&ble).iter().filter(|op|*op=="status").count(),1,"partial frame must prevent every later command");
            }
            link.close_bounded(Duration::from_millis(100)).await;
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn ble_cycle_reads_wifi_after_device_extends_rf_phase() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for (extended,lost_sync) in [(true,""),(true,"ack"),(true,"write"),(false,"")] {
            let dir=tempfile::tempdir().unwrap();let (ble,url)=start_fake_ble_with(HUNG_MAC,false,true,Arc::new(Mutex::new(Vec::new()))).await;
            ble.protocol.store(true,Ordering::SeqCst);
            ble.rf_initial_ms.store(700,Ordering::SeqCst);
            ble.rf_after_plan_ms.store(if extended {30000}else{0},Ordering::SeqCst);
            if !lost_sync.is_empty() { *ble.blocked_op.lock().unwrap()=if lost_sync=="write" {"write:sync_open"}else{"sync_open"}.into(); }
            *ble.rf_epoch.lock().unwrap()=std::time::Instant::now();
            let ctx=test_ctx(dir.path(),&[HUNG_MAC]);let svc=service(&ctx);
            let job=svc.wifi_queue(HUNG_MAC,&ctx.bridge_id,"read",vec![],device_now(HUNG_MAC)).unwrap();
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            let _=tokio::time::timeout(Duration::from_secs(8),ble_cycle(&ctx,&[HUNG_MAC.into()])).await;
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
            let read=svc.wifi_state(HUNG_MAC).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap();
            let should_read=extended && lost_sync!="write";
            assert_eq!(read.state=="completed",should_read,"extended={extended}, lost_sync={lost_sync}, state={}, error={:?}",read.state,read.error_code);
            assert_eq!(command_ops(&ble).iter().any(|op|op=="wifi_snapshot"),should_read);
            assert_eq!(command_ops(&ble).iter().filter(|op|*op=="sync_open").count(),1,"lost ACK must not resubmit SyncOpen");
            if lost_sync=="write" {
                let ops=command_ops(&ble);let start=ops.iter().position(|op|op=="sync_open").unwrap();
                assert_eq!(start+1,ops.len(),"an unconfirmed fragment write must close before another BLE command");
                assert_eq!(ble.closes.load(Ordering::SeqCst),1);
            }
            if should_read {assert_eq!(read.confirmed_revision,Some(8));}
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn recovery_timeout_always_finalizes_and_can_reenter_without_restart() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for (blocked,sent) in [("wifi_active",false),("wifi_save",true),("wifi_result",true)] {
            let dir=tempfile::tempdir().unwrap();let (ble,url)=start_fake_ble(HUNG_MAC,false).await;
            let ctx=test_ctx(dir.path(),&[HUNG_MAC]); let now=bridge_core::now_secs();
            let targets=serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}])).unwrap();
            service(&ctx).wifi_save(HUNG_MAC,0,targets,now).unwrap();
            let job=service(&ctx).wifi_queue(HUNG_MAC,&ctx.bridge_id,"recovery_save",vec!["home".into()],now).unwrap();
            if blocked=="wifi_result" { service(&ctx).wifi_bind_transaction(HUNG_MAC,&job.job_id,&ctx.bridge_id,"own").unwrap();service(&ctx).wifi_mark_sent(HUNG_MAC,&job.job_id,now).unwrap();service(&ctx).wifi_finish(HUNG_MAC,&job.job_id,"awaiting_confirmation",None,now-60).unwrap(); }
            *ble.claim_owner.lock().unwrap()=ctx.bridge_id.clone();ble.ble_claim.store(true,Ordering::SeqCst);
            let lock=delivery_lock(&ctx,HUNG_MAC);let _guard=lock.lock().await;
            *ble.blocked_op.lock().unwrap()=blocked.into();
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            let mut link=bridge_ble::DeviceConnection::connect(HUNG_MAC,TOKEN,&ctx.bridge_id).await.unwrap().unwrap();
            crate::wifi::recover_on_link_budget(&ctx,HUNG_MAC,&mut link,bridge_core::rendezvous::Budget::new(Duration::from_secs(1))).await;
            // This test targets an in-flight operation, not preflight exhaustion.
            // Multiple real loopback round trips need headroom on loaded hosts.
            assert!(ble.commands.lock().unwrap().iter().any(|c|c["op"]==blocked),"fault stage not reached: {blocked}");
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");
            link.close_bounded(Duration::from_millis(100)).await;
            let observed=service(&ctx).wifi_state(HUNG_MAC).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap();
            assert_eq!(observed.sent,sent,"{blocked}: {observed:?}");
            assert_eq!(observed.state,if sent {"awaiting_confirmation"} else {"waiting_rendezvous"});
            assert_eq!(observed.error_code.as_deref(),Some("result_unknown"),"{blocked}: {observed:?}");
            assert!(service(&ctx).wifi_begin(HUNG_MAC,&job.job_id,now+60).unwrap().is_some());
        }
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn offline_ble_sync_commits_frozen_rows_by_receipt_without_config_reread() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let dir=tempfile::tempdir().unwrap();let (ble,url)=start_fake_ble(HUNG_MAC,false).await;
        let ctx=test_ctx(dir.path(),&[HUNG_MAC]);let svc=service(&ctx);let now=device_now(HUNG_MAC);
        let snapshot=serde_json::from_value(json!({"schema":2,"device_mac":HUNG_MAC,"revision":1,"snapshot_id":"offline",
            "complete":true,"observed_at":now,"capacity":{"total":12},"networks":[]})).unwrap();
        let read=svc.wifi_queue(HUNG_MAC,&ctx.bridge_id,"read",vec![],now).unwrap();svc.wifi_complete_read(HUNG_MAC,&read.job_id,snapshot,now).unwrap();
        let targets=serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"},
            {"network_id":"travel","host_ssid":"Travel","ssid":"Travel-2G","credential":"open"}])).unwrap();
        svc.wifi_save(HUNG_MAC,0,targets,now).unwrap();let job=svc.wifi_queue(HUNG_MAC,&ctx.bridge_id,"sync",vec!["home".into(),"travel".into()],now).unwrap();
        *ble.claim_owner.lock().unwrap()=ctx.bridge_id.clone();ble.ble_claim.store(true,Ordering::SeqCst);
        let lock=delivery_lock(&ctx,HUNG_MAC);let _guard=lock.lock().await;
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        let mut link=bridge_ble::DeviceConnection::connect(HUNG_MAC,TOKEN,&ctx.bridge_id).await.unwrap().unwrap();
        crate::wifi::recover_on_link_budget(&ctx,HUNG_MAC,&mut link,bridge_core::rendezvous::Budget::new(Duration::from_secs(1))).await;
        std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");link.close_bounded(Duration::from_millis(100)).await;
        let state=svc.wifi_state(HUNG_MAC);assert_eq!(state.jobs.iter().find(|j|j.job_id==job.job_id).unwrap().state,"completed");
        assert_eq!(state.snapshot.unwrap().networks.len(),2);
        let commands=ble.commands.lock().unwrap();let saves:Vec<_>=commands.iter().filter(|c|c["op"]=="wifi_save").collect();
        assert_eq!(saves.len(),1);assert_eq!(saves[0]["request_id"],job.request_id);assert_eq!(saves[0]["expected_revision"],1);
        assert_eq!(saves[0]["changes"].as_array().unwrap().len(),2);
        assert!(!commands.iter().any(|c|c["op"]=="wifi_scheme_get"),"no complete configuration reread");
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    async fn changed_host_blocks_unsent_wifi_but_does_not_block_original_receipt_query() {
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        for (sent,namespace) in [(false,"own"),(true,"own"),(true,"old-bond"),(true,"")] {
            let dir=tempfile::tempdir().unwrap();let (ble,url)=start_fake_ble(HUNG_MAC,false).await;
            let ctx=test_ctx(dir.path(),&[HUNG_MAC]);let svc=service(&ctx);let now=device_now(HUNG_MAC);
            let snapshot=serde_json::from_value(json!({"schema":2,"device_mac":HUNG_MAC,"revision":1,"snapshot_id":"host-bound",
                "complete":true,"observed_at":now,"capacity":{"total":12},"networks":[]})).unwrap();
            let read=svc.wifi_queue(HUNG_MAC,&ctx.bridge_id,"read",vec![],now).unwrap();svc.wifi_complete_read(HUNG_MAC,&read.job_id,snapshot,now).unwrap();
            svc.wifi_save(HUNG_MAC,0,serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}])).unwrap(),now).unwrap();
            let job=svc.wifi_queue_manual_sync(HUNG_MAC,&ctx.bridge_id,vec!["home".into()],1,"host-bound","Host",now).unwrap();
            if sent {
                if !namespace.is_empty() {svc.wifi_bind_transaction(HUNG_MAC,&job.job_id,&ctx.bridge_id,namespace).unwrap();}
                svc.wifi_mark_sent(HUNG_MAC,&job.job_id,now).unwrap();svc.wifi_finish(HUNG_MAC,&job.job_id,"awaiting_confirmation",None,now-60).unwrap();
            }
            std::env::set_var("CODEX_STATUS_TEST_HOST_WIFI",json!({"state":"connected","networks":[{"ssid":"Changed"}]}).to_string());
            std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
            let mut link=bridge_ble::DeviceConnection::connect(HUNG_MAC,TOKEN,&ctx.bridge_id).await.unwrap().unwrap();
            let lock=delivery_lock(&ctx,HUNG_MAC);let _guard=lock.lock().await;
            crate::wifi::recover_on_link_budget(&ctx,HUNG_MAC,&mut link,bridge_core::rendezvous::Budget::new(Duration::from_secs(1))).await;
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");std::env::remove_var("CODEX_STATUS_TEST_HOST_WIFI");
            link.close_bounded(Duration::from_millis(100)).await;
            let observed=svc.wifi_state(HUNG_MAC).jobs.into_iter().find(|j|j.job_id==job.job_id).unwrap();
            let identity_matches=namespace=="own";
            assert_eq!(observed.state,if !sent {"failed"}else if identity_matches {"completed"}else{"needs_attention"},"{observed:?}");
            assert_eq!(observed.sent,sent);
            if !sent {assert_eq!(observed.error_code.as_deref(),Some("host_network_changed"));}
            if sent && !identity_matches {
                assert_eq!(observed.error_code.as_deref(),Some(if namespace.is_empty(){"transaction_identity_unknown"}else{"transaction_namespace_changed"}));
                assert!(observed.confirmation_pending());
                assert_eq!(observed.request_id,job.request_id);
            }
            let commands=ble.commands.lock().unwrap();
            assert!(!commands.iter().any(|c|matches!(c["op"].as_str(),Some("wifi_owner"|"wifi_save"))));
            assert_eq!(commands.iter().any(|c|c["op"]=="wifi_result"&&c["query_id"]==job.request_id),sent && identity_matches);
            if !identity_matches {assert!(!commands.iter().any(|c|c["op"]=="wifi_result_ack"));}
        }
    }

    #[tokio::test]
    async fn page_network_guard_does_not_block_programmatic_cached_sync() {
        let dir=tempfile::tempdir().unwrap();let ctx=test_ctx(dir.path(),&[HUNG_MAC]);let svc=service(&ctx);let now=device_now(HUNG_MAC);
        let read=svc.wifi_queue(HUNG_MAC,&ctx.bridge_id,"read",vec![],now).unwrap();
        let snapshot=serde_json::from_value(json!({"schema":2,"device_mac":HUNG_MAC,"revision":1,"snapshot_id":"scope",
            "complete":true,"observed_at":now,"capacity":{"total":12},"networks":[]})).unwrap();svc.wifi_complete_read(HUNG_MAC,&read.job_id,snapshot,now).unwrap();
        svc.wifi_save(HUNG_MAC,0,serde_json::from_value(json!([{"network_id":"travel","host_ssid":"Travel","ssid":"Travel-2G","credential":"open"}])).unwrap(),now).unwrap();
        let body=json!({"selected":["travel"],"local_revision":1,"snapshot_id":"scope"});
        let _global=ctx.device_delivery.lock().await;let lock=delivery_lock(&ctx,HUNG_MAC);let _device=lock.lock().await;
        assert_eq!(crate::wifi::action(&ctx,HUNG_MAC,"sync",body.clone()).await.unwrap_err(),"host_network_unknown");
        let job:Value=serde_json::from_str(&tool(&ctx,"platform_wifi_action",&json!({"mac":HUNG_MAC,"op":"sync","body":body})).await.unwrap()).unwrap();
        assert_eq!(job["kind"],"sync");assert_eq!(job["state"],"waiting_rendezvous");assert_eq!(job["error_code"],"device_channel_busy");assert_eq!(job["expected_revision"],1);assert!(job["host_ssid"].is_null());
    }

    #[test]
    fn confirmed_http_resets_consecutive_wifi_budget_without_replaying_unknown_jobs() {
        let dir=tempfile::tempdir().unwrap();let ctx=test_ctx(dir.path(),&[HUNG_MAC]);let svc=service(&ctx);let now=device_now(HUNG_MAC);
        svc.wifi_save(HUNG_MAC,0,serde_json::from_value(json!([{"network_id":"home","host_ssid":"Host","ssid":"Device","credential":"open"}])).unwrap(),now).unwrap();
        let job=svc.wifi_queue(HUNG_MAC,&ctx.bridge_id,"recovery_save",vec!["home".into()],now).unwrap();
        svc.wifi_mark_sent(HUNG_MAC,&job.job_id,now).unwrap();
        svc.wifi_finish(HUNG_MAC,&job.job_id,"awaiting_confirmation",Some("ack_unavailable"),now).unwrap();
        let before=serde_json::to_value(svc.wifi_state(HUNG_MAC).jobs).unwrap();
        for state in ["connection_failed","waiting_for_window","awaiting_confirmation","requested"] {
            let c=serde_json::from_value(json!({"host_ssid":"Host","network_id":"home","local_revision":1,"version":8,
                "state":state,"error":"connect_timeout","reason":0,"observed_at":now,"attempts":3,"next_attempt_at":now+60,"http_ready":false})).unwrap();
            svc.wifi_note_connection(HUNG_MAC,c).unwrap();
            crate::wifi::http_failed(&ctx,HUNG_MAC,"http_unavailable");
            assert_eq!(svc.wifi_state(HUNG_MAC).connection.unwrap().attempts,3,"a failed read is not a new retry grant");
            crate::wifi::http_ready(&ctx,HUNG_MAC);
            let c=svc.wifi_state(HUNG_MAC).connection.unwrap();
            assert_eq!(c.attempts,if state=="requested"{3}else{0});
            if state!="requested" {assert!(c.http_ready);assert_eq!(c.next_attempt_at,0);assert_eq!(c.error,None);}
            assert_eq!(serde_json::to_value(svc.wifi_state(HUNG_MAC).jobs).unwrap(),before,"HTTP does not replace or resend an unknown transaction");
        }
        crate::wifi::http_failed(&ctx,HUNG_MAC,"unauthorized");
        assert_eq!(svc.wifi_state(HUNG_MAC).connection.unwrap().attempts,3);
        // Persist the successful reset, then prove restart keeps the new budget.
        let mut c=svc.wifi_state(HUNG_MAC).connection.unwrap();c.state="connection_failed".into();svc.wifi_note_connection(HUNG_MAC,c).unwrap();
        crate::wifi::http_ready(&ctx,HUNG_MAC);
        drop(ctx);
        let ctx=test_ctx(dir.path(),&[HUNG_MAC]);let svc=service(&ctx);
        assert_eq!(svc.wifi_state(HUNG_MAC).connection.unwrap().attempts,0);
        let mut c=svc.wifi_state(HUNG_MAC).connection.unwrap();c.http_ready=false;c.state="waiting_for_window".into();svc.wifi_note_connection(HUNG_MAC,c).unwrap();
        assert_eq!(svc.wifi_state(HUNG_MAC).connection.unwrap().automatic_state(),"waiting_for_window");
    }

    #[tokio::test(flavor="multi_thread",worker_threads=2)]
    #[ignore="requires device-sim built in the same target directory; run explicitly with --ignored"]
    async fn wifi_activate_pending_confirmation_reports_error_without_consuming_budget() {
        use std::io::BufRead;
        const HUNG_MAC:&str="0200000000D3";
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        struct Cleanup(std::process::Child);
        impl Drop for Cleanup {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();
            std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");std::env::remove_var("CODEX_STATUS_TEST_HOST_WIFI");}}
        let dir=tempfile::tempdir().unwrap();
        let exe=std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join(format!("device-sim{}",std::env::consts::EXE_SUFFIX));
        assert!(exe.exists(),"build device-sim in the same isolated target before this test");
        let mut child=std::process::Command::new(exe)
            .args(["--listen","127.0.0.1:0","--mac",HUNG_MAC,"--data-dir"])
            .arg(dir.path().join("fake"))
            .env("CODEX_STATUS_SIM_ENDPOINT_TOKEN",TOKEN)
            .env("CODEX_STATUS_SIM_DEVICE_TOKEN","synthetic-device-token")
            .env("CODEX_STATUS_SIM_CONTROL_TOKEN","synthetic-control-token")
            .env("CODEX_STATUS_SIM_WIFI_PRINCIPAL","bridge-lock-repro")
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::inherit()).spawn().unwrap();
        let mut line=String::new();std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let _cleanup=Cleanup(child);
        let ready:Value=serde_json::from_str(&line).unwrap();let url=ready["http"].as_str().unwrap();
        let client=reqwest::Client::new();
        assert!(client.post(format!("{url}/sim/power")).bearer_auth("synthetic-control-token").json(&json!({"plugged":true})).send().await.unwrap().status().is_success());
        let endpoint=url.strip_prefix("http://").unwrap().to_owned();
        let setup_endpoint=endpoint.clone();
        let (claim_code,_)=tokio::task::spawn_blocking(move||bridge_core::device_client::claim(
            &setup_endpoint,TOKEN,"synthetic-device-token",
            &json!({"id":"bridge-lock-repro","lease":"300"}),Duration::from_secs(3)).unwrap()).await.unwrap();
        assert_eq!(claim_code,200);
        let setup_endpoint=endpoint.clone();
        let status=tokio::task::spawn_blocking(move||bridge_core::device_client::status(
            &setup_endpoint,TOKEN,Duration::from_secs(3)).unwrap()).await.unwrap();
        let setup_endpoint=endpoint.clone();let nonce=status["session_nonce"].as_str().unwrap().to_owned();
        let saved=tokio::task::spawn_blocking(move||bridge_core::device_client::wifi_command(
            &setup_endpoint,TOKEN,HUNG_MAC,"bridge-lock-repro","apply",&nonce,json!({
            "request_id":"fixture-save","expected_revision":0,
            "changes":[{"network_id":"home","host_ssid":"FakeHost","ssid":"FakeDevice","credential":"open"}]
        }),Duration::from_secs(3)).unwrap()).await.unwrap();assert_eq!(saved["outcome"],"committed");
        let ctx=test_ctx(dir.path(),&[HUNG_MAC]);let svc=service(&ctx);let now=device_now(HUNG_MAC);
        svc.wifi_save(HUNG_MAC,0,serde_json::from_value(json!([{"network_id":"home","host_ssid":"FakeHost","ssid":"FakeDevice","credential":"open"}])).unwrap(),now).unwrap();
        // One unknown submission blocks conflicting writes even after local cancellation.
        // Queue-capacity retention remains covered by the core WifiState test.
        let job=svc.wifi_queue_control(HUNG_MAC,&ctx.bridge_id,"activate",json!({"network_id":"home","version":1}),now).unwrap();
        svc.wifi_attempt(HUNG_MAC,&job.job_id, &job.bridge_id, "own",true,&BTreeMap::new(),now,
            |_,_|Ok(json!({"ok":true,"outcome":"unknown"}))).unwrap();
        svc.wifi_finish(HUNG_MAC,&job.job_id,"cancelled",None,now).unwrap();
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({HUNG_MAC:url}).to_string());
        std::env::set_var("CODEX_STATUS_TEST_HOST_WIFI",json!({"state":"connected","networks":[{"ssid":"FakeHost"}]}).to_string());
        let mut link=bridge_ble::DeviceConnection::connect(HUNG_MAC,TOKEN,&ctx.bridge_id).await.unwrap().unwrap();
        assert_eq!(link.info["wifi_management"],3);assert_eq!(link.info["wc"],1);
        for _ in 1..=3 {
            if let Some(mut c)=svc.wifi_state(HUNG_MAC).connection {c.next_attempt_at=0;svc.wifi_note_connection(HUNG_MAC,c).unwrap();}
            let result=crate::wifi::automatic_on_link(&ctx,HUNG_MAC,&mut link,bridge_core::rendezvous::Budget::new(Duration::from_secs(3)),false).await;
            assert_eq!(result,Err("wifi_confirmation_pending".into()));
            let state=svc.wifi_state(HUNG_MAC);assert_eq!(state.jobs.len(),1);
            let connection=state.connection.unwrap();assert_eq!(connection.attempts,0);
            assert_eq!(connection.error.as_deref(),Some("wifi_confirmation_pending"));
            let receipt=link.command("wifi_result",json!({"query_id":"w3_0000000000000002"})).await.unwrap();
            assert_eq!(receipt["outcome"],"unknown","second activation never reached production Fake engine");
        }
        let result=crate::wifi::automatic_on_link(&ctx,HUNG_MAC,&mut link,bridge_core::rendezvous::Budget::new(Duration::from_secs(3)),false).await;
        assert_eq!(result,Err("wifi_confirmation_pending".into()));
        assert_eq!(link.command("wifi_active",json!({})).await.unwrap()["network_id"],"");
        let radio:Value=client.post(format!("{url}/sim/wifi")).bearer_auth("synthetic-control-token").json(&json!({"outcome":0})).send().await.unwrap().json().await.unwrap();
        assert_eq!(radio["radio"]["begins"],0,"no RF call despite consuming the entire retry budget");
        eprintln!("FIX VERIFIED: unknown submission protected; automatic_on_link reports confirmation pending, attempts=0, RF begin=0.");
        link.close_bounded(Duration::from_millis(100)).await;
    }

    #[tokio::test(flavor="multi_thread",worker_threads=4)]
    #[ignore="requires same-target device-sim and CODEX_STATUS_TEST_REPORT_IP private local interface"]
    async fn wifi_new_protocol_fake_report_readback_then_idle_does_not_activate() {
        use std::io::BufRead;
        const MAC:&str="0200000000D4";
        let _serial=SIM_ENV.lock().unwrap_or_else(|p|p.into_inner());
        let ip:std::net::Ipv4Addr=std::env::var("CODEX_STATUS_TEST_REPORT_IP").expect("private local interface required").parse().unwrap();
        assert!(ip.is_private() && !ip.is_loopback());
        struct Cleanup(std::process::Child);
        impl Drop for Cleanup {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();std::env::remove_var("CODEX_STATUS_SIM_BLE_ENDPOINTS");}}
        let dir=tempfile::tempdir().unwrap();let sim_dir=tempfile::tempdir().unwrap();
        let exe=std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join(format!("device-sim{}",std::env::consts::EXE_SUFFIX));
        let address=format!("{ip}:80");
        let mut child=std::process::Command::new(exe).args(["--listen",&address,"--mac",MAC,"--data-dir"])
            .arg(sim_dir.path()).env("CODEX_STATUS_SIM_ENDPOINT_TOKEN",TOKEN)
            .env("CODEX_STATUS_SIM_DEVICE_TOKEN","aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .env("CODEX_STATUS_SIM_CONTROL_TOKEN","synthetic-control-token")
            .env("CODEX_STATUS_SIM_WIFI_PRINCIPAL","bridge-lock-repro")
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::inherit()).spawn().unwrap();
        let mut line=String::new();std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let _cleanup=Cleanup(child);let ready:Value=serde_json::from_str(&line).unwrap();let url=ready["http"].as_str().unwrap();
        let client=reqwest::Client::builder().no_proxy().build().unwrap();
        assert!(client.post(format!("{url}/sim/power")).bearer_auth("synthetic-control-token").json(&json!({"plugged":true})).send().await.unwrap().status().is_success());
        let endpoint=address.clone();
        let (code,_)=tokio::task::spawn_blocking(move||bridge_core::device_client::claim(&endpoint,TOKEN,"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &json!({"id":"bridge-lock-repro","lease":"300"}),Duration::from_secs(3)).unwrap()).await.unwrap();assert_eq!(code,200);
        let listener=tokio::net::TcpListener::bind((ip,0)).await.unwrap();
        let mut ctx=test_ctx_with_ip(dir.path(),MAC,&ip.to_string());ctx.config.port=listener.local_addr().unwrap().port();let ctx=Arc::new(ctx);
        let router=Router::new().route("/api/device/session",post(crate::report_session))
            .route("/api/device/report",post(crate::framed_ready_report)).with_state(ctx.clone());
        let server=tokio::spawn(async move{axum::serve(listener,router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();});
        std::env::set_var("CODEX_STATUS_SIM_BLE_ENDPOINTS",json!({MAC:ready["ble_http"]}).to_string());
        let mut link=bridge_ble::DeviceConnection::connect(MAC,TOKEN,&ctx.bridge_id).await.unwrap().unwrap();
        let saved=link.command("wifi_save",json!({"expected_revision":0,"changes":[
            {"network_id":"home","host_ssid":"FakeHost","ssid":"FakeDevice","credential":"open"}]})).await.unwrap();assert_eq!(saved["outcome"],"committed");
        let job=queue_read_job(&ctx,MAC);let plan=service(&ctx).plan_for_rendezvous_network_work(MAC,device_now(MAC),"rendezvous",0,true).unwrap();
        let mut body=serde_json::to_value(&plan).unwrap();body["need_http"]=json!(true);
        assert_eq!(link.command("plan",body).await.unwrap()["result"],"applied");
        let activation=link.command("wifi_activate",json!({"network_id":"home","version":1})).await.unwrap();
        assert_eq!(activation["stage"],"accepted","{activation}");
        // Poll the actual shared Wi-Fi HAL to DHCP/IP completion.
        for _ in 0..2 {assert!(client.post(format!("{url}/sim/wifi")).bearer_auth("synthetic-control-token").json(&json!({"outcome":1})).send().await.unwrap().status().is_success());}
        assert_eq!(link.command("network_start",json!({"plan_id":plan.plan_id,"bridge_addr":ip.to_string(),
            "bridge_port":ctx.config.port,"origin_request_no":plan.plan_id})).await.unwrap()["result"],"accepted");
        assert_eq!(service(&ctx).coordinator_summary(MAC,device_now(MAC)).unwrap()["plan"]["last_accepted_id"],0);
        link.close_bounded(Duration::from_secs(1)).await;
        let until=tokio::time::Instant::now()+Duration::from_secs(8);
        while observed_job(&ctx,MAC,&job).0!="completed" && tokio::time::Instant::now()<until {tokio::time::sleep(Duration::from_millis(100)).await;}
        if observed_job(&ctx,MAC,&job).0!="completed" {
            let attrs:Value=client.get(format!("{url}/sim/ble/attributes")).bearer_auth("synthetic-control-token").send().await.unwrap().json().await.unwrap();
            let endpoint=address.clone();let status=tokio::task::spawn_blocking(move||bridge_core::device_client::status(&endpoint,TOKEN,Duration::from_secs(2)).unwrap()).await.unwrap();
            eprintln!("Fake report facts: events={} power={} rv={} transports={} owner={} boot={}",attrs["events"],status["power"],status["rendezvous"],status["transports"],status["owner"],status["boot_id"]);
        }
        assert_eq!(observed_job(&ctx,MAC,&job),("completed".into(),None),"real report must trigger authenticated readback and queued work");
        assert_eq!(service(&ctx).coordinator_summary(MAC,device_now(MAC)).unwrap()["plan"]["last_accepted_id"],plan.plan_id);
        let attrs:Value=client.get(format!("{url}/sim/ble/attributes")).bearer_auth("synthetic-control-token").send().await.unwrap().json().await.unwrap();
        assert!(attrs["events"].as_array().unwrap().iter().any(|e|e["event"]=="ready_report" && e["received"]==true));
        let radio:Value=client.post(format!("{url}/sim/wifi")).bearer_auth("synthetic-control-token").json(&json!({"outcome":1})).send().await.unwrap().json().await.unwrap();
        let begins=radio["radio"]["begins"].clone();assert_eq!(begins,1);
        for _ in 0..4 {
            let result=ble_cycle(&ctx,&[MAC.to_owned()]).await;
            if !matches!(result,Ok(BleOpportunity::Connected{result:Ok(()),..})) {
                let endpoint=address.clone();let status=tokio::task::spawn_blocking(move||bridge_core::device_client::status(&endpoint,TOKEN,Duration::from_secs(2)).unwrap()).await.unwrap();
                eprintln!("post-work cycle: power={} sync={} decision={} coordinator={}",status["power"],status["sync"],service(&ctx).next_delivery(MAC,true,device_now(MAC)),service(&ctx).coordinator_summary(MAC,device_now(MAC)).unwrap()["plan"]);
            }
            assert!(matches!(result,Ok(BleOpportunity::Connected{result:Ok(()),..})),"{result:?}");
        }
        let after:Value=client.post(format!("{url}/sim/wifi")).bearer_auth("synthetic-control-token").json(&json!({"outcome":1})).send().await.unwrap().json().await.unwrap();
        assert_eq!(after["radio"]["begins"],begins,"completed HTTP work must not authorize a later activation");
        assert!(!service(&ctx).wifi_state(MAC).jobs.iter().any(|j|j.kind=="activate"));
        // An authenticated online cache can expire during slow onboarding or
        // OTA. Exercise the production scheduler -> HTTP -> shared ROM path,
        // including a second source change without an explicit push action.
        let source:Value=serde_json::from_str(include_str!("../../../../tools/test-bridge/templates/quad.json")).unwrap();
        bridge_mcp::save_device_token(&crate::mcp_config(&ctx),MAC,"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let svc=service(&ctx);let now=device_now(MAC);
        svc.template_save("quad",bridge_core::platform::model::RENDER_TARGET_154G,&source,now).unwrap();
        let mut profile=bridge_core::platform::model::Profile::draft(MAC);
        profile.template_ids=vec!["quad".into()];profile.initial_active_id=Some("quad".into());profile.sync_enabled=true;
        svc.profile_save(profile,now).unwrap();svc.publish(MAC,now).unwrap();
        assert_eq!(deliver(&ctx,MAC).await["ack"]["result"],"applied");
        assert_eq!(refresh_status(&ctx,MAC).await["result"],"ok");
        let mut previous_seq=0;
        for used in [30,35] {
            note_envelope(&ctx,&json!({"schema":1,"server_time":device_now(MAC),"account":{"plan":"plus"},
                "buckets":[{"id":"codex","windows":[{"kind":"weekly","usedPercent":used,"resetsAt":2000000000,"windowMins":10080}]}]}));
            {
                let mut cache=ctx.device_status_cache.lock().unwrap();let stale=cache.get_mut(MAC).unwrap();
                stale.fetched_at-=31;stale.fetched_mono_ms=None;
                assert!(!crate::device_status_fresh(MAC,stale,crate::scheduled_now(MAC,crate::now_secs()),30));
                assert_eq!(crate::device_cycle_targets(&svc.devices(),&cache,false),vec![MAC]);
            }
            cycle(&ctx,MAC,true,true).await;
            let read=refresh_status(&ctx,MAC).await;assert_eq!(read["result"],"ok");
            let seq=read["status"]["applied_seq"].as_u64().unwrap();assert!(seq>previous_seq,"{read}");previous_seq=seq;
            let summary=svc.coordinator_summary(MAC,device_now(MAC)).unwrap();
            assert!(summary["in_flight"].is_null());assert_eq!(summary["context"]["last_applied_seq"],seq);
            assert_eq!(summary["context"]["acked_push"],true);
            assert_eq!(summary["acked_push_fp"],summary["push_fp"]);
        }
        server.abort();
    }

}
