use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use bridge_core::codex::locate_codex;
use bridge_core::envelope::{build_envelope, EnvelopeOptions};
use bridge_core::http::serve;
use bridge_core::runtime::{run_poller, PollerConfig};
use bridge_core::template::Library;
use bridge_core::short_id;
use clap::Parser;
use tokio::sync::RwLock;

#[derive(Parser, Debug)]
#[command(name = "bridge-core", about = "Codex Status bridge core (Wi-Fi HTTP + app-server)")]
struct Args {
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,
    #[arg(long, default_value = "8765")]
    port: u16,
    #[arg(long, default_value = "tools/test-bridge/templates")]
    templates: PathBuf,
    #[arg(long)]
    codex_path: Option<PathBuf>,
    #[arg(long, default_value = "180")]
    interval: u64,
    /// Fetch once, print the envelope, exit (no HTTP server).
    #[arg(long)]
    once: bool,
}

fn host_label() -> String {
    let raw = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "bridge".to_string());
    // Device fonts are ASCII-only; replace anything else so rendering stays safe.
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '?' })
        .take(16)
        .collect();
    if cleaned.is_empty() {
        "bridge".to_string()
    } else {
        cleaned
    }
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
    let exe = locate_codex(args.codex_path.as_deref())?;
    tracing::info!("codex cli: {}", exe.display());

    let library = Arc::new(RwLock::new(Library::load(&args.templates)?));
    tracing::info!("templates: {:?}", library.read().await.ids());

    let envelope: Arc<RwLock<Option<serde_json::Value>>> = Arc::new(RwLock::new(None));
    let host_id = short_id(&host_label());

    if args.once {
        let mut client = bridge_core::codex::CodexClient::spawn(&exe).await?;
        client.initialize().await?;
        let rate_limits = client.read_rate_limits().await?;
        let label = client
            .read_account()
            .await
            .ok()
            .and_then(|account| bridge_core::envelope::account_username(&account));
        let opts = EnvelopeOptions {
            bridge_label: label,
            bridge_host_id: host_id.clone(),
            bridge_host: bridge_core::lan_ip(),
            bridge_port: args.port,
            next_sync_seconds: args.interval,
            templates: library.read().await.template_refs(),
            active_hold_seconds: 600,
        };
        println!("{}", serde_json::to_string_pretty(&build_envelope(&rate_limits, &opts))?);
        client.kill();
        return Ok(());
    }

    let addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    let activity = Arc::new(bridge_core::activity::Activity::new());
    tokio::spawn(async move {
        if let Err(e) = serve(addr).await {
            tracing::error!("http server: {e}");
        }
    });

    let poller = PollerConfig {
        exe: Some(exe),
        codex_override: args.codex_path.clone(),
        host_id: host_id.clone(),
        bridge_host: bridge_core::lan_ip(),
        bridge_port: args.port,
        interval_secs: args.interval,
        templates: library.clone(),
        active_hold_seconds: 600,
        activity,
    };
    run_poller(poller, envelope).await
}
