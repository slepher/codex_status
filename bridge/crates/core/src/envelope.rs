//! Device usage envelope (docs/history/request.md §7.5), built from the app-server payload.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Serialize)]
pub struct TemplateRef {
    pub version: u64,
    pub hash: String,
}

#[derive(Debug, Clone)]
pub struct EnvelopeOptions {
    pub bridge_label: Option<String>,
    pub bridge_host_id: String,
    /// Bridge LAN address/port advertised for device-side endpoint self-heal.
    pub bridge_host: String,
    pub bridge_port: u16,
    pub next_sync_seconds: u64,
    pub templates: BTreeMap<String, TemplateRef>,
    /// active hold window in seconds (default 600).
    pub active_hold_seconds: u64,
}

/// Window type is classified by duration, never by primary/secondary position.
pub fn window_kind(minutes: i64) -> &'static str {
    if minutes == 300 {
        "5h"
    } else if minutes >= 43200 {
        "monthly"
    } else if minutes >= 10080 {
        "weekly"
    } else {
        "window"
    }
}

fn window_json(w: Option<&Value>) -> Option<Value> {
    let w = w?;
    if w.is_null() {
        return None;
    }
    let used = w.get("usedPercent").and_then(|v| v.as_i64()).unwrap_or(0);
    let mins = w.get("windowDurationMins").and_then(|v| v.as_i64()).unwrap_or(0);
    let resets = w.get("resetsAt").and_then(|v| v.as_i64()).unwrap_or(0);
    Some(json!({
        "kind": window_kind(mins),
        "usedPercent": used,
        "resetsAt": resets,
        "windowMins": mins,
    }))
}

fn bucket_json(id: &str, rl: &Value) -> Value {
    let name = rl
        .get("limitName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut windows = Vec::new();
    if let Some(w) = window_json(rl.get("primary")) {
        windows.push(w);
    }
    if let Some(w) = window_json(rl.get("secondary")) {
        windows.push(w);
    }
    json!({
        "id": id,
        "name": name,
        "windows": windows,
        "credits": rl.get("credits").cloned().unwrap_or(Value::Null),
    })
}

fn reset_credits(result: &Value) -> (i64, i64) {
    let rc = result
        .get("rateLimitResetCredits")
        .cloned()
        .unwrap_or(Value::Null);
    let count = rc
        .get("availableCount")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let next = rc
        .get("credits")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("expiresAt").and_then(|v| v.as_i64()))
                .min()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    (count, next)
}

/// Display label for the bridge: the Codex account email local part, ASCII-safe.
pub fn account_username(account: &Value) -> Option<String> {
    let email = account.get("account")?.get("email")?.as_str()?;
    let local = email.split('@').next().unwrap_or("");
    let cleaned: String = local
        .chars()
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '?' })
        .take(16)
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

pub fn build_envelope(result: &Value, opts: &EnvelopeOptions) -> Value {
    let direct = result.get("rateLimits").cloned().unwrap_or(Value::Null);
    let by = result
        .get("rateLimitsByLimitId")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let mut buckets: Vec<Value> = Vec::new();
    if let Some(obj) = by.as_object() {
        for (id, rl) in obj {
            buckets.push(bucket_json(id, rl));
        }
    }
    if !direct.is_null()
        && !buckets
            .iter()
            .any(|b| b.get("id").and_then(|v| v.as_str()) == Some("codex"))
    {
        buckets.insert(0, bucket_json("codex", &direct));
    }

    let plan = by
        .get("codex")
        .and_then(|r| r.get("planType"))
        .and_then(|v| v.as_str())
        .or_else(|| direct.get("planType").and_then(|v| v.as_str()))
        .unwrap_or("?");

    let (reset_count, reset_next) = reset_credits(result);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let templates: Map<String, Value> = opts
        .templates
        .iter()
        .map(|(id, t)| {
            (
                id.clone(),
                json!({"version": t.version, "hash": t.hash.clone()}),
            )
        })
        .collect();

    let mut envelope = json!({
        "schema": 1,
        "server_time": now,
        "next_sync_seconds": opts.next_sync_seconds,
        "active_hold_seconds": opts.active_hold_seconds,
        "bridge": {
            "label": opts.bridge_label,
            "hostId": opts.bridge_host_id,
            "host": opts.bridge_host,
            "port": opts.bridge_port
        },
        "account": {"plan": plan},
        "buckets": buckets,
        "templates": templates,
    });
    if reset_count > 0 {
        envelope["resetCredits"] = json!({"availableCount": reset_count, "nextExpiresAt": reset_next});
    }
    envelope
}
