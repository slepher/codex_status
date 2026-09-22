//! Atomic, bounded file persistence for platform state (`<exe>/data/platform`).
//!
//! Runtime data never lives in the repository; writes replace whole files so a
//! crash yields either the old or the new complete document.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub fn ensure_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(())
}

/// Atomic whole-file replace (same directory temp + rename).
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let tmp = tmp_path(path);
    fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    if let Err(e) = fs::rename(&tmp, path) {
        // Best effort cleanup so a failed replace does not leave litter.
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("replace {}", path.display()));
    }
    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "state".into());
    name.push_str(".tmp");
    path.with_file_name(name)
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    atomic_write(path, &bytes)
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value = serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(value))
}

/// Keep the newest `keep` entries of an append-only summary list.
pub fn prune<T>(list: &mut Vec<T>, keep: usize) {
    if list.len() > keep {
        let drop = list.len() - keep;
        list.drain(0..drop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn atomic_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b.json");
        write_json(&path, &json!({"x": 1})).unwrap();
        let back: serde_json::Value = read_json(&path).unwrap().unwrap();
        assert_eq!(back["x"], 1);
        write_json(&path, &json!({"x": 2})).unwrap();
        let back: serde_json::Value = read_json(&path).unwrap().unwrap();
        assert_eq!(back["x"], 2);
        assert!(!tmp_path(&path).exists());
    }

    #[test]
    fn missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let v: Option<serde_json::Value> = read_json(&dir.path().join("nope.json")).unwrap();
        assert!(v.is_none());
    }
}
