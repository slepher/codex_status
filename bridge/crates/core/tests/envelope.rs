use std::collections::BTreeMap;

use bridge_core::envelope::{account_username, build_envelope, window_kind, EnvelopeOptions, TemplateRef};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/rate_limits.json")).expect("fixture json")
}

fn options() -> EnvelopeOptions {
    let mut templates = BTreeMap::new();
    templates.insert(
        "full".to_string(),
        TemplateRef {
            version: 3,
            hash: "c1a2faaf".to_string(),
        },
    );
    EnvelopeOptions {
        bridge_label: Some("PC-TEST".to_string()),
        bridge_host_id: "ab12".to_string(),
        bridge_host: "192.168.1.100".to_string(),
        bridge_port: 8765,
        next_sync_seconds: 60,
        templates,
        active_hold_seconds: 600,
    }
}

#[test]
fn classifies_windows_by_duration_not_position() {
    assert_eq!(window_kind(300), "5h");
    assert_eq!(window_kind(10080), "weekly");
    assert_eq!(window_kind(43200), "monthly");
    assert_eq!(window_kind(1440), "window");
}

#[test]
fn maps_real_payload() {
    let envelope = build_envelope(&fixture(), &options());
    assert_eq!(envelope["schema"], 1);
    assert_eq!(envelope["account"]["plan"], "prolite");
    assert_eq!(envelope["bridge"]["label"], "PC-TEST");
    assert_eq!(envelope["bridge"]["hostId"], "ab12");
    assert_eq!(envelope["bridge"]["host"], "192.168.1.100");
    assert_eq!(envelope["bridge"]["port"], 8765);
    assert_eq!(envelope["next_sync_seconds"], 60);
    assert_eq!(envelope["active_hold_seconds"], 600);
    assert!(envelope.get("idle_template").is_none());

    let buckets = envelope["buckets"].as_array().expect("buckets");
    let codex = buckets
        .iter()
        .find(|b| b["id"] == "codex")
        .expect("codex bucket");
    let windows = codex["windows"].as_array().expect("windows");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["kind"], "weekly");
    assert_eq!(windows[0]["windowMins"], 10080);
    assert_eq!(codex["credits"]["hasCredits"], false);
    assert!(envelope.get("resetCredits").is_none());

    let bengalfox = buckets
        .iter()
        .find(|b| b["id"] == "codex_bengalfox")
        .expect("bengalfox bucket");
    assert_eq!(bengalfox["name"], "GPT-5.3-Codex-Spark");
    let kinds: Vec<&str> = bengalfox["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["5h", "weekly"]);
    assert_eq!(bengalfox["windows"][0]["resetsAt"], 1789163705i64);

    assert_eq!(envelope["templates"]["full"]["hash"], "c1a2faaf");
    assert_eq!(envelope["templates"]["full"]["version"], 3);
}

#[test]
fn tolerates_missing_optional_fields() {
    let minimal = serde_json::json!({
        "rateLimits": {
            "limitId": "codex",
            "primary": {"usedPercent": 42, "windowDurationMins": 300, "resetsAt": 123},
            "planType": "plus"
        }
    });
    let envelope = build_envelope(&minimal, &options());
    assert_eq!(envelope["account"]["plan"], "plus");
    let buckets = envelope["buckets"].as_array().unwrap();
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0]["id"], "codex");
    assert_eq!(buckets[0]["windows"][0]["kind"], "5h");
    assert_eq!(buckets[0]["windows"][0]["usedPercent"], 42);
    assert_eq!(buckets[0]["credits"], Value::Null);
}

#[test]
fn reset_credits_take_earliest_expiry() {
    let payload = serde_json::json!({
        "rateLimits": {"limitId": "codex", "planType": "pro", "primary": {"usedPercent": 1, "windowDurationMins": 10080, "resetsAt": 1}},
        "rateLimitResetCredits": {
            "availableCount": 2,
            "credits": [
                {"status": "available", "title": "Full reset", "expiresAt": 500},
                {"status": "available", "title": "Full reset", "expiresAt": 300}
            ]
        }
    });
    let envelope = build_envelope(&payload, &options());
    assert_eq!(envelope["resetCredits"]["availableCount"], 2);
    assert_eq!(envelope["resetCredits"]["nextExpiresAt"], 300);
}

#[test]
fn reset_credits_absent_when_none_available() {
    let payload = serde_json::json!({
        "rateLimits": {"limitId": "codex", "planType": "pro", "primary": {"usedPercent": 1, "windowDurationMins": 10080, "resetsAt": 1}},
        "rateLimitResetCredits": {"availableCount": 0, "credits": []}
    });
    let envelope = build_envelope(&payload, &options());
    assert!(envelope.get("resetCredits").is_none());
}

#[test]
fn username_from_email_local_part_is_ascii_safe() {
    let account = serde_json::json!({
        "account": {"type": "chatgpt", "email": "Codex.User@example.com", "planType": "prolite"},
        "requiresOpenaiAuth": false
    });
    assert_eq!(account_username(&account).as_deref(), Some("Codex.User"));
    let unicode = serde_json::json!({"account": {"email": "用户@example.com"}});
    assert_eq!(account_username(&unicode).as_deref(), Some("??"));
    assert!(account_username(&serde_json::json!({"account": null})).is_none());
    assert!(account_username(&serde_json::json!({})).is_none());
}

#[test]
fn missing_username_leaves_label_null() {
    let mut opts = options();
    opts.bridge_label = None;
    let envelope = build_envelope(&fixture(), &opts);
    assert_eq!(envelope["bridge"]["label"], Value::Null);
}
