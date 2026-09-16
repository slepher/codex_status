//! Reusable app-server polling loop shared by `bridge-core` and the Tauri app.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::codex::CodexClient;
use crate::envelope::{account_username, build_envelope, EnvelopeOptions};
use crate::template::Library;

#[derive(Clone)]
pub struct PollerConfig {
    pub exe: PathBuf,
    pub host_id: String,
    pub interval_secs: u64,
    pub templates: Arc<RwLock<Library>>,
}

pub async fn run_poller(cfg: PollerConfig, envelope: Arc<RwLock<Option<Value>>>) -> Result<()> {
    let mut backoff = 1u64;
    loop {
        match CodexClient::spawn(&cfg.exe).await {
            Ok(mut client) => {
                if let Err(e) = client.initialize().await {
                    tracing::warn!("initialize: {e}");
                }
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
                            let template_refs = cfg.templates.read().await.template_refs();
                            let opts = EnvelopeOptions {
                                bridge_label: label.clone(),
                                bridge_host_id: cfg.host_id.clone(),
                                next_sync_seconds: cfg.interval_secs,
                                templates: template_refs,
                            };
                            *envelope.write().await = Some(build_envelope(&rate_limits, &opts));
                            tracing::info!("usage refreshed");
                        }
                        Err(e) => {
                            tracing::warn!("read_rate_limits: {e}");
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(cfg.interval_secs)).await;
                }
                client.kill();
            }
            Err(e) => tracing::warn!("spawn codex: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}
