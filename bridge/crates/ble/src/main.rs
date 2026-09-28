use std::time::Duration;

use anyhow::Result;
use bridge_ble::{lan_ip, BleConfig, Pusher};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "bridge-ble", about = "Codex Status BLE endpoint handoff")]
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
    #[arg(long, default_value = "30")]
    interval: u64,
    /// Run a single endpoint handoff and exit instead of looping.
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
    let cfg = BleConfig {
        name_prefix: args.name_prefix.clone(),
        host: args.host.clone().unwrap_or_else(lan_ip),
        port: args.port,
        token: args.token.clone(),
        scan_timeout_ms: 30000,
    };
    tracing::info!("endpoint advertised: {}:{}", cfg.host, cfg.port);

    let pusher = Pusher::new(cfg);

    if args.once {
        let adapter = Pusher::adapter().await?;
        let info = pusher.cycle_once(&adapter).await?;
        tracing::info!("device info: {info}");
        return Ok(());
    }
    pusher.run(Duration::from_secs(args.interval)).await
}
