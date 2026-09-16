use std::path::PathBuf;

use bridge_core::template::{
    canonical_bytes, encode_chunks, parse_bind, template_hash, validate_template, BindField,
    BindSpec, Library, WinSel,
};
use serde_json::json;

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
    assert_eq!(parse_bind("buckets[codex].monthly.usedPercent"), None);
    assert_eq!(parse_bind("buckets[].weekly.usedPercent"), None);
    assert_eq!(parse_bind("account.email"), None);
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
        json!({"schema": 1, "canvas": {"w": 200, "h": 200}, "elements": [{"type": "bar", "bind": "buckets[codex].monthly.usedPercent", "rect": [0,0,10,10]}]}),
        json!({"schema": 1, "canvas": {"w": 200, "h": 200}, "elements": [{"type": "rect", "rect": [0,0,-5,10]}]}),
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
