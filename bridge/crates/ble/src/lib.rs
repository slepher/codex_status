//! BLE central (btleplug) that mirrors the Wi-Fi channel for the device:
//! on connect it writes the LAN endpoint+token, pushes the current usage
//! envelope and pushes the template library (begin / chunks / end / activate).

use std::net::UdpSocket;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use btleplug::api::{
    Central, Manager as _, Peripheral as _, ScanFilter, ValueNotification, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use serde_json::json;
use tokio::sync::RwLock;
use uuid::Uuid;

use bridge_core::template::{encode_chunks, template_hash, Library};

pub const SVC_UUID: &str = "e7f1a000-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_ENDPOINT: &str = "e7f1a002-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_USAGE: &str = "e7f1a003-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_STATUS: &str = "e7f1a004-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_TPL_CTRL: &str = "e7f1a005-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_TPL_DATA: &str = "e7f1a006-4b2a-4c9e-9a11-3c0d5e9a0000";

pub const CHUNK_PAYLOAD: usize = 180;

#[derive(Debug, Clone)]
pub struct BleConfig {
    pub name_prefix: String,
    pub host: String,
    pub port: u16,
    pub token: String,
    /// Empty = push every template in the library.
    pub template_ids: Vec<String>,
}

/// Best-effort LAN IP used in the endpoint written over BLE.
pub fn lan_ip() -> String {
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

pub struct Pusher {
    cfg: BleConfig,
    library: Arc<RwLock<Library>>,
    upstream: String,
    client: reqwest::Client,
}

impl Pusher {
    pub fn new(cfg: BleConfig, library: Arc<RwLock<Library>>, upstream: String) -> Self {
        Self {
            cfg,
            library,
            upstream,
            client: reqwest::Client::new(),
        }
    }

    fn uuid(s: &str) -> Uuid {
        Uuid::parse_str(s).expect("static uuid")
    }

    pub async fn adapter() -> Result<Adapter> {
        let manager = Manager::new().await.context("bluetooth manager")?;
        manager
            .adapters()
            .await
            .context("bluetooth adapters")?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no bluetooth adapter"))
    }

    async fn wait_for_device(adapter: &Adapter, prefix: &str, timeout: Duration) -> Result<Peripheral> {
        adapter
            .start_scan(ScanFilter::default())
            .await
            .context("start scan")?;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if tokio::time::Instant::now() >= deadline {
                let _ = adapter.stop_scan().await;
                bail!("device {prefix}* not found");
            }
            for peripheral in adapter.peripherals().await? {
                let props = peripheral.properties().await?;
                let name = props.and_then(|p| p.local_name).unwrap_or_default();
                if name.starts_with(prefix) {
                    let _ = adapter.stop_scan().await;
                    return Ok(peripheral);
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    }

    async fn fetch_usage(&self) -> Result<serde_json::Value> {
        let url = format!("{}/usage", self.upstream.trim_end_matches('/'));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.cfg.token)
            .send()
            .await
            .context("GET upstream /usage")?;
        if !resp.status().is_success() {
            bail!("upstream /usage -> {}", resp.status());
        }
        Ok(resp.json().await?)
    }

    async fn write_char(
        peripheral: &Peripheral,
        uuid: &str,
        data: &[u8],
    ) -> Result<()> {
        let target = peripheral
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == Self::uuid(uuid))
            .ok_or_else(|| anyhow!("characteristic {uuid} missing"))?;
        peripheral
            .write(&target, data, WriteType::WithResponse)
            .await
            .with_context(|| format!("write {uuid}"))
    }

    async fn log_notifications(peripheral: &Peripheral) -> Result<tokio::task::JoinHandle<()>> {
        let status = peripheral
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == Self::uuid(CHR_STATUS))
            .ok_or_else(|| anyhow!("status characteristic missing"))?;
        peripheral
            .subscribe(&status)
            .await
            .context("subscribe status")?;
        let mut stream = peripheral.notifications().await?;
        let handle = tokio::spawn(async move {
            while let Some(ValueNotification { uuid, value, .. }) = stream.next().await {
                if uuid == Uuid::parse_str(CHR_STATUS).unwrap() {
                    tracing::info!(
                        "[status] {}",
                        String::from_utf8_lossy(&value)
                    );
                }
            }
        });
        Ok(handle)
    }

    async fn push_endpoint(&self, peripheral: &Peripheral) -> Result<()> {
        let payload = json!({
            "schema": 1,
            "host": self.cfg.host,
            "port": self.cfg.port,
            "token": self.cfg.token,
        });
        Self::write_char(peripheral, CHR_ENDPOINT, payload.to_string().as_bytes()).await
    }

    async fn push_usage(&self, peripheral: &Peripheral) -> Result<()> {
        match self.fetch_usage().await {
            Ok(usage) => {
                Self::write_char(peripheral, CHR_USAGE, usage.to_string().as_bytes()).await?;
                tracing::info!("usage pushed over BLE");
                Ok(())
            }
            Err(e) => {
                tracing::warn!("usage fetch failed ({e}); pushing templates only");
                Ok(())
            }
        }
    }

    async fn push_template(&self, peripheral: &Peripheral, id: &str, bytes: &[u8], version: u64) -> Result<()> {
        let crc = crc32fast::hash(bytes);
        let hash = template_hash(bytes);
        let begin = json!({
            "op": "begin",
            "id": id,
            "version": version,
            "hash": hash,
            "len": bytes.len(),
            "crc": crc,
        });
        Self::write_char(peripheral, CHR_TPL_CTRL, begin.to_string().as_bytes()).await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        for chunk in encode_chunks(bytes, CHUNK_PAYLOAD) {
            Self::write_char(peripheral, CHR_TPL_DATA, &chunk).await?;
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        Self::write_char(peripheral, CHR_TPL_CTRL, br#"{"op":"end"}"#).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let activate = json!({"op": "activate", "id": id});
        Self::write_char(peripheral, CHR_TPL_CTRL, activate.to_string().as_bytes()).await?;
        tracing::info!("template {id} pushed and activated ({} bytes)", bytes.len());
        Ok(())
    }

    async fn push_templates(&self, peripheral: &Peripheral) -> Result<()> {
        let items: Vec<(String, Vec<u8>, u64)> = {
            let library = self.library.read().await;
            let ids: Vec<String> = if self.cfg.template_ids.is_empty() {
                library.ids()
            } else {
                self.cfg.template_ids.clone()
            };
            ids.into_iter()
                .filter_map(|id| {
                    library
                        .get(&id)
                        .map(|e| (id.clone(), e.bytes.clone(), e.version))
                })
                .collect()
        };
        for (id, bytes, version) in items {
            if let Err(e) = self.push_template(peripheral, &id, &bytes, version).await {
                tracing::warn!("push template {id}: {e}");
            }
        }
        Ok(())
    }

    /// One connect → push → disconnect cycle.
    pub async fn cycle_once(&self, adapter: &Adapter) -> Result<()> {
        let peripheral = Self::wait_for_device(adapter, &self.cfg.name_prefix, Duration::from_secs(30)).await?;
        let props = peripheral.properties().await?;
        let name = props
            .and_then(|p| p.local_name)
            .unwrap_or_else(|| self.cfg.name_prefix.clone());
        tracing::info!("connecting {name} ({})", peripheral.address());
        peripheral.connect().await.context("connect")?;
        peripheral.discover_services().await.context("discover")?;

        let notify_handle = Self::log_notifications(&peripheral).await.ok();

        let result = async {
            self.push_endpoint(&peripheral).await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            self.push_usage(&peripheral).await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            self.push_templates(&peripheral).await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;

        tokio::time::sleep(Duration::from_secs(1)).await;
        if let Some(handle) = notify_handle {
            handle.abort();
        }
        let _ = peripheral.disconnect().await;
        result
    }

    /// Scan/connect/retry loop.
    pub async fn run(&self, interval: Duration) -> Result<()> {
        loop {
            let adapter = Self::adapter().await?;
            match self.cycle_once(&adapter).await {
                Ok(()) => tracing::info!("BLE cycle done"),
                Err(e) => tracing::warn!("BLE cycle failed: {e}"),
            }
            tokio::time::sleep(interval).await;
        }
    }
}
