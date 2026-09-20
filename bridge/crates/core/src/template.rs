//! Template library: canonical bytes, hash, validation (device-engine parity),
//! and the BLE chunk encoding used to push templates to the device.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::envelope::TemplateRef;

pub const FONTS: [&str; 5] = ["f8", "f12", "f16", "f20", "f24"];
pub const CANVAS: i64 = 200;
pub const MAX_TEMPLATE_BYTES: usize = 32768;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WinSel {
    Weekly,
    Monthly,
    FiveHour,
    Primary,
    Secondary,
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindField {
    UsedPercent,
    Remaining,
    ResetsAt,
    WindowMins,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindSpec {
    Plan,
    Label,
    HostId,
    ServerTime,
    ResetCount,
    ResetExpires,
    DeviceChannel,
    DeviceIp,
    DeviceSync,
    DeviceBattery,
    DeviceState,
    DeviceOfflineMins,
    DeviceNow,
    DeviceMode,
    Bucket {
        bucket: String,
        win: WinSel,
        field: BindField,
    },
}

/// Same grammar as the device engine (`template_engine.cpp`).
pub fn parse_bind(path: &str) -> Option<BindSpec> {
    match path {
        "account.plan" => return Some(BindSpec::Plan),
        "bridge.label" => return Some(BindSpec::Label),
        "bridge.hostId" => return Some(BindSpec::HostId),
        "server_time" => return Some(BindSpec::ServerTime),
        "resetCredits.availableCount" => return Some(BindSpec::ResetCount),
        "resetCredits.nextExpiresAt" => return Some(BindSpec::ResetExpires),
        "device.channel" => return Some(BindSpec::DeviceChannel),
        "device.ip" => return Some(BindSpec::DeviceIp),
        "device.sync_hhmm" => return Some(BindSpec::DeviceSync),
        "device.battery" => return Some(BindSpec::DeviceBattery),
        "device.state" => return Some(BindSpec::DeviceState),
        "device.offline_mins" => return Some(BindSpec::DeviceOfflineMins),
        "device.now" => return Some(BindSpec::DeviceNow),
        "device.mode" => return Some(BindSpec::DeviceMode),
        _ => {}
    }
    let rest = path.strip_prefix("buckets[")?;
    let close = rest.find(']')?;
    let bucket = &rest[..close];
    if bucket.is_empty() {
        return None;
    }
    let rest = rest[close + 1..].strip_prefix('.')?;
    let (win_tok, field_tok) = rest.split_once('.')?;
    let win = match win_tok {
        "weekly" => WinSel::Weekly,
        "monthly" => WinSel::Monthly,
        "5h" => WinSel::FiveHour,
        "primary" => WinSel::Primary,
        "secondary" => WinSel::Secondary,
        other => WinSel::Index(other.strip_prefix("windows[")?.strip_suffix(']')?.parse().ok()?),
    };
    let field = match field_tok {
        "usedPercent" => BindField::UsedPercent,
        "remaining" => BindField::Remaining,
        "resetsAt" => BindField::ResetsAt,
        "windowMins" => BindField::WindowMins,
        _ => return None,
    };
    Some(BindSpec::Bucket {
        bucket: bucket.to_string(),
        win,
        field,
    })
}

fn rect_ok(rect: Option<&Value>) -> bool {
    let Some(arr) = rect.and_then(|r| r.as_array()) else {
        return false;
    };
    if arr.len() < 4 {
        return false;
    }
    let x = arr[0].as_i64().unwrap_or(-1);
    let y = arr[1].as_i64().unwrap_or(-1);
    let mut w = arr[2].as_i64().unwrap_or(0);
    let mut h = arr[3].as_i64().unwrap_or(0);
    if w <= 0 || h <= 0 {
        return false;
    }
    let (mut x, mut y) = (x, y);
    if x < 0 {
        w += x;
        x = 0;
    }
    if y < 0 {
        h += y;
        y = 0;
    }
    if x >= CANVAS || y >= CANVAS {
        return false;
    }
    if x + w > CANVAS {
        w = CANVAS - x;
    }
    if y + h > CANVAS {
        h = CANVAS - y;
    }
    w > 0 && h > 0
}

fn text_region_ok(region: Option<&Value>) -> bool {
    let Some(arr) = region.and_then(|r| r.as_array()) else {
        return false;
    };
    if arr.len() != 4 {
        return false;
    }
    let x = arr[0].as_i64();
    let y = arr[1].as_i64();
    let w = arr[2].as_i64();
    let h = arr[3].as_i64();
    let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) else {
        return false;
    };
    x >= 0 && y >= 0 && x < CANVAS && y < CANVAS && w > 0 && h > 0
        && w <= CANVAS && h <= CANVAS && x <= CANVAS - w && y <= CANVAS - h
}

fn epoch_bind(bind: &str) -> bool {
    matches!(
        parse_bind(bind),
        Some(BindSpec::ServerTime)
            | Some(BindSpec::ResetExpires)
            | Some(BindSpec::Bucket {
                field: BindField::ResetsAt,
                ..
            })
    )
}

fn validate_condition(e: &Value) -> Result<(), String> {
    let Some(raw) = e.get("when") else {
        return Ok(());
    };
    let Some(obj) = raw.as_object() else {
        return Err("when".into());
    };
    if obj.len() != 2
        || obj
            .keys()
            .any(|key| key != "bind" && key != "exists" && key != "equals")
    {
        return Err("when".into());
    }
    let bind = obj.get("bind").and_then(|v| v.as_str()).ok_or("when bind")?;
    if parse_bind(bind).is_none() {
        return Err(format!("when bind {bind}"));
    }
    let has_exists = obj.contains_key("exists");
    let has_equals = obj.contains_key("equals");
    if has_exists == has_equals {
        return Err("when".into());
    }
    if has_exists {
        if obj.get("exists").and_then(|v| v.as_bool()).is_none() {
            return Err("when exists".into());
        }
    } else {
        match obj.get("equals") {
            Some(Value::String(_)) => {}
            Some(Value::Number(n)) if n.is_i64() || n.is_u64() => {}
            _ => return Err("when equals".into()),
        }
    }
    Ok(())
}

fn validate_element(e: &Value) -> Result<(), String> {
    validate_condition(e)?;
    let ty = e.get("type").and_then(|v| v.as_str()).ok_or("type")?;
    match ty {
        "text" => {
            let font = e.get("font").and_then(|v| v.as_str()).ok_or("font")?;
            if !FONTS.contains(&font) {
                return Err(format!("font {font}"));
            }
            let bind = e.get("bind").and_then(|v| v.as_str()).unwrap_or("");
            let text = e.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if bind.is_empty() && text.is_empty() {
                return Err("text empty".into());
            }
            if !bind.is_empty() && parse_bind(bind).is_none() {
                return Err(format!("bind {bind}"));
            }
            if let Some(scale) = e.get("scale") {
                let scale = scale.as_i64().ok_or("scale")?;
                if !(1..=3).contains(&scale) {
                    return Err("scale".into());
                }
            }
            if e.get("region").is_some() && !text_region_ok(e.get("region")) {
                return Err("region".into());
            }
            if let Some(align) = e.get("align") {
                if e.get("region").is_none() {
                    return Err("align".into());
                }
                match align.as_str() {
                    Some("left") | Some("center") | Some("right") => {}
                    _ => return Err("align".into()),
                }
            }
            if let Some(format) = e.get("time_format") {
                let format = format.as_str().ok_or("time_format")?;
                if !matches!(format, "date" | "hhmm")
                    || bind.is_empty()
                    || !epoch_bind(bind)
                {
                    return Err("time_format".into());
                }
            }
        }
        "bar" => {
            let bind = e.get("bind").and_then(|v| v.as_str()).ok_or("bind")?;
            if parse_bind(bind).is_none() {
                return Err(format!("bind {bind}"));
            }
            if !rect_ok(e.get("rect")) {
                return Err("rect".into());
            }
        }
        "rect" => {
            if !rect_ok(e.get("rect")) {
                return Err("rect".into());
            }
        }
        "line" => {
            for key in ["x1", "y1", "x2", "y2"] {
                if e.get(key).and_then(|v| v.as_i64()).is_none() {
                    return Err(format!("line {key}"));
                }
            }
        }
        "icon" => {
            let w = e.get("w").and_then(|v| v.as_i64()).unwrap_or(0);
            let h = e.get("h").and_then(|v| v.as_i64()).unwrap_or(0);
            if w <= 0 || h <= 0 {
                return Err("icon size".into());
            }
            if e.get("bits")
                .and_then(|v| v.as_str())
                .map(|s| s.is_empty())
                .unwrap_or(true)
            {
                return Err("icon bits".into());
            }
        }
        other => return Err(format!("type {other}")),
    }
    Ok(())
}

/// Structural validation identical to the device engine's dry run.
pub fn validate_template(v: &Value) -> Result<(), String> {
    if v.get("schema").and_then(|x| x.as_i64()) != Some(1) {
        return Err("schema".into());
    }
    let canvas = v.get("canvas").ok_or("canvas")?;
    if canvas.get("w").and_then(|x| x.as_i64()) != Some(CANVAS)
        || canvas.get("h").and_then(|x| x.as_i64()) != Some(CANVAS)
    {
        return Err("canvas size".into());
    }
    let els = v
        .get("elements")
        .and_then(|x| x.as_array())
        .ok_or("elements")?;
    if els.is_empty() {
        return Err("elements empty".into());
    }
    for e in els {
        validate_element(e)?;
    }
    Ok(())
}

/// Canonical JSON (sorted keys, no whitespace) — must stay byte-compatible
/// with the Python test bridge (`json.dumps(sort_keys=True, separators=(",", ":"))`).
pub fn canonical_bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).expect("serialize template")
}

