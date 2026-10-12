use std::time::Duration;

use anyhow::{Context, Result};
use bridge_ble::{lan_ip, DeviceConnection};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "bridge-ble", about = "Codex Status BLE endpoint handoff")]
struct Args {
    /// Registered device Wi-Fi MAC; verified before endpoint writes.
    #[arg(long)]
    mac: String,
    /// Host advertised to the device (defaults to the detected LAN IP).
    #[arg(long)]
    host: Option<String>,
    /// HTTP port of the Wi-Fi channel (written as the device endpoint).
    #[arg(long, default_value = "8765")]
    port: u16,
    #[arg(long)]
    token: String,
    #[arg(long, default_value = "30")]
    interval: u64,
    /// Stable principal for this endpoint; use the existing Bridge installation ID.
    #[arg(long)]
    bridge_id: String,
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
    let host = args.host.clone().unwrap_or_else(lan_ip);
    tracing::info!("endpoint advertised: {}:{}", host, args.port);
    loop {
        let mut link = DeviceConnection::connect(&args.mac, &args.token, &args.bridge_id).await?
            .context("registered device is not advertising; open its BLE window")?;
        let result = async {
            link.write_endpoint(&host, args.port, &args.token).await?;
            let status = link.command("status", serde_json::json!({})).await?;
            anyhow::ensure!(status["result"] == "applied", "device did not confirm the current protocol session");
            Ok::<(), anyhow::Error>(())
        }.await;
        link.close().await;
        result?;
        tracing::info!(device_mac = %args.mac, "endpoint handoff confirmed");
        if args.once { return Ok(()); }
        tokio::time::sleep(Duration::from_secs(args.interval.max(1))).await;
    }
}
