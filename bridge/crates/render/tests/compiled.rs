//! Host parity: the compiled-template render path must be pixel-identical to
//! the JSON engine path for every fixture and state, survive the fixed-layout
//! serialization round trip, and reject corrupted records.

use bridge_render::{
    compile, compiled_deserialize, compiled_requirements, compiled_serialize, render_bits,
    render_compiled_bits, render_compiled_bits_size, Env,
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
            wifi_state: "connecting",
            wifi_blink: 1,
            ble_manual_active: false,
            offline_mins: -1,
            mode: "light",
        },
        Env {
            channel: "BLE",
            ip: "0.0.0.0",
            sync_hhmm: "--:--",
            battery: -1,
            wifi_state: "off",
            wifi_blink: -1,
            ble_manual_active: false,
            offline_mins: 42,
            mode: "deep",
        },
    ]
}

#[test]
fn note4_percent_origins_follow_one_two_and_three_digits() {
    let _guard = SERIAL.lock().unwrap();
    let source = std::fs::read_to_string(repo_root().join(
        "bridge/crates/core/tests/fixtures/codex-status-a-400x300.json",
    )).unwrap();
    let dynamic: serde_json::Value = serde_json::from_str(&source).unwrap();
    for (remaining, index) in [(0, 0), (5, 0), (9, 0), (10, 1), (40, 1), (99, 1), (100, 2)] {
      for five_hour_present in [true, false] {
        let used = 100 - remaining;
        let mut data = json!({
            "schema": 1,
            "buckets": [{"id": "codex", "windows": [
                {"windowMins": 300, "usedPercent": used, "resetsAt": 1_700_010_000i64},
                {"windowMins": 10080, "usedPercent": used, "resetsAt": 1_700_500_000i64}
            ]}]
        });
        if !five_hour_present { data["buckets"][0]["windows"][0] = serde_json::Value::Null; }
        let data = data.to_string();
        let mut fixed = dynamic.clone();
        for element in fixed["elements"].as_array_mut().unwrap() {
            if element.get("digit_x").is_some() {
                let x = element["digit_x"][index].clone();
                element["x"] = x;
                element.as_object_mut().unwrap().remove("digit_x");
                element.as_object_mut().unwrap().remove("digit_bind");
            }
        }
        let expected = render_bits(&fixed.to_string(), &data, &Env::default()).unwrap();
        let json_bits = render_bits(&source, &data, &Env::default()).unwrap();
        compile(&source).unwrap();
        let compiled_bits = render_compiled_bits_size(&data, &Env::default(), 400, 300).unwrap();
        let diff = |actual: &[u8]| actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
        assert_eq!(diff(&json_bits), 0, "JSON remaining={remaining} five_hour={five_hour_present}");
        assert_eq!(diff(&compiled_bits), 0, "compiled remaining={remaining} five_hour={five_hour_present}");
      }
    }
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
                    env.wifi_state, env.mode
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

#[test]
fn note4_window_layouts_cover_free_prolite_and_paid() {
    let _guard = SERIAL.lock().unwrap();
    let source = std::fs::read_to_string(repo_root().join(
        "bridge/crates/core/tests/fixtures/codex-status-a-400x300.json",
    ))
    .unwrap();
    // 400 wide, 1bpp: 50 bytes per row, MSB first.
    // 1bpp frames store 1 = white, 0 = ink.
    let ink = |bits: &[u8], x0: usize, x1: usize, y0: usize, y1: usize| {
        (y0..y1).any(|y| (x0..x1).any(|x| bits[y * 50 + x / 8] & (0x80 >> (x % 8)) == 0))
    };
    let data = |window_mins: i64| json!({
        "schema": 1, "server_time": 1_700_000_000i64, "account": {"plan": "free"},
        "buckets": [{"id": "codex", "windows": [
            {"windowMins": window_mins, "usedPercent": 23, "resetsAt": 1_800_000_000i64}
        ]}]
    }).to_string();
    let monthly = render_bits(&source, &data(43200), &Env::default()).unwrap();
    // Free/Go accounts only expose the monthly window: draw its balance, % and
    // reset time, and nothing from the paid 5H/WEEK layout.
    assert!(ink(&monthly, 12, 388, 66, 217), "monthly quota must render");
    let mut weekly = serde_json::from_str::<serde_json::Value>(&data(10080)).unwrap();
    weekly["account"]["plan"] = json!("prolite");
    let weekly = weekly.to_string();
    let weekly_bits = render_bits(&source, &weekly, &Env::default()).unwrap();
    assert!(ink(&weekly_bits, 120, 388, 66, 217), "weekly-only prolite quota must render");
    let mut empty = serde_json::from_str::<serde_json::Value>(&weekly).unwrap();
    empty["buckets"][0]["windows"] = json!([]);
    let empty_bits = render_bits(&source, &empty.to_string(), &Env::default()).unwrap();
    assert!(!ink(&empty_bits, 12, 388, 73, 217), "no quota labels without windows");
    // Paid data keeps the two-column layout.
    let paid = json!({
        "schema": 1, "server_time": 1_700_000_000i64, "account": {"plan": "plus"},
        "buckets": [{"id": "codex", "windows": [
            {"windowMins": 300, "usedPercent": 10, "resetsAt": 1_700_010_000i64},
            {"windowMins": 10080, "usedPercent": 30, "resetsAt": 1_700_500_000i64}
        ]}]
    }).to_string();
    let paid_bits = render_bits(&source, &paid, &Env::default()).unwrap();
    assert!(ink(&paid_bits, 12, 188, 66, 217), "5H column must render");
    assert!(ink(&paid_bits, 212, 388, 66, 217), "WEEK column must render");
    // Monthly overlays weekly-only; 5h restores the original paid layout even
    // when a source temporarily supplies monthly as well.
    let mut mixed = serde_json::from_str::<serde_json::Value>(&paid).unwrap();
    mixed["buckets"][0]["windows"].as_array_mut().unwrap().push(
        json!({"windowMins":43200,"usedPercent":23,"resetsAt":1_800_000_000i64}));
    assert_eq!(paid_bits, render_bits(&source, &mixed.to_string(), &Env::default()).unwrap());
    mixed["buckets"][0]["windows"] = json!([
        {"windowMins":10080,"usedPercent":75,"resetsAt":1_800_000_001i64},
        {"windowMins":43200,"usedPercent":23,"resetsAt":1_800_000_000i64}]);
    mixed["account"] = json!({"plan":"free"});
    mixed.as_object_mut().unwrap().remove("resetCredits");
    assert_eq!(monthly, render_bits(&source, &mixed.to_string(), &Env::default()).unwrap());
    compile(&source).unwrap();
    assert_eq!(weekly_bits, render_compiled_bits_size(&weekly, &Env::default(), 400, 300).unwrap());
    assert_eq!(
        monthly,
        render_compiled_bits_size(&data(43200), &Env::default(), 400, 300).unwrap()
    );
    assert_eq!(
        paid_bits,
        render_compiled_bits_size(&paid, &Env::default(), 400, 300).unwrap()
    );
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
            "bridge/crates/core/tests/fixtures/codex-status-a-400x300.json",
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

#[test]
fn wifi_metadata_controls_template_pixels_independently_of_legacy_state() {
    let _guard = SERIAL.lock().unwrap();
    for path in ["tools/test-bridge/templates/quad.json",
                 "bridge/crates/core/tests/fixtures/codex-status-a-400x300.json"] {
        let mut source: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(repo_root().join(path)).unwrap()).unwrap();
        source["elements"].as_array_mut().unwrap().retain(|e|
            matches!(e["when"]["bind"].as_str(), Some("device.wifi_state" | "device.wifi_blink")));
        assert!(!source["elements"].as_array().unwrap().is_empty());
        let source = source.to_string();
        let (w,h) = bridge_render::canvas_size(&source).unwrap();
        compile(&source).unwrap();
        let mut frames = Vec::new();
        for (phase, blink, visible) in [("idle",-1,false),("connecting",0,false),
            ("connecting",1,true),("connected",-1,true),("off",-1,true)] {
            // Legacy OFF must not override the independent Wi-Fi facts.
            let env = Env {wifi_state:phase,wifi_blink:blink,..Env::default()};
            let json = render_bits(&source,"{}",&env).unwrap();
            let compiled = render_compiled_bits_size("{}",&env,w,h).unwrap();
            assert_eq!(json,compiled,"{path} {phase}/{blink}");
            assert_eq!(json.iter().any(|b|*b!=255),visible,"{path} {phase}/{blink}");
            frames.push(json);
        }
        assert_eq!(frames[0],frames[1]);
        assert_eq!(frames[2],frames[3]);
        assert_ne!(frames[3],frames[4]);
        // Templates can render arbitrary content from the same metadata.
        let mut custom: serde_json::Value = serde_json::from_str(&source).unwrap();
        custom["elements"][0]["when"]["bind"] = json!("device.wifi_unknown");
        assert!(compile(&custom.to_string()).is_err());
    }
}

#[test]
fn manual_gatt_icon_is_independent_of_wifi_and_automatic_ble() {
    let _guard = SERIAL.lock().unwrap();
    let mut source: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        repo_root().join("bridge/crates/core/tests/fixtures/codex-status-a-400x300.json")).unwrap()).unwrap();
    source["elements"].as_array_mut().unwrap().retain(|e| e["when"]["bind"] == "device.ble_manual_active");
    assert!(!source["elements"].as_array().unwrap().is_empty());
    let source=source.to_string();
    let (w,h)=bridge_render::canvas_size(&source).unwrap();
    compile(&source).unwrap();
    for phase in ["idle","connecting","connected","off"] {
        for active in [false,true] {
            let env=Env {wifi_state:phase,ble_manual_active:active,..Env::default()};
            let pixels=render_bits(&source,"{}",&env).unwrap();
            assert_eq!(pixels,render_compiled_bits_size("{}",&env,w,h).unwrap());
            assert_eq!(pixels.iter().any(|b|*b!=255),active,"{phase}/{active}");
        }
    }
}

