//! Application-service front end for the four UI pages and MCP (v2 §2/§11).
//!
//! Both interfaces call these functions; there is no second business model.
//! Saving never publishes, publishing never auto-enables sync, and every device
//! write is a user-visible action that goes through the coordinator.

use std::sync::Arc;

use bridge_core::coordinator::DeliveryKind;
use bridge_core::platform::model::{DeviceCapabilities, DeviceIdentity, FamilyProfile, Profile};
use bridge_core::platform::service::PlatformService;
use bridge_core::v2_client;
use serde_json::{json, Value};
use std::time::Duration;

use crate::AppCtx;

/// Resolved per-device transport facts (token from the cached device token).
pub struct DeviceLink {
    pub mac: String,
    pub ip: String,
    pub token: String,
    pub bridge_id: String,
}

pub fn service(ctx: &AppCtx) -> &Arc<PlatformService> {
    &ctx.platform
}

/// Device link from the live identity + cached device token; `None` when the
/// token has not been negotiated yet (click BOOT to open the BLE session).
pub fn device_link(ctx: &AppCtx) -> Option<DeviceLink> {
    let ip = ctx.device_ip.lock().unwrap().clone();
    if ip.is_empty() || ip == "0.0.0.0" {
        return None;
    }
    let mac = ctx
        .device_mac
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| "".to_string());
    if mac.is_empty() {
        return None;
    }
    // v2 business endpoints authenticate with the endpoint token the bridge
    // wrote over BLE (the same trust as /usage and /template); the device
    // operation token stays reserved for /claim, /update and /doUpdate.
    let token = ctx.config.token.clone();
    if token.is_empty() {
        return None;
    }
    Some(DeviceLink {
        mac,
        ip,
        token,
        bridge_id: ctx.bridge_id.clone(),
    })
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
    v2_status: &Value,
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
    let (capabilities, legacy) = caps_from_status(status)?;
    if legacy {
        return Err("v2 capabilities missing: device reports a legacy protocol".into());
    }
    let authenticated_mac = v2_status
        .get("device_mac")
        .and_then(Value::as_str)
        .and_then(DeviceIdentity::normalized_mac)
        .ok_or_else(|| "authenticated /v2/status missing valid device_mac".to_string())?;
    if authenticated_mac != requested_mac || authenticated_mac != status_mac {
        return Err(format!(
            "authenticated /v2/status reports MAC {authenticated_mac}, expected {requested_mac}"
        ));
    }
    Ok((capabilities, requested_mac))
}

fn registration_identity(mac: &str, endpoint: &str, name: &str) -> Result<DeviceIdentity, String> {
    let mut identity = DeviceIdentity::new(mac, name).map_err(err_text)?;
    identity.ip = Some(endpoint.to_string());
    identity.discovered_via = "manual".into();
    identity.last_seen_at = now_secs();
    Ok(identity)
}

