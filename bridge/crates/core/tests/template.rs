use std::path::PathBuf;

use bridge_core::template::{
    canonical_bytes, encode_chunks, parse_bind, template_hash, validate_template, BindField,
    BindSpec, Library, WinSel,
};
use serde_json::{json, Value};

fn template_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tools/test-bridge/templates")
}

#[test]
fn hashes_match_python_test_bridge() {
    let library = Library::load(&template_dir()).expect("library");
    let full = library.get("full").expect("full template");
    let mini = library.get("mini").expect("mini template");
    assert_eq!(full.hash, "c1a2faaf");
    assert_eq!(mini.hash, "e6ba459e");
    assert_eq!(full.version, 3);
    assert_eq!(mini.version, 2);
    let quad = library.get("quad").expect("quad template");
    assert_eq!(quad.version, 7);
    assert_eq!(quad.min_fw.as_deref(), Some("0.12"));
    // v0.12 single layout, canonical hash checked against the Python format.
    assert_eq!(quad.hash, "c598adc0");
}

#[test]
fn canonical_json_is_sorted_and_compact() {
    let value: serde_json::Value =
        serde_json::from_str(r#"{"b":1,"a":{"d":2,"c":[3,4]}}"#).unwrap();
    let text = String::from_utf8(canonical_bytes(&value)).unwrap();
    assert_eq!(text, r#"{"a":{"c":[3,4],"d":2},"b":1}"#);
}

#[test]
fn parse_bind_grammar() {
    assert_eq!(parse_bind("account.plan"), Some(BindSpec::Plan));
    assert_eq!(parse_bind("device.sync_hhmm"), Some(BindSpec::DeviceSync));
    assert_eq!(
        parse_bind("buckets[codex].weekly.remaining"),
        Some(BindSpec::Bucket {
            bucket: "codex".into(),
            win: WinSel::Weekly,
            field: BindField::Remaining,
        })
    );
    assert_eq!(
        parse_bind("buckets[codex_bengalfox].windows[1].usedPercent"),
        Some(BindSpec::Bucket {
            bucket: "codex_bengalfox".into(),
            win: WinSel::Index(1),
            field: BindField::UsedPercent,
        })
    );
    assert_eq!(parse_bind("buckets[codex].yearly.usedPercent"), None);
    assert_eq!(parse_bind("buckets[].weekly.usedPercent"), None);
    assert_eq!(parse_bind("account.email"), None);
    assert_eq!(parse_bind("device.state"), Some(BindSpec::DeviceState));
    assert_eq!(
        parse_bind("device.offline_mins"),
        Some(BindSpec::DeviceOfflineMins)
    );
    assert_eq!(parse_bind("device.idle_reason"), None);
    assert_eq!(
        parse_bind("buckets[codex].monthly.remaining"),
        Some(BindSpec::Bucket {
            bucket: "codex".into(),
            win: WinSel::Monthly,
            field: BindField::Remaining,
        })
    );
}

#[test]
fn ignores_legacy_element_mode_key() {
    let template = |mode: Value| {
        json!({
            "schema": 1,
            "canvas": {"w": 200, "h": 200},
            "elements": [
                {"type": "text", "text": "X", "font": "f12", "x": 0, "y": 0, "mode": mode}
            ]
        })
    };
    // v0.12 removed the idle/live element split: the device engine ignores the
    // unknown `mode` key, so the canonical validator must accept it too.
    assert!(validate_template(&template(json!("any"))).is_ok());
    assert!(validate_template(&template(json!("idle"))).is_ok());
    assert!(validate_template(&template(json!("live"))).is_ok());
    assert!(validate_template(&template(json!("always"))).is_ok());
    assert!(validate_template(&template(json!(1))).is_ok());
}

#[test]
fn validates_templates_like_device() {
    assert!(validate_template(&json!({
        "schema": 1,
        "canvas": {"w": 200, "h": 200},
        "elements": [{"type": "text", "bind": "account.plan", "font": "f16", "x": 8, "y": 8}]
    }))
    .is_ok());

    for bad in [
        json!({"schema": 2, "canvas": {"w": 200, "h": 200}, "elements": [{"type": "rect", "rect": [0,0,10,10]}]}),
        json!({"schema": 1, "canvas": {"w": 100, "h": 200}, "elements": [{"type": "rect", "rect": [0,0,10,10]}]}),
        json!({"schema": 1, "canvas": {"w": 200, "h": 200}, "elements": []}),
        json!({"schema": 1, "canvas": {"w": 200, "h": 200}, "elements": [{"type": "sparkline"}]}),
        json!({"schema": 1, "canvas": {"w": 200, "h": 200}, "elements": [{"type": "text", "text": "x", "font": "f99", "x": 0, "y": 0}]}),
        json!({"schema": 1, "canvas": {"w": 200, "h": 200}, "elements": [{"type": "bar", "bind": "buckets[codex].yearly.usedPercent", "rect": [0,0,10,10]}]}),
        json!({"schema": 1, "canvas": {"w": 200, "h": 200}, "elements": [{"type": "rect", "rect": [0,0,-5,10]}]}),
    ] {
        assert!(validate_template(&bad).is_err(), "should reject: {bad}");
    }
}

#[test]
fn validates_quad_text_bounds_formats_and_conditions() {
    let valid = json!({
        "schema": 1,
        "canvas": {"w": 200, "h": 200},
        "elements": [
            {"type": "text", "bind": "buckets[codex].5h.resetsAt",
             "time_format": "hhmm", "region": [4, 4, 100, 30],
             "align": "center", "scale": 3, "font": "f20"},
            {"type": "icon", "when": {"bind": "buckets[codex].5h.remaining", "exists": false},
             "x": 4, "y": 40, "w": 8, "h": 8, "bits": "AAAAAAAAAAA="}
        ]
    });
    assert!(validate_template(&valid).is_ok());

    for bad in [
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","scale":0}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","scale":4}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","scale":1.0}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","scale":null}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","region":[0,0,201,10]}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","region":[2147483647,0,1,1]}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","region":[0,0,10,10],"align":"diagonal"}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","align":"right"}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","bind":"account.plan","font":"f8","time_format":"hhmm"}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","bind":"server_time","font":"f8","time_format":"bad"}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","bind":"server_time","font":"f8","time_format":null}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","when":null}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","when":{"bind":"buckets[codex].5h.remaining","exists":1}}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","when":{"bind":"buckets[codex].yearly.remaining","exists":true}}]}),
        json!({"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"text","text":"x","font":"f8","when":{"bind":"account.plan","exists":true,"extra":false}}]}),
    ] {
        assert!(validate_template(&bad).is_err(), "should reject: {bad}");
    }
}

#[test]
fn chunks_are_sequential_with_offset_prefix() {
    let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    let chunks = encode_chunks(&data, 180);
    let mut rebuilt = Vec::new();
    let mut expected_off = 0usize;
    for chunk in &chunks {
        let off = chunk[0] as usize | ((chunk[1] as usize) << 8);
        assert_eq!(off, expected_off);
        rebuilt.extend_from_slice(&chunk[2..]);
        expected_off += chunk.len() - 2;
    }
    assert_eq!(rebuilt, data);
    assert_eq!(chunks.last().unwrap().len(), 2 + 1000 % 180);
    assert_eq!(template_hash(&data), format!("{:08x}", crc32fast::hash(&data)));
}
