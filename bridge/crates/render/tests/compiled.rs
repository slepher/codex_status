//! Host parity: the compiled-template render path must be pixel-identical to
//! the JSON engine path for every fixture and state, survive the fixed-layout
//! serialization round trip, and reject corrupted records.

use bridge_render::{
    compile, compiled_deserialize, compiled_requirements, compiled_serialize, render_bits,
    render_compiled_bits, Env,
};
use serde_json::json;

/// The engine has one global compiled template; tests must not interleave.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn template(id: &str) -> String {
    std::fs::read_to_string(repo_root().join(format!("tools/test-bridge/templates/{id}.json")))
        .unwrap()
}

fn usage(normal: bool) -> String {
    let mut env = json!({
        "schema": 1,
        "server_time": 1_700_000_000i64,
        "bridge": {"label": "tester", "hostId": "abcd"},
        "account": {"plan": "plus"},
        "buckets": [
            {"id": "codex", "windows": [
                {"kind": "weekly", "usedPercent": 30, "resetsAt": 1_700_500_000i64, "windowMins": 10080},
                {"kind": "5h", "usedPercent": 10, "resetsAt": 1_700_010_000i64, "windowMins": 300}
            ]}
        ],
        "resetCredits": {"availableCount": 2, "nextExpiresAt": 1_700_900_000i64}
    });
    if !normal {
        // Edge state: no 5h window, no reset credits, no username -> the
        // template's static-100/missing branches must render identically.
        env["buckets"][0]["windows"] = json!([
            {"kind": "weekly", "usedPercent": 0, "windowMins": 10080}
        ]);
        env["resetCredits"] = json!({"availableCount": 0});
        env["bridge"]["label"] = serde_json::Value::Null;
    }
    env.to_string()
}

fn envs() -> Vec<Env<'static>> {
    vec![
        Env {
            channel: "WIFI",
            ip: "192.168.3.163",
            sync_hhmm: "12:34",
            battery: 75,
            state: "WIFI CONN",
            offline_mins: -1,
            mode: "light",
        },
        Env {
            channel: "BLE",
            ip: "0.0.0.0",
            sync_hhmm: "--:--",
            battery: -1,
            state: "WIFI OFF",
            offline_mins: 42,
            mode: "deep",
        },
    ]
}

#[test]
fn compiled_render_is_pixel_identical_to_json_render() {
    let _guard = SERIAL.lock().unwrap();
    for id in ["quad", "mini", "full"] {
        let tmpl = template(id);
        compile(&tmpl).unwrap_or_else(|e| panic!("{id} compile: {e}"));
        for normal in [true, false] {
            let u = usage(normal);
            for env in envs() {
                let json_bits = render_bits(&tmpl, &u, &env).unwrap();
                let ct_bits = render_compiled_bits(&u, &env).unwrap();
                assert_eq!(
                    json_bits, ct_bits,
                    "{id} normal={normal} state={} mode={} differs",
                    env.state, env.mode
                );
            }
        }
    }
}

#[test]
fn compiled_round_trip_survives_serialization() {
    let _guard = SERIAL.lock().unwrap();
    let tmpl = template("quad");
    compile(&tmpl).unwrap();
    let blob = compiled_serialize().unwrap();
    let before = render_compiled_bits(&usage(true), &Env::default()).unwrap();
    // Corrupt a byte in the middle: load must fail (CRC), and a wrong ABI too.
    let mut corrupt = blob.clone();
    let n = corrupt.len();
    corrupt[n / 2] ^= 0x55;
    assert!(compiled_deserialize(&corrupt).is_err());
    // Round trip restores the identical render.
    compiled_deserialize(&blob).unwrap();
    let after = render_compiled_bits(&usage(true), &Env::default()).unwrap();
    assert_eq!(before, after);
    // Truncated record is rejected.
    assert!(compiled_deserialize(&blob[..blob.len() / 2]).is_err());
}

#[test]
fn requirements_are_indexed_and_bounded() {
    let _guard = SERIAL.lock().unwrap();
    compile(&template("quad")).unwrap();
    let reqs = compiled_requirements().unwrap();
    assert!(reqs.len() >= 10, "quad needs many distinct fields");
    assert!(reqs.iter().any(|r| r == "buckets[codex].weekly.remaining"));
    assert!(reqs.iter().any(|r| r == "device.now"));
    // No duplicates (bounded, dense index table).
    let mut sorted = reqs.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), reqs.len());
}

#[test]
fn compiled_regions_match_the_json_region_derivation() {
    let _guard = SERIAL.lock().unwrap();
    for id in ["quad", "mini", "full"] {
        let tmpl = template(id);
        assert!(bridge_render::rgn_build(&tmpl).is_ok(), "{id} json regions");
        let json_regions = bridge_render::rgn_dump();
        compile(&tmpl).unwrap();
        let blob = compiled_serialize().unwrap();
        let (w, h) = bridge_render::canvas_size(&tmpl).unwrap();
        bridge_render::rgn_build_compiled(&blob, w, h).unwrap_or_else(|e| panic!("{id}: {e}"));
        let ct_regions = bridge_render::rgn_dump();
        assert_eq!(json_regions, ct_regions, "{id} region derivation differs");
    }
}

/// The region policy keeps one global panel size, so a 400x300 template must be
/// derived against its own canvas rather than the 200x200 default. A wrong panel
/// size makes ops fall outside the panel, which the policy answers with the
/// whole-frame fallback (`rgn_build` then fails), so a successful derivation of
/// this template is the regression guard.
#[test]
fn note4_regions_use_the_template_canvas() {
    let _guard = SERIAL.lock().unwrap();
    let tmpl = std::fs::read_to_string(
        repo_root().join(
            "project-workflow/generic-display-platform-implementation/\
             concepts-400x300/codex-status-a-400x300.json",
        ),
    )
    .expect("400x300 canonical template fixture");
    let (w, h) = bridge_render::canvas_size(&tmpl).expect("canvas");
    assert_eq!((w, h), (400, 300));
    let n = bridge_render::rgn_build(&tmpl)
        .unwrap_or_else(|e| panic!("400x300 region derivation fell back to whole-frame: {e}"));
    assert!(n > 0);
    let json_regions = bridge_render::rgn_dump();
    compile(&tmpl).unwrap();
    let blob = compiled_serialize().unwrap();
    bridge_render::rgn_build_compiled(&blob, w, h).unwrap();
    assert_eq!(json_regions, bridge_render::rgn_dump());
}

#[test]
fn invalid_template_is_rejected_with_a_reason() {
    let _guard = SERIAL.lock().unwrap();
    let err = compile(r#"{"schema":1,"canvas":{"w":200,"h":200},"elements":[{"type":"bogus"}]}"#)
        .unwrap_err();
    assert!(!err.is_empty());
}