async fn register_device_v2(ctx: &AppCtx, args: &Value) -> Result<Value, String> {
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
    let (endpoint_status, endpoint_v2_status, token) =
        (endpoint.clone(), endpoint.clone(), ctx.config.token.clone());
    let (status, v2_status) = blocking(move || {
        let status = bridge_core::device::fetch(&endpoint_status, timeout())
            .map_err(err_text)?
            .raw
            .ok_or_else(|| "endpoint did not return structured /status.json".to_string())?;
        let v2_status =
            v2_client::status(&endpoint_v2_status, &token, timeout()).map_err(err_text)?;
        Ok((status, v2_status))
    })
    .await?;
    let (capabilities, mac) = registered_device_facts(&mac, &status, &v2_status)?;
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
        .device_upsert(identity, capabilities, false)
        .map_err(err_text)?;
    service(ctx)
        .device_get(&mac)
        .ok_or_else(|| "registered device record unavailable".into())
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

    fn v2_status(mac: &str) -> Value {
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
            registered_device_facts("aa-bb-cc-dd-ee-ff", &status(), &v2_status("AABBCCDDEEFF"))
                .unwrap();
        assert_eq!(mac, "AABBCCDDEEFF");
        assert_eq!(caps.max_templates, 8);

        assert!(registered_device_facts(
            "00:00:00:00:00:01",
            &status(),
            &v2_status("AABBCCDDEEFF")
        )
        .is_err());
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &json!({"fw_target":"codex-status-154g"}),
            &v2_status("AABBCCDDEEFF")
        )
        .is_err());
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &json!({
                "mac": "00:00:00:00:00:01",
                "fw_target": "codex-status-154g",
                "render_target": "epd-ssd1681-200x200-1bpp"
            }),
            &v2_status("AABBCCDDEEFF")
        )
        .is_err());
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &status(),
            &v2_status("00:00:00:00:00:01")
        )
        .is_err());
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &json!({"mac":"AABBCCDDEEFF"}),
            &v2_status("AABBCCDDEEFF")
        )
        .unwrap_err()
        .contains("v2 capabilities missing"));
        assert!(registered_device_facts(
            "AABBCCDDEEFF",
            &json!({"mac":"AABBCCDDEEFF", "fw_target":"zectrix-note4-400x300", "render_target":"epd-ssd2683-400x300-1bpp"}),
            &v2_status("AABBCCDDEEFF")
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

fn endpoint(ctx: &AppCtx) -> String {
    ctx.device_ip.lock().unwrap().clone()
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

// ---------------------------------------------------------------------------
// Overview / templates / profiles
// ---------------------------------------------------------------------------

pub fn overview(ctx: &AppCtx) -> Value {
    let mut value = service(ctx).overview();
    value["device"] = device_summary(ctx);
    value["legacy_device"] = json!(
        !service(ctx).devices().is_empty()
            && service(ctx).devices()[0]["legacy"]
                .as_bool()
                .unwrap_or(false)
    );
    value["link"] = match device_link(ctx) {
        Some(link) => json!({"mac": link.mac, "ip": link.ip, "token_cached": true}),
        None => {
            json!({"mac": ctx.device_mac.lock().unwrap().clone(), "ip": endpoint(ctx), "token_cached": false})
        }
    };
    value
}

fn device_summary(ctx: &AppCtx) -> Value {
    let mac = ctx.device_mac.lock().unwrap().clone();
    let name = ctx.device_name.lock().unwrap().clone();
    json!({
        "device_mac": mac,
        "name": name,
        "ip": endpoint(ctx),
    })
}

pub fn device_rows(ctx: &AppCtx) -> Value {
    json!({
        "devices": service(ctx).devices(),
        "templates": service(ctx).templates(),
    })
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
    let bits = bridge_render::render_bits(&text, &usage, &bridge_render::Env::default())
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
    let mac = ctx.device_mac.lock().unwrap().clone().unwrap_or_default();
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
        .family_profile_copy_from_device(mac, id, name, now_secs())
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
        .publish_checked(mac, now_secs(), expected_target_id, Some(&ctx.bridge_id))
        .map_err(err_text)?;
    let delivery = deliver(ctx, mac).await;
    Ok(json!({
        "job": job,
        "delivery": delivery,
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
    service(ctx).cancel_job(mac, now_secs());
    json!({"cancelled": true, "job": service(ctx).job(mac)})
}

pub async fn activate(ctx: &AppCtx, mac: &str, template_id: &str) -> Result<Value, String> {
    service(ctx).activate(mac, template_id).map_err(err_text)?;
    Ok(deliver(ctx, mac).await)
}

/// Execute at most one pending coordinator action against the device.
pub async fn deliver(ctx: &AppCtx, mac: &str) -> Value {
    let _delivery = ctx.v2_delivery.lock().await;
    if service(ctx).asset_job_pending(mac) {
        return json!({"result": "waiting_for_device_protocol", "reason": "versioned manifest/object endpoints are pending joint device confirmation"});
    }
    let Some(link) = device_link_for_mac(ctx, mac) else {
        return json!({"result": "waiting_for_link", "reason": "device token/ip not available; open a BOOT session"});
    };
    let reachable = true;
    let decision = service(ctx).next_http_delivery(mac, reachable, now_secs());
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
                v2_client::install_bundle(
                    &ip,
                    &token,
                    &expected_mac,
                    &bridge_id,
                    &payload,
                    v2_client::BUNDLE_CHUNK_BYTES,
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
                        service(ctx).retry_job(mac, &job_id, true, now_secs());
                        if !ctx_id.is_empty() {
                            let _ = service(ctx).adopt_activation_context(mac, ctx_id, now_secs());
                        }
                    } else {
                        let _ = service(ctx).retry_job(mac, &job_id, false, now_secs());
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
                v2_client::activate(
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
                            let _ = service(ctx).note_activate_done(mac, ctx_id, now_secs());
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
            match blocking(move || {
                v2_client::data(&ip, &token, &expected_mac, &sent, timeout()).map_err(err_text)
            })
            .await
            {
                Ok(ack) => {
                    let applied = ack["result"] == "applied";
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
                    tracing::info!(device = mac, seq, outcome = %outcome["outcome"],
                        transport = "http", ack = %ack, "v2 data acknowledgement");
                    json!({"result": "data", "transport": "http", "ack": ack, "confirmation": outcome})
                }
                Err(e) => json!({"result": "deferred", "error": e}),
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
    let _delivery = ctx.v2_delivery.lock().await;
    let Some(link) = device_link_for_mac(ctx, mac) else {
        return json!({"result": "waiting_for_link"});
    };
    let plan = match service(ctx).plan_for_rendezvous(
        mac,
        now_secs(),
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
    match blocking(move || {
        v2_client::plan(&ip, &token, &expected_mac, &sent, timeout()).map_err(err_text)
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
                    now_secs(),
                );
            }
            json!({"result": "plan", "plan": plan, "ack": ack, "accepted": accepted})
        }
        Err(e) => json!({"result": "deferred", "error": e}),
    }
}

/// User-requested light window. Freeze one formal plan before attempting any
/// transport so a deep-sleeping device receives the same ID over BLE later.
pub async fn request_light(ctx: &AppCtx, mac: &str) -> Value {
    let _delivery = ctx.v2_delivery.lock().await;
    let (plan, already_pending) = match service(ctx).queue_explicit_light(mac, now_secs()) {
        Ok(value) => value,
        Err(e) => return json!({"result": "failed", "error": e.to_string()}),
    };
    ctx.force_ble.notify_one();
    let selected_mac = ctx
        .device_mac
        .lock()
        .unwrap()
        .as_deref()
        .and_then(DeviceIdentity::normalized_mac);
    let requested_mac = DeviceIdentity::normalized_mac(mac);
    let online = selected_mac.is_some()
        && selected_mac == requested_mac
        && ctx
            .device_cache
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|c| c.online);
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
    match blocking(move || {
        v2_client::plan(&ip, &token, &expected_mac, &body, timeout()).map_err(err_text)
    })
    .await
    {
        Ok(ack) if ack["result"] == "applied" => {
            let remaining = ack["accepted_remaining_s"].as_u64().unwrap_or(0) as u32;
            let confirmation =
                service(ctx).note_plan_ack(mac, plan.plan_id, remaining, false, now_secs());
            json!({"result": "applied", "transport": "http", "plan": plan,
                "already_pending": already_pending, "ack": ack, "confirmation": confirmation})
        }
        Ok(ack) => json!({"result": "queued", "transport": "ble_rendezvous",
            "plan": plan, "already_pending": already_pending, "ack": ack}),
        Err(error) => json!({"result": "queued", "transport": "ble_rendezvous",
            "plan": plan, "already_pending": already_pending, "ack": null, "http_error": error}),
    }
}

/// Refresh the bridge-side view from the device's authenticated status.
pub async fn refresh_status(ctx: &AppCtx, mac: &str) -> Value {
    let Some(link) = device_link_for_mac(ctx, mac) else {
        crate::update_v2_status_cache(
            &mut ctx.v2_status_cache.lock().unwrap(),
            mac,
            false,
            None,
            crate::now_secs(),
        );
        return json!({"result": "waiting_for_link"});
    };
    let (ip, token, expected_mac) = (link.ip.clone(), link.token.clone(), link.mac.clone());
    match blocking(move || v2_client::status(&ip, &token, timeout()).map_err(err_text)).await {
        Ok(status) => {
            let matches_target = status["device_mac"]
                .as_str()
                .and_then(DeviceIdentity::normalized_mac)
                .zip(DeviceIdentity::normalized_mac(mac))
                .is_some_and(|(actual, expected)| actual == expected);
            if !matches_target {
                crate::update_v2_status_cache(
                    &mut ctx.v2_status_cache.lock().unwrap(),
                    mac,
                    false,
                    None,
                    crate::now_secs(),
                );
                return json!({"result": "error", "error": format!("authenticated status MAC {} does not match target MAC {}", status["device_mac"].as_str().unwrap_or("<missing>"), expected_mac)});
            }
            let _ = service(ctx).note_device_status(mac, &status);
            crate::update_v2_status_cache(
                &mut ctx.v2_status_cache.lock().unwrap(),
                mac,
                true,
                Some(status.clone()),
                crate::now_secs(),
            );
            json!({"result": "ok", "status": status})
        }
        Err(e) => {
            crate::update_v2_status_cache(
                &mut ctx.v2_status_cache.lock().unwrap(),
                mac,
                false,
                None,
                crate::now_secs(),
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
pub async fn post_ota_window(ctx: &AppCtx, secs: u32) {
    let mac = ctx.device_mac.lock().unwrap().clone().unwrap_or_default();
    if mac.is_empty() {
        return;
    }
    if let Err(e) = service(ctx).hold_light(&mac, now_secs() + secs as u64) {
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
        v2_client::plan(&ip, &token, &expected_mac, &sent, timeout()).map_err(err_text)
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
                    now_secs(),
                );
            }
            tracing::info!(device = mac, %ack, "post-OTA light plan");
        }
        Err(e) => tracing::warn!("post-OTA light plan failed: {e}"),
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

pub fn power_view(ctx: &AppCtx) -> Value {
    let mac = ctx.device_mac.lock().unwrap().clone().unwrap_or_default();
    let summary = service(ctx).coordinator_summary(&mac, now_secs());
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
                if now_secs() < hold_until { "applied" } else { "expired" }
            } else { "none" },
            "pending_plan": pending, "ack": ack},
        "note": "reads never extend the light deadline; only a formal PowerPlan does",
    })
}

pub fn recovery(ctx: &AppCtx, digest: &Value) -> Result<Value, String> {
    let mac = ctx.device_mac.lock().unwrap().clone().unwrap_or_default();
    let profile = service(ctx)
        .recovery_import(&mac, digest, now_secs())
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

/// Capabilities from a `/status.json` document. Firmware that reports
/// `fw_target` speaks v2; anything older is legacy (≤3 templates, legacy
/// channel) and must be shown as such instead of silently truncating.
pub fn caps_from_status(raw: &Value) -> Result<(DeviceCapabilities, bool), String> {
    let fw_target = raw.get("fw_target").and_then(|v| v.as_str());
    let Some(fw_target) = fw_target else {
        let mut caps = DeviceCapabilities::ssd1681_154g();
        caps.max_templates = 3;
        caps.max_light_s = 600;
        return Ok((caps, true));
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
    Ok((caps, false))
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
        let (caps, legacy) = caps_from_status(&complete).unwrap();
        assert!(!legacy);
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
}

/// Feed the device's own status digest (MAC-verified HTTP read) into the
/// coordinator; the authenticated `/v2/status` remains authoritative for writes.
pub fn note_status_json(ctx: &AppCtx, raw: &Value) {
    let mac = ctx.device_mac.lock().unwrap().clone().unwrap_or_default();
    if mac.is_empty() {
        return;
    }
    let _ = service(ctx).note_device_status(&mac, raw);
}

/// Register/refresh the current device in the platform service. Legacy devices
/// keep the legacy flag so the UI shows the limitation instead of truncating.
pub fn ensure_device(ctx: &AppCtx, capabilities: DeviceCapabilities, legacy: bool) {
    let mac = ctx.device_mac.lock().unwrap().clone().unwrap_or_default();
    if mac.is_empty() {
        return;
    }
    let name = ctx.device_name.lock().unwrap().clone();
    if let Ok(identity) = DeviceIdentity::new(&mac, &name) {
        let mut identity = identity;
        identity.ip = Some(endpoint(ctx));
        let _ = service(ctx).device_upsert(identity, capabilities, legacy);
    }
}

/// True when the device speaks the v2 protocol: the legacy Wi-Fi push channel
/// must then stay quiet (one device, one data channel).
pub fn is_v2_device(ctx: &AppCtx) -> bool {
    let mac = ctx.device_mac.lock().unwrap().clone().unwrap_or_default();
    if mac.is_empty() {
        return false;
    }
    service(ctx)
        .device_get(&mac)
        .map(|d| !d["legacy"].as_bool().unwrap_or(true))
        .unwrap_or(false)
}

/// Outcome of one v2 BLE scan and, when connected, its rendezvous.
#[derive(Debug)]
pub enum BleOpportunity {
    /// None of the registered candidates was advertising.
    NoDevice,
    /// A verified MAC connected; the rendezvous may still have failed.
    Connected { mac: String, result: Result<(), String> },
}

/// One real GATT rendezvous. Small snapshots stay on BLE; larger work receives
/// a formal light plan and is then delivered by the HTTP cycle.
pub async fn ble_cycle(ctx: &AppCtx, candidates: &[String]) -> Result<BleOpportunity, String> {
    if candidates.is_empty() {
        return Ok(BleOpportunity::NoDevice);
    }
    let _delivery = ctx.v2_delivery.lock().await;
    let (mac, mut link) = match bridge_ble::V2Connection::connect_any(
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
    let work = async {
        let still_registered = service(ctx).devices().into_iter().any(|device| {
            device["legacy"].as_bool() == Some(false)
                && device["device_mac"].as_str().and_then(DeviceIdentity::normalized_mac)
                    == Some(mac.clone())
        });
        if !still_registered {
            return Err(format!("connected BLE device {mac} is no longer registered as v2"));
        }
        let state = link.command("status", json!({})).await.map_err(err_text)?;
        if state["result"] != "applied" {
            return Err("BLE status rejected".to_owned());
        }
        service(ctx)
            .note_device_status(&mac, &state)
            .map_err(err_text)?;
        let decision = service(ctx).next_delivery(&mac, true, now_secs());
        if decision["decision"] == "ble_data" {
            if let Some(body) = service(ctx).data_message_body(&mac) {
                let ack = link.command("data", body.clone()).await.map_err(err_text)?;
                let applied = ack["result"] == "applied"
                    && ack["data_seq"] == body["seq"]
                    && ack["active_context_id"] == body["active_context_id"];
                let outcome = service(ctx).note_ack(
                    &mac,
                    DeliveryKind::BleData,
                    body["seq"].as_u64().unwrap_or(0),
                    body["crc"].as_str().unwrap_or(""),
                    applied,
                    ack["display_state"].as_str().unwrap_or("unchanged"),
                );
                tracing::info!(device = mac, outcome = %outcome["outcome"], transport = "ble",
                    ack = %ack, "v2 data acknowledgement");
            }
        }
        let remaining = state["power"]["provisional_remaining_s"]
            .as_u64()
            .unwrap_or(0) as u32;
        let plan = service(ctx)
            .plan_for_rendezvous(
                &mac,
                now_secs(),
                if remaining > 0 {
                    "manual"
                } else {
                    "rendezvous"
                },
                remaining,
            )
            .map_err(err_text)?;
        let ack = link
            .command("plan", serde_json::to_value(&plan).map_err(err_text)?)
            .await
            .map_err(err_text)?;
        if ack["result"] != "applied" {
            return Err(format!("BLE PowerPlan rejected: {ack}"));
        }
        let confirmation = service(ctx).note_plan_ack(
            &mac,
            plan.plan_id,
            ack["accepted_remaining_s"].as_u64().unwrap_or(0) as u32,
            remaining > 0,
            now_secs(),
        );
        tracing::info!(device = mac, plan_id = plan.plan_id, ack = %ack,
            confirmation = %confirmation, "v2 PowerPlan acknowledgement");
        Ok(())
    };
    let result = tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .map_err(|_| "BLE rendezvous timed out".to_owned())
        .and_then(|r| r);
    link.close().await;
    Ok(BleOpportunity::Connected { mac, result })
}

/// Run one coordinator cycle for a registered v2 device.
pub async fn cycle(ctx: &AppCtx, mac: &str, refresh: bool, deliver_now: bool) {
    let Some(mac) = DeviceIdentity::normalized_mac(mac) else {
        return;
    };
    let Some(device) = service(ctx).device_get(&mac) else {
        return;
    };
    if device["legacy"].as_bool().unwrap_or(true) {
        return;
    }
    if !matches!(crate::v2_occupancy_gate(ctx, &mac).await, crate::Occupancy::Owned) {
        return;
    }
    if refresh {
        if refresh_status(ctx, &mac).await["result"].as_str() != Some("ok") {
            return;
        }
        // Formal plan for the current rendezvous: the Bridge is the only source
        // of light/sleep decisions. A BOOT wake is answered with the *remaining*
        // provisional window; everything else is a fresh decision. Repeats are
        // idempotent on the device; a new id is only generated when the decision
        // really changes or the window has expired.
        let status = service(ctx)
            .coordinator_summary(&mac, now_secs())
            .unwrap_or(Value::Null);
        let provisional = status["session"]["power"]["provisional"]
            .as_bool()
            .unwrap_or(false);
        let remaining = status["session"]["power"]["provisional_remaining_s"]
            .as_u64()
            .unwrap_or(0) as u32;
        if provisional {
            let _ = send_plan(ctx, &mac, "manual", remaining).await;
        } else {
            let _ = send_plan(ctx, &mac, "rendezvous", 0).await;
        }
    }
    if deliver_now {
        let _ = deliver(ctx, &mac).await;
    }
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
        "platform_device_register_v2" => register_device_v2(ctx, args).await?,
        "platform_overview" => overview(ctx),
        "template_list" => templates(ctx),
        "template_get_v2" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("missing id")?;
            let target = args.get("render_target").and_then(|v| v.as_str());
            template_get(ctx, id, target)?
        }
        "template_save_v2" => {
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
        "template_validate_v2" => template_validate(&json_arg(args)?),
        "profile_get_v2" => profile_get(ctx),
        "profile_save_v2" => {
            let profile: Profile =
                serde_json::from_value(args.get("profile").cloned().ok_or("missing profile")?)
                    .map_err(err_text)?;
            profile_save(ctx, profile)?
        }
        "family_profiles_v2" => {
            family_profiles(ctx, args.get("render_target").and_then(Value::as_str))
        }
        "family_profile_save_v2" => {
            let profile: FamilyProfile =
                serde_json::from_value(args.get("profile").cloned().ok_or("missing profile")?)
                    .map_err(err_text)?;
            family_profile_save(ctx, profile)?
        }
        "family_profile_delete_v2" => {
            let render_target = args
                .get("render_target")
                .and_then(Value::as_str)
                .ok_or("missing render_target")?;
            let id = args.get("id").and_then(Value::as_str).ok_or("missing id")?;
            family_profile_delete(ctx, render_target, id)?
        }
        "family_profile_copy_v2" => {
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
            let mac = device_mac(ctx)?;
            publish(
                ctx,
                &mac,
                args.get("expected_target_id").and_then(Value::as_str),
            )
            .await?
        }
        "platform_publish_preview" => {
            let mac = device_mac(ctx)?;
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
            let mac = device_mac(ctx)?;
            job_cancel(ctx, &mac)
        }
        "template_activate" => {
            let mac = device_mac(ctx)?;
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("missing id")?;
            activate(ctx, &mac, id).await?
        }
        "data_sources_v2" => data_sources(ctx),
        "data_source_save_v2" => {
            let source = args.get("source").cloned().ok_or("missing source")?;
            data_source_save(ctx, source)?
        }
        "data_probe_v2" => {
            let id = args
                .get("source_id")
                .and_then(|v| v.as_str())
                .ok_or("missing source_id")?;
            data_probe(ctx, id)?
        }
        "power_view_v2" => power_view(ctx),
        "power_plan" => {
            let mac = device_mac(ctx)?;
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
                        v2_client::plan(&ip, &token, &expected_mac, &sent, timeout())
                            .map_err(err_text)
                    })
                    .await?;
                    json!({"plan": body, "ack": ack})
                }
                other => return Err(format!("mode must be light|sleep (got {other})")),
            }
        }
        "platform_status_refresh" => {
            let mac = device_mac(ctx)?;
            refresh_status(ctx, &mac).await
        }
        // Explicit user/agent action: deliver pending data now instead of
        // waiting for the automatic cadence.
        "platform_push_now" => {
            let mac = device_mac(ctx)?;
            deliver(ctx, &mac).await
        }
        "platform_recovery" => {
            let digest = args.get("digest").cloned().ok_or("missing digest")?;
            recovery(ctx, &digest)?
        }
        other => return Err(format!("unknown platform tool: {other}")),
    };
    serde_json::to_string_pretty(&value).map_err(err_text)
}

fn device_mac(ctx: &AppCtx) -> Result<String, String> {
    ctx.device_mac
        .lock()
        .unwrap()
        .clone()
        .filter(|m| !m.is_empty())
        .ok_or_else(|| "device MAC not learned yet".to_string())
}