pub fn template_hash(bytes: &[u8]) -> String {
    format!("{:08x}", crc32fast::hash(bytes))
}

/// Device BLE chunk format: 2-byte little-endian offset + payload.
pub fn encode_chunks(bytes: &[u8], payload: usize) -> Vec<Vec<u8>> {
    assert!(payload > 0 && payload <= u16::MAX as usize - 2);
    let mut out = Vec::new();
    let mut off = 0usize;
    while off < bytes.len() {
        let n = payload.min(bytes.len() - off);
        let mut chunk = Vec::with_capacity(2 + n);
        chunk.push((off & 0xff) as u8);
        chunk.push(((off >> 8) & 0xff) as u8);
        chunk.extend_from_slice(&bytes[off..off + n]);
        out.push(chunk);
        off += n;
    }
    out
}

#[derive(Debug, Clone)]
pub struct TemplateEntry {
    pub id: String,
    pub version: u64,
    pub min_fw: Option<String>,
    pub bytes: Vec<u8>,
    pub hash: String,
    pub json: Value,
}

#[derive(Debug, Default)]
pub struct Library {
    pub dir: PathBuf,
    pub entries: BTreeMap<String, TemplateEntry>,
}

impl Library {
    pub fn load(dir: &Path) -> Result<Self> {
        let mut entries = BTreeMap::new();
        if dir.exists() {
            for ent in std::fs::read_dir(dir)? {
                let ent = ent?;
                let path = ent.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("read {}", path.display()))?;
                let json: Value = serde_json::from_str(&text)
                    .with_context(|| format!("parse {}", path.display()))?;
                if let Err(e) = validate_template(&json) {
                    tracing::warn!("skip {}: {e}", path.display());
                    continue;
                }
                let id = json
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if id.is_empty() {
                    tracing::warn!("skip {}: missing id", path.display());
                    continue;
                }
                let version = json.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
                let min_fw = json
                    .get("min_fw")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let bytes = canonical_bytes(&json);
                if bytes.len() > MAX_TEMPLATE_BYTES {
                    tracing::warn!("skip {id}: too large");
                    continue;
                }
                let hash = template_hash(&bytes);
                entries.insert(
                    id.clone(),
                    TemplateEntry {
                        id,
                        version,
                        min_fw,
                        bytes,
                        hash,
                        json,
                    },
                );
            }
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            entries,
        })
    }

    pub fn get(&self, id: &str) -> Option<&TemplateEntry> {
        self.entries.get(id)
    }

    pub fn ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn template_refs(&self) -> BTreeMap<String, TemplateRef> {
        self.entries
            .iter()
            .map(|(id, e)| {
                (
                    id.clone(),
                    TemplateRef {
                        version: e.version,
                        hash: e.hash.clone(),
                    },
                )
            })
            .collect()
    }
}
