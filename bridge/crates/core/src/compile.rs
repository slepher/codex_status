//! CompiledTemplate (v2 §8): field requirements and the render plan in one
//! bounded, pointer-free artifact. Compilation happens at save/install time;
//! normal wake/data/switch/render paths load only the active compiled record and
//! never parse template JSON.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::template::{parse_bind, BindSpec, CANVAS, FONTS};

pub const COMPILER_ABI: u32 = 1;
pub const MAX_RENDER_OPS: usize = 256;
pub const MAX_REQUIREMENTS: usize = 64;

/// Reference from a render op to a compiled requirement slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldRef {
    pub index: u32,
    pub field: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub index: u32,
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RenderOp {
    Text(TextOp),
    Bar(BarOp),
    Rect(RectOp),
    Line(LineOp),
    Icon(IconOp),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextOp {
    pub font: String,
    pub x: i32,
    pub y: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind: Option<FieldRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(default = "one")]
    pub scale: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<[i32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_format: Option<String>,
    #[serde(default = "black")]
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
}

fn one() -> u32 {
    1
}

fn black() -> String {
    "black".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarOp {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub bind: FieldRef,
    #[serde(default = "black")]
    pub fg: String,
    #[serde(default)]
    pub bg: String,
    #[serde(default)]
    pub border: String,
    #[serde(default = "hundred")]
    pub max: i64,
}

fn hundred() -> i64 {
    100
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RectOp {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(default = "black")]
    pub color: String,
    #[serde(default)]
    pub fill: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineOp {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    #[serde(default = "black")]
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IconOp {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Resource id embedded in the Bundle (`Bundle.resources`).
    pub resource: String,
    #[serde(default = "black")]
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
}

/// One compiled template: requirements + render plan + local deps + resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledTemplate {
    pub template_id: String,
    pub render_target: String,
    pub compiler_abi: u32,
    pub version: u64,
    pub min_fw: Option<String>,
    pub source_crc: String,
    pub canvas_w: u32,
    pub canvas_h: u32,
    pub requirements: Vec<crate::platform::model::FieldRequirement>,
    pub render_ops: Vec<RenderOp>,
    /// Field names injected by the device (clock, battery, channel, …).
    pub local_dependencies: Vec<String>,
    /// Embedded resources required by the render plan (id + payload).
    pub resources: Vec<CompiledResource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledResource {
    pub id: String,
    pub data: String,
}

/// Canonical, bounded serialization: no native pointers, fixed meaning.
pub fn encode(t: &CompiledTemplate) -> Vec<u8> {
    crate::template::canonical_bytes(&serde_json::to_value(t).expect("compiled template"))
}

pub fn decode(bytes: &[u8]) -> Result<CompiledTemplate> {
    let t: CompiledTemplate =
        serde_json::from_slice(bytes).context("compiled template: invalid encoding")?;
    validate(&t)?;
    Ok(t)
}

/// Revalidate every bound on load (device does the same, independently).
pub fn validate(t: &CompiledTemplate) -> Result<()> {
    if t.compiler_abi != COMPILER_ABI {
        bail!(
            "compiled template {}: ABI {} unsupported (expected {COMPILER_ABI})",
            t.template_id,
            t.compiler_abi
        );
    }
    if t.template_id.is_empty() || t.render_target.is_empty() {
        bail!("compiled template: empty identity");
    }
    if t.canvas_w != CANVAS as u32 || t.canvas_h != CANVAS as u32 {
        bail!(
            "compiled template {}: canvas {}x{} unsupported",
            t.template_id,
            t.canvas_w,
            t.canvas_h
        );
    }
    if t.requirements.len() > MAX_REQUIREMENTS {
        bail!("compiled template: too many requirements");
    }
    if t.render_ops.len() > MAX_RENDER_OPS {
        bail!("compiled template: too many render ops");
    }
    let max_index = t.requirements.len() as u32;
    for (i, r) in t.requirements.iter().enumerate() {
        if r.index != i as u32 {
            bail!(
                "compiled template: requirement index {} is not dense",
                r.index
            );
        }
        if r.field.is_empty() {
            bail!("compiled template: empty requirement field");
        }
    }
    let check_ref = |f: &FieldRef| -> Result<()> {
        if f.index >= max_index {
            bail!(
                "compiled template: field ref {} index {} out of bounds",
                f.field,
                f.index
            );
        }
        if t.requirements[f.index as usize].field != f.field {
            bail!(
                "compiled template: field ref {} does not match slot {}",
                f.field,
                f.index
            );
        }
        Ok(())
    };
    for op in &t.render_ops {
        let when = match op {
            RenderOp::Text(o) => {
                if !FONTS.contains(&o.font.as_str()) {
                    bail!("compiled template: font {} unsupported", o.font);
                }
                if o.bind.is_none() && o.text.as_deref().unwrap_or_default().is_empty() {
                    bail!("compiled template: text op without bind or text");
                }
                if let Some(r) = &o.bind {
                    check_ref(r)?;
                }
                o.when.as_ref()
            }
            RenderOp::Bar(o) => {
                check_ref(&o.bind)?;
                if o.w <= 0 || o.h <= 0 {
                    bail!("compiled template: bar with empty rect");
                }
                None
            }
            RenderOp::Rect(o) => {
                if o.w <= 0 || o.h <= 0 {
                    bail!("compiled template: rect with empty rect");
                }
                o.when.as_ref()
            }
            RenderOp::Line(_) => None,
            RenderOp::Icon(o) => {
                if o.w <= 0 || o.h <= 0 || !t.resources.iter().any(|r| r.id == o.resource) {
                    bail!("compiled template: icon resource {} missing", o.resource);
                }
                o.when.as_ref()
            }
        };
        if let Some(c) = when {
            if c.index >= max_index {
                bail!(
                    "compiled template: condition index {} out of bounds",
                    c.index
                );
            }
            if t.requirements[c.index as usize].field != c.field {
                bail!("compiled template: condition field mismatch");
            }
            if c.exists.is_none() && c.equals.is_none() {
                bail!("compiled template: condition without exists/equals");
            }
        }
    }
    for dep in &t.local_dependencies {
        if !t.requirements.iter().any(|r| &r.field == dep) {
            bail!("compiled template: local dependency {dep} not in requirements");
        }
    }
    Ok(())
}

/// Stable, bounded hash of the compiled artifact (used in Bundle/profile diffs).
pub fn compiled_crc(t: &CompiledTemplate) -> String {
    format!("{:08x}", crc32f(&encode(t)))
}

fn crc32f(bytes: &[u8]) -> u32 {
    crc32fast::hash(bytes)
}

struct Compiler {
    render_target: String,
    requirements: Vec<crate::platform::model::FieldRequirement>,
    local: std::collections::BTreeSet<String>,
    resources: BTreeMap<String, String>,
    id_index: BTreeMap<String, u32>,
}

impl Compiler {
    fn requirement(&mut self, field: &str) -> Result<FieldRef> {
        if let Some(index) = self.id_index.get(field) {
            return Ok(FieldRef {
                index: *index,
                field: field.to_string(),
            });
        }
        let index = self.requirements.len() as u32;
        if index as usize >= MAX_REQUIREMENTS {
            bail!("template uses more than {MAX_REQUIREMENTS} fields");
        }
        let spec = parse_bind(field).with_context(|| format!("unknown bind {field}"))?;
        let local = matches!(
            spec,
            BindSpec::DeviceChannel
                | BindSpec::DeviceIp
                | BindSpec::DeviceSync
                | BindSpec::DeviceBattery
                | BindSpec::DeviceState
                | BindSpec::DeviceOfflineMins
                | BindSpec::DeviceNow
                | BindSpec::DeviceMode
        );
        if local {
            self.local.insert(field.to_string());
        }
        self.id_index.insert(field.to_string(), index);
        self.requirements
            .push(crate::platform::model::FieldRequirement {
                index,
                field: field.to_string(),
                kind: field_kind(&spec),
                missing: crate::platform::model::MissingPolicy::Hide,
                local,
            });
        Ok(FieldRef {
            index,
            field: field.to_string(),
        })
    }

    fn condition(&mut self, raw: &Value) -> Result<Option<Condition>> {
        let Some(obj) = raw.as_object() else {
            bail!("when must be an object");
        };
        if obj.len() != 2
            || obj
                .keys()
                .any(|k| k != "bind" && k != "exists" && k != "equals")
        {
            bail!("when must contain exactly bind + exists|equals");
        }
        let field = obj
            .get("bind")
            .and_then(|v| v.as_str())
            .context("when bind")?;
        let r = self.requirement(field)?;
        if let Some(v) = obj.get("exists") {
            let Some(b) = v.as_bool() else {
                bail!("when exists must be boolean");
            };
            return Ok(Some(Condition {
                index: r.index,
                field: r.field,
                exists: Some(b),
                equals: None,
            }));
        }
        let equals = obj.get("equals").context("when equals")?;
        match equals {
            Value::String(_) => {}
            Value::Number(n) if n.is_i64() || n.is_u64() => {}
            _ => bail!("when equals must be a string or integer"),
        }
        Ok(Some(Condition {
            index: r.index,
            field: r.field,
            exists: None,
            equals: Some(equals.clone()),
        }))
    }
}

fn field_kind(spec: &BindSpec) -> crate::platform::model::FieldKind {
    use crate::platform::model::FieldKind;
    match spec {
        BindSpec::Plan
        | BindSpec::Label
        | BindSpec::HostId
        | BindSpec::DeviceIp
        | BindSpec::DeviceSync
        | BindSpec::DeviceState
        | BindSpec::DeviceNow
        | BindSpec::DeviceMode
        | BindSpec::DeviceChannel => FieldKind::Text,
        _ => FieldKind::Number,
    }
}

fn rect_of(v: &Value) -> Result<[i32; 4]> {
    let a = v.as_array().context("rect")?;
    if a.len() < 4 {
        bail!("rect needs 4 numbers");
    }
    let mut r = [0i32; 4];
    for (i, slot) in r.iter_mut().enumerate() {
        *slot = a[i].as_i64().unwrap_or(0) as i32;
    }
    // clamp like the engine (clip to canvas, drop fully outside)
    let (mut x, mut y, mut w, mut h) = (r[0], r[1], r[2], r[3]);
    if x < 0 {
        w += x;
        x = 0;
    }
    if y < 0 {
        h += y;
        y = 0;
    }
    if x >= CANVAS as i32 || y >= CANVAS as i32 {
        bail!("rect fully outside canvas");
    }
    if x + w > CANVAS as i32 {
        w = CANVAS as i32 - x;
    }
    if y + h > CANVAS as i32 {
        h = CANVAS as i32 - y;
    }
    if w <= 0 || h <= 0 {
        bail!("rect has no area");
    }
    Ok([x, y, w, h])
}

fn resource_id(bits: &str) -> String {
    format!("r{:08x}", crc32f(bits.as_bytes()))
}

/// Compile a validated template source for one render target.
pub fn compile(source: &Value, render_target: &str) -> Result<CompiledTemplate> {
    crate::template::validate_template(source).map_err(|e| anyhow::anyhow!("template: {e}"))?;
    let template_id = source
        .get("id")
        .and_then(|v| v.as_str())
        .context("template id")?
        .to_string();
    let version = source.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    let min_fw = source
        .get("min_fw")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let mut c = Compiler {
        render_target: render_target.to_string(),
        requirements: Vec::new(),
        local: Default::default(),
        resources: Default::default(),
        id_index: BTreeMap::new(),
    };

    let elements = source
        .get("elements")
        .and_then(|v| v.as_array())
        .context("elements")?;
    let mut ops = Vec::with_capacity(elements.len());
    for e in elements {
        let ty = e
            .get("type")
            .and_then(|v| v.as_str())
            .context("element type")?;
        let when = match e.get("when") {
            Some(v) => c.condition(v)?,
            None => None,
        };
        match ty {
            "text" => {
                let font = e.get("font").and_then(|v| v.as_str()).context("font")?;
                if !FONTS.contains(&font) {
                    bail!("unknown font {font}");
                }
                let bind = e.get("bind").and_then(|v| v.as_str());
                let bind_ref = match bind {
                    Some(b) if !b.is_empty() => Some(c.requirement(b)?),
                    _ => None,
                };
                let region = match e.get("region") {
                    Some(v) => Some(rect_of(v)?),
                    None => None,
                };
                let scale = e.get("scale").and_then(|v| v.as_u64()).unwrap_or(1);
                if !(1..=3).contains(&scale) {
                    bail!("text scale out of range");
                }
                ops.push(RenderOp::Text(TextOp {
                    font: font.into(),
                    x: e.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    y: e.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    bind: bind_ref,
                    text: e.get("text").and_then(|v| v.as_str()).map(str::to_string),
                    prefix: e.get("prefix").and_then(|v| v.as_str()).map(str::to_string),
                    suffix: e.get("suffix").and_then(|v| v.as_str()).map(str::to_string),
                    scale: scale as u32,
                    region,
                    align: e.get("align").and_then(|v| v.as_str()).map(str::to_string),
                    time_format: e
                        .get("time_format")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    color: e
                        .get("color")
                        .and_then(|v| v.as_str())
                        .unwrap_or("black")
                        .to_string(),
                    bg: e.get("bg").and_then(|v| v.as_str()).map(str::to_string),
                    when,
                }));
            }
            "bar" => {
                let bind = e.get("bind").and_then(|v| v.as_str()).context("bar bind")?;
                let r = c.requirement(bind)?;
                let rect = rect_of(e.get("rect").context("bar rect")?)?;
                ops.push(RenderOp::Bar(BarOp {
                    x: rect[0],
                    y: rect[1],
                    w: rect[2],
                    h: rect[3],
                    bind: r,
                    fg: e
                        .get("fg")
                        .and_then(|v| v.as_str())
                        .unwrap_or("black")
                        .into(),
                    bg: e.get("bg").and_then(|v| v.as_str()).unwrap_or("").into(),
                    border: e
                        .get("border")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    max: e.get("max").and_then(|v| v.as_i64()).unwrap_or(100),
                }));
            }
            "rect" => {
                let rect = rect_of(e.get("rect").context("rect rect")?)?;
                ops.push(RenderOp::Rect(RectOp {
                    x: rect[0],
                    y: rect[1],
                    w: rect[2],
                    h: rect[3],
                    color: e
                        .get("color")
                        .and_then(|v| v.as_str())
                        .unwrap_or("black")
                        .into(),
                    fill: e.get("fill").and_then(|v| v.as_bool()).unwrap_or(false),
                    when,
                }));
            }
            "line" => {
                let num = |k: &'static str| -> Result<i32> {
                    Ok(e.get(k).and_then(|v| v.as_i64()).context(k)? as i32)
                };
                ops.push(RenderOp::Line(LineOp {
                    x1: num("x1")?,
                    y1: num("y1")?,
                    x2: num("x2")?,
                    y2: num("y2")?,
                    color: e
                        .get("color")
                        .and_then(|v| v.as_str())
                        .unwrap_or("black")
                        .into(),
                    when: None,
                }));
            }
            "icon" => {
                let bits = e
                    .get("bits")
                    .and_then(|v| v.as_str())
                    .context("icon bits")?;
                let w = e.get("w").and_then(|v| v.as_i64()).context("icon w")? as i32;
                let h = e.get("h").and_then(|v| v.as_i64()).context("icon h")? as i32;
                if w <= 0 || h <= 0 {
                    bail!("icon size");
                }
                let resource = resource_id(bits);
                c.resources.insert(resource.clone(), bits.to_string());
                ops.push(RenderOp::Icon(IconOp {
                    x: e.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    y: e.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    w,
                    h,
                    resource,
                    color: e
                        .get("color")
                        .and_then(|v| v.as_str())
                        .unwrap_or("black")
                        .into(),
                    when,
                }));
            }
            other => bail!("unknown element type {other}"),
        }
        if ops.len() > MAX_RENDER_OPS {
            bail!("template has more than {MAX_RENDER_OPS} elements");
        }
    }

    let compiled = CompiledTemplate {
        template_id,
        render_target: c.render_target.clone(),
        compiler_abi: COMPILER_ABI,
        version,
        min_fw,
        source_crc: crate::template::template_hash(&crate::template::canonical_bytes(source)),
        canvas_w: CANVAS as u32,
        canvas_h: CANVAS as u32,
        requirements: c.requirements,
        render_ops: ops,
        local_dependencies: c.local.into_iter().collect(),
        resources: c
            .resources
            .into_iter()
            .map(|(id, data)| CompiledResource { id, data })
            .collect(),
    };
    validate(&compiled)?;
    Ok(compiled)
}

/// Requirements for a profile: union over its compiled templates, deduped, with
/// the active template marked for runtime delivery.
pub fn merge_requirements<'a>(
    templates: impl Iterator<Item = &'a CompiledTemplate>,
) -> Vec<crate::platform::model::FieldRequirement> {
    let mut out: Vec<crate::platform::model::FieldRequirement> = Vec::new();
    for t in templates {
        for r in &t.requirements {
            if !out.iter().any(|o| o.field == r.field) {
                out.push(r.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn quad() -> Value {
        serde_json::from_str(include_str!(
            "../../../../tools/test-bridge/templates/quad.json"
        ))
        .unwrap()
    }

    #[test]
    fn compiles_quad_with_requirements_and_resources() {
        let c = compile(&quad(), crate::platform::model::RENDER_TARGET_154G).unwrap();
        assert_eq!(c.template_id, "quad");
        assert_eq!(c.compiler_abi, COMPILER_ABI);
        assert!(c
            .requirements
            .iter()
            .any(|r| r.field == "buckets[codex].weekly.remaining"));
        assert!(c
            .requirements
            .iter()
            .any(|r| r.field == "device.now" && r.local));
        assert!(!c.resources.is_empty());
        // Deterministic and round-trips without pointers.
        let bytes = encode(&c);
        let back = decode(&bytes).unwrap();
        assert_eq!(c, back);
        assert_eq!(compiled_crc(&c), compiled_crc(&back));
    }

    #[test]
    fn rejects_unknown_font_and_bind() {
        let mut t = quad();
        t["elements"][0] = json!({"type":"text","font":"f99","text":"x","x":1,"y":1});
        assert!(compile(&t, "x").is_err());
        let mut t = quad();
        t["elements"][0] =
            json!({"type":"text","font":"f12","bind":"buckets[nope].x.y","x":1,"y":1});
        assert!(compile(&t, "x").is_err());
    }

    #[test]
    fn abi_and_bounds_rejected_on_load() {
        let mut c = compile(&quad(), crate::platform::model::RENDER_TARGET_154G).unwrap();
        c.compiler_abi = 99;
        assert!(validate(&c).is_err());
        let mut c = compile(&quad(), crate::platform::model::RENDER_TARGET_154G).unwrap();
        if let Some(RenderOp::Text(op)) = c
            .render_ops
            .iter_mut()
            .find(|o| matches!(o, RenderOp::Text(t) if t.when.is_some()))
        {
            if let Some(w) = op.when.as_mut() {
                w.index = 999;
            }
        }
        assert!(validate(&c).is_err());
    }

    #[test]
    fn tampered_bytes_are_rejected() {
        let c = compile(&quad(), crate::platform::model::RENDER_TARGET_154G).unwrap();
        let mut bytes = encode(&c);
        let n = bytes.len();
        bytes[n / 2] ^= 0x01;
        assert!(decode(&bytes).is_err());
    }
}
