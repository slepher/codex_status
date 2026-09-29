//! Application-service front end for the four UI pages and MCP (platform design §2/§11).
//!
//! Both interfaces call these functions; there is no second business model.
//! Saving never publishes, publishing never auto-enables sync, and every device
//! write is a user-visible action that goes through the coordinator.

use std::sync::Arc;

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

fn delivery_lock(ctx: &AppCtx, mac: &str) -> Arc<tokio::sync::Mutex<()>> {
    ctx.per_mac_delivery.lock().unwrap().entry(mac.to_uppercase())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))).clone()
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
    let Some(previous_endpoint) = existing["ip"].as_str().map(str::to_string) else {
        return Ok(false);
    };
    if previous_endpoint == endpoint {
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
    if current["ip"].as_str() != Some(&previous_endpoint) {
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

async fn sync_http_inner(ctx: &AppCtx, mac: &str, may_continue: bool) {
    let Some(link) = device_link_for_mac(ctx, mac) else { return; };
    let root = bridge_core::paths::data_root();
    match blocking(move || {
        let mut last = json!({"result":"idle"});
        for attempt in 0..3 {
            last = sync_diagnostics::transfer(&root, &link).map_err(err_text)?;
            if last["result"] != "complete" { break; }
            if !may_continue || attempt == 2 { break; }
            // A completed one-shot batch can close Wi-Fi immediately. Probe
            // briefly before attempting another batch; a full retry here
            // waits through association timeouts after every normal sync.
            let next = match device_client::status(&link.ip, &link.token, Duration::from_secs(1)) {
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
        else if current["value"] != latest["value"] { "upgrade_available" }
        else if comparable && image["sha256"] != release["sha256"] { "different_image" }
        else { "current" };
    let verdict_label = match verdict {
        "current" => "已是最新版", "upgrade_available" => "可升级",
        "different_image" => "同版本但运行镜像不同", _ => "无法判断",
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
        "delivery":{"enabled":raw["sync_enabled"],"source":"bridge_setting"},
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
        "png_base64": base64(&png),
    }))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
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
            match blocking(move || {
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
            match blocking(move || {
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
            match blocking(move || {
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
    match blocking(move || {
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
    match blocking(move || {
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
    match blocking(move || device_client::status(&ip, &token, timeout()).map_err(err_text)).await {
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
            let _ = service(ctx).note_device_status_at(mac, &status, "http", received_at);
            if let Err(e) = service(ctx).note_authenticated_status_at(mac, &status, received_at) {
                return json!({"result": "error", "error": format!("persist authenticated status: {e}")});
            }
            let _ = service(ctx).ota_note_authenticated_version(mac, &status);
            if let Some(job) = service(ctx).ota_job(mac).filter(|job|
                job.state == "awaiting_confirmation") {
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

/// Post-OTA window. The fresh firmware keeps a minimum light window on its own
/// (`rtcPostOtaHoldS`, 300 s); this is the bridge taking control: raise the
/// coordinator light hold and, once the rebooted device answers HTTP, send one
/// explicit 300 s light PowerPlan. Run from a spawned task: the bounded wait
/// (reboot + Wi-Fi) must not delay the MCP response.
pub async fn post_ota_window(ctx: &AppCtx, requested_mac: &str, secs: u32) {
    let Some(mac) = DeviceIdentity::normalized_mac(requested_mac) else {
        tracing::warn!("post-OTA window skipped: invalid device MAC");
        return;
    };
    if let Err(e) = service(ctx).hold_light(&mac, device_now(&mac) + secs as u64) {
        tracing::debug!("post-OTA hold: {e}");
        return;
    }
    let mut online = false;
    for _ in 0..20 {
        if refresh_status(ctx, &mac).await["result"].as_str() == Some("ok") {
            online = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    if !online {
        tracing::info!(
            "post-OTA window: device did not answer HTTP; hold stays for the next rendezvous"
        );
        return;
    }
    let Ok(plan) = service(ctx).explicit_plan(
        &mac,
        bridge_core::platform::model::PlanMode::Light,
        secs,
        "post-ota",
    ) else {
        return;
    };
    let Some(link) = device_link_for_mac(ctx, &mac) else {
        return;
    };
    let mut body = serde_json::to_value(&plan).unwrap_or(Value::Null);
    body["bridge_id"] = json!(ctx.bridge_id);
    let (ip, token, expected_mac) = (link.ip.clone(), link.token.clone(), link.mac.clone());
    let sent = body.clone();
    match blocking(move || {
        device_client::plan(&ip, &token, &expected_mac, &sent, timeout()).map_err(err_text)
    })
    .await
    {
        Ok(ack) => {
            if ack["result"] == "applied" {
                let _ = service(ctx).note_plan_ack(
                    &mac,
                    plan.plan_id,
                    ack["accepted_remaining_s"].as_u64().unwrap_or(0) as u32,
                    false,
                    device_now(&mac),
                );
            }
            tracing::info!(
                event = "ack",
                device_mac = mac,
                plan_id = plan.plan_id,
                result = ack["result"].as_str().unwrap_or("unknown"),
                ack_plan_id = ack["plan_id"].as_u64().unwrap_or(0),
                accepted_remaining_s = ack["accepted_remaining_s"].as_u64().unwrap_or(0),
                "post-OTA light plan acknowledgement"
            );
        }
        Err(e) => tracing::warn!(event = "result", device_mac = mac, plan_id = plan.plan_id,
            error_category = safe_protocol_error_category(&e), "post-OTA light plan failed"),
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

/// True when the selected device is registered in the platform store: the
/// single-device Wi-Fi push channel must then stay quiet (one device, one data
/// channel). Every registered device speaks device.
pub fn is_registered_device(ctx: &AppCtx) -> bool {
    let Some(mac) = crate::selected_mac(ctx) else {
        return false;
    };
    service(ctx).device_get(&mac).is_some()
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

async fn cache_device_token_if_missing(link: &bridge_ble::DeviceConnection, mac: &str) {
    if bridge_mcp::load_runtime_device_token(mac).is_some() {
        return;
    }
    match tokio::time::timeout(Duration::from_secs(1), link.request_device_token()).await {
        Ok(Ok(token)) => match bridge_mcp::save_runtime_device_token(mac, &token) {
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

/// One real GATT rendezvous. Small snapshots stay on BLE; larger work receives
/// a formal light plan and is then delivered by the HTTP cycle.
pub async fn ble_cycle(ctx: &AppCtx, candidates: &[String]) -> Result<BleOpportunity, String> {
    if candidates.is_empty() {
        return Ok(BleOpportunity::NoDevice);
    }
    let _delivery = ctx.device_delivery.lock().await;
    let (mac, mut link) = match bridge_ble::DeviceConnection::connect_any(
        candidates,
        &ctx.config.token,
        &ctx.bridge_id,
    )
    .await
    .map_err(err_text)?
    {
        Some((mac, link)) => (mac, link),
        None => return Ok(BleOpportunity::NoDevice),
    };
    let lock = delivery_lock(ctx, &mac);
    let _device_delivery = lock.lock().await;
    let still_registered = service(ctx).devices().into_iter().any(|device| {
        device["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac)
            == Some(mac.clone())
    });
    // The firmware closes BLE 200 ms after the Plan ACK. Fetch before the
    // ordinary timeout starts so a failed optional request cannot consume the
    // Data/Plan exchange budget.
    if still_registered {
        cache_device_token_if_missing(&link, &mac).await;
    }
    let work = async {
        if !still_registered {
            return Err(format!("connected BLE device {mac} is no longer registered for device protocol"));
        }
        link.write_endpoint(&bridge_ble::lan_ip(), ctx.config.port, &ctx.config.token)
            .await
            .map_err(err_text)?;
        let state = link.command("status", json!({})).await.map_err(err_text)?;
        if state["result"] != "applied" {
            return Err("BLE status rejected".to_owned());
        }
        let received_at = device_now(&mac);
        service(ctx)
            .note_device_status_at(&mac, &state, "ble", received_at)
            .map_err(err_text)?;
        service(ctx).note_ble_contact_at(&mac, received_at).map_err(err_text)?;
        let sync_capable = state["sync"]["v"] == 1;
        let mut sync_enabled = state["sync"]["enabled"] == true;
        if sync_capable && !sync_enabled {
            if let Ok(reply) = link.command("sync_config", json!({"enabled":true})).await {
                sync_enabled = reply["result"] == "applied";
            }
        }
        let decision = service(ctx).next_delivery(&mac, true, device_now(&mac));
        if decision["decision"] == "ble_data" {
            if let Some(body) = service(ctx).data_message_body(&mac) {
                let ack = link.command("data", body.clone()).await.map_err(err_text)?;
                let applied = ack["result"] == "applied"
                    && ack["data_seq"] == body["seq"]
                    && ack["active_context_id"] == body["active_context_id"];
                let ack_category = if ack["result"] != "applied" {
                    "ack_rejected"
                } else if !applied {
                    "ack_mismatch"
                } else {
                    "none"
                };
                let outcome = service(ctx).note_ack(
                    &mac,
                    DeliveryKind::BleData,
                    body["seq"].as_u64().unwrap_or(0),
                    body["crc"].as_str().unwrap_or(""),
                    applied,
                    ack["display_state"].as_str().unwrap_or("unchanged"),
                );
                tracing::info!(
                    event = "ack",
                    device_mac = mac,
                    seq = body["seq"].as_u64().unwrap_or(0),
                    data_seq = ack["data_seq"].as_u64().unwrap_or(0),
                    result = ack["result"].as_str().unwrap_or("unknown"),
                    display_state = ack["display_state"].as_str().unwrap_or("unknown"),
                    error_category = ack_category,
                    outcome = outcome["outcome"].as_str().unwrap_or("unknown"),
                    transport = "ble",
                    "device data acknowledgement"
                );
            }
        }
        let remaining = state["power"]["provisional_remaining_s"]
            .as_u64()
            .unwrap_or(0) as u32;
        let plan = service(ctx)
            .plan_for_rendezvous(
                &mac,
                device_now(&mac),
                if remaining > 0 {
                    "manual"
                } else {
                    "rendezvous"
                },
                remaining,
            )
            .map_err(err_text)?;
        let mut plan_body = serde_json::to_value(&plan).map_err(err_text)?;
        if !sync_enabled && wake_history_sync_due(&mac, &state) {
            plan_body["history_sync_ms"] = json!(2500);
        }
        let ack = link
            .command("plan", plan_body)
            .await
            .map_err(err_text)?;
        let accepted = ack["result"] == "applied";
        let ack_category = if !accepted {
            "ack_rejected"
        } else if ack["plan_id"].as_u64().is_some_and(|id| id != plan.plan_id) {
            "ack_mismatch"
        } else {
            "none"
        };
        if !accepted {
            tracing::warn!(event = "ack", device_mac = %mac, plan_id = plan.plan_id,
                result = ack["result"].as_str().unwrap_or("unknown"),
                ack_plan_id = ack["plan_id"].as_u64().unwrap_or(0),
                accepted_remaining_s = ack["accepted_remaining_s"].as_u64().unwrap_or(0),
                error_category = ack_category, transport = "ble", "device PowerPlan rejected");
            return Err("BLE PowerPlan rejected (ack_rejected)".to_owned());
        }
        let confirmation = service(ctx).note_plan_ack(
            &mac,
            plan.plan_id,
            ack["accepted_remaining_s"].as_u64().unwrap_or(0) as u32,
            remaining > 0,
            device_now(&mac),
        );
        tracing::info!(
            event = "ack",
            device_mac = mac,
            plan_id = plan.plan_id,
            result = ack["result"].as_str().unwrap_or("unknown"),
            ack_plan_id = ack["plan_id"].as_u64().unwrap_or(0),
            accepted_remaining_s = ack["accepted_remaining_s"].as_u64().unwrap_or(0),
            error_category = ack_category,
            confirmation = confirmation["outcome"].as_str().unwrap_or("unknown"),
            transport = "ble",
            "device PowerPlan acknowledgement"
        );
        let mut sync_opened = false;
        let should_open = sync_enabled && state["sync"]["retry_skip"] == 0 &&
            (state["sync"]["due"] == true || state["sync"]["pending"] == true);
        if should_open {
            let reason = if state["sync"]["rounds"] == 15 &&
                state["sync"]["pending"] != true { "periodic" } else { "retry" };
            let open = link.command("sync_open", json!({
                "reason":reason,
                "open_id":uuid::Uuid::new_v4().simple().to_string(),
                "wake_generation":state["wake_generation"],
                "wake_seq":state["wake_seq"],
            })).await.map_err(err_text)?;
            sync_opened = open["result"] == "applied";
            if !sync_opened { return Err(format!("sync_open rejected: {}", open["error"])); }
        }
        let light_plan = ack["accepted_remaining_s"].as_u64().unwrap_or(0) > 0;
        Ok((state, sync_enabled, sync_opened, light_plan))
    };
    let normal_result = tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .map_err(|_| "BLE rendezvous timed out".to_owned())
        .and_then(|r| r);
    if let Ok((state, enabled, _, _)) = &normal_result {
        // Keep diagnostics outside the normal rendezvous deadline: a history
        // timeout can never turn a successful status/data/plan exchange into
        // a failed contact.
        if !enabled { sync_wake_history(&mut link, &mac, state).await; }
    }
    let sync_opened = normal_result.as_ref().is_ok_and(|(_, enabled, opened, light)|
        *opened || (*enabled && *light));
    let may_continue = normal_result.as_ref().is_ok_and(|(_, _, _, light)| *light);
    let result = normal_result.map(|_| ());
    link.close().await;
    if sync_opened { sync_http_inner(ctx, &mac, may_continue).await; }
    Ok(BleOpportunity::Connected { mac, result })
}

/// Run one coordinator cycle for a registered device.
pub async fn cycle(ctx: &AppCtx, mac: &str, refresh: bool, deliver_now: bool) {
    let Some(mac) = DeviceIdentity::normalized_mac(mac) else {
        return;
    };
    if service(ctx).device_get(&mac).is_none() {
        return;
    }
    if refresh {
        if refresh_status(ctx, &mac).await["result"].as_str() != Some("ok") {
            return;
        }
    }
    match crate::device_occupancy_gate(ctx, &mac).await {
        crate::Occupancy::Owned => {}
        crate::Occupancy::Yielded => return,
        crate::Occupancy::Other(owner) => {
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
    let mut ota_window = false;
    if refresh {
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
    let ticket = match blocking(move || {
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
    if result.is_ok() {
        drop(_delivery);
        post_ota_window(ctx, mac, 300).await;
    }
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
                    let ack = blocking(move || {
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
