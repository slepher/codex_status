use base64::Engine;
use bridge_core::platform::fonts::FontLibrary;
use serde_json::json;
use std::ffi::CString;

extern "C" {
    fn codex_bundle_font_install(bundle: *const std::os::raw::c_char, reset: i32) -> i32;
    fn codex_bundle_font_reboot() -> i32;
    fn codex_font_text_width(text: *const std::os::raw::c_char) -> i32;
}

#[test]
fn unicode_asset_validation_rendering_and_bundle_recovery() {
    let font = include_bytes!("../../core/tests/fixtures/unicode-subset.bin");
    let descriptor = FontLibrary::validate(font).unwrap();
    assert_eq!(descriptor.version, 2);
    assert_eq!(descriptor.glyph_count, 98);
    assert!(bridge_render::font_asset_check(font).is_ok());
    // The codepoint table follows the three even-padded string sections.
    let cp_offset = 64 + [34, 36, 38].iter().map(|i| {
        let n = u16::from_le_bytes([font[*i], font[*i+1]]) as usize;
        n + (n & 1)
    }).sum::<usize>();
    for (count, accepted) in [(0usize, false), (1, false), (95, true), (4088, true), (4089, false), (4097, false)] {
        let mut boundary = font[..cp_offset].to_vec();
        for i in 0..count {
            let cp = if i < 95 { 32 + i as u32 } else { 0x4e00 + i as u32 - 95 };
            boundary.extend_from_slice(&cp.to_le_bytes());
        }
        boundary.resize(boundary.len() + count * 8, 0);
        let len = boundary.len() as u32;
        boundary[8..12].copy_from_slice(&len.to_le_bytes());
        boundary[16..20].copy_from_slice(&0u32.to_le_bytes());
        boundary[20..24].copy_from_slice(&(count as u32).to_le_bytes());
        boundary[40..42].copy_from_slice(&0u16.to_le_bytes());
        let crc = crc32fast::hash(&boundary[64..]);
        boundary[12..16].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(FontLibrary::validate(&boundary).is_ok(), accepted, "Rust count {count}");
        assert_eq!(bridge_render::font_asset_check(&boundary).is_ok(), accepted, "C++ count {count}");
    }
    for cp in [0xD800u32, 0x110000, 0x4E2D, 0x7F] {
        let mut bad = font.to_vec();
        bad[cp_offset+96*4..cp_offset+97*4].copy_from_slice(&cp.to_le_bytes());
        let crc = crc32fast::hash(&bad[64..]);
        bad[12..16].copy_from_slice(&crc.to_le_bytes());
        assert!(FontLibrary::validate(&bad).is_err());
        assert!(bridge_render::font_asset_check(&bad).is_err());
    }
    for n in [0, 63, font.len()-1] {
        assert!(FontLibrary::validate(&font[..n]).is_err());
        assert!(bridge_render::font_asset_check(&font[..n]).is_err());
    }
    let source = json!({"schema":1,"id":"unicode","canvas":{"w":400,"h":300},
        "elements":[{"type":"text","x":0,"y":0,"font":"ntthin18","text":"中文𠀀?"}]});
    let mut bounded = source.clone();
    bounded["elements"][0]["text"] = json!("中".repeat(16));
    assert!(bridge_render::validate(&bounded.to_string()).is_err());
    assert!(bridge_core::compile::compile(&bounded, "epd-ssd2683-400x300-1bpp").is_err());
    bounded["elements"][0]["text"] = json!("中".repeat(15));
    assert!(bridge_render::validate(&bounded.to_string()).is_ok());
    bridge_render::font_clear_assets();
    let ascii = bridge_render::render_bits(&source.to_string(), "", &bridge_render::Env::default()).unwrap();
    assert!(bridge_render::font_bind_asset(font));
    for (text, width) in [("中文𠀀?".as_bytes(), 28), (b"?".as_slice(), 4),
                          ("缺".as_bytes(), 4), (b"\xf0\x80\x80\x80".as_slice(), 16),
                          (b"\xe4\xb8".as_slice(), 8), (b"\xed\xa0\x80".as_slice(), 12)] {
        assert_eq!(unsafe { codex_font_text_width(CString::new(text).unwrap().as_ptr()) }, width);
    }
    let unicode = bridge_render::render_bits(&source.to_string(), "", &bridge_render::Env::default()).unwrap();
    assert_ne!(unicode, ascii);
    let mut dynamic = source.clone();
    dynamic["elements"][0].as_object_mut().unwrap().remove("text");
    dynamic["elements"][0]["bind"] = json!("bridge.label");
    assert_eq!(unicode, bridge_render::render_bits(&dynamic.to_string(),
        &json!({"bridge":{"label":"中文𠀀?"}}).to_string(), &bridge_render::Env::default()).unwrap());
    bridge_render::compile(&source.to_string()).unwrap();
    assert_eq!(unicode, bridge_render::render_compiled_bits_size("", &bridge_render::Env::default(), 400, 300).unwrap());
    let mut fallback = source.clone();
    fallback["elements"][0]["text"] = json!("缺");
    let missing = bridge_render::render_bits(&fallback.to_string(), "", &bridge_render::Env::default()).unwrap();
    fallback["elements"][0]["text"] = json!("?");
    assert_eq!(missing, bridge_render::render_bits(&fallback.to_string(), "", &bridge_render::Env::default()).unwrap());
    let bundle = json!({"job_id":"unicode-font", "firmware_target":"zectrix-note4-400x300",
        "render_target":"epd-ssd2683-400x300-1bpp","compiler_abi":bridge_core::compile::COMPILER_ABI,
        "profile":{"template_ids":["unicode"],"initial_active_id":"unicode"},
        "templates":[{"key":{"template_id":"unicode","render_target":"epd-ssd2683-400x300-1bpp"},"source":source}],
        "fonts":[{"name":"ntthin18","font_id":descriptor.id,
                   "data":base64::engine::general_purpose::STANDARD.encode(font)}]});
    let payload = CString::new(bundle.to_string()).unwrap();
    assert_eq!(unsafe { codex_bundle_font_install(payload.as_ptr(), 1) }, 0);
    assert_eq!(unicode, bridge_render::render_bits(&source.to_string(), "", &bridge_render::Env::default()).unwrap());
    assert_eq!(unsafe { codex_bundle_font_reboot() }, 0);
    assert_eq!(unicode, bridge_render::render_bits(&source.to_string(), "", &bridge_render::Env::default()).unwrap());
    bridge_render::font_clear_assets();
}
