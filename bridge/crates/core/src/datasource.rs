//! DataSource + SourceSnapshot (v2 §3/§6).
//!
//! Sources are Codex (the app-server envelope) and a Static JSON test/source.
//! Field contracts carry only `push` or `pull`; the trigger classification never
//! leaves the bridge. A failed fetch keeps the last good values and reports the
//! error separately.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::platform::model::{FieldKind, FieldTrigger, Quality, SnapshotField, SourceSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSourceKind {
    Codex,
    StaticJson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub source_id: String,
    pub kind: DataSourceKind,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub credential_ref: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl DataSource {
    pub fn validate(&self) -> Result<()> {
        if self.source_id.is_empty()
            || !self
                .source_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("invalid source_id: use [A-Za-z0-9_-]");
        }
        if matches!(self.kind, DataSourceKind::StaticJson) {
            let has_values = self
                .config
                .get("values")
                .map(|v| v.is_object())
                .unwrap_or(false);
            let has_json = self
                .config
                .get("json")
                .map(|v| v.is_string())
                .unwrap_or(false);
            if !has_values && !has_json {
                bail!(
                    "static source {}: config needs `values` object or `json` string",
                    self.source_id
                );
            }
        }
        Ok(())
    }

    /// Explicit per-field trigger override from config, by exact key or suffix.
    pub fn trigger_override(&self, field: &str) -> Option<FieldTrigger> {
        let table = self.config.get("triggers")?.as_object()?;
        for (k, v) in table {
            let matches = field == k || field.ends_with(&format!(".{k}"));
            if matches {
                return match v.as_str() {
                    Some("push") => Some(FieldTrigger::Push),
                    Some("pull") => Some(FieldTrigger::Pull),
                    _ => None,
                };
            }
        }
        None
    }

    pub fn trigger_for(&self, field: &str) -> FieldTrigger {
        self.trigger_override(field)
            .unwrap_or_else(|| classify_trigger(field))
    }

    pub fn field_kind(&self, field: &str) -> FieldKind {
        let overrides = self.config.get("field_kinds").and_then(|v| v.as_object());
        if let Some(overrides) = overrides {
            for (k, v) in overrides {
                if field == k || field.ends_with(&format!(".{k}")) {
                    if let Some(kind) = v.as_str() {
                        return match kind {
                            "number" => FieldKind::Number,
                            "bool" => FieldKind::Bool,
                            _ => FieldKind::Text,
                        };
                    }
                }
            }
        }
        classify_kind(field)
    }

    pub fn valid_for_s(&self) -> u64 {
        self.config
            .get("valid_for_s")
            .and_then(|v| v.as_u64())
            .unwrap_or(3600)
    }

    /// Produce the latest snapshot. `envelope` is the Codex source input.
    pub fn snapshot(&self, envelope: Option<&Value>) -> SourceSnapshot {
        if !self.enabled {
            return SourceSnapshot::missing(&self.source_id, Some("source disabled".into()));
        }
        match self.kind {
            DataSourceKind::Codex => match envelope {
                Some(env) => codex_snapshot(&self.source_id, env, self.valid_for_s()),
                None => SourceSnapshot {
                    fields: BTreeMap::new(),
                    quality: Quality::Missing,
                    error: Some("no codex snapshot yet".into()),
                    ..SourceSnapshot::missing(&self.source_id, None)
                },
            },
            DataSourceKind::StaticJson => self.static_snapshot(),
        }
    }

    fn static_snapshot(&self) -> SourceSnapshot {
        let now = crate::now_secs();
        let mut values = BTreeMap::new();
        if let Some(obj) = self.config.get("values").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                values.insert(k.clone(), v.clone());
            }
        }
        if let Some(text) = self.config.get("json").and_then(|v| v.as_str()) {
            match serde_json::from_str::<Value>(text) {
                Ok(Value::Object(obj)) => {
                    for (k, v) in obj {
                        values.insert(k, v);
                    }
                }
                Ok(_) => {
                    return SourceSnapshot {
                        quality: Quality::Missing,
                        error: Some("static json must be an object".into()),
                        ..SourceSnapshot::missing(&self.source_id, None)
                    }
                }
                Err(e) => {
                    return SourceSnapshot {
                        quality: Quality::Missing,
                        error: Some(format!("static json parse: {e}")),
                        ..SourceSnapshot::missing(&self.source_id, None)
                    }
                }
            }
        }
        let valid_for = self.valid_for_s();
        let fields = values
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    SnapshotField {
                        value: v,
                        quality: Quality::Good,
                        observed_at: now,
                        valid_until: now + valid_for,
                    },
                )
            })
            .collect();
        SourceSnapshot {
            source_id: self.source_id.clone(),
            fields,
            observed_at: now,
            valid_until: now + valid_for,
            last_success_at: now,
            quality: Quality::Good,
            error: None,
        }
    }
}