#[test]
fn note4_wifi_region_survives_boot_close_and_automatic_ble_closeout() {
    let _guard = SERIAL.lock().unwrap();
    let source = std::fs::read_to_string(repo_root().join(
        "bridge/crates/core/tests/fixtures/codex-status-a-400x300.json")).unwrap();
    compile(&source).unwrap();
    let region = |bits: &[u8]| -> Vec<bool> {
        (8..28).flat_map(|y| (272..292).map(move |x| bits[y*50+x/8] & (0x80 >> (x%8)) != 0)).collect()
    };
    for (phase, blink, visible) in [("idle",-1,false),("connecting",0,false),
        ("connecting",1,true),("connected",-1,true),("off",-1,true)] {
        let mut previous = None;
        // Only the manual BLE fact enters the renderer. Automatic BLE has no
        // legacy state-word input through which it could affect these pixels.
        for manual in [true,false,false,true] {
            let env = Env { wifi_state:phase, wifi_blink:blink,
                ble_manual_active:manual, mode:"light", ..Env::default() };
            let bits = render_bits(&source,&usage(true),&env).unwrap();
            assert_eq!(bits,render_compiled_bits_size(&usage(true),&env,400,300).unwrap());
            let pixels = region(&bits);
            assert_eq!(pixels.iter().any(|v| !v),visible,"{phase}/{manual}");
            if let Some(prior) = previous { assert_eq!(pixels,prior,"{phase}/{manual}"); }
            previous = Some(pixels);
        }
    }
}

#[test]
fn unsupported_fields_are_rejected_in_json_and_persisted_compiled_records() {
    let _guard = SERIAL.lock().unwrap();
    for bind in ["device.state", "device.future_field"] {
        for condition in [false,true] {
            let mut element = json!({"type":"text","x":0,"y":0,"font":"f12","text":"safe"});
            if condition { element["when"] = json!({"bind":bind,"exists":false}); }
            else { element.as_object_mut().unwrap().remove("text"); element["bind"] = json!(bind); }
            let source = json!({"schema":1,"id":"unsupported","canvas":{"w":200,"h":200},"elements":[element]});
            assert!(bridge_core::template::validate_template(&source).is_err());
            assert!(compile(&source.to_string()).is_err());
        }
    }
    extern "C" { fn codex_ct_binding_rejection_check() -> i32; }
    assert_eq!(unsafe { codex_ct_binding_rejection_check() },0);
}
