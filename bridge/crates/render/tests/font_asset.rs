//! Device-side font asset parser vs the shipped containers.
//!
//! `src/font_asset.cpp` is the code the device runs; these tests feed it the very
//! containers the generator writes and the bridge serves, so a format drift
//! between `tools/note4-fonts/rasterize_ttf.py` and the firmware shows up here
//! instead of on the panel.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn asset(name: &str) -> Vec<u8> {
    let path = repo_root().join(format!("bridge/assets/fonts/font_{name}.bin"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn shipped_containers_validate_with_the_device_parser() {
    let thin = bridge_render::font_asset_check(&asset("ntthin18"))
        .unwrap_or_else(|e| panic!("ntthin18 rejected: {e}"));
    assert!(thin.contains("id=18c2e4ed"), "{thin}");
    assert!(thin.contains("name=ntthin18"), "{thin}");
    assert!(thin.contains("coverage=ascii"), "{thin}");
    assert!(thin.contains("size=18 weight=100 bpp=1 pf=bw"), "{thin}");
    assert!(thin.contains("line=26 base=6"), "{thin}");
    assert!(thin.contains("filled=95"), "{thin}");
    // The view must be buildable in place: the descriptor table is aligned.
    assert!(thin.contains("view=1"), "{thin}");

    let big = bridge_render::font_asset_check(&asset("ntreg64"))
        .unwrap_or_else(|e| panic!("ntreg64 rejected: {e}"));
    assert!(big.contains("id=4dc3b226"), "{big}");
    assert!(big.contains("name=ntreg64"), "{big}");
    assert!(big.contains("coverage=digits"), "{big}");
    assert!(big.contains("size=64 weight=400 bpp=1 pf=bw"), "{big}");
    assert!(big.contains("line=88 base=19"), "{big}");
    assert!(big.contains("filled=17"), "{big}");
    assert!(big.contains("view=1"), "{big}");
}

#[test]
fn corrupted_containers_are_rejected_with_a_reason() {
    let good = asset("ntthin18");

    // A flipped payload byte is caught by the payload CRC.
    let mut flipped = good.clone();
    let n = flipped.len();
    flipped[n - 1] ^= 0x55;
    let err = bridge_render::font_asset_check(&flipped).unwrap_err();
    assert!(err.contains("font_crc"), "{err}");

    // Truncation is caught by the declared length / payload range.
    let err = bridge_render::font_asset_check(&good[..good.len() / 2]).unwrap_err();
    assert!(err.contains("font_length") || err.contains("font_payload"),
            "{err}");

    // A wrong magic is rejected before anything else.
    let mut magic = good.clone();
    magic[0] = b'X';
    let err = bridge_render::font_asset_check(&magic).unwrap_err();
    assert!(err.contains("font_magic"), "{err}");

    // An unsupported pixel format/bpp combination is rejected explicitly: a
    // gray4 container must never be rendered by a 1bpp target.
    let mut gray = good.clone();
    gray[25] = 1;   // pixelFormat = gray4
    // The payload CRC covers only the payload, so the header change survives the
    // CRC check and must be caught by the format rule.
    let err = bridge_render::font_asset_check(&gray).unwrap_err();
    assert!(err.contains("font_bpp_format") || err.contains("font_pixel_format"),
            "{err}");

    // A section length that disagrees with the alignment rule shifts the
    // descriptor table, so the declared payload no longer adds up to the file
    // length. The container must be rejected instead of having its glyph table
    // read one byte off.
    let mut odd = good.clone();
    odd[34] = 9;    // nameLen 8 -> 9: the reader pads it, the writer did not
    let err = bridge_render::font_asset_check(&odd).unwrap_err();
    assert!(err.contains("font_payload_range") || err.contains("font_payload_length")
            || err.contains("font_crc") || err.contains("font_align")
            || err.contains("font_name"), "{err}");
}

#[test]
fn every_emitted_container_has_a_distinct_content_address() {
    let thin = bridge_render::font_asset_check(&asset("ntthin18")).unwrap();
    let thin_again = bridge_render::font_asset_check(&asset("ntthin18")).unwrap();
    assert_eq!(thin, thin_again, "the id must be a pure function of the bytes");

    // Any single-byte change to the *payload* changes the identity, which is what
    // makes a font immutable and an update always a new id.
    let mut changed = asset("ntthin18");
    let n = changed.len();
    changed[n - 1] ^= 0x01;
    assert!(bridge_render::font_asset_check(&changed).is_err());
    let mut with_slack = asset("ntthin18");
    with_slack.push(0);
    // Declared length no longer matches the file: rejected, not silently accepted.
    assert!(bridge_render::font_asset_check(&with_slack).is_err());
}

#[test]
fn profile_font_asset_changes_the_shared_engine_pixels() {
    let extra_light = std::fs::read(repo_root().join(
        "bridge/assets/fonts/font_ntthin18-extralight200.bin"
    )).unwrap();
    let info = bridge_render::font_asset_check(&extra_light).unwrap();
    assert!(info.contains("name=ntthin18"), "{info}");
    assert!(info.contains("weight=200"), "{info}");
    let template = std::fs::read_to_string(repo_root().join(
        "bridge/crates/core/tests/fixtures/codex-status-a-400x300.json"
    )).unwrap();
    let before = bridge_render::render_bits(&template, "", &bridge_render::Env::default()).unwrap();
    let after = bridge_render::render_bits_with_fonts(&template, "", &bridge_render::Env::default(), vec![extra_light]).unwrap();
    assert_ne!(before, after);
    assert_eq!(before, bridge_render::render_bits(&template, "", &bridge_render::Env::default()).unwrap());
}
