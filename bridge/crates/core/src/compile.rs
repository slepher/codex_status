//! One authoritative compiled format: CTP1 from the shared C++ engine.
//! Rust owns metadata and transport; it does not compile a second render plan.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::template::{parse_bind, BindSpec, CANVAS, NOTE4_WIDTH, NOTE4_HEIGHT};
use crate::platform::model::{FieldRequirement, MissingPolicy};

pub const COMPILER_ABI: u32 = 2;

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
    pub requirements: Vec<FieldRequirement>,
    /// Hex CTP1 bytes, directly consumed by tplCtDeserialize on the device.
    #[serde(default)]
    pub binary: String,
    #[serde(default)]
    pub op_count: u32,
    pub local_dependencies: Vec<String>,
    pub resources: Vec<CompiledResource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledResource { pub id: String, pub data: String }

#[derive(Deserialize)]
struct ArtifactMeta {
    template_id: String,
    compiler_abi: u32,
    source_crc: u32,
    op_count: u32,
    requirements: Vec<ArtifactRequirement>,
    resources: Vec<String>,
}
#[derive(Deserialize)]
struct ArtifactRequirement { field: String, local: bool }

pub fn binary_bytes(t: &CompiledTemplate) -> Result<Vec<u8>> {
    if t.binary.is_empty() || t.binary.len() % 2 != 0 || t.binary.len() > 128 * 1024 ||
        !t.binary.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("compiled template: invalid CTP1 encoding");
    }
    t.binary.as_bytes().chunks_exact(2).map(|s| {
        Ok(u8::from_str_radix(std::str::from_utf8(s)?, 16)?)
    }).collect()
}

fn requirements(meta: &ArtifactMeta) -> Result<Vec<FieldRequirement>> {
    meta.requirements.iter().enumerate().map(|(index, r)| {
        let spec = parse_bind(&r.field).context("compiled bind")?;
        Ok(FieldRequirement { index: index as u32, field: r.field.clone(),
            kind: field_kind(&spec), missing: MissingPolicy::Hide, local: r.local })
    }).collect()
}
fn resources(meta: &ArtifactMeta) -> Vec<CompiledResource> {
    meta.resources.iter().map(|data| CompiledResource {
        id: format!("r{:08x}", crc32fast::hash(data.as_bytes())), data: data.clone()
    }).collect()
}

pub fn validate(t: &CompiledTemplate) -> Result<()> {
    let expected = if t.render_target == crate::platform::model::RENDER_TARGET_NOTE4 {
        (NOTE4_WIDTH as u32, NOTE4_HEIGHT as u32)
    } else { (CANVAS as u32, CANVAS as u32) };
    if t.compiler_abi != COMPILER_ABI || t.render_target.is_empty() ||
        (t.canvas_w, t.canvas_h) != expected {
        bail!("compiled template: unsupported target/ABI/canvas");
    }
    let (_, meta) = bridge_render::compiled_artifact_with_canvas(None, &binary_bytes(t)?, expected.0, expected.1)?;
    let meta: ArtifactMeta = serde_json::from_str(&meta)?;
    let reqs = requirements(&meta)?;
    let local: Vec<String> = reqs.iter().filter(|r| r.local).map(|r| r.field.clone()).collect();
    if t.template_id != meta.template_id || t.compiler_abi != meta.compiler_abi ||
        t.source_crc != format!("{:08x}", meta.source_crc) || t.op_count != meta.op_count ||
        t.requirements != reqs || t.resources != resources(&meta) || t.local_dependencies != local {
        bail!("compiled template: metadata does not match CTP1");
    }
    Ok(())
}

/// Envelope serialization; its only render representation is the CTP1 binary.
pub fn encode(t: &CompiledTemplate) -> Vec<u8> {
    crate::template::canonical_bytes(&serde_json::to_value(t).expect("compiled template"))
}
pub fn decode(bytes: &[u8]) -> Result<CompiledTemplate> {
    let t = serde_json::from_slice(bytes).context("compiled template envelope")?;
    validate(&t)?;
    Ok(t)
}
pub fn compiled_crc(t: &CompiledTemplate) -> String {
    format!("{:08x}", crc32fast::hash(&encode(t)))
}

fn field_kind(spec: &BindSpec) -> crate::platform::model::FieldKind {
    use crate::platform::model::FieldKind;
    match spec {
        BindSpec::Plan | BindSpec::Label | BindSpec::HostId | BindSpec::DeviceIp |
        BindSpec::DeviceSync | BindSpec::DeviceState | BindSpec::DeviceNow | BindSpec::DeviceDate |
        BindSpec::DeviceMode | BindSpec::DeviceChannel => FieldKind::Text,
        _ => FieldKind::Number,
    }
}

