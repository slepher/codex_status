use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use bridge_ble::{lan_ip, BleConfig, Pusher};
use bridge_core::template::Library;
use clap::Parser;
use tokio::sync::RwLock;

#[derive(Parser, Debug)]
#[command(name = "bridge-ble", about = "Codex Status BLE central pusher")]
struct Args {
    /// Device name prefix to scan for.
    #[arg(long, default_value = "CodexStatus-")]
    name_prefix: String,
    /// Host advertised to the device (defaults to the detected LAN IP).
    #[arg(long)]
    host: Option<String>,
    /// HTTP port of the Wi-Fi channel (written as the device endpoint).
    #[arg(long, default_value = "8765")]
    port: u16,
    #[arg(long, default_value = "test-token-123")]
    token: String,
    #[arg(long, default_value = "tools/test-bridge/templates")]
    templates: PathBuf,
    /// Template ids to push (omitted = all in library, explicit full sync).
    #[arg(long, num_args = 0..)]
    template_ids: Vec<String>,
    /// Base URL of the running bridge-core HTTP server used as usage source.
    #[arg(long, default_value = "http://127.0.0.1:8765")]
    upstream: String,
    #[arg(long, default_value = "30")]
    interval: u64,
    /// Run a single push cycle and exit instead of looping.
    #[arg(long)]
    once: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let library = Library::load(&args.templates)?;
    tracing::info!("templates: {:?}", library.ids());

    let cfg = BleConfig {
        name_prefix: args.name_prefix.clone(),
        host: args.host.clone().unwrap_or_else(lan_ip),
        port: args.port,
        token: args.token.clone(),
        template_ids: if args.template_ids.is_empty() { None } else { Some(args.template_ids.clone()) },
        activate: None,
        scan_timeout_ms: 30000,
    };
    tracing::info!("endpoint advertised: {}:{}", cfg.host, cfg.port);

    let pusher = Pusher::new(cfg, Arc::new(RwLock::new(library)), args.upstream.clone());

    if args.once {
        let adapter = Pusher::adapter().await?;
        pusher.cycle_once(&adapter).await?;
        return Ok(());
    }
    pusher.run(Duration::from_secs(args.interval)).await
}
