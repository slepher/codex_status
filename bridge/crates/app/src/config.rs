use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub port: u16,
    pub token: String,
    /// Runtime working copies edited by the panel/MCP (never the repo).
    pub templates: PathBuf,
    /// Seed templates shipped in the repo, copied into `templates` on first run.
    pub seeds: PathBuf,
    pub interval_secs: u64,
    pub ble_interval_secs: u64,
    pub mcp_port: u16,
    /// Last known device address (attribute, not identity).
    pub device_ip: String,
    /// Device identity: Wi-Fi MAC learned from UDP/HTTP/BLE, persisted.
    pub device_mac: Option<String>,
    /// Editable display name (not a uniqueness key; defaults to
    /// `CodexStatus-<MAC suffix>` on first identification).
    pub device_name: String,
    /// Bridge display name reported in /claim (defaults to the host name).
    pub bridge_name: String,
    pub codex_path: Option<PathBuf>,
    pub autostart: bool,
    /// active hold window in seconds.
    pub active_hold_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        let data = bridge_core::paths::data_root();
        Self {
            port: 8765,
            token: "test-token-123".to_string(),
            templates: data.join("templates"),
            seeds: bridge_core::paths::seed_templates(),
            interval_secs: 180,
            ble_interval_secs: 300,
            mcp_port: 8766,
            device_ip: "192.168.1.50".to_string(),
            device_mac: None,
            device_name: String::new(),
            bridge_name: String::new(),
            codex_path: None,
            autostart: false,
            active_hold_seconds: 600,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        let mut cfg = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Config>(&raw).ok())
            .unwrap_or_default();
        cfg.apply_env();
        cfg
    }

    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("CODEX_STATUS_PORT") {
            if let Ok(port) = v.parse() {
                self.port = port;
            }
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_TOKEN") {
            self.token = v;
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_TEMPLATES") {
            self.templates = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_SEEDS") {
            self.seeds = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_INTERVAL") {
            if let Ok(secs) = v.parse() {
                self.interval_secs = secs;
            }
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_BLE_INTERVAL") {
            if let Ok(secs) = v.parse() {
                self.ble_interval_secs = secs;
            }
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_MCP_PORT") {
            if let Ok(port) = v.parse() {
                self.mcp_port = port;
            }
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_DEVICE_IP") {
            self.device_ip = v;
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_DEVICE_MAC") {
            self.device_mac = Some(v);
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_DEVICE_NAME") {
            self.device_name = v;
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_BRIDGE_NAME") {
            self.bridge_name = v;
        }
        if let Ok(v) = std::env::var("CODEX_STATUS_CODEX_PATH") {
            self.codex_path = Some(PathBuf::from(v));
        }
    }
}

/// Repo root for default paths: use the config file location when present,
/// otherwise fall back to the current working directory.
pub fn repo_root() -> PathBuf {
    if let Ok(explicit) = std::env::var("CODEX_STATUS_ROOT") {
        return PathBuf::from(explicit);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("artifacts").is_dir() {
        return cwd;
    }
    if let Ok(exe) = std::env::current_exe() {
        // target/debug/bridge-app.exe -> repo root
        let mut probe = exe.parent().map(Path::to_path_buf);
        for _ in 0..4 {
            let Some(dir) = probe else { break };
            if dir.join("artifacts").is_dir() {
                return dir;
            }
            probe = dir.parent().map(Path::to_path_buf);
        }
    }
    cwd
}

pub fn config_path(_root: &Path) -> PathBuf {
    std::env::var("CODEX_STATUS_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| bridge_core::paths::data_root().join("bridge-app.json"))
}