pub fn compile(source: &Value, render_target: &str) -> Result<CompiledTemplate> {
    crate::template::validate_template(source).map_err(|e| anyhow::anyhow!("template: {e}"))?;
    let (width, height) = if render_target == crate::platform::model::RENDER_TARGET_NOTE4 {
        (NOTE4_WIDTH as u32, NOTE4_HEIGHT as u32)
    } else { (CANVAS as u32, CANVAS as u32) };
    if source["canvas"]["w"].as_u64() != Some(width as u64)
        || source["canvas"]["h"].as_u64() != Some(height as u64) {
        bail!("template canvas does not match render target {render_target}");
    }
    let text = String::from_utf8(crate::template::canonical_bytes(source))?;
    let (blob, meta) = bridge_render::compiled_artifact_with_canvas(Some(&text), &[], width, height)?;
    let meta: ArtifactMeta = serde_json::from_str(&meta)?;
    let reqs = requirements(&meta)?;
    let compiled = CompiledTemplate {
        template_id: meta.template_id.clone(), render_target: render_target.into(),
        compiler_abi: meta.compiler_abi, version: source["version"].as_u64().unwrap_or(0),
        min_fw: source["min_fw"].as_str().map(str::to_owned),
        source_crc: format!("{:08x}", meta.source_crc), canvas_w: width, canvas_h: height,
        local_dependencies: reqs.iter().filter(|r| r.local).map(|r| r.field.clone()).collect(),
        requirements: reqs, resources: resources(&meta), op_count: meta.op_count,
        binary: blob.iter().map(|b| format!("{b:02x}")).collect(),
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
        c.requirements[0].index = 999;
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

    #[test]
    fn compiled_template_accepts_sixteen_distinct_resources_and_rejects_seventeen() {
        fn template_with_icons(count: usize) -> Value {
            let bits = [
                "AQ==", "Ag==", "Aw==", "BA==", "BQ==", "Bg==", "Bw==", "CA==",
                "CQ==", "Cg==", "Cw==", "DA==", "DQ==", "Dg==", "Dw==", "EA==",
                "EQ==",
            ];
            let mut template = quad();
            template["elements"] = Value::Array(
                bits[..count]
                    .iter()
                    .map(|bits| {
                        json!({
                            "type": "icon",
                            "x": 0,
                            "y": 0,
                            "w": 8,
                            "h": 1,
                            "bits": bits,
                        })
                    })
                    .collect(),
            );
            template
        }

        let compiled = compile(
            &template_with_icons(16),
            crate::platform::model::RENDER_TARGET_154G,
        )
        .expect("16 unique bitmap resources fit the compiled resource table");
        assert_eq!(compiled.resources.len(), 16);
        assert_eq!(compiled.compiler_abi, COMPILER_ABI);

        assert!(compile(
            &template_with_icons(17),
            crate::platform::model::RENDER_TARGET_154G,
        )
        .is_err());
    }

    #[test]
    fn compiled_template_accepts_sixty_four_ops_and_rejects_sixty_five() {
        fn template_with_rects(count: usize) -> Value {
            let mut template = quad();
            template["elements"] = Value::Array(
                (0..count)
                    .map(|_| json!({"type": "rect", "rect": [0, 0, 1, 1]}))
                    .collect(),
            );
            template
        }

        let compiled = compile(
            &template_with_rects(64),
            crate::platform::model::RENDER_TARGET_154G,
        )
        .expect("64 operations fit the compiled operation table");
        assert_eq!(compiled.op_count, 64);

        assert!(compile(
            &template_with_rects(65),
            crate::platform::model::RENDER_TARGET_154G,
        )
        .is_err());
    }

    #[test]
    fn note4_template_compiles_and_renders_at_native_size() {
        let source: Value = serde_json::from_str(include_str!(
            "../../../../project-workflow/generic-display-platform-implementation/concepts-400x300/codex-status-a-400x300.json"
        )).unwrap();
        let compiled = compile(&source, crate::platform::model::RENDER_TARGET_NOTE4).unwrap();
        assert_eq!((compiled.canvas_w, compiled.canvas_h), (400, 300));
        assert_eq!(decode(&encode(&compiled)).unwrap(), compiled);
        let usage = serde_json::json!({"account":{"plan":"Pro"},"bridge":{"label":"ACCOUNT"},
            "buckets":[{"id":"codex","windows":[{"windowMins":300,"usedPercent":6,"resetsAt":1800000000},
                {"windowMins":10080,"usedPercent":75,"resetsAt":1800500000}]}],
            "resetCredits":{"availableCount":2}}).to_string();
        let bits = bridge_render::render_bits(&source.to_string(), &usage, &bridge_render::Env::default()).unwrap();
        assert_eq!(bits.len(), 15_000);
        let png = bridge_render::bits_to_png_size(&bits, 400, 300).unwrap();
        assert!(png.len() > 1000);
    }
}
