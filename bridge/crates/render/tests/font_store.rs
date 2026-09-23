//! Device font store: `/fonts/<profile_id>/<font_id>.bin` on the real firmware
//! code (`src/font_store.cpp`) against the host LittleFS shim.
//!
//! These are the guarantees the transport depends on: an id already on the device
//! is never rewritten, a torn write never becomes a visible font, a damaged file
//! is reported instead of used, and replacing a Profile's font set removes
//! exactly the files the new set does not reference.

use bridge_render::{
    font_store_begin, font_store_clear, font_store_inventory, font_store_load,
    font_store_prune, font_store_put_raw, font_store_reset, font_store_set_write_budget,
    font_store_usage, font_store_write, FontStoreWrite,
};
use std::path::PathBuf;

/// The engine has one global store; tests must not interleave.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Serialize on the engine's single store. Poisoning is recovered from: a failing
/// test must not turn every other test into a spurious failure.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn asset(name: &str) -> Vec<u8> {
    let path = repo_root().join(format!("bridge/assets/fonts/font_{name}.bin"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for b in bytes {
        crc ^= *b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320u32 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

#[test]
fn first_push_stores_and_second_push_transfers_nothing() {
    let _guard = serial();
    font_store_reset();
    assert!(font_store_begin("70AABBCCDDEE"));
    let thin = asset("ntthin18");

    match font_store_write(&thin).unwrap() {
        FontStoreWrite::Stored { id, name } => {
            assert_eq!(id, "18c2e4ed");
            assert_eq!(name, "ntthin18");
        }
        other => panic!("first push must store, got {other:?}"),
    }
    let (bytes_after_first, count, _) = font_store_usage();
    assert_eq!(count, 1);
    assert_eq!(bytes_after_first, thin.len() as u32);

    // The id is a content address: the same container again is a no-op, so the
    // second push transfers zero bytes and does not rewrite the file.
    match font_store_write(&thin).unwrap() {
        FontStoreWrite::AlreadyPresent { id, .. } => assert_eq!(id, "18c2e4ed"),
        other => panic!("second push must be a no-op, got {other:?}"),
    }
    let (bytes_after_second, count, _) = font_store_usage();
    assert_eq!(count, 1);
    assert_eq!(bytes_after_second, bytes_after_first);

    let (loaded_id, name, len, crc) = font_store_load("18c2e4ed").unwrap();
    assert_eq!(loaded_id, "18c2e4ed");
    assert_eq!(name, "ntthin18");
    assert_eq!(len as usize, thin.len());
    assert_eq!(crc, crc32(&thin), "stored bytes must round trip exactly");
}

#[test]
fn a_torn_write_never_becomes_a_visible_font() {
    let _guard = serial();
    font_store_reset();
    assert!(font_store_begin("70AABBCCDDEE"));
    let thin = asset("ntthin18");
    let big = asset("ntreg64");

    font_store_write(&thin).unwrap();

    // Interrupt the write of a second font part way through.
    font_store_set_write_budget(1000);
    let err = font_store_write(&big).unwrap_err();
    font_store_set_write_budget(-1);
    assert!(err.contains("font_write"), "{err}");

    // The half-written file is not visible: inventory sees only the good font,
    // and the previously stored font is still byte-exact.
    let (count, bad, detail) = font_store_inventory();
    assert_eq!(count, 1, "{detail}");
    assert_eq!(bad, 0, "{detail}");
    let (_, _, len, crc) = font_store_load("18c2e4ed").unwrap();
    assert_eq!(len as usize, thin.len());
    assert_eq!(crc, crc32(&thin));
    // The staged temp file did not survive either.
    assert!(font_store_load("4dc3b226").is_err());
}

#[test]
fn a_damaged_or_misnamed_file_is_reported_not_used() {
    let _guard = serial();
    font_store_reset();
    assert!(font_store_begin("70AABBCCDDEE"));
    let thin = asset("ntthin18");

    // A file stored under the wrong id must not be used, and inventory must not
    // list it under its content id either: that would make the bridge believe the
    // id is installed while `load` rejects it.
    assert!(font_store_put_raw("deadbeef", &thin));
    let err = font_store_load("deadbeef").unwrap_err();
    assert!(err.contains("font_id_mismatch"), "{err}");
    let (count, bad, detail) = font_store_inventory();
    assert_eq!(count, 0, "{detail}");
    assert_eq!(bad, 1, "{detail}");

    // A truncated file is corrupt, not a font: inventory reports it separately so
    // the bridge can re-push that id. Both damaged files are still on disk (the
    // mis-named one above plus this truncated one), so both are counted.
    assert!(font_store_put_raw("18c2e4ed", &thin[..thin.len() / 3]));
    let (count, bad, detail) = font_store_inventory();
    assert_eq!(count, 0, "{detail}");
    assert_eq!(bad, 2, "{detail}");
}

#[test]
fn replacing_a_profile_font_set_prunes_exactly_the_unreferenced_files() {
    let _guard = serial();
    font_store_reset();
    assert!(font_store_begin("70AABBCCDDEE"));
    font_store_write(&asset("ntthin18")).unwrap();
    font_store_write(&asset("ntreg64")).unwrap();
    let (_, count, _) = font_store_usage();
    assert_eq!(count, 2);

    // Keep only the large face: the other one must go, and the kept one must stay
    // byte-exact (no cross-Profile sharing, no reference counting).
    let (removed, left, bytes) = font_store_prune(&["4dc3b226"]);
    assert_eq!(removed, 1);
    assert_eq!(left, 1);
    assert_eq!(bytes as usize, asset("ntreg64").len());

    let (count, bad, detail) = font_store_inventory();
    assert_eq!(count, 1, "{detail}");
    assert_eq!(bad, 0, "{detail}");
    assert!(font_store_load("18c2e4ed").is_err());
    let (_, name, _, crc) = font_store_load("4dc3b226").unwrap();
    assert_eq!(name, "ntreg64");
    assert_eq!(crc, crc32(&asset("ntreg64")));

    // Clearing the Profile removes the directory contents entirely.
    assert!(font_store_clear());
    let (bytes, count, _) = font_store_usage();
    assert_eq!((bytes, count), (0, 0));
}

#[test]
fn an_unbound_store_fails_closed() {
    let _guard = serial();
    font_store_reset();
    assert!(!font_store_begin("bad id!"));
    assert!(font_store_write(&asset("ntthin18")).is_err());
    let (bytes, count, profile) = font_store_usage();
    assert_eq!((bytes, count), (0, 0));
    assert_eq!(profile, "");
    // Both Profile directories are independent: switching Profile never exposes
    // the other Profile's fonts.
    assert!(font_store_begin("profileA"));
    font_store_write(&asset("ntthin18")).unwrap();
    assert!(font_store_begin("profileB"));
    let (count, _, _) = font_store_inventory();
    assert_eq!(count, 0);
    let (bytes, count, profile) = font_store_usage();
    assert_eq!((bytes, count, profile.as_str()), (0, 0, "profileB"));
    assert!(font_store_begin("profileA"));
    let (count, _, _) = font_store_inventory();
    assert_eq!(count, 1);
}
