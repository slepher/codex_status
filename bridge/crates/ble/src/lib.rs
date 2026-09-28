//! BLE central for device discovery, endpoint handoff, and authenticated commands.

use std::collections::HashSet;
use std::future::Future;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use btleplug::api::{
    Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter, ValueNotification, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use serde_json::json;
use uuid::Uuid;

static NEXT_SCAN_ID: AtomicU64 = AtomicU64::new(1);
static EMPTY_SCAN_SUMMARY: OnceLock<Mutex<EmptyScanSummary>> = OnceLock::new();

#[derive(Default)]
struct EmptyScanSummary {
    last_emitted: Option<Instant>,
    suppressed_windows: u64,
}

#[derive(Default)]
struct ScanCounts {
    discovered: u64,
    updated: u64,
    candidate_count: u64,
    target_candidate_count: u64,
    ignored_non_candidate: u64,
    ignored_non_target: u64,
    ignored_properties: u64,
}

fn should_log_empty_window(elapsed: Option<Duration>, interval: Duration) -> bool {
    elapsed.map_or(true, |elapsed| elapsed >= interval)
}

fn should_log_scan_start(result: &str, candidate_count: u64) -> bool {
    result != "timeout" || candidate_count > 0
}

fn empty_scan_suppressed_windows(now: Instant, interval: Duration) -> Option<u64> {
    let summary = EMPTY_SCAN_SUMMARY.get_or_init(Default::default);
    let Ok(mut summary) = summary.lock() else {
        return Some(0);
    };
    let elapsed = summary
        .last_emitted
        .map(|last| now.saturating_duration_since(last));
    if should_log_empty_window(elapsed, interval) {
        let suppressed = std::mem::take(&mut summary.suppressed_windows);
        summary.last_emitted = Some(now);
        Some(suppressed)
    } else {
        summary.suppressed_windows += 1;
        None
    }
}

fn error_category(stage: &'static str) -> &'static str {
    match stage {
        "adapter" => "unavailable",
        "start_scan" => "start_failed",
        "connect" => "connect_failed",
        "discover_services" => "gatt_discovery_failed",
        "read_info" => "info_read_failed",
        "verify_mac" => "identity_mismatch",
        "write" => "write_failed",
        "ack" => "ack_failed",
        _ => "failed",
    }
}

async fn trace_stage<T>(
    scan_id: u64,
    stage: &'static str,
    action: impl Future<Output = Result<T>>,
) -> Result<(T, u128)> {
    let started = Instant::now();
    match action.await {
        Ok(value) => {
            let duration_ms = started.elapsed().as_millis();
            tracing::info!(
                scan_id,
                stage,
                result = "ok",
                duration_ms,
                "BLE stage completed"
            );
            Ok((value, duration_ms))
        }
        Err(error) => {
            tracing::warn!(
                scan_id,
                stage,
                category = error_category(stage),
                duration_ms = started.elapsed().as_millis(),
                "BLE stage failed"
            );
            Err(error)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum AdvertisementDecision {
    IgnoreNonCandidate,
    Candidate { matches_target: bool },
}

fn classify_advertisement(name: &str, matches_target: bool) -> AdvertisementDecision {
    if name.starts_with("CodexStatus-") || matches_target {
        AdvertisementDecision::Candidate { matches_target }
    } else {
        AdvertisementDecision::IgnoreNonCandidate
    }
}

pub const SVC_UUID: &str = "e7f1a000-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_INFO: &str = "e7f1a001-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_ENDPOINT: &str = "e7f1a002-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_USAGE: &str = "e7f1a003-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_STATUS: &str = "e7f1a004-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_TPL_CTRL: &str = "e7f1a005-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_TPL_DATA: &str = "e7f1a006-4b2a-4c9e-9a11-3c0d5e9a0000";
pub const CHR_AUTH: &str = "e7f1a007-4b2a-4c9e-9a11-3c0d5e9a0000";

pub const JSON_WRITE_LIMIT: usize = 180;

#[derive(Debug, Clone)]
pub struct BleConfig {
    pub name_prefix: String,
    pub host: String,
    pub port: u16,
    pub token: String,
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
}

impl Pusher {
    pub fn new(cfg: BleConfig) -> Self {
        Self { cfg }
    }

    fn uuid(s: &str) -> Uuid {
        Uuid::parse_str(s).expect("static uuid")
    }

    fn log_service_lookup(peripheral: &Peripheral, scan_id: u64) {
        let started = Instant::now();
        let found = peripheral
            .services()
            .iter()
            .any(|service| service.uuid == Self::uuid(SVC_UUID));
        if found {
            tracing::info!(
                scan_id,
                service = "codex_status",
                duration_ms = started.elapsed().as_millis(),
                "BLE GATT service found"
            );
        } else {
            tracing::warn!(
                scan_id,
                stage = "service_lookup",
                category = "service_missing",
                duration_ms = started.elapsed().as_millis(),
                "BLE GATT service lookup failed"
            );
        }
    }

    /// A fresh Windows GATT central. Each rendezvous attempt gets its own
    /// adapter (and therefore its own advertisement watcher): reusing one
    /// adapter and restarting its scan re-registers the WinRT advertisement
    /// handler on every resume (btleplug has no unregister API), and the
    /// Manager itself is a zero-sized wrapper, so recreating is the cheap,
    /// leak-free option here.
    pub async fn adapter() -> Result<Adapter> {
        let result = async {
            let manager = Manager::new().await.context("bluetooth manager")?;
            let adapter = manager
                .adapters()
                .await
                .context("bluetooth adapters")?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("no bluetooth adapter"))?;
            Ok::<_, anyhow::Error>(adapter)
        }
        .await;
        match &result {
            Ok(_) => tracing::debug!(result = "selected", "BLE adapter available"),
            Err(_) => tracing::warn!(
                stage = "adapter",
                category = error_category("adapter"),
                "BLE adapter unavailable"
            ),
        }
        result
    }

    /// One scan pass for the device prefix, driven by advertisement events
    /// instead of polling. `Ok(None)` means it never advertised inside the
    /// timeout (no connection was attempted).
    ///
    /// The scan is stopped before returning a match: measured on this Windows
    /// host, connecting while the advertisement watcher is scanning made every
    /// second rendezvous hang both connect attempts for the full 4 s timeout.
    async fn find_device(
        adapter: &Adapter,
        prefix: &str,
        timeout: Duration,
    ) -> Result<Option<(u64, Peripheral)>> {
        Self::find_device_matching(adapter, timeout, |name| name.starts_with(prefix)).await
    }

    async fn find_device_matching<F>(
        adapter: &Adapter,
        timeout: Duration,
        matches: F,
    ) -> Result<Option<(u64, Peripheral)>>
    where
        F: Fn(&str) -> bool,
    {
        let scan_id = NEXT_SCAN_ID.fetch_add(1, Ordering::Relaxed);
        let started = Instant::now();
        tracing::debug!(
            scan_id,
            timeout_ms = timeout.as_millis(),
            "BLE scan started"
        );
        if let Err(error) = adapter.start_scan(ScanFilter::default()).await {
            Self::log_scan_start(scan_id, started);
            tracing::warn!(
                scan_id,
                stage = "start_scan",
                category = error_category("start_scan"),
                "BLE scan failed"
            );
            tracing::info!(
                scan_id,
                result = "error",
                duration_ms = started.elapsed().as_millis(),
                "BLE scan ended"
            );
            return Err(error).context("start scan");
        }
        let mut events = match adapter.events().await {
            Ok(events) => events,
            Err(error) => {
                let _ = adapter.stop_scan().await;
                Self::log_scan_start(scan_id, started);
                tracing::warn!(
                    scan_id,
                    stage = "scan_events",
                    category = "event_stream_failed",
                    "BLE scan failed"
                );
                tracing::info!(
                    scan_id,
                    result = "error",
                    duration_ms = started.elapsed().as_millis(),
                    "BLE scan ended"
                );
                return Err(error).context("scan events");
            }
        };
        let deadline = tokio::time::Instant::now() + timeout;
        let mut counts = ScanCounts::default();
        let mut seen_candidates = HashSet::new();
        loop {
            let event = match tokio::time::timeout_at(deadline, events.next()).await {
                Ok(Some(event)) => event,
                Ok(None) => {
                    let _ = adapter.stop_scan().await;
                    tracing::warn!(
                        scan_id,
                        stage = "scan_events",
                        category = "event_stream_closed",
                        "BLE scan event stream closed"
                    );
                    Self::log_scan_end(scan_id, started, "error", &counts);
                    return Err(anyhow!("scan event stream closed"));
                }
                Err(_) => {
                    let _ = adapter.stop_scan().await;
                    Self::log_scan_end(scan_id, started, "timeout", &counts);
                    return Ok(None);
                }
            };
            let (id, discovered) = match event {
                CentralEvent::DeviceDiscovered(id) => (id, true),
                CentralEvent::DeviceUpdated(id) => (id, false),
                _ => continue,
            };
            if discovered {
                counts.discovered += 1;
            } else {
                counts.updated += 1;
            }
            let Ok(peripheral) = adapter.peripheral(&id).await else {
                counts.ignored_properties += 1;
                tracing::warn!(
                    scan_id,
                    stage = "peripheral_lookup",
                    category = "lookup_failed",
                    "BLE advertisement lookup failed"
                );
                continue;
            };
            if let Some(found) = Self::consider_advertisement(
                scan_id,
                started,
                &peripheral,
                discovered,
                &matches,
                &mut counts,
                &mut seen_candidates,
            )
            .await
            {
                let _ = adapter.stop_scan().await;
                Self::log_scan_end(scan_id, started, "matched", &counts);
                return Ok(Some((scan_id, found)));
            }
        }
    }

    async fn consider_advertisement<F>(
        scan_id: u64,
        started: Instant,
        peripheral: &Peripheral,
        discovered: bool,
        matches: &F,
        counts: &mut ScanCounts,
        seen: &mut HashSet<String>,
    ) -> Option<Peripheral>
    where
        F: Fn(&str) -> bool,
    {
        let address = peripheral.address().to_string();
        let properties = match peripheral.properties().await {
            Ok(Some(properties)) => properties,
            _ => {
                counts.ignored_properties += 1;
                return None;
            }
        };
        let name = properties.local_name.unwrap_or_default();
        let is_match = matches(&name);
        if classify_advertisement(&name, is_match) == AdvertisementDecision::IgnoreNonCandidate {
            counts.ignored_non_candidate += 1;
            return None;
        }
        if !seen.insert(address.clone()) {
            return None;
        }
        counts.candidate_count += 1;
        if is_match {
            counts.target_candidate_count += 1;
        } else {
            counts.ignored_non_target += 1;
        }
        tracing::info!(
            scan_id,
            first_seen_ms = started.elapsed().as_millis(),
            advertisement = if discovered { "discovered" } else { "updated" },
            name = %name,
            ble_address = %address,
            rssi = ?properties.rssi,
            decision = if is_match { "target_candidate" } else { "ignored_non_target" },
            wake_association = "uncertain",
            association_basis = "advertisement",
            "BLE CodexStatus advertisement"
        );
        is_match.then(|| peripheral.clone())
    }

    fn log_scan_start(scan_id: u64, started: Instant) {
        tracing::info!(
            scan_id,
            window_started_ms_ago = started.elapsed().as_millis(),
            "BLE scan started"
        );
    }

    fn log_scan_end(scan_id: u64, started: Instant, result: &'static str, counts: &ScanCounts) {
        let duration_ms = started.elapsed().as_millis();
        if should_log_scan_start(result, counts.candidate_count) {
            Self::log_scan_start(scan_id, started);
        }
        if result == "timeout" && counts.candidate_count == 0 {
            let interval = Duration::from_secs(30);
            match empty_scan_suppressed_windows(Instant::now(), interval) {
                Some(suppressed_windows) => tracing::info!(
                    scan_id,
                    result,
                    duration_ms,
                    discovered = counts.discovered,
                    updated = counts.updated,
                    candidate_count = counts.candidate_count,
                    target_candidate_count = counts.target_candidate_count,
                    ignored_non_candidate = counts.ignored_non_candidate,
                    ignored_properties = counts.ignored_properties,
                    suppressed_windows,
                    "BLE scan summary"
                ),
                None if tracing::enabled!(tracing::Level::DEBUG) => tracing::debug!(
                    scan_id,
                    result,
                    duration_ms,
                    discovered = counts.discovered,
                    updated = counts.updated,
                    candidate_count = counts.candidate_count,
                    ignored_non_candidate = counts.ignored_non_candidate,
                    "BLE empty scan sampled"
                ),
                None => {}
            }
        } else {
            tracing::info!(
                scan_id,
                result,
                duration_ms,
                discovered = counts.discovered,
                updated = counts.updated,
                candidate_count = counts.candidate_count,
                target_candidate_count = counts.target_candidate_count,
                ignored_non_candidate = counts.ignored_non_candidate,
                ignored_non_target = counts.ignored_non_target,
                ignored_properties = counts.ignored_properties,
                "BLE scan ended"
            );
        }
    }

    async fn wait_for_device(
        adapter: &Adapter,
        prefix: &str,
        timeout: Duration,
    ) -> Result<(u64, Peripheral)> {
        Self::find_device(adapter, prefix, timeout)
            .await?
            .with_context(|| format!("device {prefix}* not found"))
    }

    fn peer_bonded(info: &serde_json::Value) -> bool {
        info.get("peerBonded").and_then(|v| v.as_bool()) == Some(true)
    }

    fn peer_encrypted_bonded(info: &serde_json::Value) -> bool {
        Self::peer_bonded(info) && info.get("peerEncrypted").and_then(|v| v.as_bool()) == Some(true)
    }

    fn valid_device_token(token: &str) -> bool {
        token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    fn info_matches_mac(info: &serde_json::Value, expected_mac: &str) -> bool {
        let Some(expected) =
            bridge_core::platform::model::DeviceIdentity::normalized_mac(expected_mac)
        else {
            return false;
        };
        bridge_core::platform::model::DeviceIdentity::normalized_mac(
            info.get("mac")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        ) == Some(expected)
    }

    async fn read_info(peripheral: &Peripheral, scan_id: u64) -> Result<serde_json::Value> {
        let lookup_start = Instant::now();
        let target = peripheral
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == Self::uuid(CHR_INFO))
            .ok_or_else(|| anyhow!("info characteristic missing"));
        let target = match target {
            Ok(target) => target,
            Err(error) => {
                tracing::warn!(
                    scan_id,
                    stage = "info_characteristic",
                    category = "characteristic_missing",
                    duration_ms = lookup_start.elapsed().as_millis(),
                    "BLE GATT characteristic lookup failed"
                );
                return Err(error);
            }
        };
        tracing::info!(
            scan_id,
            characteristic = "info",
            duration_ms = lookup_start.elapsed().as_millis(),
            "BLE GATT characteristic found"
        );
        let read_start = Instant::now();
        let raw = match peripheral.read(&target).await {
            Ok(raw) => raw,
            Err(error) => {
                tracing::warn!(
                    scan_id,
                    stage = "read_info",
                    category = error_category("read_info"),
                    duration_ms = read_start.elapsed().as_millis(),
                    "BLE info characteristic read failed"
                );
                return Err(error).context("read device info");
            }
        };
        tracing::info!(
            scan_id,
            duration_ms = read_start.elapsed().as_millis(),
            "BLE info characteristic read"
        );
        let info: serde_json::Value = serde_json::from_slice(&raw).context("parse device info")?;
        if !Self::peer_bonded(&info) {
            bail!("device is not bonded; pair manually in Windows Bluetooth settings: hold BOOT for 2 seconds to open the 120 second pairing window, connect CodexStatus, then retry")
        }
        Ok(info)
    }

    async fn write_char(peripheral: &Peripheral, uuid: &str, data: &[u8]) -> Result<()> {
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

    async fn request_token_on_peripheral(peripheral: &Peripheral) -> Result<String> {
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
        Self::write_json(peripheral, CHR_AUTH, br#"{"cmd":"token"}"#).await?;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, stream.next()).await {
                Ok(Some(ValueNotification { uuid, value, .. })) => {
                    if uuid != Self::uuid(CHR_STATUS) {
                        continue;
                    }
                    let Ok(doc) = serde_json::from_slice::<serde_json::Value>(&value) else {
                        continue;
                    };
                    if doc.get("ack").and_then(|v| v.as_str()) != Some("auth")
                        || doc.get("ok").and_then(|v| v.as_bool()) != Some(true)
                    {
                        continue;
                    }
                    let token = doc
                        .get("token")
                        .and_then(|v| v.as_str())
                        .filter(|token| Self::valid_device_token(token))
                        .ok_or_else(|| anyhow!("device token response was invalid"))?;
                    return Ok(token.to_string());
                }
                Ok(None) | Err(_) => break,
            }
        }
        Err(anyhow!("device token response timed out"))
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
                    tracing::debug!(bytes = value.len(), "BLE status notification received");
                }
            }
        });
        Ok(handle)
    }

    async fn push_endpoint(&self, peripheral: &Peripheral) -> Result<()> {
        Self::write_json(
            peripheral,
            CHR_ENDPOINT,
            &Self::endpoint_payload(&self.cfg.host, self.cfg.port, &self.cfg.token),
        )
        .await
    }

    fn endpoint_payload(host: &str, port: u16, token: &str) -> Vec<u8> {
        json!({
            "schema": 1,
            "host": host,
            "port": port,
            "token": token,
        })
        .to_string()
        .into_bytes()
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
        let (scan_id, peripheral) = Self::wait_for_device(adapter, name_prefix, scan).await?;
        trace_stage(scan_id, "connect", async {
            peripheral.connect().await.context("connect")
        })
        .await?;
        if let Err(error) = trace_stage(scan_id, "discover_services", async {
            peripheral.discover_services().await.context("discover")
        })
        .await
        {
            let _ = peripheral.disconnect().await;
            return Err(error);
        }
        Self::log_service_lookup(&peripheral, scan_id);
        let info = trace_stage(scan_id, "read_info", Self::read_info(&peripheral, scan_id))
            .await
            .map(|(info, _)| info);
        let _ = peripheral.disconnect().await;
        info
    }

    /// Connect to an advertising device (BLE session on, e.g. after a BOOT
    /// click) and request the OTA/Wi-Fi operation token over the bonded auth
    /// characteristic. The token is disclosed only over this encrypted link.
    pub async fn request_device_token(
        adapter: &Adapter,
        name_prefix: &str,
        expected_mac: &str,
        scan_timeout_ms: u64,
    ) -> Result<String> {
        if bridge_core::platform::model::DeviceIdentity::normalized_mac(expected_mac).is_none() {
            bail!("invalid target device MAC");
        }
        let scan = Duration::from_millis(scan_timeout_ms.max(1000));
        let (scan_id, peripheral) = Self::wait_for_device(adapter, name_prefix, scan).await?;
        trace_stage(scan_id, "connect", async {
            peripheral.connect().await.context("connect")
        })
        .await?;
        if let Err(error) = trace_stage(scan_id, "discover_services", async {
            peripheral.discover_services().await.context("discover")
        })
        .await
        {
            let _ = peripheral.disconnect().await;
            return Err(error);
        }
        Self::log_service_lookup(&peripheral, scan_id);
        let info =
            match trace_stage(scan_id, "read_info", Self::read_info(&peripheral, scan_id)).await {
                Ok((info, _)) => info,
                Err(e) => {
                    let _ = peripheral.disconnect().await;
                    return Err(e);
                }
            };
        if !Self::info_matches_mac(&info, expected_mac) {
            tracing::warn!(
                scan_id,
                stage = "verify_mac",
                category = error_category("verify_mac"),
                "BLE device identity mismatch"
            );
            let _ = peripheral.disconnect().await;
            bail!("BLE device Wi-Fi MAC does not match target");
        }
        if !Self::peer_encrypted_bonded(&info) {
            let _ = peripheral.disconnect().await;
            bail!("BLE device link is not encrypted and bonded");
        }
        tracing::info!(
            scan_id,
            verified_mac = expected_mac,
            "BLE device identity verified"
        );
        let token = Self::request_token_on_peripheral(&peripheral).await;
        let _ = peripheral.disconnect().await;
        token
    }

    /// One connect → endpoint handoff → disconnect cycle. Returns the device info JSON so
    /// the caller can adopt its identity (mac/ip) when needed.
    pub async fn cycle_once(&self, adapter: &Adapter) -> Result<serde_json::Value> {
        let scan = Duration::from_millis(self.cfg.scan_timeout_ms.max(1000));
        let (scan_id, peripheral) =
            Self::wait_for_device(adapter, &self.cfg.name_prefix, scan).await?;
        trace_stage(scan_id, "connect", async {
            peripheral.connect().await.context("connect")
        })
        .await?;
        if let Err(error) = trace_stage(scan_id, "discover_services", async {
            peripheral.discover_services().await.context("discover")
        })
        .await
        {
            let _ = peripheral.disconnect().await;
            return Err(error);
        }
        Self::log_service_lookup(&peripheral, scan_id);
        let info =
            match trace_stage(scan_id, "read_info", Self::read_info(&peripheral, scan_id)).await {
                Ok((info, _)) => info,
                Err(e) => {
                    let _ = peripheral.disconnect().await;
                    return Err(e);
                }
            };

        let notify_handle = Self::log_notifications(&peripheral).await.ok();

        let result = self.push_endpoint(&peripheral).await;

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
fn stamp_clock(body: &mut serde_json::Value, mac: &str) {
    body["server_time"] = json!(bridge_core::device_clock::wall_secs(mac));
    body["tz_offset_min"] = json!(bridge_core::local_offset_minutes());
}

/// Authenticated device opportunity on the existing GATT table. The status value
/// is read as a long attribute, so an ACK is not lost to notification MTU cuts.
pub struct DeviceConnection {
    peripheral: Option<Peripheral>,
    fake_url: Option<String>,
    token: String,
    bridge_id: String,
    nonce: String,
    device_mac: String,
    auth_ready: bool,
    wake_identity: Option<WakeIdentity>,
    wake_cause: String,
    device_last_stage: String,
    /// Stage timings (`find`, `connect`, `discover`, `info`, one entry per
    /// command) for the Plan C wake-budget evidence.
    timings: Vec<(String, u128)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WakeIdentity {
    generation: u64,
    seq: u64,
}

fn wake_identity(value: &serde_json::Value) -> Option<WakeIdentity> {
    Some(WakeIdentity {
        generation: value.get("wake_generation")?.as_u64()?,
        seq: value.get("wake_seq")?.as_u64()?,
    })
}

fn text_field(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}

fn identity_text(identity: Option<WakeIdentity>) -> (String, String) {
    identity.map_or_else(
        || ("unknown".to_owned(), "unknown".to_owned()),
        |identity| (identity.generation.to_string(), identity.seq.to_string()),
    )
}

impl DeviceConnection {
    /// `Ok(None)` = the device was not *freshly* advertising (rendezvous window
    /// closed or stale scan entry): no connection was attempted and the caller
    /// may scan again immediately.
    pub async fn connect(mac: &str, token: &str, bridge_id: &str) -> Result<Option<Self>> {
        Ok(Self::connect_any(&[mac.to_owned()], token, bridge_id)
            .await?
            .map(|(_, connection)| connection))
    }

    /// Scan once for any registered target. The advertisement name is only a
    /// candidate filter; the full Wi-Fi MAC in GATT info authorizes the link.
    pub async fn connect_any(
        target_macs: &[String],
        token: &str,
        bridge_id: &str,
    ) -> Result<Option<(String, Self)>> {
        let targets = normalize_target_macs(target_macs)?;
        if targets.is_empty() {
            return Ok(None);
        }
        if let Ok(raw) = std::env::var("CODEX_STATUS_SIM_BLE_ENDPOINTS") {
            let endpoints: serde_json::Value = serde_json::from_str(&raw)
                .context("invalid fake BLE endpoint registry")?;
            let client = reqwest::Client::builder().timeout(Duration::from_secs(3)).build()?;
            for mac in &targets {
                let Some(url) = endpoints[mac].as_str() else { continue; };
                let parsed = reqwest::Url::parse(url)?;
                if parsed.scheme() != "http" || !matches!(parsed.host_str(),
                    Some("127.0.0.1" | "localhost")) || parsed.port().is_none() {
                    bail!("fake BLE endpoint must be an explicit loopback HTTP address");
                }
                let response = match client.get(format!("{}/sim/ble/info", url.trim_end_matches('/')))
                    .bearer_auth(token).send().await {
                    Ok(response) => response,
                    Err(error) => {
                        tracing::warn!(device_mac = mac, %error, "fake BLE candidate unavailable");
                        continue;
                    }
                };
                if response.status() == reqwest::StatusCode::NOT_FOUND { continue; }
                let info: serde_json::Value = response.error_for_status()?.json().await?;
                let authorized_mac = info_authorized_target(&info, &targets)
                    .context("fake BLE device identity mismatch")?;
                if authorized_mac != *mac || info["rendezvous"] != true {
                    bail!("fake BLE rendezvous identity/version mismatch");
                }
                let device_mac = authorized_mac.as_bytes().chunks(2)
                    .map(|p| std::str::from_utf8(p).unwrap())
                    .collect::<Vec<_>>().join(":");
                return Ok(Some((authorized_mac, Self {
                    peripheral: None, fake_url: Some(url.trim_end_matches('/').to_owned()),
                    token: token.to_owned(), bridge_id: bridge_id.to_owned(),
                    nonce: String::new(), device_mac,
                    auth_ready: info["bonded"] == true && info["encrypted"] == true,
                    wake_identity: wake_identity(&info),
                    wake_cause: text_field(&info, &["wake_cause"]).unwrap_or_default(),
                    device_last_stage: "info".into(), timings: vec![],
                })));
            }
            return Ok(None);
        }
        let adapter = Pusher::adapter().await?;
        let find_start = Instant::now();
        let found = Pusher::find_device_matching(&adapter, Duration::from_secs(3), |name| {
            advertisement_matches_any_target(name, &targets)
        })
        .await;
        let Some((scan_id, peripheral)) = found? else {
            return Ok(None);
        };
        let mut timings: Vec<(String, u128)> =
            vec![("find".to_string(), find_start.elapsed().as_millis())];
        let connect_start = Instant::now();
        let mut connected = trace_stage(scan_id, "connect", async {
            tokio::time::timeout(Duration::from_secs(4), peripheral.connect())
                .await
                .map_err(anyhow::Error::from)
                .and_then(|r| r.map_err(anyhow::Error::from))
        })
        .await;
        if connected.is_err() {
            // Windows refuses the first connect to a just-seen advertisement
            // often enough to lose the whole rendezvous; retry inside the
            // window instead of waiting for the next scan tick.
            tracing::debug!(scan_id, stage = "connect", "BLE connect retry");
            tokio::time::sleep(Duration::from_millis(120)).await;
            connected = trace_stage(scan_id, "connect", async {
                tokio::time::timeout(Duration::from_secs(4), peripheral.connect())
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(|r| r.map_err(anyhow::Error::from))
            })
            .await;
        }
        if let Err(error) = connected {
            let _ = peripheral.disconnect().await;
            return Err(error.context("ble connect"));
        }
        timings.push(("connect".to_string(), connect_start.elapsed().as_millis()));
        let setup = async {
            let mut stages: Vec<(String, u128)> = Vec::new();
            // Re-discovery is not optional here: `close` must call
            // `disconnect` (see below), which clears btleplug's GATT object
            // cache, and skipping both on a kept-open link made every second
            // rendezvous lose its connect. Measured on the Windows backend:
            // discover ~300 ms, INFO ~30 ms.
            let (_, discover_ms) = trace_stage(scan_id, "discover_services", async {
                tokio::time::timeout(Duration::from_secs(3), peripheral.discover_services())
                    .await
                    .context("discover timeout")
                    .and_then(|result| result.context("discover"))
            })
            .await?;
            stages.push(("discover".to_string(), discover_ms));
            Pusher::log_service_lookup(&peripheral, scan_id);
            let (info, info_ms) = trace_stage(scan_id, "read_info", async {
                tokio::time::timeout(
                    Duration::from_secs(3),
                    Pusher::read_info(&peripheral, scan_id),
                )
                .await
                .map_err(|_| anyhow!("info timeout"))
                .and_then(|result| result.map_err(|e| e.context("info")))
            })
            .await?;
            stages.push(("info".to_string(), info_ms));
            Ok::<_, anyhow::Error>((info, stages))
        }
        .await;
        let (info, stages) = match setup {
            Ok(value) => value,
            Err(error) => {
                let _ = peripheral.disconnect().await;
                return Err(error);
            }
        };
        timings.extend(stages);
        let Some(authorized_mac) = info_authorized_target(&info, &targets) else {
            let _ = peripheral.disconnect().await;
            tracing::warn!(
                scan_id,
                stage = "verify_mac",
                category = error_category("verify_mac"),
                "BLE device identity mismatch"
            );
            bail!("BLE device identity mismatch");
        };
        let initial_wake_identity = wake_identity(&info);
        let (wake_generation, wake_seq) = identity_text(initial_wake_identity);
        tracing::info!(
            scan_id,
            verified_mac = %authorized_mac,
            wake_generation = %wake_generation,
            wake_seq = %wake_seq,
            wake_association = if initial_wake_identity.is_some() { "confirmed" } else { "incomplete" },
            association_basis = "authenticated_info",
            "BLE device identity verified and wake contact associated"
        );
        if info["rendezvous"] != true {
            let _ = peripheral.disconnect().await;
            bail!("device BLE rendezvous is disabled");
        }
        tracing::debug!(?timings, "device link ready");
        let device_mac = authorized_mac
            .as_bytes()
            .chunks(2)
            .map(|p| std::str::from_utf8(p).unwrap())
            .collect::<Vec<_>>()
            .join(":");
        Ok(Some((
            authorized_mac,
            Self {
                peripheral: Some(peripheral),
                fake_url: None,
                token: token.to_owned(),
                bridge_id: bridge_id.to_owned(),
                nonce: String::new(),
                device_mac,
                auth_ready: Pusher::peer_encrypted_bonded(&info),
                wake_identity: initial_wake_identity,
                wake_cause: text_field(&info, &["wake_cause", "wake_type"])
                    .unwrap_or_else(|| "unknown".to_owned()),
                device_last_stage: text_field(&info, &["wake_stage", "last_stage", "stage"])
                    .unwrap_or_else(|| "unknown".to_owned()),
                timings,
            },
        )))
    }

    /// Refresh the endpoint credentials on this already MAC-verified link.
    /// The caller must verify the registered target before invoking this.
    pub async fn write_endpoint(&self, host: &str, port: u16, token: &str) -> Result<()> {
        if self.fake_url.is_some() { return Ok(()); }
        Pusher::write_json(
            self.peripheral.as_ref().unwrap(),
            CHR_ENDPOINT,
            &Pusher::endpoint_payload(host, port, token),
        )
        .await
    }

    /// Request the device operation token on this already MAC-verified link.
    /// The link must have reported both a persistent bond and encryption.
    pub async fn request_device_token(&self) -> Result<String> {
        if !self.auth_ready {
            bail!("BLE device link is not encrypted and bonded");
        }
        if let Some(url) = &self.fake_url {
            let reply: serde_json::Value = reqwest::Client::new()
                .get(format!("{url}/sim/ble/token"))
                .bearer_auth(&self.token).send().await?.error_for_status()?.json().await?;
            return Ok(reply["token"].as_str().context("fake BLE token response")?.to_owned());
        }
        Pusher::request_token_on_peripheral(self.peripheral.as_ref().unwrap()).await
    }

    pub async fn command(
        &mut self,
        op: &str,
        mut body: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = format!(
            "r{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        body["op"] = json!(op);
        body["request_id"] = json!(id);
        body["session_nonce"] = json!(self.nonce);
        body["bridge_id"] = json!(self.bridge_id);
        body["device_mac"] = json!(self.device_mac);
        body["token"] = json!(self.token);
        stamp_clock(&mut body, &self.device_mac);
        let bytes = serde_json::to_vec(&body)?;
        if bytes.len() > 8192 {
            bail!("device BLE command exceeds 8192 bytes");
        }
        if let Some(url) = &self.fake_url {
            let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build()?;
            let mut reply = serde_json::Value::Null;
            for (index, chunk) in bytes.chunks(180).enumerate() {
                let final_chunk = (index + 1) * 180 >= bytes.len();
                reply = client.post(format!("{url}/sim/ble/fragment?offset={}&final={}",
                    index * 180, u8::from(final_chunk)))
                    .bearer_auth(&self.token).body(chunk.to_vec()).send().await?
                    .error_for_status()?.json().await?;
                if !final_chunk && reply["result"] != "more" {
                    bail!("fake BLE fragment was not accepted");
                }
            }
            if reply["ack"] != "command" || reply["request_id"] != id {
                bail!("fake BLE ACK identity mismatch");
            }
            if op == "status" && reply["result"] == "applied" {
                self.nonce = reply["session_nonce"].as_str()
                    .context("fake BLE session nonce")?.to_owned();
            }
            self.timings.push((op.to_owned(), 0));
            return Ok(reply);
        }
        let lookup_start = Instant::now();
        let status = self
            .peripheral
            .as_ref().unwrap()
            .characteristics()
            .into_iter()
            .find(|c| c.uuid == Pusher::uuid(CHR_STATUS))
            .context("status characteristic");
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                self.log_failure(&id, op, "status_characteristic", "characteristic_missing");
                tracing::warn!(device_mac = %self.device_mac, op, stage = "status_characteristic", category = "characteristic_missing", duration_ms = lookup_start.elapsed().as_millis(), "BLE ACK characteristic lookup failed");
                return Err(error);
            }
        };
        tracing::info!(device_mac = %self.device_mac, op, characteristic = "status", duration_ms = lookup_start.elapsed().as_millis(), "BLE ACK characteristic found");
        let write_start = Instant::now();
        if let Err(error) = Pusher::write_json(self.peripheral.as_ref().unwrap(), CHR_TPL_CTRL, &bytes).await {
            self.log_failure(&id, op, "write", "write_failed");
            tracing::warn!(device_mac = %self.device_mac, op, request_id = %id, stage = "write", category = error_category("write"), duration_ms = write_start.elapsed().as_millis(), "BLE command write failed");
            return Err(error);
        }
        let (write_wake_generation, write_wake_seq) = identity_text(self.wake_identity);
        tracing::info!(
            device_mac = %self.device_mac,
            op,
            request_id = %id,
            wake_generation = %write_wake_generation,
            wake_seq = %write_wake_seq,
            wake_association = if self.wake_identity.is_some() { "confirmed" } else { "incomplete" },
            duration_ms = write_start.elapsed().as_millis(),
            "BLE command written"
        );
        let started = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let raw = match tokio::time::timeout_at(deadline,
                self.peripheral.as_ref().unwrap().read(&status)).await {
                Ok(Ok(raw)) => raw,
                Ok(Err(_)) => {
                    self.log_failure(&id, op, "ack", "read_failed");
                    tracing::warn!(device_mac = %self.device_mac, op, request_id = %id, stage = "ack", category = "read_failed", duration_ms = started.elapsed().as_millis(), "BLE command ACK read failed");
                    return Err(anyhow!("device BLE ACK read failed"));
                }
                Err(_) => {
                    self.log_failure(&id, op, "ack", "timeout");
                    tracing::warn!(device_mac = %self.device_mac, op, request_id = %id, stage = "ack", category = error_category("ack"), duration_ms = started.elapsed().as_millis(), "BLE command ACK failed");
                    return Err(anyhow!("device BLE ACK timed out"));
                }
            };
            if let Ok(reply) = serde_json::from_slice::<serde_json::Value>(&raw) {
                if reply["ack"] == "command" && reply["request_id"] == id {
                    let ack_identity = wake_identity(&reply);
                    if let (Some(expected), Some(actual)) = (self.wake_identity, ack_identity) {
                        if expected != actual {
                            let (expected_generation, expected_seq) = identity_text(Some(expected));
                            let (actual_generation, actual_seq) = identity_text(Some(actual));
                            tracing::warn!(
                                device_mac = %self.device_mac,
                                op,
                                request_id = %id,
                                expected_wake_generation = %expected_generation,
                                expected_wake_seq = %expected_seq,
                                ack_wake_generation = %actual_generation,
                                ack_wake_seq = %actual_seq,
                                stage = "wake_identity",
                                category = "mismatch",
                                "BLE ACK wake identity differs from authenticated INFO"
                            );
                        }
                    }
                    if self.wake_identity.is_none() {
                        self.wake_identity = ack_identity;
                    }
                    if let Some(cause) = text_field(&reply, &["wake_cause", "wake_type"]) {
                        self.wake_cause = cause;
                    }
                    if let Some(stage) = text_field(&reply, &["wake_stage", "last_stage", "stage"])
                    {
                        self.device_last_stage = stage;
                    }
                    if op == "status" && reply["result"] == "applied" {
                        self.nonce = reply["session_nonce"]
                            .as_str()
                            .context("session nonce")?
                            .to_owned();
                    }
                    let ms = started.elapsed().as_millis();
                    self.timings.push((op.to_string(), ms));
                    let (wake_generation, wake_seq) = identity_text(self.wake_identity);
                    tracing::info!(
                        device_mac = %self.device_mac,
                        op,
                        request_id = %id,
                        result = reply["result"].as_str().unwrap_or("acknowledged"),
                        wake_generation = %wake_generation,
                        wake_seq = %wake_seq,
                        wake_association = if self.wake_identity.is_some() { "confirmed" } else { "incomplete" },
                        duration_ms = ms,
                        "BLE command acknowledged"
                    );
                    return Ok(reply);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                self.log_failure(&id, op, "ack", "timeout");
                tracing::warn!(device_mac = %self.device_mac, op, request_id = %id, stage = "ack", category = error_category("ack"), duration_ms = started.elapsed().as_millis(), "BLE command ACK failed");
                bail!("device BLE ACK timed out");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn log_failure(&self, request_id: &str, op: &str, bridge_stage: &str, result: &str) {
        let (wake_generation, wake_seq) = identity_text(self.wake_identity);
        tracing::warn!(
            event = "wake_contact_summary",
            device_mac = %self.device_mac,
            wake_generation = %wake_generation,
            wake_seq = %wake_seq,
            wake_association = if self.wake_identity.is_some() { "confirmed" } else { "incomplete" },
            wake_cause = %self.wake_cause,
            device_last_stage = %self.device_last_stage,
            bridge_last_stage = %bridge_stage,
            stage_durations = ?self.timings,
            op,
            request_id = %request_id,
            result,
            "wake contact failure summary"
        );
    }

    /// Close the link explicitly. Measured 2026-09-22: leaving the Windows
    /// GATT device object alive across cycles (no disconnect) made every
    /// second rendezvous fail both connect attempts. `disconnect` also clears
    /// btleplug's cached GATT objects, so `discover_services` must run on
    /// every cycle (that is why discovery is not skipped in `connect`).
    pub async fn close(self) {
        if let Some(peripheral) = self.peripheral {
            let _ = peripheral.disconnect().await;
        }
        tracing::debug!(timings = ?self.timings, "device rendezvous link closed");
    }
}

fn normalize_target_macs(target_macs: &[String]) -> Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(target_macs.len());
    for mac in target_macs {
        let mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(mac)
            .context("invalid device MAC")?;
        if seen.insert(mac.clone()) {
            normalized.push(mac);
        }
    }
    Ok(normalized)
}

fn advertisement_matches_any_target(name: &str, target_macs: &[String]) -> bool {
    target_macs
        .iter()
        .any(|mac| name.starts_with(&format!("CodexStatus-{}", &mac[6..])))
}

fn info_authorized_target(info: &serde_json::Value, target_macs: &[String]) -> Option<String> {
    let mac = bridge_core::platform::model::DeviceIdentity::normalized_mac(
        info.get("mac")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    )?;
    target_macs
        .iter()
        .any(|target| target == &mac)
        .then_some(mac)
}

#[cfg(test)]
mod tests {
    use super::{
        advertisement_matches_any_target, classify_advertisement, empty_scan_suppressed_windows,
        info_authorized_target, normalize_target_macs, should_log_empty_window,
        should_log_scan_start, wake_identity, AdvertisementDecision, Pusher,
    };
    use serde_json::json;
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    #[test]
    fn scan_sampling_logs_at_the_thirty_second_boundary() {
        let interval = Duration::from_secs(30);
        assert!(should_log_empty_window(None, interval));
        assert!(!should_log_empty_window(
            Some(interval - Duration::from_millis(1)),
            interval
        ));
        assert!(should_log_empty_window(Some(interval), interval));
        assert!(should_log_empty_window(
            Some(interval + Duration::from_millis(1)),
            interval
        ));
    }

    #[test]
    fn scan_start_info_is_deferred_only_for_candidate_or_error_windows() {
        assert!(should_log_scan_start("matched", 1));
        assert!(should_log_scan_start("error", 0));
        assert!(should_log_scan_start("timeout", 1));
        assert!(!should_log_scan_start("timeout", 0));
    }

    #[test]
    fn advertisement_classification_keeps_unrelated_names_private_and_deduplicates_updates() {
        assert_eq!(
            classify_advertisement("Headphones", false),
            AdvertisementDecision::IgnoreNonCandidate
        );
        assert_eq!(
            classify_advertisement("CodexStatus-AABBEE", false),
            AdvertisementDecision::Candidate {
                matches_target: false
            }
        );
        assert_eq!(
            classify_advertisement("CodexStatus-AABBCC", true),
            AdvertisementDecision::Candidate {
                matches_target: true
            }
        );

        let mut seen = HashSet::new();
        let mut counts = super::ScanCounts::default();
        for address in ["70:04:1D:AA:BB:CC", "70:04:1D:AA:BB:CC"] {
            if seen.insert(address.to_owned()) {
                counts.candidate_count += 1;
                counts.target_candidate_count += 1;
            }
        }
        assert_eq!(counts.candidate_count, 1);
        assert_eq!(counts.target_candidate_count, 1);
    }

    #[test]
    fn empty_scan_sampling_reports_suppressed_window_count() {
        let first = Instant::now();
        assert_eq!(
            empty_scan_suppressed_windows(first, Duration::from_secs(30)),
            Some(0)
        );
        assert_eq!(
            empty_scan_suppressed_windows(first + Duration::from_secs(1), Duration::from_secs(30)),
            None
        );
        assert_eq!(
            empty_scan_suppressed_windows(first + Duration::from_secs(29), Duration::from_secs(30)),
            None
        );
        assert_eq!(
            empty_scan_suppressed_windows(first + Duration::from_secs(30), Duration::from_secs(30)),
            Some(2)
        );
    }

    #[test]
    fn json_fragments_reassemble_at_boundary_and_preserve_utf8_bytes() {
        let mut input = vec![b'x'; 180];
        input.extend_from_slice("界尾".as_bytes());
        let parts = Pusher::fragment_payload(&input, super::JSON_WRITE_LIMIT);
        assert_eq!(parts.iter().map(Vec::len).max(), Some(180));
        assert!(parts
            .iter()
            .all(|part| part.len() <= super::JSON_WRITE_LIMIT));
        assert_eq!(parts.concat(), input);
    }

    #[test]
    fn endpoint_payload_contains_current_bridge_credentials() {
        let payload: serde_json::Value = serde_json::from_slice(&Pusher::endpoint_payload(
            "192.168.1.2",
            8765,
            "bridge-token",
        ))
        .unwrap();
        assert_eq!(payload["schema"], 1);
        assert_eq!(payload["host"], "192.168.1.2");
        assert_eq!(payload["port"], 8765);
        assert_eq!(payload["token"], "bridge-token");
    }

    #[test]
    fn info_gate_requires_persistent_bond() {
        assert!(Pusher::peer_bonded(&json!({"peerBonded": true})));
        assert!(!Pusher::peer_bonded(
            &json!({"peerBonded": false, "peerEncrypted": true})
        ));
        assert!(!Pusher::peer_bonded(&json!({"peerEncrypted": true})));
    }

    #[test]
    fn token_request_requires_encrypted_bond_and_hex_token() {
        assert!(Pusher::peer_encrypted_bonded(
            &json!({"peerBonded": true, "peerEncrypted": true})
        ));
        assert!(!Pusher::peer_encrypted_bonded(
            &json!({"peerBonded": true, "peerEncrypted": false})
        ));
        assert!(Pusher::valid_device_token(&"a1".repeat(16)));
        assert!(!Pusher::valid_device_token(&"z".repeat(32)));
        assert!(!Pusher::valid_device_token(&"a".repeat(31)));
    }

    #[test]
    fn token_info_must_match_expected_wifi_mac() {
        assert!(Pusher::info_matches_mac(
            &json!({"mac": "70:04:1d:aa:bb:cc"}),
            "70041DAABBCC"
        ));
        assert!(!Pusher::info_matches_mac(
            &json!({"mac": "70:04:1d:aa:bb:cd"}),
            "70041DAABBCC"
        ));
        assert!(!Pusher::info_matches_mac(&json!({}), "70041DAABBCC"));
    }

    #[test]
    fn rendezvous_commands_carry_the_bridge_clock() {
        let mut body = json!({"op": "plan"});
        super::stamp_clock(&mut body, "70041DD7A340");
        let now = body["server_time"].as_u64().unwrap();
        assert!(now > 1_600_000_000, "server_time must be a fresh epoch");
        let tz = body["tz_offset_min"].as_i64().unwrap();
        assert!(
            (-840..=840).contains(&tz),
            "tz_offset_min out of range: {tz}"
        );
    }

    #[test]
    fn any_target_candidates_cover_all_registered_macs() {
        let targets = normalize_target_macs(&[
            "70:04:1D:AA:BB:CC".to_owned(),
            "70:04:1D:AA:BB:DD".to_owned(),
        ])
        .unwrap();
        assert!(advertisement_matches_any_target(
            "CodexStatus-AABBCC",
            &targets
        ));
        assert!(advertisement_matches_any_target(
            "CodexStatus-AABBDD",
            &targets
        ));
        assert!(!advertisement_matches_any_target(
            "CodexStatus-AABBEE",
            &targets
        ));
    }

    #[test]
    fn target_macs_reject_invalid_values_and_deduplicate_normalized_values() {
        assert!(normalize_target_macs(&["not-a-mac".to_owned()]).is_err());
        assert_eq!(
            normalize_target_macs(&["70:04:1D:AA:BB:CC".to_owned(), "70041daabbcc".to_owned(),])
                .unwrap(),
            vec!["70041DAABBCC"]
        );
    }

    #[test]
    fn full_info_mac_authorizes_candidate_even_when_advertisement_suffix_collides() {
        let targets = normalize_target_macs(&[
            "10:20:30:AA:BB:CC".to_owned(),
            "40:50:60:AA:BB:CC".to_owned(),
        ])
        .unwrap();
        assert!(advertisement_matches_any_target(
            "CodexStatus-AABBCC",
            &targets
        ));
        assert_eq!(
            info_authorized_target(&json!({"mac": "40:50:60:AA:BB:CC"}), &targets),
            Some("405060AABBCC".to_owned())
        );
        assert_eq!(
            info_authorized_target(&json!({"mac": "00:00:00:AA:BB:CC"}), &targets),
            None
        );
    }

    #[test]
    fn wake_association_requires_both_rtc_identity_parts() {
        assert_eq!(
            wake_identity(&json!({"wake_generation": 4, "wake_seq": 19})),
            Some(super::WakeIdentity {
                generation: 4,
                seq: 19,
            })
        );
        assert_eq!(wake_identity(&json!({"wake_generation": 4})), None);
        assert_eq!(wake_identity(&json!({"wake_seq": 19})), None);
        assert_eq!(
            wake_identity(&json!({"wake_generation": "4", "wake_seq": 19})),
            None
        );
    }
}
