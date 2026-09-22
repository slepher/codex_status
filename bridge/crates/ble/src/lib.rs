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
pub const CHR_INFO: &str = "e7f1a001-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_ENDPOINT: &str = "e7f1a002-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_USAGE: &str = "e7f1a003-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_STATUS: &str = "e7f1a004-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_TPL_CTRL: &str = "e7f1a005-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_TPL_DATA: &str = "e7f1a006-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_AUTH: &str = "e7f1a007-4b2a-4c9e-9a11-3c0d5e9a0000";

pub const JSON_WRITE_LIMIT: usize = 180;
pub const CHUNK_PAYLOAD: usize = JSON_WRITE_LIMIT - 2;

#[derive(Debug, Clone)]
pub struct BleConfig {
    pub name_prefix: String,
    pub host: String,
    pub port: u16,
    pub token: String,
    /// Empty = push every template in the library.
    /// `None` pushes every template (explicit full sync), `Some(vec![])` pushes
    /// none (periodic sync), `Some(list)` pushes exactly those.
    pub template_ids: Option<Vec<String>>,
    /// Template to activate after an explicit push (profile's active choice).
    pub activate: Option<String>,
    /// Scan window per cycle. Long for one-shot tools, short (5 s) for the
    /// low-duty tray loop.
    pub scan_timeout_ms: u64,
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