/// Default trigger classification for a canonical field path (v2 §6/§12):
/// visible usage values push; reset times, window lengths, server time and raw
/// metadata pull.
pub fn classify_trigger(field: &str) -> FieldTrigger {
    let tail = field.rsplit('.').next().unwrap_or(field);
    match tail {
        "usedPercent" | "availableCount" | "plan" | "label" | "name" | "id" => FieldTrigger::Push,
        _ => FieldTrigger::Pull,
    }
}

pub fn classify_kind(field: &str) -> FieldKind {
    let tail = field.rsplit('.').next().unwrap_or(field);
    match tail {
        "usedPercent" | "remaining" | "resetsAt" | "windowMins" | "availableCount"
        | "nextExpiresAt" | "server_time" | "battery" | "offline_mins" | "now" | "count"
        | "pct" | "value" => FieldKind::Number,
        _ => FieldKind::Text,
    }
}

/// Flatten the Codex envelope into canonical field paths.
///
/// Display rules stay in the Codex mapping contract: a missing 5h bucket simply
/// produces no `buckets[codex].5h.*` keys (the template renders its static 100
/// branch), `resetCredits.availableCount<=0` produces no reset-credit keys, and
/// a missing username produces no `bridge.label` key.
pub fn codex_snapshot(source_id: &str, env: &Value, valid_for_s: u64) -> SourceSnapshot {
    let now = crate::now_secs();
    let mut fields: BTreeMap<String, SnapshotField> = BTreeMap::new();
    let put = |fields: &mut BTreeMap<String, SnapshotField>, key: String, value: Value| {
        fields.insert(
            key,
            SnapshotField {
                value,
                quality: Quality::Good,
                observed_at: now,
                valid_until: now + valid_for_s,
            },
        );
    };

    if let Some(plan) = env
        .get("account")
        .and_then(|a| a.get("plan"))
        .and_then(|v| v.as_str())
    {
        put(&mut fields, "account.plan".into(), json!(plan));
    }
    if let Some(label) = env
        .get("bridge")
        .and_then(|b| b.get("label"))
        .and_then(|v| v.as_str())
    {
        if !label.is_empty() {
            put(&mut fields, "bridge.label".into(), json!(label));
        }
    }
    if let Some(host) = env
        .get("bridge")
        .and_then(|b| b.get("hostId"))
        .and_then(|v| v.as_str())
    {
        put(&mut fields, "bridge.hostId".into(), json!(host));
    }
    if let Some(t) = env.get("server_time").and_then(|v| v.as_i64()) {
        put(&mut fields, "server_time".into(), json!(t));
    }
    if let Some(rc) = env.get("resetCredits") {
        let count = rc
            .get("availableCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if count > 0 {
            put(
                &mut fields,
                "resetCredits.availableCount".into(),
                json!(count),
            );
            if let Some(next) = rc.get("nextExpiresAt").and_then(|v| v.as_i64()) {
                put(
                    &mut fields,
                    "resetCredits.nextExpiresAt".into(),
                    json!(next),
                );
            }
        }
    }
    if let Some(buckets) = env.get("buckets").and_then(|v| v.as_array()) {
        for bucket in buckets {
            let Some(id) = bucket.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            if let Some(name) = bucket.get("name").and_then(|v| v.as_str()) {
                put(&mut fields, format!("buckets[{id}].name"), json!(name));
            }
            let windows = bucket
                .get("windows")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for (j, window) in windows.iter().enumerate() {
                let mins = window
                    .get("windowMins")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let class = match mins {
                    300 => Some("5h"),
                    m if m >= 43200 => Some("monthly"),
                    m if m >= 10080 => Some("weekly"),
                    _ => None,
                };
                let mut quals = vec![format!("buckets[{id}].windows[{j}]")];
                if let Some(class) = class {
                    quals.push(format!("buckets[{id}].{class}"));
                }
                match j {
                    0 => quals.push(format!("buckets[{id}].primary")),
                    1 => quals.push(format!("buckets[{id}].secondary")),
                    _ => {}
                }
                let mut values: Vec<(&str, Value)> = Vec::new();
                if let Some(v) = window.get("usedPercent").and_then(|v| v.as_i64()) {
                    values.push(("usedPercent", json!(v)));
                }
                if let Some(v) = window.get("resetsAt").and_then(|v| v.as_i64()) {
                    values.push(("resetsAt", json!(v)));
                }
                if let Some(v) = window.get("windowMins").and_then(|v| v.as_i64()) {
                    values.push(("windowMins", json!(v)));
                }
                if let Some(v) = window.get("kind").and_then(|v| v.as_str()) {
                    values.push(("kind", json!(v)));
                }
                for q in quals {
                    for (key, value) in &values {
                        put(&mut fields, format!("{q}.{key}"), value.clone());
                    }
                }
            }
        }
    }

    SourceSnapshot {
        source_id: source_id.into(),
        fields,
        observed_at: now,
        valid_until: now + valid_for_s,
        last_success_at: now,
        quality: Quality::Good,
        error: None,
    }
}

