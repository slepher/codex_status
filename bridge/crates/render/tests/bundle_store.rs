use std::ffi::CString;
use serde_json::json;

extern "C" {
    fn codex_bundle_store_check(bundle: *const std::os::raw::c_char) -> i32;
}

#[test]
fn firmware_consumes_bridge_binary_and_survives_torn_storage_writes() {
    let source: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tools/test-bridge/templates/quad.json"
    )).unwrap();
    let compiled = bridge_core::compile::compile(&source, "epd-ssd1681-200x200-1bpp").unwrap();
    let bundle = json!({
        "job_id": "job-test", "firmware_target": "codex-status-154g",
        "render_target": "epd-ssd1681-200x200-1bpp", "compiler_abi": 2,
        "profile": {"template_ids": ["quad"], "initial_active_id": "quad"},
        "templates": [{"key": {"template_id": "quad", "render_target": "epd-ssd1681-200x200-1bpp"},
                       "source": source, "compiled": compiled}],
        "resources": [], "bindings": []
    });
    let bytes = bridge_core::template::canonical_bytes(&bundle);
    let text = CString::new(bytes).unwrap();
    assert_eq!(unsafe { codex_bundle_store_check(text.as_ptr()) }, 0);
}