    /// One scan pass for the device prefix. `Ok(None)` means it never
    /// advertised inside the timeout (no connection was attempted).
    async fn find_device(
        adapter: &Adapter,
        prefix: &str,
        timeout: Duration,
    ) -> Result<Option<Peripheral>> {
        adapter
            .start_scan(ScanFilter::default())
            .await
            .context("start scan")?;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if tokio::time::Instant::now() >= deadline {
                let _ = adapter.stop_scan().await;
                return Ok(None);
            }
            for peripheral in adapter.peripherals().await? {
                let props = peripheral.properties().await?;
                let name = props.and_then(|p| p.local_name).unwrap_or_default();
                if name.starts_with(prefix) {
                    let _ = adapter.stop_scan().await;
                    return Ok(Some(peripheral));
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    }

    async fn wait_for_device(adapter: &Adapter, prefix: &str, timeout: Duration) -> Result<Peripheral> {
        Self::find_device(adapter, prefix, timeout)
            .await?
            .with_context(|| format!("device {prefix}* not found"))
    }

    fn peer_bonded(info: &serde_json::Value) -> bool {
        info.get("peerBonded").and_then(|v| v.as_bool()) == Some(true)
    }

    async fn read_info(peripheral: &Peripheral) -> Result<serde_json::Value> {
        let target = peripheral
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == Self::uuid(CHR_INFO))
            .ok_or_else(|| anyhow!("info characteristic missing"))?;
        let raw = peripheral.read(&target).await.context("read device info")?;
        let info: serde_json::Value = serde_json::from_slice(&raw).context("parse device info")?;
        if !Self::peer_bonded(&info) {
            bail!("device is not bonded; pair manually in Windows Bluetooth settings: hold BOOT for 2 seconds to open the 120 second pairing window, connect CodexStatus, then retry")
        }
        Ok(info)
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

    fn fragment_payload(data: &[u8], limit: usize) -> Vec<Vec<u8>> {
        assert!(limit > 0);
        data.chunks(limit).map(|chunk| chunk.to_vec()).collect()
    }

    async fn write_json(peripheral: &Peripheral, uuid: &str, data: &[u8]) -> Result<()> {
        for chunk in Self::fragment_payload(data, JSON_WRITE_LIMIT) {
            Self::write_char(peripheral, uuid, &chunk).await?;
        }
        Ok(())
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
        Self::write_json(peripheral, CHR_ENDPOINT, payload.to_string().as_bytes()).await
    }

    async fn push_usage(&self, peripheral: &Peripheral) -> Result<()> {
        match self.fetch_usage().await {
            Ok(usage) => {
                Self::write_json(peripheral, CHR_USAGE, usage.to_string().as_bytes()).await?;
                tracing::info!("usage pushed over BLE");
                Ok(())
            }
            Err(e) => {
                tracing::warn!("usage fetch failed ({e}); pushing templates only");
                Ok(())
            }
        }
    }

    async fn push_template(&self, peripheral: &Peripheral, id: &str, bytes: &[u8], version: u64, activate: bool) -> Result<()> {
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
        Self::write_json(peripheral, CHR_TPL_CTRL, begin.to_string().as_bytes()).await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        for chunk in encode_chunks(bytes, CHUNK_PAYLOAD) {
            Self::write_char(peripheral, CHR_TPL_DATA, &chunk).await?;
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        Self::write_json(peripheral, CHR_TPL_CTRL, br#"{"op":"end"}"#).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        if activate {
            let activate = json!({"op": "activate", "id": id});
            Self::write_json(peripheral, CHR_TPL_CTRL, activate.to_string().as_bytes()).await?;
            tracing::info!("template {id} pushed and activated ({} bytes)", bytes.len());
        } else {
            tracing::info!("template {id} pushed, not activated ({} bytes)", bytes.len());
        }
        Ok(())
    }

    async fn push_templates(&self, peripheral: &Peripheral, info: &serde_json::Value) -> Result<()> {
        if matches!(&self.cfg.template_ids, Some(list) if list.is_empty()) {
            tracing::info!("no template push requested; templates left untouched");
            return Ok(());
        }
        let items: Vec<(String, Vec<u8>, u64)> = {
            let library = self.library.read().await;
            let ids: Vec<String> = match &self.cfg.template_ids {
                None => library.ids(),
                Some(list) => list.clone(),
            };
            ids.into_iter()
                .filter_map(|id| {
                    library
                        .get(&id)
                        .map(|e| (id.clone(), e.bytes.clone(), e.version))
                })
                .collect()
        };
        let device = info.get("templates").and_then(|v| v.as_array());
        let mut pushed: Vec<String> = Vec::new();
        for (id, bytes, version) in items {
            let hash = template_hash(&bytes);
            let up_to_date = device
                .map(|arr| {
                    arr.iter().any(|t| {
                        t.get("id").and_then(|v| v.as_str()) == Some(id.as_str())
                            && t.get("hash").and_then(|v| v.as_str()) == Some(hash.as_str())
                    })
                })
                .unwrap_or(false);
            if up_to_date {
                tracing::info!("template {id} up to date ({hash}); skip");
                continue;
            }
            if let Err(e) = self.push_template(peripheral, &id, &bytes, version, false).await {
                tracing::warn!("push template {id}: {e}");
                continue;
            }
            pushed.push(id);
        }
        // Template pushes are explicit user/agent actions: show the profile's
        // chosen template (or the last pushed one).
        if let Some(target) = self.cfg.activate.clone().or_else(|| pushed.last().cloned()) {
            let activate = json!({"op": "activate", "id": target});
            Self::write_json(peripheral, CHR_TPL_CTRL, activate.to_string().as_bytes()).await?;
            tracing::info!("activated template {target}");
        } else {
            tracing::info!("no template changes to push; device left as-is");
        }
        Ok(())
    }

    /// Connect to an advertising device and read its info JSON
    /// (`{schema,model,fw,proto,mac,ip,http_port,templates,...}`). Used by the
    /// explicit `device_discover via=ble` fallback; no usage/template writes.
    pub async fn read_device_info(
        adapter: &Adapter,
        name_prefix: &str,
        scan_timeout_ms: u64,
    ) -> Result<serde_json::Value> {
        let scan = Duration::from_millis(scan_timeout_ms.max(1000));
        let peripheral = Self::wait_for_device(adapter, name_prefix, scan).await?;
        tracing::info!("connecting {} for device info", peripheral.address());
        peripheral.connect().await.context("connect")?;
        peripheral.discover_services().await.context("discover")?;
        let info = Self::read_info(&peripheral).await;
        if let Ok(info) = &info {
            tracing::info!("device info: {info}");
        }
        let _ = peripheral.disconnect().await;
        info
    }

    /// Connect to an advertising device (BLE session on, e.g. after a BOOT
    /// click) and request the OTA/Wi-Fi operation token over the bonded auth
    /// characteristic. The token is disclosed only over this encrypted link.
    pub async fn request_device_token(
        adapter: &Adapter,
        name_prefix: &str,
        scan_timeout_ms: u64,
    ) -> Result<String> {
        let scan = Duration::from_millis(scan_timeout_ms.max(1000));
        let peripheral = Self::wait_for_device(adapter, name_prefix, scan).await?;
        tracing::info!("connecting {} for device token", peripheral.address());
        peripheral.connect().await.context("connect")?;
        peripheral.discover_services().await.context("discover")?;
        if let Err(e) = Self::read_info(&peripheral).await {
            let _ = peripheral.disconnect().await;
            return Err(e);
        }
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
        Self::write_json(&peripheral, CHR_AUTH, br#"{"cmd":"token"}"#).await?;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let mut token: Option<String> = None;
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, stream.next()).await {
                Ok(Some(ValueNotification { uuid, value, .. })) => {
                    if uuid != Self::uuid(CHR_STATUS) {
                        continue;
                    }
                    let Ok(doc) = serde_json::from_slice::<serde_json::Value>(&value) else {
                        continue;
                    };
                    if doc.get("ack").and_then(|v| v.as_str()) != Some("auth") {
                        continue;
                    }
                    if let Some(value) = doc.get("token").and_then(|v| v.as_str()) {
                        token = Some(value.to_string());
                    }
                    break;
                }
                Ok(None) | Err(_) => break,
            }
        }
        let _ = peripheral.disconnect().await;
        token.ok_or_else(|| anyhow!("device token response timed out"))
    }

    /// One connect → push → disconnect cycle. Returns the device info JSON so
    /// the caller can adopt its identity (mac/ip) when needed.
    pub async fn cycle_once(&self, adapter: &Adapter) -> Result<serde_json::Value> {
        let scan = Duration::from_millis(self.cfg.scan_timeout_ms.max(1000));
        let peripheral = Self::wait_for_device(adapter, &self.cfg.name_prefix, scan).await?;
        let props = peripheral.properties().await?;
        let name = props
            .and_then(|p| p.local_name)
            .unwrap_or_else(|| self.cfg.name_prefix.clone());
        tracing::info!("connecting {name} ({})", peripheral.address());
        peripheral.connect().await.context("connect")?;
        peripheral.discover_services().await.context("discover")?;
        let info = match Self::read_info(&peripheral).await {
            Ok(info) => info,
            Err(e) => {
                let _ = peripheral.disconnect().await;
                return Err(e);
            }
        };
        tracing::info!("device info: {}", info);

        let notify_handle = Self::log_notifications(&peripheral).await.ok();

        let result = async {
            self.push_endpoint(&peripheral).await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            if info["v2_bundle"] != true { self.push_usage(&peripheral).await?; }
            tokio::time::sleep(Duration::from_millis(500)).await;
            if info["v2_bundle"] != true { self.push_templates(&peripheral, &info).await?; }
            Ok::<_, anyhow::Error>(())
        }
        .await;

        tokio::time::sleep(Duration::from_secs(1)).await;
        if let Some(handle) = notify_handle {
            handle.abort();
        }
        let _ = peripheral.disconnect().await;
        result?;
        Ok(info)
    }

    /// Scan/connect/retry loop.
    pub async fn run(&self, interval: Duration) -> Result<()> {
        loop {
            let adapter = Self::adapter().await?;
            match self.cycle_once(&adapter).await {
                Ok(_) => tracing::info!("BLE cycle done"),
                Err(e) => tracing::warn!("BLE cycle failed: {e}"),
            }
            tokio::time::sleep(interval).await;
        }
    }
}

/// Plan C: stamp every rendezvous command with the bridge's wall clock and UTC
/// offset so the device can sync before its single post-window render. Missing
/// or invalid fields leave the device on its local-RTC fallback.
fn stamp_clock(body: &mut serde_json::Value) {
    body["server_time"] = json!(bridge_core::now_secs());
    body["tz_offset_min"] = json!(bridge_core::local_offset_minutes());
}

/// Authenticated v2 opportunity on the existing GATT table. The status value
/// is read as a long attribute, so an ACK is not lost to notification MTU cuts.
pub struct V2Connection {
    peripheral: Peripheral,
    token: String,
    bridge_id: String,
    nonce: String,
    device_mac: String,
}

impl V2Connection {
    /// `Ok(None)` = the device was not advertising (rendezvous window closed):
    /// no connection was attempted and the caller may scan again immediately.
    pub async fn connect(mac: &str, token: &str, bridge_id: &str) -> Result<Option<Self>> {
        let compact = bridge_core::platform::model::DeviceIdentity::normalized_mac(mac)
            .context("invalid device MAC")?;
        let adapter = Pusher::adapter().await?;
        let found = Pusher::find_device(
            &adapter,
            &format!("CodexStatus-{}", &compact[6..]),
            Duration::from_secs(3),
        )
        .await;
        let _ = adapter.stop_scan().await;
        let Some(peripheral) = found? else {
            return Ok(None);
        };
        tracing::debug!("v2 BLE device found at {}", peripheral.address());
        let mut connected = tokio::time::timeout(Duration::from_secs(4), peripheral.connect()).await
            .map_err(anyhow::Error::from).and_then(|r| r.map_err(anyhow::Error::from));
        if connected.is_err() {
            // Windows refuses the first connect to a just-seen advertisement
            // often enough to lose the whole rendezvous; retry inside the
            // window instead of waiting for the next scan tick.
            tracing::debug!("v2 BLE connect retry");
            tokio::time::sleep(Duration::from_millis(120)).await;
            connected = tokio::time::timeout(Duration::from_secs(4), peripheral.connect()).await
                .map_err(anyhow::Error::from).and_then(|r| r.map_err(anyhow::Error::from));
        }
        if let Err(error) = connected {
            let _ = peripheral.disconnect().await;
            return Err(error.context("ble connect"));
        }
        let setup = async {
            tokio::time::timeout(Duration::from_secs(3), peripheral.discover_services()).await
                .context("discover timeout")?.context("discover")?;
            let info = tokio::time::timeout(Duration::from_secs(3), Pusher::read_info(&peripheral))
                .await
                .map_err(|_| anyhow!("info timeout"))?
                .map_err(|e| e.context("info"))?;
            if bridge_core::platform::model::DeviceIdentity::normalized_mac(
                info["mac"].as_str().unwrap_or("")) != Some(compact.clone()) {
                bail!("BLE device identity mismatch");
            }
            if info["rendezvous_v"].as_u64().unwrap_or(0) < 2 {
                bail!("device BLE rendezvous is disabled");
            }
            Ok(())
        }.await;
        if let Err(error) = setup {
            let _ = peripheral.disconnect().await;
            return Err(error);
        }
        let device_mac = compact.as_bytes().chunks(2)
            .map(|p| std::str::from_utf8(p).unwrap()).collect::<Vec<_>>().join(":");
        Ok(Some(Self { peripheral, token: token.to_owned(), bridge_id: bridge_id.to_owned(), nonce: String::new(), device_mac }))
    }

    pub async fn command(&mut self, op: &str, mut body: serde_json::Value) -> Result<serde_json::Value> {
        let id = format!("r{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos());
        body["rv"] = json!(2);
        body["protocol"] = json!(2);
        body["op"] = json!(op);
        body["request_id"] = json!(id);
        body["session_nonce"] = json!(self.nonce);
        body["bridge_id"] = json!(self.bridge_id);
        body["device_mac"] = json!(self.device_mac);
        body["token"] = json!(self.token);
        stamp_clock(&mut body);
        let bytes = serde_json::to_vec(&body)?;
        if bytes.len() > 8192 { bail!("v2 BLE command exceeds 8192 bytes"); }
        let status = self.peripheral.characteristics().into_iter()
            .find(|c| c.uuid == Pusher::uuid(CHR_STATUS)).context("status characteristic")?;
        Pusher::write_json(&self.peripheral, CHR_TPL_CTRL, &bytes).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let raw = tokio::time::timeout_at(deadline, self.peripheral.read(&status)).await??;
            if let Ok(reply) = serde_json::from_slice::<serde_json::Value>(&raw) {
                if reply["ack"] == "v2" && reply["request_id"] == id {
                    if op == "status" && reply["result"] == "applied" {
                        self.nonce = reply["session_nonce"].as_str().context("session nonce")?.to_owned();
                    }
                    return Ok(reply);
                }
            }
            if tokio::time::Instant::now() >= deadline { bail!("v2 BLE ACK timed out"); }
            tokio::time::sleep(Duration::from_millis(80)).await;
        }
    }

    pub async fn close(self) { let _ = self.peripheral.disconnect().await; }
}

#[cfg(test)]
mod tests {
    use super::Pusher;
    use serde_json::json;

    #[test]
    fn json_fragments_reassemble_at_boundary_and_preserve_utf8_bytes() {
        let mut input = vec![b'x'; 180];
        input.extend_from_slice("界尾".as_bytes());
        let parts = Pusher::fragment_payload(&input, super::JSON_WRITE_LIMIT);
        assert_eq!(parts.iter().map(Vec::len).max(), Some(180));
        assert!(parts.iter().all(|part| part.len() <= super::JSON_WRITE_LIMIT));
        assert_eq!(parts.concat(), input);
    }

    #[test]
    fn info_gate_requires_persistent_bond() {
        assert!(Pusher::peer_bonded(&json!({"peerBonded": true})));
        assert!(!Pusher::peer_bonded(&json!({"peerBonded": false, "peerEncrypted": true})));
        assert!(!Pusher::peer_bonded(&json!({"peerEncrypted": true})));
    }

    #[test]
    fn rendezvous_commands_carry_the_bridge_clock() {
        let mut body = json!({"op": "plan"});
        super::stamp_clock(&mut body);
        let now = body["server_time"].as_u64().unwrap();
        assert!(now > 1_600_000_000, "server_time must be a fresh epoch");
        let tz = body["tz_offset_min"].as_i64().unwrap();
        assert!(
            (-840..=840).contains(&tz),
            "tz_offset_min out of range: {tz}"
        );
    }
}
