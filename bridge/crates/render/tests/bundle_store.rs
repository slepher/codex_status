use std::ffi::CString;
use base64::Engine;
use serde_json::json;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

extern "C" {
    fn codex_bundle_store_check(bundle: *const std::os::raw::c_char) -> i32;
    fn codex_bundle_font_install(bundle: *const std::os::raw::c_char, reset: i32) -> i32;
    fn codex_bundle_font_reboot() -> i32;
}

#[test]
fn firmware_consumes_bridge_binary_and_survives_torn_storage_writes() {
    let _guard = SERIAL.lock().unwrap();
    let source: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/test-bridge/templates/quad.json"
    )).unwrap();
    let compiled = bridge_core::compile::compile(&source, "epd-ssd1681-200x200-1bpp").unwrap();
    let bundle = json!({
        "job_id": "job-test", "firmware_target": "codex-status-154g",
        "render_target": "epd-ssd1681-200x200-1bpp", "compiler_abi": bridge_core::compile::COMPILER_ABI,
        "profile": {"template_ids": ["quad"], "initial_active_id": "quad"},
        "templates": [{"key": {"template_id": "quad", "render_target": "epd-ssd1681-200x200-1bpp"},
                       "source": source, "compiled": compiled}],
        "resources": [], "bindings": []
    });
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    let text = CString::new(bytes).unwrap();
    assert_eq!(unsafe { codex_bundle_store_check(text.as_ptr()) }, 0);
}

#[test]
fn profile_font_is_installed_with_bundle_and_restored_after_boot() {
    let _guard = SERIAL.lock().unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let source: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join(
        "bridge/crates/core/tests/fixtures/codex-status-a-400x300.json"
    )).unwrap()).unwrap();
    let font = std::fs::read(root.join("bridge/assets/fonts/font_ntthin18-extralight200.bin")).unwrap();
    let info = bridge_render::font_asset_check(&font).unwrap();
    assert!(info.contains("id=81d583bc name=ntthin18"), "{info}");
    let mut bundle = json!({
        "job_id": "font-test", "firmware_target": "zectrix-note4-400x300",
        "render_target": "epd-ssd2683-400x300-1bpp", "compiler_abi": bridge_core::compile::COMPILER_ABI,
        "profile": {"template_ids": ["codex-status-a"], "initial_active_id": "codex-status-a"},
        "templates": [{"key": {"template_id": "codex-status-a", "render_target": "epd-ssd2683-400x300-1bpp"}, "source": source}],
        "fonts": [{"name": "ntthin18", "font_id": "81d583bc",
                   "data": base64::engine::general_purpose::STANDARD.encode(&font)}]
    });
    bridge_render::font_clear_assets();
    let baseline = bridge_render::render_bits(&source.to_string(), "", &bridge_render::Env::default()).unwrap();
    let payload = CString::new(bundle.to_string()).unwrap();
    assert_eq!(unsafe { codex_bundle_font_install(payload.as_ptr(), 1) }, 0);
    let extra_light = bridge_render::render_bits(&source.to_string(), "", &bridge_render::Env::default()).unwrap();
    assert_ne!(baseline, extra_light);
    assert_eq!(unsafe { codex_bundle_font_reboot() }, 0);
    assert_eq!(extra_light, bridge_render::render_bits(&source.to_string(), "", &bridge_render::Env::default()).unwrap());
    bundle["fonts"][0]["font_id"] = json!("deadbeef");
    let bad = CString::new(bundle.to_string()).unwrap();
    assert_eq!(unsafe { codex_bundle_font_install(bad.as_ptr(), 0) }, 1);
    assert_eq!(extra_light, bridge_render::render_bits(&source.to_string(), "", &bridge_render::Env::default()).unwrap());
    bridge_render::font_clear_assets();
}
