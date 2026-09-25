//! Portable (green) file-system layout.
//!
//! The program is unpack-and-run: runtime data lives in a `data/` directory
//! next to the executable, seeds ship in `seed/` next to the executable (or in
//! the repository during development). Nothing is ever written to the repo by
//! the running program.

use std::path::{Path, PathBuf};

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Repository root: only used as a development fallback for seeds; found by
/// walking up from the executable for a source-tree marker.
pub fn repo_root() -> PathBuf {
    if let Ok(explicit) = std::env::var("CODEX_STATUS_ROOT") {
        return PathBuf::from(explicit);
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut probe = exe.parent().map(Path::to_path_buf);
        for _ in 0..5 {
            let Some(dir) = probe else { break };
            if dir.join("platformio.ini").is_file() || dir.join("artifacts").is_dir() {
                return dir;
            }
            probe = dir.parent().map(Path::to_path_buf);
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Runtime data directory: `<exe>/data` (portable); `CODEX_STATUS_DATA`
/// overrides it.
pub fn data_root() -> PathBuf {
    if let Ok(explicit) = std::env::var("CODEX_STATUS_DATA") {
        return PathBuf::from(explicit);
    }
    exe_dir().join("data")
}

/// Seed templates: `<exe>/seed/templates` when bundled, else the repo copy
/// (development). `CODEX_STATUS_SEEDS` overrides.
pub fn seed_templates() -> PathBuf {
    if let Ok(explicit) = std::env::var("CODEX_STATUS_SEEDS") {
        return PathBuf::from(explicit);
    }
    let bundled = exe_dir().join("seed/templates");
    if bundled.is_dir() {
        return bundled;
    }
    repo_root().join("tools/test-bridge/templates")
}
