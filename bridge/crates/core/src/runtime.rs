//! Reusable app-server polling loop shared by `bridge-core` and the Tauri app.
//!
//! Resilient by design (docs/power-state.md §9): an app-server exit never kills
//! the caller, and a failed `codex.exe` spawn re-runs path discovery because
//! Codex auto-upgrades move the binary to a new directory.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::codex::{locate_codex, CodexClient};
use crate::envelope::{account_username, build_envelope, EnvelopeOptions};
use crate::template::Library;

#[derive(Clone)]
pub struct PollerConfig {
    /// Known-good app-server path; `None` triggers discovery on the first pass.
    pub exe: Option<PathBuf>,
    /// Explicit override (`CODEX_STATUS_CODEX` / config) used for re-discovery.
    pub codex_override: Option<PathBuf>,
    pub host_id: String,
    pub bridge_host: String,
    pub bridge_port: u16,
    pub interval_secs: u64,
    pub templates: Arc<RwLock<Library>>,
    /// active hold window in seconds (default 600).
    pub active_hold_seconds: u64,
}

pub async fn run_poller(cfg: PollerConfig, envelope: Arc<RwLock<Option<Value>>>) -> Result<()> {
    let mut backoff = 1u64;
    let mut exe = cfg.exe.clone();
    loop {
        if exe.is_none() {
            match locate_codex(cfg.codex_override.as_deref()) {
                Ok(found) => {
                    tracing::info!("codex cli: {}", found.display());
                    exe = Some(found);
                }
                Err(e) => {
                    tracing::warn!("locate codex: {e}");
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(30);
                    continue;
                }
            }
        }
        let path = exe.clone().expect("codex path");
        match CodexClient::spawn(&path).await {
            Ok(mut client) => {
                match client.initialize().await {
                    Ok(_) => {
                        let label = match client.read_account().await {
                            Ok(account) => account_username(&account),
                            Err(e) => {
                                tracing::warn!("account/read: {e}");
                                None
                            }
                        };
                        backoff = 1;
                        loop {
                            match client.read_rate_limits().await {
                                Ok(rate_limits) => {
                                    let template_refs =
                                        cfg.templates.read().await.template_refs();
                                    let opts = EnvelopeOptions {
                                        bridge_label: label.clone(),
                                        bridge_host_id: cfg.host_id.clone(),
                                        bridge_host: cfg.bridge_host.clone(),
                                        bridge_port: cfg.bridge_port,
                                        next_sync_seconds: cfg.interval_secs,
                                        templates: template_refs,
                                        active_hold_seconds: cfg.active_hold_seconds,
                                    };
                                    *envelope.write().await =
                                        Some(build_envelope(&rate_limits, &opts));
                                    tracing::info!("usage refreshed");
                                }
                                Err(e) => {
                                    tracing::warn!("read_rate_limits: {e}");
                                    break;
                                }
                            }
                            tokio::time::sleep(Duration::from_secs(cfg.interval_secs)).await;
                        }
                    }
                    Err(e) => tracing::warn!("initialize: {e}"),
                }
                client.kill();
            }
            Err(e) => tracing::warn!("spawn codex: {e}"),
        }
        // The app-server died or never started: re-discover in case Codex was
        // upgraded into a new directory between spawns.
        match locate_codex(cfg.codex_override.as_deref()) {
            Ok(found) => {
                if exe.as_ref() != Some(&found) {
                    tracing::info!("codex path changed: {} -> {}", path.display(), found.display());
                }
                exe = Some(found);
            }
            Err(e) => {
                tracing::warn!("re-locate codex: {e}");
                exe = None;
            }
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}