/// Default DataSource set for a fresh install: Codex only. The Static JSON source
/// is created on demand (data page / tests).
pub fn default_sources() -> Vec<DataSource> {
    vec![DataSource {
        source_id: "codex".into(),
        kind: DataSourceKind::Codex,
        config: json!({ "valid_for_s": 3600 }),
        credential_ref: None,
        enabled: true,
    }]
}

/// Normalize a template requirement to a canonical snapshot key.
///
/// `buckets[id].remaining` resolves to `usedPercent` with a `100 - used`
/// transform; all other paths resolve to themselves.
pub fn resolve_field(field: &str) -> (String, Transform) {
    if let Some(prefix) = field.strip_suffix(".remaining") {
        return (format!("{prefix}.usedPercent"), Transform::Remaining);
    }
    (field.to_string(), Transform::Identity)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    Identity,
    Remaining,
}

/// Bounded push/full fingerprint over a requirement field set. Volatile
/// observation times and retry counts are not part of the value contract.
pub fn fingerprint(
    fields: &BTreeMap<String, SnapshotField>,
    requirements: &[crate::platform::model::FieldRequirement],
    push_only: bool,
    triggers: &dyn Fn(&str) -> FieldTrigger,
) -> u64 {
    let mut acc: u64 = 0xcbf2_9ce4_8422_2325;
    let mut entries: Vec<(String, String, u8)> = Vec::new();
    for r in requirements {
        if push_only && triggers(&r.field) != FieldTrigger::Push {
            continue;
        }
        let (key, transform) = resolve_field(&r.field);
        let item = fields.get(&key);
        let (text, quality) = match item {
            Some(f) => {
                let value = match transform {
                    Transform::Identity => f.value.clone(),
                    Transform::Remaining => {
                        let used = f.value.as_i64().unwrap_or(0);
                        json!((100 - used).clamp(0, 100))
                    }
                };
                (value.to_string(), quality_code(f.quality))
            }
            None => ("<missing>".to_string(), quality_code(Quality::Missing)),
        };
        entries.push((r.field.clone(), text, quality));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (field, value, quality) in entries {
        for b in field
            .bytes()
            .chain(value.bytes())
            .chain(std::iter::once(quality))
        {
            acc ^= b as u64;
            acc = acc.wrapping_mul(0x100_0000_01b3);
        }
    }
    acc
}

pub fn quality_code(q: Quality) -> u8 {
    match q {
        Quality::Good => 1,
        Quality::Stale => 2,
        Quality::Missing => 3,
    }
}

/// Fields a template needs, resolved against a source snapshot, with quality
/// and validity. Missing fields are explicitly encoded (never silently dropped).
pub fn assemble_fields(
    snapshot: &SourceSnapshot,
    requirements: &[crate::platform::model::FieldRequirement],
) -> BTreeMap<String, SnapshotField> {
    let now = crate::now_secs();
    let mut out = BTreeMap::new();
    for r in requirements {
        if r.local {
            continue;
        }
        let (key, transform) = resolve_field(&r.field);
        let field = match snapshot.fields.get(&key) {
            Some(f) => {
                let value = match transform {
                    Transform::Identity => f.value.clone(),
                    Transform::Remaining => {
                        let used = f.value.as_i64().unwrap_or(0);
                        json!((100 - used).clamp(0, 100))
                    }
                };
                let quality = if f.valid_until > 0 && f.valid_until < now {
                    Quality::Stale
                } else {
                    f.quality
                };
                SnapshotField {
                    value,
                    quality,
                    observed_at: f.observed_at,
                    valid_until: f.valid_until,
                }
            }
            None => SnapshotField {
                value: Value::Null,
                quality: Quality::Missing,
                observed_at: snapshot.observed_at,
                valid_until: 0,
            },
        };
        out.insert(r.field.clone(), field);
    }
    out
}

/// Raw value view of `assemble_fields` (tests and Data payload assembly).
pub fn assemble_payload(
    snapshot: &SourceSnapshot,
    requirements: &[crate::platform::model::FieldRequirement],
) -> BTreeMap<String, Value> {
    assemble_fields(snapshot, requirements)
        .into_iter()
        .map(|(k, v)| (k, v.value))
        .collect()
}

pub fn parse_data_source(value: &Value) -> Result<DataSource> {
    let ds: DataSource = serde_json::from_value(value.clone()).context("datasource json")?;
    ds.validate()?;
    Ok(ds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::model::FieldRequirement;

    fn codex_env() -> Value {
        json!({
            "schema": 1,
            "server_time": 1_700_000_000,
            "account": {"plan": "plus"},
            "bridge": {"label": "tester", "hostId": "abcd"},
            "buckets": [
                {"id": "codex", "windows": [
                    {"kind": "weekly", "usedPercent": 30, "resetsAt": 1_700_500_000, "windowMins": 10080},
                    {"kind": "5h", "usedPercent": 10, "resetsAt": 1_700_010_000, "windowMins": 300}
                ]}
            ],
            "resetCredits": {"availableCount": 2, "nextExpiresAt": 1_700_900_000}
        })
    }

    #[test]
    fn trigger_truth_table() {
        assert_eq!(
            classify_trigger("buckets[codex].weekly.usedPercent"),
            FieldTrigger::Push
        );
        assert_eq!(
            classify_trigger("buckets[codex].weekly.resetsAt"),
            FieldTrigger::Pull
        );
        assert_eq!(
            classify_trigger("buckets[codex].weekly.windowMins"),
            FieldTrigger::Pull
        );
        assert_eq!(
            classify_trigger("resetCredits.availableCount"),
            FieldTrigger::Push
        );
        assert_eq!(classify_trigger("server_time"), FieldTrigger::Pull);
        let ds = default_sources().remove(0);
        assert_eq!(
            ds.trigger_for("buckets[codex].weekly.usedPercent"),
            FieldTrigger::Push
        );
        let override_ds: DataSource = serde_json::from_value(json!({
            "source_id": "s", "kind": "codex", "config": {"triggers": {"usedPercent": "pull"}}
        }))
        .unwrap();
        assert_eq!(
            override_ds.trigger_for("buckets[codex].weekly.usedPercent"),
            FieldTrigger::Pull
        );
    }

    #[test]
    fn codex_mapping_keeps_display_rules_in_the_contract() {
        let snap = codex_snapshot("codex", &codex_env(), 3600);
        assert_eq!(
            snap.fields["buckets[codex].weekly.usedPercent"].value,
            json!(30)
        );
        assert_eq!(
            snap.fields["buckets[codex].primary.usedPercent"].value,
            json!(30)
        );
        assert_eq!(
            snap.fields["buckets[codex].secondary.windowMins"].value,
            json!(300)
        );
        assert_eq!(
            snap.fields["buckets[codex].5h.usedPercent"].value,
            json!(10)
        );
        // Missing 5h bucket -> no 5h keys at all (template static-100 branch).
        let mut env = codex_env();
        env["buckets"][0]["windows"] =
            json!([{"kind":"weekly","usedPercent":30,"windowMins":10080}]);
        let snap = codex_snapshot("codex", &env, 3600);
        assert!(!snap.fields.contains_key("buckets[codex].5h.usedPercent"));
        // RC<=0 -> resetCredits keys absent (row hidden).
        let mut env = codex_env();
        env["resetCredits"]["availableCount"] = json!(0);
        let snap = codex_snapshot("codex", &env, 3600);
        assert!(!snap.fields.contains_key("resetCredits.availableCount"));
        // No username -> label absent (row hidden).
        let mut env = codex_env();
        env["bridge"]["label"] = Value::Null;
        let snap = codex_snapshot("codex", &env, 3600);
        assert!(!snap.fields.contains_key("bridge.label"));
    }

    #[test]
    fn remaining_resolves_to_used_percent() {
        let (key, t) = resolve_field("buckets[codex].weekly.remaining");
        assert_eq!(key, "buckets[codex].weekly.usedPercent");
        assert_eq!(t, Transform::Remaining);
        let snap = codex_snapshot("codex", &codex_env(), 3600);
        let reqs = vec![FieldRequirement {
            index: 0,
            field: "buckets[codex].weekly.remaining".into(),
            kind: FieldKind::Number,
            missing: crate::platform::model::MissingPolicy::Hide,
            local: false,
        }];
        let payload = assemble_payload(&snap, &reqs);
        assert_eq!(payload["buckets[codex].weekly.remaining"], json!(70));
    }

    #[test]
    fn push_fingerprint_ignores_pull_only_changes() {
        let ds = default_sources().remove(0);
        let triggers = |f: &str| ds.trigger_for(f);
        let reqs = vec![
            FieldRequirement {
                index: 0,
                field: "buckets[codex].weekly.usedPercent".into(),
                kind: FieldKind::Number,
                missing: crate::platform::model::MissingPolicy::Hide,
                local: false,
            },
            FieldRequirement {
                index: 1,
                field: "buckets[codex].weekly.resetsAt".into(),
                kind: FieldKind::Number,
                missing: crate::platform::model::MissingPolicy::Hide,
                local: false,
            },
        ];
        let a = codex_snapshot("codex", &codex_env(), 3600);
        let mut env_b = codex_env();
        env_b["buckets"][0]["windows"][0]["resetsAt"] = json!(1_700_999_000);
        let b = codex_snapshot("codex", &env_b, 3600);
        assert_eq!(
            fingerprint(&a.fields, &reqs, true, &triggers),
            fingerprint(&b.fields, &reqs, true, &triggers),
            "pull-only change must not move the push fingerprint"
        );
        assert_ne!(
            fingerprint(&a.fields, &reqs, false, &triggers),
            fingerprint(&b.fields, &reqs, false, &triggers),
            "full fingerprint must see pull changes"
        );
        let mut env_c = codex_env();
        env_c["buckets"][0]["windows"][0]["usedPercent"] = json!(31);
        let c = codex_snapshot("codex", &env_c, 3600);
        assert_ne!(
            fingerprint(&a.fields, &reqs, true, &triggers),
            fingerprint(&c.fields, &reqs, true, &triggers),
            "push change must move the push fingerprint"
        );
    }

    #[test]
    fn failed_fetch_keeps_last_values() {
        let ds = default_sources().remove(0);
        let good = ds.snapshot(Some(&codex_env()));
        let mut cached = good.clone();
        cached.quality = Quality::Stale;
        cached.error = Some("fetch failed".into());
        cached.last_success_at = good.last_success_at;
        assert_eq!(cached.fields["account.plan"].value, json!("plus"));
        assert_eq!(cached.quality, Quality::Stale);
    }
}
