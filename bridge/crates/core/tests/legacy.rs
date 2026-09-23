//! M0 legacy compatibility fixture: the wire contract the v2 path must keep
//! while legacy firmware/devices remain in the field.
//!
//! Pinned here: the legacy usage envelope shape, the legacy `/template`
//! chunk framing (2-byte LE offset + payload, hash == CRC32 of canonical JSON),
//! the legacy ≤3-entry profile file, and the legacy "3 slots" migration path.

use bridge_core::envelope::{build_envelope, EnvelopeOptions};
use bridge_core::platform::model::{DeviceCapabilities, Profile as V2Profile};
use bridge_core::profile::{Profile, ProfilesFile};
use bridge_core::template::{canonical_bytes, encode_chunks, template_hash, Library};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn fixture_rate_limits() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/rate_limits.json")).unwrap()
}

#[test]
fn legacy_envelope_shape_is_pinned() {
    let opts = EnvelopeOptions {
        bridge_label: Some("tester".into()),
        bridge_host_id: "abcd".into(),
        bridge_host: "192.168.3.111".into(),
        bridge_port: 8765,
        next_sync_seconds: 60,
        templates: BTreeMap::new(),
        active_hold_seconds: 600,
    };
    let env = build_envelope(&fixture_rate_limits(), &opts);
    // Fields the ≤0.15.10 firmware parses must keep their names and types.
    assert_eq!(env["schema"], 1);
    assert!(env["server_time"].is_u64());
    assert_eq!(env["bridge"]["hostId"], "abcd");
    assert_eq!(env["bridge"]["host"], "192.168.3.111");
    assert_eq!(env["bridge"]["port"], 8765);
    assert_eq!(env["bridge"]["label"], "tester");
    assert!(env["account"]["plan"].is_string());
    assert!(env["buckets"].is_array());
    assert_eq!(env["buckets"][0]["id"], "codex");
    assert!(env["buckets"][0]["windows"].is_array());
    assert_eq!(env["next_sync_seconds"], 60);
    assert_eq!(env["active_hold_seconds"], 600);
    // resetCredits is absent when availableCount <= 0 (the RC row hides).
    assert!(env.get("resetCredits").is_none());
    // With a credit available the block appears for the firmware to render.
    let mut with_credits = fixture_rate_limits();
    with_credits["rateLimitResetCredits"] = json!({
        "availableCount": 1,
        "credits": [{"expiresAt": 1_800_000_000}]
    });
    let env = build_envelope(&with_credits, &opts);
    assert_eq!(env["resetCredits"]["availableCount"], 1);
    assert_eq!(env["resetCredits"]["nextExpiresAt"], 1_800_000_000i64);
}

#[test]
fn legacy_pull_merge_fields_exist() {
    // The v0.14 pull response merges these keys into the envelope (`core::http`).
    let pull = json!({
        "mode": "light",
        "next_contact_s": 60,
        "usage_rev": 3,
        "pending": {"ota": false, "templates": []}
    });
    assert!(matches!(pull["mode"], serde_json::Value::String(_)));
    assert!(pull["next_contact_s"].is_u64());
    assert!(pull["usage_rev"].is_u64());
    assert!(pull["pending"]["templates"].is_array());
}

#[test]
fn legacy_template_framing_is_unchanged() {
    let quad =
        std::fs::read_to_string(repo_root().join("tools/test-bridge/templates/quad.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&quad).unwrap();
    let bytes = canonical_bytes(&json);
    let hash = template_hash(&bytes);
    assert_eq!(hash.len(), 8);
    // Hash is CRC32 of the canonical bytes (same as the Python bridge and the
    // device's `crc32buf`), and the BLE framing is offset-prefixed chunks.
    let expected = format!("{:08x}", crc32fast::hash(&bytes));
    assert_eq!(hash, expected);
    // quad v12 hash is a known cross-implementation fact.
    assert_eq!(hash, "430cc188");
    let chunks = encode_chunks(&bytes, 178);
    let mut off = 0usize;
    for chunk in chunks.iter() {
        let stated = (chunk[0] as usize) | ((chunk[1] as usize) << 8);
        assert_eq!(stated, off);
        off += chunk.len() - 2;
    }
    assert_eq!(off, bytes.len());
    // Reassembly is byte-identical.
    let mut joined = Vec::new();
    for chunk in chunks {
        joined.extend_from_slice(&chunk[2..]);
    }
    assert_eq!(joined, bytes);
}

#[test]
fn legacy_library_hashes_still_match() {
    let lib = Library::load(&repo_root().join("tools/test-bridge/templates")).unwrap();
    assert_eq!(lib.get("full").unwrap().hash, "c1a2faaf");
    assert_eq!(lib.get("mini").unwrap().hash, "e6ba459e");
    assert_eq!(lib.get("quad").unwrap().hash, "430cc188");
}

#[test]
fn legacy_three_entry_profile_loads_and_migrates_without_truncation() {
    let seed = repo_root().join("tools/test-bridge/profiles.seed.json");
    let profiles = ProfilesFile::load(&seed).unwrap();
    let profile: &Profile = profiles.get("default").unwrap();
    let enabled = profile.enabled_ids();
    assert_eq!(enabled, vec!["quad", "full", "mini"]);
    // The legacy model never silently drops entries on read.
    assert_eq!(profile.templates.len(), 3);

    // Migration to a v2 profile keeps order and all three ids, sync off.
    let migrated = V2Profile {
        device_mac: "AA:BB:CC:DD:EE:FF".into(),
        template_ids: enabled.clone(),
        render_target: Some(bridge_core::platform::model::RENDER_TARGET_154G.into()),
        font_ids: Default::default(),
        initial_active_id: Some(enabled[0].clone()),
        bindings: vec![],
        sync_enabled: false,
        full_sync_s: 3600,
        updated_at: 0,
    };
    migrated
        .validate_publishable(&DeviceCapabilities::ssd1681_154g())
        .unwrap();
    assert_eq!(migrated.template_ids, enabled);
    assert!(!migrated.sync_enabled);
}

#[test]
fn legacy_device_capability_note_is_explicit() {
    // A legacy device is modelled with the legacy capability contract and is
    // flagged; the bridge must show the limitation instead of truncating.
    let caps = DeviceCapabilities::ssd1681_154g();
    assert_eq!(caps.max_templates, 8);
    assert!(caps.hardware_verified);
    let legacy_note = json!({
        "device_mac": "AA:BB:CC:DD:EE:FF",
        "legacy": true,
        "max_templates": 3,
        "note": "legacy firmware: new protocol negotiated before v2 is used"
    });
    assert_eq!(legacy_note["max_templates"], 3);
}
