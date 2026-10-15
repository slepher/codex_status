//! BLE central for device discovery, endpoint handoff, and authenticated commands.

mod sim_peripheral;

use std::collections::HashSet;
use std::future::Future;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use bridge_core::rendezvous::CLOSE_BUDGET;
use btleplug::api::{
    Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter, ValueNotification, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use serde_json::json;
use uuid::Uuid;

static NEXT_SCAN_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_ENDPOINT_ID: AtomicU64 = AtomicU64::new(1);

/// Finite, credential-free registration feedback shared by UI/MCP and rendezvous.
#[derive(Debug)]
pub struct EndpointFailure {
    pub outcome: &'static str,
    pub stage: &'static str,
    pub error: &'static str,
}
impl EndpointFailure {
    pub fn feedback(&self) -> serde_json::Value {
        json!({"state":if self.outcome == "unknown" {"awaiting_confirmation"} else {"failed"},
            "outcome":self.outcome,"stage":self.stage,"error":self.error,
            "message":self.to_string(),
            "action":match self.error {
                "storage" => "offline_migration_or_storage_repair",
                "physical_required" | "binding_required" | "binding_conflict" => "press_boot_and_restore_binding",
                "capacity" => "review_registered_bridges",
                "update_required" => "update_matching_firmware",
                _ if self.outcome == "unknown" => "read_registration_before_retry",
                _ => "correct_registration_and_retry",
            }})
    }
}
impl std::fmt::Display for EndpointFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "endpoint {}: {} ({})", self.outcome, self.error, self.stage)
    }
}
impl std::error::Error for EndpointFailure {}

// 0: idle, 1: native operation/link reserved, 2: unknown; restart is required.
static NATIVE_RADIO_STATE: AtomicU8 = AtomicU8::new(0);
// Simulation uses the production reservation logic, isolated from the physical radio.
static SIM_RADIO_STATE: AtomicU8 = AtomicU8::new(0);
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

#[derive(Debug)]
struct NativeLease<'a> { state: &'a AtomicU8, stopped: bool }
impl<'a> NativeLease<'a> {
    fn acquire(state: &'a AtomicU8) -> Result<Self> {
        state.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|value| anyhow!(if value == 2 { "ble_native_quarantined_restart_required" } else { "ble_native_busy" }))?;
        Ok(Self { state, stopped: false })
    }
}
impl Drop for NativeLease<'_> {
    fn drop(&mut self) {
        self.state.store(if self.stopped { 0 } else { 2 }, Ordering::Release);
        if !self.stopped {
            tracing::warn!(error = "ble_native_quarantined_restart_required",
                action = "restart_bridge", "BLE native shutdown is unconfirmed; further BLE work blocked");
        }
    }
}

/// The production scan and fault adapter share this entire start/work/stop path.
/// Native cancellation is not proof that the platform watcher has stopped.
async fn scan_with_cleanup<T, F>(state: &AtomicU8, limit: Duration, cleanup_budget: Duration,
    start: impl Future<Output = Result<()>>,
    scan: impl FnOnce(tokio::time::Instant) -> F,
    stop: impl Future<Output = Result<()>>) -> Result<Option<T>>
where F: Future<Output = Result<Option<T>>>
{
    anyhow::ensure!(limit > cleanup_budget, "ble_scan_budget_exhausted");
    let mut lease = NativeLease::acquire(state)?;
    let deadline = tokio::time::Instant::now() + limit;
    let work_deadline = deadline - cleanup_budget.min(limit);
    let started = tokio::time::timeout_at(work_deadline, start).await;
    let start_unknown = started.is_err();
    let result = match started {
        Ok(Ok(())) => match tokio::time::timeout_at(work_deadline, scan(work_deadline)).await {
            Ok(result) => result,
            Err(_) => Err(anyhow!("ble_scan_work_timeout")),
        },
        Ok(Err(error)) => Err(error.context("ble_scan_start_failed")),
        Err(_) => Err(anyhow!("ble_scan_start_timeout")),
    };
    match tokio::time::timeout_at(deadline, stop).await {
        Ok(Ok(())) => lease.stopped = !start_unknown,
        Ok(Err(error)) => return Err(result.err().unwrap_or(error).context("ble_scan_stop_failed")),
        Err(_) => return Err(result.err().unwrap_or_else(|| anyhow!("ble_scan_stop_timeout"))
            .context("ble_scan_stop_timeout")),
    }
    result
}

async fn disconnect_bounded(
    scan_id: Option<u64>,
    limit: Duration,
    disconnect: impl Future<Output = btleplug::Result<()>>,
) -> bool {
    let started = Instant::now();
    tracing::info!(scan_id, stage = "disconnect", limit_ms = limit.as_millis() as u64,
        "BLE cleanup started");
    match tokio::time::timeout(limit, disconnect).await {
        Ok(Ok(())) => { tracing::info!(scan_id, stage = "disconnect", result = "ok",
            elapsed_ms = started.elapsed().as_millis() as u64, "BLE cleanup completed"); true },
        Ok(Err(_)) => { tracing::warn!(scan_id, stage = "disconnect", category = "disconnect_failed",
            elapsed_ms = started.elapsed().as_millis() as u64, "BLE cleanup failed; link abandoned"); false },
        Err(_) => { tracing::warn!(scan_id, stage = "disconnect", category = "disconnect_timeout",
            elapsed_ms = started.elapsed().as_millis() as u64, limit_ms = limit.as_millis() as u64,
            "BLE cleanup budget exhausted; link abandoned"); false },
    }
}

async fn open_link<'a, T, F>(state: &'a AtomicU8, scan_id: u64, limit: Duration,
    mut connect: impl FnMut() -> F,
    setup: impl Future<Output = Result<T>>,
    disconnect: impl Future<Output = btleplug::Result<()>>) -> Result<(T, NativeLease<'a>)>
where F: Future<Output = Result<()>>
{
    anyhow::ensure!(limit > CLOSE_BUDGET, "ble_open_budget_exhausted");
    let mut lease = NativeLease::acquire(state)?;
    let deadline = tokio::time::Instant::now() + limit;
    let work_deadline = deadline - CLOSE_BUDGET;
    let result = async {
        for attempt in 0..2 {
            anyhow::ensure!(tokio::time::Instant::now() < work_deadline, "ble_open_budget_exhausted");
            // Retry only a completed native error. A timeout may still connect
            // later, so a second native connect would overlap the first.
            match tokio::time::timeout_at(work_deadline.min(tokio::time::Instant::now() + Duration::from_secs(4)), connect()).await {
                Ok(Ok(())) => return tokio::time::timeout_at(work_deadline, setup).await.context("ble_setup_timeout")?,
                Ok(Err(error)) if attempt == 1 => return Err(error.context("ble_connect_failed")),
                Ok(Err(_)) => tokio::time::sleep_until(work_deadline.min(tokio::time::Instant::now() + Duration::from_millis(120))).await,
                Err(error) => return Err(anyhow::Error::new(error).context("ble_connect_timeout")),
            }
        }
        unreachable!()
    }.await;
    match result {
        Ok(value) => Ok((value, lease)),
        Err(error) => {
            let unknown = error.downcast_ref::<tokio::time::error::Elapsed>().is_some();
            let stopped = disconnect_bounded(Some(scan_id), deadline.saturating_duration_since(tokio::time::Instant::now()).min(CLOSE_BUDGET), disconnect).await;
            lease.stopped = stopped && !unknown;
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

pub struct Pusher;

impl Pusher {
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
        let mut lease = NativeLease::acquire(&NATIVE_RADIO_STATE)?;
        let result = tokio::time::timeout(Duration::from_secs(5), async {
            let manager = Manager::new().await.context("bluetooth manager")?;
            let adapter = manager
                .adapters()
                .await
                .context("bluetooth adapters")?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("no bluetooth adapter"))?;
            Ok::<_, anyhow::Error>(adapter)
        }).await;
        lease.stopped = result.is_ok();
        let result = result.context("ble_adapter_timeout")?;
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
        // Keep the existing observation window; add only the bounded cleanup
        // reserve. Start and all advertisement lookups consume that window.
        let result = scan_with_cleanup(&NATIVE_RADIO_STATE, timeout + CLOSE_BUDGET, CLOSE_BUDGET,
            async { adapter.start_scan(ScanFilter::default()).await.context("start scan") },
            |deadline| async move {
                let mut events = match adapter.events().await {
                    Ok(events) => events,
                    Err(error) => {
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
                let mut counts = ScanCounts::default();
                let mut seen_candidates = HashSet::new();
                loop {
                    let event = match tokio::time::timeout_at(deadline, events.next()).await {
                        Ok(Some(event)) => event,
                        Ok(None) => {
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
                        Self::log_scan_end(scan_id, started, "matched", &counts);
                        return Ok(Some((scan_id, found)));
                    }
                }
            },
            async { adapter.stop_scan().await.context("stop scan") }).await;
        if result.is_err() {
            tracing::warn!(scan_id, stage = "scan_lifecycle",
                quarantined = NATIVE_RADIO_STATE.load(Ordering::Acquire) == 2,
                duration_ms = started.elapsed().as_millis(), "BLE scan lifecycle failed");
        }
        result
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
        let address = peripheral.id().to_string();
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
            peripheral_id = %address,
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

    async fn read_info_raw(peripheral: &Peripheral, scan_id: u64) -> Result<serde_json::Value> {
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
        let info = decode_wire(&raw).context("parse device info")?;
        Ok(info)
    }

    async fn read_info(peripheral: &Peripheral, scan_id: u64) -> Result<serde_json::Value> {
        let info = Self::read_info_raw(peripheral, scan_id).await?;
        if !Self::peer_bonded(&info) {
            bail!("device is not bonded; open its 120 second pairing window using the USB pair command, then pair and retry")
        }
        Ok(info)
    }

    #[cfg(any(test, windows, target_os = "macos"))]
    fn pairing_probe_allowed(info: &serde_json::Value, expected_mac: &str) -> bool {
        Self::info_matches_mac(info, expected_mac)
            && (Self::peer_bonded(info) || info["pairingWindow"] == true)
    }

    fn link_auth_budget(info: &serde_json::Value, expected_mac: &str, explicit_pairing: bool) -> Result<Duration> {
        anyhow::ensure!(Self::info_matches_mac(info, expected_mac), "BLE device Wi-Fi MAC does not match target");
        anyhow::ensure!(explicit_pairing || Self::peer_bonded(info), "ble_pairing_required_use_explicit_pairing");
        anyhow::ensure!(Self::pairing_probe_allowed(info, expected_mac), "open the device pairing window before first pairing");
        Ok(Duration::from_secs(if explicit_pairing { 60 } else { 3 }))
    }

    async fn token_link_info(peripheral: &Peripheral, scan_id: u64, expected_mac: &str, explicit_pairing: bool) -> Result<serde_json::Value> {
        let info = Self::read_info_raw(peripheral, scan_id).await?;
        if !Self::info_matches_mac(&info, expected_mac) { bail!("BLE device Wi-Fi MAC does not match target"); }
        #[cfg(any(windows, target_os = "macos"))]
        if !Self::peer_encrypted_bonded(&info) {
            let budget = Self::link_auth_budget(&info, expected_mac, explicit_pairing)?;
            // Accessing an encrypted characteristic triggers CoreBluetooth's
            // system pairing prompt after the complete MAC has been checked.
            // This probe does not issue tokens or alter the endpoint.
            tokio::time::timeout(budget,
                Self::write_json(peripheral, CHR_AUTH, br#"{"cmd":"pair_probe"}"#))
                .await.context("system pairing timed out")??;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            loop {
                let fresh = tokio::time::timeout_at(deadline, Self::read_info_raw(peripheral, scan_id))
                    .await.context("encrypted link verification timed out")??;
                if !Self::info_matches_mac(&fresh, expected_mac) { bail!("BLE identity changed during pairing"); }
                if Self::peer_encrypted_bonded(&fresh) { return Ok(fresh); }
                if tokio::time::Instant::now() >= deadline { bail!("BLE pairing did not establish a bonded encrypted link"); }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        if !Self::peer_encrypted_bonded(&info) { bail!("BLE device link is not encrypted and bonded"); }
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

    fn endpoint_payload_for(host: &str,port:u16,token:&str,principal:&str,request_id:&str,check_only:bool)->Vec<u8> {
        json!({"schema":2,"host":host,"port":port,"token":token,"bridge_id":principal,"request_id":request_id,"check_only":check_only}).to_string().into_bytes()
    }
    /// Explicit bootstrap operations use the same native ownership and failure
    /// cleanup as rendezvous links; successful reads keep their result even if
    /// close fails, while the native reservation remains quarantined.
    async fn on_link<T>(peripheral: &Peripheral, scan_id: u64, limit: Duration,
        work: impl Future<Output = Result<T>>) -> Result<T>
    {
        let deadline = tokio::time::Instant::now() + limit;
        let (value, mut lease) = open_link(&NATIVE_RADIO_STATE, scan_id, limit,
            || async { peripheral.connect().await.context("connect") },
            async {
                tokio::time::timeout(Duration::from_secs(3), peripheral.discover_services())
                    .await.context("discover timeout")?.context("discover")?;
                Self::log_service_lookup(peripheral, scan_id);
                work.await
            }, peripheral.disconnect()).await?;
        lease.stopped = disconnect_bounded(Some(scan_id),
            deadline.saturating_duration_since(tokio::time::Instant::now()).min(CLOSE_BUDGET),
            peripheral.disconnect()).await;
        Ok(value)
    }

    /// Explicit discovery returns an identity hint and never writes credentials.
    pub async fn read_device_info(adapter: &Adapter, name_prefix: &str,
        scan_timeout_ms: u64) -> Result<serde_json::Value>
    {
        let scan = Duration::from_millis(scan_timeout_ms.max(1000));
        let (scan_id, peripheral) = Self::wait_for_device(adapter, name_prefix, scan).await?;
        Self::on_link(&peripheral, scan_id, Duration::from_secs(18),
            Self::read_info(&peripheral, scan_id)).await
    }

    /// Candidate INFO is untrusted until the full MAC is verified during pairing.
    pub async fn read_candidate_info() -> Result<serde_json::Value> {
        let adapter = Self::adapter().await?;
        let Some((scan_id, peripheral)) = Self::find_device(&adapter, "CodexStatus-", Duration::from_secs(8)).await? else {
            bail!("no advertising device");
        };
        Self::on_link(&peripheral, scan_id, Duration::from_secs(12),
            Self::read_info_raw(&peripheral, scan_id)).await
    }

    /// Explicit pairing/token request. Full identity is checked before the
    /// encrypted probe; only this user-requested path can open first pairing.
    pub async fn request_device_token(adapter: &Adapter, name_prefix: &str,
        expected_mac: &str, scan_timeout_ms: u64) -> Result<String>
    {
        anyhow::ensure!(bridge_core::platform::model::DeviceIdentity::normalized_mac(expected_mac).is_some(),
            "invalid target device MAC");
        let scan = Duration::from_millis(scan_timeout_ms.max(1000));
        let (scan_id, peripheral) = Self::wait_for_device(adapter, name_prefix, scan).await?;
        Self::on_link(&peripheral, scan_id, Duration::from_secs(90), async {
            Self::token_link_info(&peripheral, scan_id, expected_mac, true).await?;
            tracing::info!(scan_id, verified_mac = expected_mac, "BLE device identity verified");
            Self::request_token_on_peripheral(&peripheral).await
        }).await
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
    pub info: serde_json::Value,
    peripheral: Option<Peripheral>,
    native_lease: Option<NativeLease<'static>>,
    fake_url: Option<String>,
    rf_deadline: Option<tokio::time::Instant>,
    command_budget_deadline: Option<tokio::time::Instant>,
    write_uncertain: AtomicBool,
    endpoint_unconfirmed: AtomicBool,
    token: String,
    bridge_id: String,
    nonce: String,
    session: [u8; 16],
    next_request_no: u64,
    context_id: String,
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

// The only BLE-specific schema translation. Legacy ROMs continue through
// their bounded reader; capability v1 is explicit and cannot masquerade as it.
pub fn decode_wire(raw: &[u8]) -> Result<serde_json::Value> {
    if raw.is_empty() || raw.len() > 512 { bail!("invalid BLE attribute length"); }
    let mut value: serde_json::Value = serde_json::from_slice(raw)?;
    let Some(version) = value.get("ble_wire") else { return Ok(value); };
    if version != 1 { bail!("unsupported BLE wire version"); }
    if let Some(contact) = value.get("ac").cloned() {
        if !contact.is_boolean() { bail!("invalid automatic contact flag"); }
        value["automatic_contact"] = contact;
    }
    if let Some(error) = value.get("ne") {
        value["network_error"] = json!(match error.as_u64() {
            Some(1) => "wifi_connect_failed",
            Some(2) => "bridge_http_unreachable",
            Some(3) => "wifi_link_lost",
            _ => bail!("invalid network failure category"),
        });
    }
    fn text(value: &serde_json::Value, max: usize) -> Result<String> {
        use base64::Engine;
        let encoded = value.as_str().context("BLE encoded identifier missing")?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        if bytes.len() > max || bytes.contains(&0) { bail!("invalid BLE identifier"); }
        Ok(String::from_utf8(bytes)?)
    }
    if value.get("er").is_some() { value["endpoint_registration"] = value["er"].clone(); }
    if value.get("f").is_some() { value["fw"] = json!(text(&value["f"], 64)?); }
    if value.get("rid").is_some() {
        let request = text(&value["rid"], 64)?;
        if request.is_empty() { bail!("empty BLE request identity"); }
        value["request_id"] = json!(request);
        if value["ack"] == "endpoint" {
            value["bridge_id"] = json!(text(&value["pid"], 64)?);
        } else {
            let wake = value["w"].as_array().context("BLE wake missing")?;
            if wake.len() != 2 || wake.iter().any(|n| n.as_u64().is_none_or(|n| n > u32::MAX as u64)) {
                bail!("invalid BLE wake identity");
            }
            let wake = wake.clone();
            value["wake_generation"] = wake[0].clone(); value["wake_seq"] = wake[1].clone();
        }
    }

    if value.get("ctx").is_some() { value["active_context_id"] = json!(text(&value["ctx"], 32)?); }
    if let Some(summary) = value.get("s") {
        let a = summary.as_array().context("invalid BLE summary")?.clone();
        if a.len() != 16 { bail!("invalid BLE summary length"); }
        let nonce = a[0].as_str().context("BLE nonce missing")?;
        if nonce.len() != 32 || !nonce.bytes().all(|c| c.is_ascii_hexdigit()) { bail!("invalid BLE nonce"); }
        for i in [4,6,7,8,11,13,15] {
            let n = a[i].as_u64().context("BLE counter missing")?;
            let max = if matches!(i, 11 | 13) { u8::MAX as u64 } else if matches!(i, 7 | 8 | 15) { u32::MAX as u64 } else { u64::MAX };
            if n > max { bail!("invalid BLE counter"); }
        }
        for i in [5,9,10,12,14] { if !a[i].is_boolean() { bail!("invalid BLE flag"); } }
        value["result"] = json!("applied"); value["session_nonce"] = a[0].clone();
        value["active_context_id"] = json!(text(&a[1], 32)?);
        value["active_template_id"] = json!(text(&a[2], 16)?);
        value["committed_job_id"] = json!(text(&a[3], 16)?);
        value["applied_seq"] = a[4].clone(); value["commit_seq"] = a[15].clone();
        value["power"] = json!({"mode":if a[5] == true {"light"} else {"sleep"},
            "plan_id":a[6],"remaining_s":a[7],"provisional_remaining_s":a[8]});
        if a[9] == true { value["sync"] = json!({"v":1,"enabled":a[10],"rounds":a[11],
            "due":a[12],"retry_skip":a[13],"pending":a[14]}); }
    }
    Ok(value)
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
                let mut native_lease = if std::env::var("CODEX_STATUS_SIM_BLE_LIFECYCLE").as_deref() == Ok("1") {
                    Some(NativeLease::acquire(&SIM_RADIO_STATE)?)
                } else { None };
                let info_started = tokio::time::Instant::now();
                let response = match client.get(format!("{}/sim/ble/info?connect=1", url.trim_end_matches('/')))
                    .bearer_auth(token).send().await {
                    Ok(response) => response,
                    Err(error) => {
                        // A refused carrier connection sent no connect request;
                        // a timeout after dispatch may have connected the device.
                        if error.is_connect() {
                            if let Some(lease) = native_lease.as_mut() { lease.stopped = true; }
                        }
                        tracing::warn!(device_mac = mac, %error, "fake BLE candidate unavailable");
                        continue;
                    }
                };
                if matches!(response.status().as_u16(), 400 | 401 | 403 | 404) {
                    if let Some(lease) = native_lease.as_mut() { lease.stopped = true; }
                }
                if response.status() == reqwest::StatusCode::NOT_FOUND { continue; }
                let response = response.error_for_status()?;
                let generation = response.headers().get("x-sim-ble-generation").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).context("fake BLE connection generation missing")?;
                let mut info = decode_wire(&response.bytes().await?)?;
                info["_sim_generation"] = json!(generation);
                if native_lease.is_some() {
                    let result=tokio::time::timeout(Duration::from_secs(4),
                        sim_peripheral::discover(url.trim_end_matches('/'),token,generation)).await;
                    if !matches!(&result,Ok(Ok(()))) {
                        let stopped=disconnect_bounded(None,CLOSE_BUDGET,
                            sim_peripheral::disconnect(url.trim_end_matches('/'),token,generation)).await;
                        if let Some(lease)=native_lease.as_mut(){lease.stopped=stopped && result.is_ok();}
                        return match result {
                            Ok(Err(error))=>Err(error.context("ble_connect_failed")),
                            Err(error)=>Err(anyhow::Error::new(error).context("ble_connect_timeout")),
                            Ok(Ok(()))=>unreachable!(),
                        };
                    }
                }
                let authorized_mac = info_authorized_target(&info, &targets)
                    .context("fake BLE device identity mismatch")?;
                if authorized_mac != *mac || info["rendezvous"] != true {
                    bail!("fake BLE rendezvous identity/version mismatch");
                }
                let device_mac = authorized_mac.as_bytes().chunks(2)
                    .map(|p| std::str::from_utf8(p).unwrap())
                    .collect::<Vec<_>>().join(":");
                return Ok(Some((authorized_mac, Self {
                    info: info.clone(),
                    peripheral: None, native_lease, fake_url: Some(url.trim_end_matches('/').to_owned()),
                    write_uncertain: std::sync::atomic::AtomicBool::new(false),
                    endpoint_unconfirmed: std::sync::atomic::AtomicBool::new(false),
                    command_budget_deadline: None,
                    rf_deadline: info["rf_remaining_ms"].as_u64().map(|ms| info_started + Duration::from_millis(ms)),
                    token: token.to_owned(), bridge_id: bridge_id.to_owned(),
                    nonce: String::new(), session: [0; 16], next_request_no: 1, context_id: String::new(), device_mac,
                    auth_ready: Pusher::peer_encrypted_bonded(&info),
                    wake_identity: wake_identity(&info),
                    wake_cause: text_field(&info, &["wake_cause"]).unwrap_or_default(),
                    device_last_stage: "info".into(), timings: vec![],
                })));
            }
            return Ok(None);
        }
        let open_deadline = tokio::time::Instant::now() + Duration::from_secs(25);
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
        let info_started = tokio::time::Instant::now();
        let connect_start = Instant::now();
        let ((info, authorized_mac), native_lease) = open_link(&NATIVE_RADIO_STATE, scan_id,
            open_deadline.saturating_duration_since(tokio::time::Instant::now()).min(Duration::from_secs(18)),
            || async { peripheral.connect().await.map_err(anyhow::Error::from) },
            async {
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
                            Pusher::read_info_raw(&peripheral, scan_id),
                        )
                        .await
                        .context("info timeout")
                        .and_then(|result| result.map_err(|e| e.context("info")))
                    })
                    .await?;
                    stages.push(("info".to_string(), info_ms));
                    Ok::<_, anyhow::Error>((info, stages))
                }
                .await;
                let (mut info, stages) = setup?;
                timings.extend(stages);
                let Some(authorized_mac) = info_authorized_target(&info, &targets) else {
                    tracing::warn!(
                        scan_id,
                        stage = "verify_mac",
                        category = error_category("verify_mac"),
                        "BLE device identity mismatch"
                    );
                    bail!("BLE device identity mismatch");
                };
                if !Pusher::peer_encrypted_bonded(&info) {
                    info = Pusher::token_link_info(&peripheral, scan_id, &authorized_mac, false).await?;
                }
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
                    bail!("device BLE rendezvous is disabled");
                }
                Ok((info, authorized_mac))
            }, peripheral.disconnect()).await?;
        let initial_wake_identity = wake_identity(&info);
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
                info: info.clone(),
                peripheral: Some(peripheral),
                native_lease: Some(native_lease),
                fake_url: None,
                write_uncertain: std::sync::atomic::AtomicBool::new(false),
                endpoint_unconfirmed: std::sync::atomic::AtomicBool::new(false),
                command_budget_deadline: None,
                rf_deadline: info["rf_remaining_ms"].as_u64().map(|ms| info_started + Duration::from_millis(ms)),
                token: token.to_owned(),
                bridge_id: bridge_id.to_owned(),
                nonce: String::new(),
                session: [0; 16],
                next_request_no: 1,
                context_id: String::new(),
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

    fn command_deadline(&self) -> tokio::time::Instant {
        let normal = tokio::time::Instant::now() + Duration::from_secs(5);
        let deadline = self.rf_deadline.map_or(normal, |rf| rf.min(normal));
        self.command_budget_deadline.map_or(deadline, |phase| phase.min(deadline))
    }

    /// A shared phase deadline belongs inside the carrier: an application
    /// timeout must not drop a fragmented write before its outcome is recorded.
    /// Call once on phase entry; individual commands cannot renew this budget.
    pub fn set_command_budget(&mut self, budget: Option<bridge_core::rendezvous::Budget>) {
        self.command_budget_deadline = budget.map(|b| tokio::time::Instant::now() + b.remaining());
    }

    /// Refresh only from the same authenticated peripheral/wake. A new phase
    /// may advertise a larger RF budget; repeated commands alone never renew it.
    pub async fn refresh_info(&mut self, limit: Duration) -> Result<()> {
        let started = tokio::time::Instant::now();
        let previous_remaining = self.rf_deadline.map(|rf|rf.saturating_duration_since(started).as_millis());
        let info = tokio::time::timeout(limit, async {
            if let Some(url) = &self.fake_url {
                let response = reqwest::Client::new().get(format!("{url}/sim/ble/info"))
                    .header("x-sim-ble-generation", self.info["_sim_generation"].as_u64().unwrap_or(0).to_string())
                    .bearer_auth(&self.token).send().await?.error_for_status()?;
                decode_wire(&response.bytes().await?)
            } else {
                Pusher::read_info_raw(self.peripheral.as_ref().context("BLE peripheral missing")?, 0).await
            }
        }).await.context("BLE info refresh timed out")??;
        anyhow::ensure!(bridge_core::platform::model::DeviceIdentity::normalized_mac(info["mac"].as_str().unwrap_or(""))
            == bridge_core::platform::model::DeviceIdentity::normalized_mac(&self.device_mac), "BLE refreshed MAC mismatch");
        if let Some(expected) = self.wake_identity {
            anyhow::ensure!(wake_identity(&info) == Some(expected), "BLE refreshed wake mismatch");
        }
        let generation = self.info["_sim_generation"].clone();
        self.info = info;
        if !generation.is_null() { self.info["_sim_generation"] = generation; }
        if let Some(ms) = self.info["rf_remaining_ms"].as_u64() {
            self.rf_deadline = Some(started + Duration::from_millis(ms));
            tracing::info!(device_mac=%self.device_mac,stage="info_refresh",previous_remaining_ms=previous_remaining,
                rf_remaining_ms=ms,duration_ms=started.elapsed().as_millis(),"authenticated RF phase refreshed");
        }
        Ok(())
    }

    async fn control_write_until(&self, deadline: tokio::time::Instant,
        write: impl Future<Output = Result<()>>) -> Result<()>
    {
        anyhow::ensure!(tokio::time::Instant::now() < deadline, "device BLE RF window expired before write");
        anyhow::ensure!(!self.endpoint_unconfirmed.load(Ordering::Acquire), "previous_write_unknown");
        self.write_uncertain.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| anyhow!("previous_write_unknown"))?;
        // Cancellation drops only the future; keep the flag until the complete
        // native write returns success. Endpoint/auth obey the same rule as CMD.
        tokio::time::timeout_at(deadline, write).await.context("device BLE control write timed out")??;
        self.write_uncertain.store(false, Ordering::Release);
        Ok(())
    }

    /// Refresh the endpoint credentials on this already MAC-verified link.
    /// The caller must verify the registered target before invoking this.
    pub async fn write_endpoint(&self, host: &str, port: u16, token: &str) -> Result<()> {
        if self.info["endpoint_registration"] != 1 {
            return Err(EndpointFailure {outcome:"not_sent",stage:"preflight",error:"update_required"}.into());
        }
        let deadline = self.command_deadline();
        match self.write_endpoint_until(host, port, token, true, deadline).await {
            Ok(()) => Ok(()),
            Err(error) if error.downcast_ref::<EndpointFailure>().is_some_and(|e|
                e.outcome == "rejected" && e.stage == "verify" && e.error == "registration_mismatch") =>
                self.write_endpoint_until(host, port, token, false, deadline).await,
            Err(error) => Err(error),
        }
    }

    async fn write_endpoint_until(&self, host: &str, port: u16, token: &str, check_only: bool,
        deadline: tokio::time::Instant) -> Result<()>
    {
        let failure = |outcome, stage, error| EndpointFailure {outcome, stage, error};
        if !self.auth_ready {
            return Err(failure("not_sent", "preflight", "binding_required").into());
        }
        if host.is_empty() || host.len() > 64 || host.contains('\0') || port == 0 ||
            token.is_empty() || token.len() > 128 || token.contains('\0') ||
            self.bridge_id.is_empty() || self.bridge_id.len() > 64 || self.bridge_id.contains('\0') {
            return Err(failure("not_sent", "preflight", "shape").into());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(failure("not_sent", "preflight", "window_budget_exhausted").into());
        }
        let status = self.peripheral.as_ref().and_then(|p| p.characteristics().into_iter()
            .find(|c| c.uuid == Pusher::uuid(CHR_STATUS)));
        if status.is_none() && self.fake_url.is_none() {
            return Err(failure("not_sent", "preflight", "characteristic_missing").into());
        }
        // Correlate this registration across stale attributes and earlier links.
        let id = format!("ep{:x}-{:x}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),
            NEXT_ENDPOINT_ID.fetch_add(1, Ordering::Relaxed));
        let payload = Pusher::endpoint_payload_for(host, port, token, &self.bridge_id, &id, check_only);
        if self.endpoint_unconfirmed.load(Ordering::Acquire) {
            return Err(failure("not_sent", "preflight", "previous_write_unknown").into());
        }
        self.write_uncertain.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| failure("not_sent", "preflight", "previous_write_unknown"))?;
        self.endpoint_unconfirmed.store(true, Ordering::Release);
        // Keep the link blocked through ACK confirmation, including cancellation.
        // A GATT write response only proves receipt, never durable registration.
        let written = tokio::time::timeout_at(deadline, async {
            if let Some(url) = &self.fake_url {
                reqwest::Client::new().post(format!("{url}/sim/ble/endpoint"))
                    .header("x-sim-ble-generation", self.info["_sim_generation"].as_u64().unwrap().to_string())
                    .bearer_auth(&self.token).header("content-type", "application/json").body(payload).send().await?.error_for_status()?;
            } else {
                Pusher::write_json(self.peripheral.as_ref().unwrap(), CHR_ENDPOINT, &payload).await?;
            }
            Ok::<_, anyhow::Error>(())
        }).await;
        if !matches!(written, Ok(Ok(()))) {
            return Err(failure("unknown", "write", "write_failed").into());
        }
        // Completed native writes can be safely disconnected even if their
        // business ACK is lost; do not quarantine the adapter for missing ACKs.
        self.write_uncertain.store(false, Ordering::Release);
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(failure("unknown", "ack", "window_budget_exhausted").into());
            }
            let raw = match tokio::time::timeout_at(deadline, self.read_attribute(status.as_ref())).await {
                Ok(Ok(raw)) => raw,
                Ok(Err(_)) => return Err(failure("unknown", "ack", "read_failed").into()),
                Err(_) => return Err(failure("unknown", "ack", "window_budget_exhausted").into()),
            };
            if let Ok(reply) = decode_wire(&raw) {
                if reply["ack"] == "endpoint" && reply["request_id"] == id &&
                    reply["bridge_id"] == self.bridge_id &&
                    bridge_core::platform::model::DeviceIdentity::normalized_mac(reply["device_mac"].as_str().unwrap_or("")) ==
                    bridge_core::platform::model::DeviceIdentity::normalized_mac(&self.device_mac) {
                    if reply["ok"] == true && reply["result"] == "applied" && reply["stage"] == if check_only {"verified"} else {"persisted"} {
                        self.endpoint_unconfirmed.store(false, Ordering::Release);
                        return Ok(());
                    }
                    let error = match reply["error"].as_str().unwrap_or("") {
                        "registration_mismatch" => "registration_mismatch",
                        "shape" => "shape", "binding_required" => "binding_required",
                        "storage" => "storage", "storage_unknown" => "storage_unknown",
                        "token_conflict" => "token_conflict", "binding_conflict" => "binding_conflict",
                        "physical_required" => "physical_required", "capacity" => "capacity",
                        "memory" => "memory", "binding_ambiguous" => "binding_ambiguous",
                        _ => "invalid_ack",
                    };
                    let stage = match reply["stage"].as_str().unwrap_or("") {
                        "validate" => "validate", "binding" => "binding", "persist" => "persist", "verify" => "verify",
                        _ => "ack",
                    };
                    if reply["ok"] == false && reply["result"] == "rejected" &&
                        error != "storage_unknown" && error != "invalid_ack" && stage != "ack" {
                        self.endpoint_unconfirmed.store(false, Ordering::Release);
                        return Err(failure("rejected", stage, error).into());
                    }
                    return Err(failure("unknown", stage, error).into());
                }
            }
            tokio::time::sleep_until((tokio::time::Instant::now() + Duration::from_millis(20)).min(deadline)).await;
        }
    }

    /// Read token responses as attributes too: lost notification is not lost ACK.
    pub async fn request_device_token(&self) -> Result<String> {
        if !self.auth_ready { bail!("BLE device link is not encrypted and bonded"); }
        let deadline = self.command_deadline();
        if tokio::time::Instant::now() >= deadline { bail!("device BLE RF window expired before auth"); }
        let status = self.peripheral.as_ref().and_then(|p| p.characteristics().into_iter().find(|c| c.uuid == Pusher::uuid(CHR_STATUS)));
        self.control_write_until(deadline, async {
            if let Some(url) = &self.fake_url {
                reqwest::Client::new().post(format!("{url}/sim/ble/token"))
                    .header("x-sim-ble-generation", self.info["_sim_generation"].as_u64().unwrap().to_string())
                    .bearer_auth(&self.token).body(br#"{"cmd":"token"}"#.to_vec())
                    .send().await?.error_for_status()?;
            } else { Pusher::write_json(self.peripheral.as_ref().unwrap(), CHR_AUTH, br#"{"cmd":"token"}"#).await?; }
            Ok::<(), anyhow::Error>(())
        }).await.context("device BLE auth write failed")?;
        loop {
            let raw = tokio::time::timeout_at(deadline, self.read_attribute(status.as_ref())).await??;
            if let Ok(reply) = decode_wire(&raw) {
                if reply["ack"] == "auth" && reply["ok"] == true {
                    let token = reply["token"].as_str().context("invalid BLE token response")?;
                    // Fake seeds are explicit test credentials; real tokens remain hex.
                    if self.fake_url.is_none() && !Pusher::valid_device_token(token) { bail!("invalid BLE token response"); }
                    return Ok(token.to_owned());
                }
            }
            tokio::time::sleep_until((tokio::time::Instant::now() + Duration::from_millis(20)).min(deadline)).await;
        }
    }

    pub async fn command(
        &mut self,
        op: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = format!(
            "r{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        self.command_with_id(op, body, &id).await
    }

    pub async fn command_with_id(&mut self, op: &str, body: serde_json::Value, id: &str) -> Result<serde_json::Value> {
        self.command_with_id_budget(op, body, id, Duration::from_secs(5)).await
    }

    /// Keep the shared deadline inside the carrier so cancellation cannot erase
    /// whether no bytes, a partial frame, or a complete frame was dispatched.
    pub async fn command_with_id_budget(&mut self, op: &str, body: serde_json::Value, id: &str, limit: Duration) -> Result<serde_json::Value> {
        let deadline = self.command_deadline().min(tokio::time::Instant::now() + limit);
        let result = self.command_inner(op, body, id, deadline).await;
        result.map_err(|error| {
            if bridge_core::rendezvous::command_failure(&error).is_some() { error }
            else {
                let failure = self.command_failure(id, op, bridge_core::rendezvous::DispatchOutcome::NotSent,
                    "preflight", "command_preflight_failed", deadline, Duration::ZERO);
                error.context(failure)
            }
        })
    }

    async fn command_inner(&mut self, op: &str, mut body: serde_json::Value, id: &str, deadline: tokio::time::Instant) -> Result<serde_json::Value> {
        if self.write_uncertain.load(Ordering::Acquire) || self.endpoint_unconfirmed.load(Ordering::Acquire) {
            return Err(self.command_failure(id, op, bridge_core::rendezvous::DispatchOutcome::NotSent,
                "before_write", "previous_write_unknown", deadline, Duration::ZERO).into());
        }
        if id.is_empty() || id.len() > 64 || id.contains('\0') { bail!("invalid BLE request identity"); }
        if op.starts_with("wifi_") && self.nonce.is_empty() {
            // Obtain the ordinary session before a configuration command.
            // Physical rescue still works if endpoint auth has not been set up.
            let _ = Box::pin(self.command_with_id_budget("status", json!({}), "session",
                deadline.saturating_duration_since(tokio::time::Instant::now()))).await;
        }
        body["op"] = json!(op);
        body["request_id"] = json!(id);
        body["session_nonce"] = json!(self.nonce);
        body["bridge_id"] = json!(self.bridge_id);
        body["device_mac"] = json!(self.device_mac);
        body["token"] = json!(self.token);
        stamp_clock(&mut body, &self.device_mac);
        let payload = serde_json::to_vec(&body)?;
        let frame_op = bridge_core::compact_frame::operation(op)
            .with_context(|| format!("unregistered BLE operation {op}"))?;
        let session = if op == "status" { [0; 16] } else {
            anyhow::ensure!(self.session != [0; 16], "BLE session is not established");
            self.session
        };
        let mut context = [0; 16];
        let context_text = body["active_context_id"].as_str()
            .unwrap_or_else(|| if self.context_id.len() == 16 &&
                self.context_id.bytes().all(|b| b.is_ascii_hexdigit()) {
                &self.context_id
            } else { "" });
        if !context_text.is_empty() {
            anyhow::ensure!(context_text.len() == 16, "invalid BLE context length");
            context.copy_from_slice(context_text.as_bytes());
        }
        let request_no = self.next_request_no;
        self.next_request_no = self.next_request_no.checked_add(1)
            .context("BLE request number exhausted")?;
        let header = bridge_core::compact_frame::Header {
            kind: 0, op: frame_op, result: 0,
            payload_len: payload.len().try_into().context("BLE payload too large")?,
            session, request_no, context,
        };
        let mut bytes = header.encode().map_err(anyhow::Error::msg)?.to_vec();
        bytes.extend_from_slice(&payload);
        let wifi_limit=if self.info["wifi_management"]==3 {4096}else{1024};
        if op.starts_with("wifi_") && bytes.len() > wifi_limit { bail!("Wi-Fi BLE command exceeds {wifi_limit} bytes"); }
        if bytes.len() > 8192 {
            bail!("device BLE command exceeds 8192 bytes");
        }
        let lookup_start = Instant::now();
        let status = self
            .peripheral
            .as_ref().map(|p| p.characteristics()).unwrap_or_default()
            .into_iter()
            .find(|c| c.uuid == Pusher::uuid(CHR_STATUS))
            .context("status characteristic");
        let status = match status {
            Ok(status) => Some(status),
            Err(_) if self.fake_url.is_some() => None,
            Err(error) => {
                return Err(error.context(self.command_failure(id, op,
                    bridge_core::rendezvous::DispatchOutcome::NotSent, "status_characteristic",
                    "characteristic_missing", deadline, lookup_start.elapsed())));
            }
        };
        tracing::info!(device_mac = %self.device_mac, op, characteristic = "status", duration_ms = lookup_start.elapsed().as_millis(), "BLE ACK characteristic found");
        let write_start = Instant::now();
        if tokio::time::Instant::now() >= deadline {
            return Err(self.command_failure(id, op, bridge_core::rendezvous::DispatchOutcome::NotSent,
                "before_write", "window_budget_exhausted", deadline, write_start.elapsed()).into());
        }
        let write_budget_ms = deadline.saturating_duration_since(tokio::time::Instant::now()).as_millis();
        // Remains set if an outer future cancels the fragment write. Only a
        // complete write permits another command on this connection.
        self.write_uncertain.store(true, Ordering::Release);
        tracing::info!(device_mac=%self.device_mac,op,request_id=%id,stage="write_dispatch",
            budget_ms=write_budget_ms,"BLE command dispatch started; result not yet known");
        match tokio::time::timeout_at(deadline, self.write_command(&bytes)).await {
            Ok(Ok(())) => {self.write_uncertain.store(false, Ordering::Release);},
            Ok(Err(error)) => return Err(error.context(self.command_failure(id, op,
                bridge_core::rendezvous::DispatchOutcome::WriteUnknown, "write", "write_failed", deadline, write_start.elapsed()))),
            Err(_) => return Err(self.command_failure(id, op,
                bridge_core::rendezvous::DispatchOutcome::WriteUnknown, "write", "window_budget_exhausted", deadline, write_start.elapsed()).into()),
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
            budget_ms = write_budget_ms,
            remaining_ms = deadline.saturating_duration_since(tokio::time::Instant::now()).as_millis(),
            "BLE command written"
        );
        let started = Instant::now();
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(self.command_failure(id, op, bridge_core::rendezvous::DispatchOutcome::AckUnknown,
                    "ack", "window_budget_exhausted", deadline, started.elapsed()).into());
            }
            let raw = match tokio::time::timeout_at(deadline,
                self.read_attribute(status.as_ref())).await {
                Ok(Ok(raw)) => raw,
                Ok(Err(_)) => return Err(self.command_failure(id, op,
                    bridge_core::rendezvous::DispatchOutcome::AckUnknown, "ack", "read_failed", deadline, started.elapsed()).into()),
                Err(_) => return Err(self.command_failure(id, op,
                    bridge_core::rendezvous::DispatchOutcome::AckUnknown, "ack", "window_budget_exhausted", deadline, started.elapsed()).into()),
            };
            if let Ok((reply_header, reply_payload)) = bridge_core::compact_frame::Header::decode(&raw, 512) {
                if reply_header.kind != 1 || reply_header.op != frame_op ||
                    reply_header.request_no != request_no ||
                    (op != "status" && reply_header.session != session) {
                    // A stale readable characteristic is not a fresh reply.
                    // Bound polling on both a real radio and the Fake transport.
                    tokio::time::sleep_until((tokio::time::Instant::now() +
                        Duration::from_millis(20)).min(deadline)).await;
                    continue;
                }
                let Ok(reply) = decode_wire(reply_payload) else {
                    tokio::time::sleep_until((tokio::time::Instant::now() +
                        Duration::from_millis(20)).min(deadline)).await;
                    continue;
                };
                if reply["ack"] == if op.starts_with("wifi_") { "wifi" } else { "command" } && reply["request_id"] == id {
                    let ack_identity = wake_identity(&reply);
                    let mac_matches = reply["device_mac"].as_str().map_or(!op.starts_with("wifi_"), |mac|
                        bridge_core::platform::model::DeviceIdentity::normalized_mac(mac) ==
                        bridge_core::platform::model::DeviceIdentity::normalized_mac(&self.device_mac));
                    let wake_matches = !self.wake_identity.zip(ack_identity).is_some_and(|(a,b)| a != b);
                    let version_matches = self.info["ble_wire"] != 1 || (reply["ble_wire"] == 1 && ack_identity.is_some());
                    let operation_matches = reply["op"].as_str().map_or(true, |actual| actual == op || actual == "command" || (actual == "wifi" && op.starts_with("wifi_")));
                    let nonce_matches = op == "status" || !reply["session_nonce"].as_str().is_some_and(|n| n != self.nonce);
                    if !mac_matches || !wake_matches || !version_matches || !nonce_matches || !operation_matches {
                        tokio::time::sleep_until((tokio::time::Instant::now() + Duration::from_millis(20)).min(deadline)).await;
                        continue;
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
                        if reply_header.session == [0; 16] || reply["session_nonce"].as_str().is_none() {
                            return Err(self.command_failure(id, op, bridge_core::rendezvous::DispatchOutcome::AckUnknown,
                                "ack", "invalid_ack_session", deadline, started.elapsed()).into());
                        }
                        self.session = reply_header.session;
                        self.nonce = reply["session_nonce"]
                            .as_str()
                            .context("session nonce")?
                            .to_owned();
                        self.context_id = reply["active_context_id"]
                            .as_str().unwrap_or("").to_owned();
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
                        wake_association = if ack_identity.is_some() { "confirmed" } else { "incomplete" },
                        duration_ms = ms,
                        "BLE command acknowledged"
                    );
                    return Ok(reply);
                }
            }
            tokio::time::sleep_until((tokio::time::Instant::now() +
                Duration::from_millis(20)).min(deadline)).await;
            if tokio::time::Instant::now() >= deadline {
                return Err(self.command_failure(id, op, bridge_core::rendezvous::DispatchOutcome::AckUnknown,
                    "ack", "window_budget_exhausted", deadline, started.elapsed()).into());
            }
        }
    }

    async fn write_command(&self, bytes: &[u8]) -> Result<()> {
        if let Some(url) = &self.fake_url {
            let client = reqwest::Client::builder().timeout(Duration::from_secs(3)).build()?;
            for (index, chunk) in bytes.chunks(180).enumerate() {
                let final_chunk = (index + 1) * 180 >= bytes.len();
                let response = client.post(format!("{url}/sim/ble/fragment?offset={}&final={}", index * 180, u8::from(final_chunk)))
                    .header("x-sim-ble-generation", self.info["_sim_generation"].as_u64().unwrap().to_string())
                    .bearer_auth(&self.token).body(chunk.to_vec()).send().await?.error_for_status()?;
                let accepted: serde_json::Value = response.json().await?;
                if accepted["result"] != if final_chunk { "accepted" } else { "more" } { bail!("fake BLE write was not accepted"); }
            }
            Ok(())
        } else { Pusher::write_json(self.peripheral.as_ref().unwrap(), CHR_TPL_CTRL, bytes).await }
    }
    async fn read_attribute(&self, status: Option<&btleplug::api::Characteristic>) -> Result<Vec<u8>> {
        if let Some(url) = &self.fake_url {
            Ok(reqwest::Client::new().get(format!("{url}/sim/ble/status"))
                .header("x-sim-ble-generation", self.info["_sim_generation"].as_u64().unwrap().to_string())
                .bearer_auth(&self.token).send().await?.error_for_status()?.bytes().await?.to_vec())
        } else { Ok(self.peripheral.as_ref().unwrap().read(status.context("status characteristic")?).await?) }
    }

    /// ATT values are limited to 512 bytes. Read frozen Wi-Fi summaries through
    /// bounded hex pages on the existing Status attribute, without a new GATT table.
    pub async fn wifi_read(&mut self, op: &str) -> Result<serde_json::Value> {
        if !matches!(op, "wifi_scheme_get" | "wifi_capabilities" | "wifi_status") { bail!("invalid Wi-Fi read"); }
        let mut body = json!({"offset":0});
        let mut bytes = Vec::new();
        let mut snapshot = None;
        loop {
            let reply = self.command(op, body).await?;
            if reply["ok"] != true { bail!("Wi-Fi summary read rejected"); }
            let total = reply["total"].as_u64().context("Wi-Fi summary total")? as usize;
            let next = reply["next_offset"].as_u64().context("Wi-Fi summary offset")? as usize;
            let id = reply["snapshot"].as_u64().context("Wi-Fi summary snapshot")?;
            if snapshot.is_some_and(|previous| previous != id) || total > 4096 { bail!("Wi-Fi summary changed or exceeds limit"); }
            snapshot = Some(id);
            let chunk = reply["chunk"].as_str().context("Wi-Fi summary chunk")?;
            if chunk.is_empty() || chunk.len() > 240 || chunk.len() % 2 != 0 || !chunk.is_ascii() { bail!("invalid Wi-Fi summary chunk"); }
            for pair in chunk.as_bytes().chunks(2) {
                bytes.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
            }
            if bytes.len() != next || next > total { bail!("Wi-Fi summary offset mismatch"); }
            if next == total { return serde_json::from_slice(&bytes).context("Wi-Fi summary JSON"); }
            body = json!({"offset":next,"snapshot":id});
        }
    }

    /// Same frozen snapshot used by HTTP, carried in bounded GATT replies.
    pub async fn wifi_snapshot(&mut self) -> Result<serde_json::Value> {
        let mut offset=0usize;let mut snapshot=String::new();let mut revision=None;let mut total=None;let mut text=String::new();
        loop {
            let reply=self.command("wifi_snapshot",json!({"offset":offset,"snapshot_id":snapshot})).await?;
            if reply["ok"]!=true {bail!("{}",reply["error"].as_str().unwrap_or("wifi_snapshot_rejected"));}
            let id=reply["snapshot_id"].as_str().context("wifi_snapshot_id")?;
            let rev=reply["revision"].as_u64().context("wifi_revision")?;
            let size=reply["total"].as_u64().context("wifi_snapshot_length")? as usize;
            let next=reply["next_offset"].as_u64().context("wifi_snapshot_offset")? as usize;
            let chunk=reply["chunk"].as_str().context("wifi_snapshot_chunk")?;
            if reply["schema"]!=2 || size>16384 || reply["offset"]!=offset || next<=offset || next>size || chunk.len()!=next-offset ||
                (offset>0 && (id!=snapshot || revision!=Some(rev) || total!=Some(size))) {tracing::warn!(offset,next,size,chunk_bytes=chunk.len(),returned_offset=?reply["offset"].as_u64(),id_matches=id==snapshot,revision_matches=revision==Some(rev),total_matches=total==Some(size),"Wi-Fi snapshot page mismatch");bail!("wifi_snapshot_mismatch");}
            snapshot=id.to_owned();revision=Some(rev);total=Some(size);text.push_str(chunk);offset=next;
            if reply["complete"]==true {
                if offset!=size {bail!("wifi_snapshot_incomplete");}
                let parsed:bridge_core::platform::wifi::Snapshot=serde_json::from_str(&text)?;parsed.validate(&bridge_core::platform::model::DeviceIdentity::normalized_mac(&self.device_mac).context("invalid_device_mac")?)?;
                if parsed.snapshot_id!=snapshot || parsed.revision!=rev {bail!("wifi_snapshot_mismatch");}
                return Ok(serde_json::to_value(parsed)?);
            }
        }
    }

    fn command_failure(&self, id: &str, op: &str, outcome: bridge_core::rendezvous::DispatchOutcome,
        stage: &'static str, reason: &'static str, deadline: tokio::time::Instant, elapsed: Duration,
    ) -> bridge_core::rendezvous::CommandFailure {
        let failure = bridge_core::rendezvous::CommandFailure { outcome, stage, reason };
        self.log_failure(id, op, stage, failure.code());
        tracing::warn!(device_mac=%self.device_mac, op, request_id=%id, stage,
            category=failure.code(), reason, dispatch=?outcome, duration_ms=elapsed.as_millis(),
            budget_source=if self.rf_deadline.is_some_and(|rf|rf<=deadline) {"device_rf"}else{"shared_or_command"},
            remaining_ms=deadline.saturating_duration_since(tokio::time::Instant::now()).as_millis(),
            rf_remaining_ms=self.rf_deadline.map(|rf|rf.saturating_duration_since(tokio::time::Instant::now()).as_millis()),
            "BLE command failed");
        failure
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
        self.close_bounded(CLOSE_BUDGET).await;
    }

    /// Bounded variant of [`Self::close`] for callers that must not be blocked
    /// by link cleanup. Our btleplug patch acknowledges a peripheral already
    /// removed by a native disconnect callback. An actual native timeout or
    /// uncertain write still quarantines the shared
    /// radio reservation; dropping the handle is not proof of shutdown.
    /// The next native attempt returns a restart-required error.
    ///
    /// The future is cancelled in place rather than detached with
    /// `tokio::spawn`: a background disconnect would race the next rendezvous
    /// while btleplug's cached GATT objects are still alive (see `close`
    /// above).
    pub async fn close_bounded(mut self, limit: Duration) {
        if let Some(url) = &self.fake_url {
            if let Some(lease) = self.native_lease.as_mut() {
                lease.stopped = disconnect_bounded(None, limit,
                    sim_peripheral::disconnect(url, &self.token, self.info["_sim_generation"].as_u64().unwrap())).await
                    && !self.write_uncertain.load(Ordering::Acquire);
                return;
            }
            let request = reqwest::Client::new().post(format!("{url}/sim/ble/close"))
                .header("x-sim-ble-generation", self.info["_sim_generation"].as_u64().unwrap().to_string())
                .bearer_auth(&self.token).send();
            if tokio::time::timeout(limit, request).await.is_err() {
                tracing::warn!(device_mac = %self.device_mac, event = "close",
                    category = "disconnect_timeout", limit_ms = limit.as_millis() as u64,
                    "BLE link close did not complete; link abandoned");
            }
        }
        if let Some(peripheral) = self.peripheral {
            let stopped = disconnect_bounded(None, limit, peripheral.disconnect()).await;
            if let Some(lease) = self.native_lease.as_mut() { lease.stopped = stopped && !self.write_uncertain.load(Ordering::Acquire); }
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

    #[tokio::test]
    async fn scan_failure_cleans_up_then_the_next_scan_can_run() {
        use super::{scan_with_cleanup, AtomicU8, Ordering};
        let state = AtomicU8::new(0);
        let stops = std::cell::Cell::new(0);
        let error = scan_with_cleanup::<(), _>(&state, Duration::from_millis(100), Duration::from_millis(20),
            async { anyhow::bail!("injected_start_failure") },
            |_| async { panic!("failed start must not scan") },
            async { stops.set(stops.get()+1); Ok(()) }).await.unwrap_err();
        assert!(format!("{error:#}").contains("injected_start_failure"));
        assert_eq!(stops.get(), 1);
        assert_eq!(state.load(Ordering::Acquire), 0);
        assert_eq!(scan_with_cleanup(&state, Duration::from_millis(100), Duration::from_millis(20), async { Ok(()) },
            |_| async { Ok(Some(7)) }, async { Ok(()) }).await.unwrap(), Some(7));
        assert_eq!(state.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn hanging_scan_stages_return_and_uncertain_stop_prevents_native_reentry() {
        use super::{scan_with_cleanup, AtomicU8, Ordering};
        for stage in ["start", "events", "stop"] {
            let state = AtomicU8::new(0);
            let started = Instant::now();
            let error = scan_with_cleanup::<(), _>(&state, Duration::from_millis(80), Duration::from_millis(20),
                async { if stage == "start" { std::future::pending::<()>().await; } Ok(()) },
                |_| async { if stage == "events" { std::future::pending::<()>().await; } Ok(Some(())) },
                async { if stage == "stop" { std::future::pending::<()>().await; } Ok(()) }).await.unwrap_err();
            assert!(started.elapsed() < Duration::from_millis(500));
            assert!(error.to_string().contains(if stage == "events" { "work_timeout" } else if stage == "start" { "start_timeout" } else { "stop_timeout" }));
            if stage == "events" {
                assert_eq!(state.load(Ordering::Acquire), 0, "confirmed stop ends read-only scanning");
            } else {
                assert_eq!(state.load(Ordering::Acquire), 2);
                let error = scan_with_cleanup::<(), _>(&state, Duration::from_millis(80), Duration::from_millis(20),
                    async { panic!("quarantine must not issue native requests") },
                    |_| async { panic!("quarantine must not scan") },
                    async { panic!("quarantine must not spawn cleanup") }).await.unwrap_err();
                assert_eq!(error.to_string(), "ble_native_quarantined_restart_required");
            }
        }
    }

    #[tokio::test]
    async fn cancelled_scan_and_failed_stop_never_report_the_adapter_idle() {
        use super::{scan_with_cleanup, AtomicU8, Ordering};
        let state = AtomicU8::new(0);
        assert!(tokio::time::timeout(Duration::from_millis(10),
            scan_with_cleanup::<(), _>(&state, Duration::from_secs(1), Duration::from_millis(20), async { Ok(()) },
                |_| std::future::pending(), async { Ok(()) })).await.is_err());
        assert_eq!(state.load(Ordering::Acquire), 2);
        let state = AtomicU8::new(0);
        let result = scan_with_cleanup(&state, Duration::from_millis(100), Duration::from_millis(20), async { Ok(()) },
            |_| async { Ok(Some(7)) }, async { anyhow::bail!("injected_stop_failure") }).await;
        assert!(result.is_err(), "a match is unusable until scan shutdown is confirmed");
        assert_eq!(state.load(Ordering::Acquire), 2);
    }

    #[tokio::test]
    async fn scan_reservation_rejects_overlap_and_empty_budget_without_native_calls() {
        use super::{scan_with_cleanup, AtomicU8, Ordering};
        let state = AtomicU8::new(0);
        let mut active = Box::pin(scan_with_cleanup::<(), _>(&state, Duration::from_secs(1),
            Duration::from_millis(20), std::future::pending(),
            |_| async { Ok(None) }, async { Ok(()) }));
        assert!(futures::poll!(&mut active).is_pending());
        let error = scan_with_cleanup::<(), _>(&state, Duration::from_millis(80), Duration::from_millis(20),
            async { panic!("overlapping native start") }, |_| async { Ok(None) },
            async { panic!("overlapping native stop") }).await.unwrap_err();
        assert_eq!(error.to_string(), "ble_native_busy");
        drop(active);
        assert_eq!(state.load(Ordering::Acquire), 2);
        let state = AtomicU8::new(0);
        assert!(scan_with_cleanup::<(), _>(&state, Duration::ZERO, Duration::ZERO,
            async { panic!("expired native start") }, |_| async { Ok(None) },
            async { panic!("expired native stop") }).await.is_err());
        assert_eq!(state.load(Ordering::Acquire), 0);
        assert!(scan_with_cleanup::<(), _>(&state, Duration::from_millis(80), Duration::from_millis(20),
            async { Ok(()) }, |deadline| async move {
                tokio::time::sleep_until(deadline).await;
                Ok(None)
            }, async { Ok(()) }).await.unwrap().is_none());
        assert_eq!(state.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn failed_connect_cleanup_releases_lock_and_allows_next_attempt() {
        use super::{open_link, AtomicU8, Ordering, CLOSE_BUDGET};
        let state = AtomicU8::new(0);
        let attempts = std::cell::Cell::new(0);
        let error = open_link::<(), _>(&state, 1, CLOSE_BUDGET + Duration::from_millis(300),
            || { attempts.set(attempts.get()+1); std::future::ready(Err(anyhow::anyhow!("injected connect failure"))) },
            async { panic!("failed connect must not construct a link") }, async { Ok(()) }).await.unwrap_err();
        assert!(format!("{error:#}").contains("injected connect failure"));
        assert_eq!(attempts.get(), 2);
        assert_eq!(state.load(Ordering::Acquire), 0);
        let (value, mut lease) = open_link(&state, 2, CLOSE_BUDGET + Duration::from_millis(300),
            || async { Ok(()) }, async { Ok(7) }, std::future::pending()).await.unwrap();
        assert_eq!(value, 7);
        assert_eq!(state.load(Ordering::Acquire), 1, "the returned link owns the native radio");
        lease.stopped = true; drop(lease);
        assert_eq!(state.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn failed_native_open_and_unknown_cleanup_quarantine_without_retrying_native_work() {
        use super::{open_link, AtomicU8, Ordering, CLOSE_BUDGET};
        for stage in ["connect", "setup", "cleanup"] {
            let state = AtomicU8::new(0);
            let calls = std::cell::Cell::new(0);
            let result = tokio::time::timeout(Duration::from_secs(3), open_link::<(), _>(&state, 1,
                CLOSE_BUDGET + Duration::from_millis(150), || async {
                    calls.set(calls.get()+1);
                    if stage == "connect" { std::future::pending::<()>().await; }
                    Ok(())
                }, async {
                    if stage == "setup" { std::future::pending::<()>().await; }
                    anyhow::bail!("injected setup failure")
                }, async {
                    if stage == "cleanup" { std::future::pending::<()>().await; }
                    Ok(())
                })).await.expect("native open exceeded its total deadline");
            assert!(result.is_err());
            assert_eq!(calls.get(), 1, "unknown connect must not be retried");
            assert_eq!(state.load(Ordering::Acquire), 2, "{stage}");
            assert!(open_link::<(), _>(&state, 2, CLOSE_BUDGET + Duration::from_millis(150),
                || async { panic!("quarantined native connect") }, async { Ok(()) },
                async { panic!("quarantined native disconnect") }).await.is_err());
        }
    }

    #[tokio::test]
    async fn rejected_setup_cleans_once_and_cancelled_or_dropped_links_remain_reserved() {
        use super::{open_link, AtomicU8, Ordering, CLOSE_BUDGET};
        for stage in ["discover", "info", "mac", "bond", "rendezvous"] {
            let state = AtomicU8::new(0);
            let closes = std::cell::Cell::new(0);
            let error = open_link::<(), _>(&state, 1, CLOSE_BUDGET + Duration::from_millis(150),
                || async { Ok(()) }, async { anyhow::bail!("injected {stage}") },
                async { closes.set(closes.get()+1); Ok(()) }).await.unwrap_err();
            assert!(error.to_string().contains(stage));
            assert_eq!(closes.get(), 1);
            assert_eq!(state.load(Ordering::Acquire), 0);
        }
        let state = AtomicU8::new(0);
        let mut open = Box::pin(open_link::<(), _>(&state, 1, Duration::from_secs(3),
            || async { Ok(()) }, std::future::pending(), async { Ok(()) }));
        assert!(futures::poll!(&mut open).is_pending()); drop(open);
        assert_eq!(state.load(Ordering::Acquire), 2);
        let state = AtomicU8::new(0);
        let (_, lease) = open_link(&state, 1, Duration::from_secs(3),
            || async { Ok(()) }, async { Ok(()) }, async { Ok(()) }).await.unwrap();
        drop(lease);
        assert_eq!(state.load(Ordering::Acquire), 2, "dropping a live native handle is not confirmed disconnect");
    }

    #[test]
    fn automatic_auth_never_opens_first_pairing_and_has_a_short_budget() {
        let mut info = json!({"mac":"AA:BB:CC:DD:EE:01", "peerBonded":false, "pairingWindow":true});
        assert!(Pusher::link_auth_budget(&info, "AABBCCDDEE01", false).is_err());
        assert_eq!(Pusher::link_auth_budget(&info, "AABBCCDDEE01", true).unwrap(), Duration::from_secs(60));
        info["peerBonded"] = json!(true);
        assert_eq!(Pusher::link_auth_budget(&info, "AABBCCDDEE01", false).unwrap(), Duration::from_secs(3));
        assert!(Pusher::link_auth_budget(&info, "AABBCCDDEE02", true).is_err());
    }

    #[test]
    fn pairing_probe_requires_full_identity_and_window_or_existing_bond() {
        let mut info = json!({"mac":"AA:BB:CC:DD:EE:01", "peerBonded":false, "pairingWindow":true});
        assert!(Pusher::pairing_probe_allowed(&info, "AABBCCDDEE01"));
        assert!(!Pusher::pairing_probe_allowed(&info, "AABBCCDDEE02"));
        info["pairingWindow"] = json!(false);
        assert!(!Pusher::pairing_probe_allowed(&info, "AABBCCDDEE01"));
        info["peerBonded"] = json!(true);
        assert!(Pusher::pairing_probe_allowed(&info, "AABBCCDDEE01"));
        info.as_object_mut().unwrap().remove("mac");
        assert!(!Pusher::pairing_probe_allowed(&info, "AABBCCDDEE01"));
    }

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
        let payload: serde_json::Value = serde_json::from_slice(&Pusher::endpoint_payload_for(
            "192.168.1.2",
            8765,
            "bridge-token",
            "bridge-test",
            "endpoint-test", true,
        ))
        .unwrap();
        assert_eq!(payload["check_only"], true);
        assert_eq!(payload["request_id"], "endpoint-test");
        assert_eq!(payload["schema"], 2);
        assert_eq!(payload["host"], "192.168.1.2");
        assert_eq!(payload["port"], 8765);
        assert_eq!(payload["token"], "bridge-token");
        assert_eq!(payload["bridge_id"], "bridge-test");
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
    #[test]
    fn ble_decoder_rejects_invalid_versions_attributes_and_correlation() {
        assert!(super::decode_wire(&vec![b' ';513]).is_err());
        assert!(super::decode_wire(br#"{"ble_wire":2}"#).is_err());
        for bytes in [br#"{"ble_wire":1,"rid":"","w":[1,2]}"#.as_slice(),
            br#"{"ble_wire":1,"rid":"cg==","w":[1,4294967296]}"#.as_slice(),
            br#"{"ble_wire":1,"rid":"cg==","w":[1,2],"s":[]}"#.as_slice(),
            br#"{"ble_wire":1,"rid":"AA==","w":[1,2]}"#.as_slice()] {
            assert!(super::decode_wire(bytes).is_err());
        }
    }
    fn test_link() -> super::DeviceConnection {
        super::DeviceConnection {
            info: json!({}), peripheral: None, native_lease: None, fake_url: None,
            write_uncertain: std::sync::atomic::AtomicBool::new(false),
            endpoint_unconfirmed: std::sync::atomic::AtomicBool::new(false),
            command_budget_deadline: None, rf_deadline: None, token: String::new(),
            bridge_id: String::new(), nonce: String::new(), session: [0; 16], device_mac: String::new(),
            next_request_no: 1, context_id: String::new(),
            auth_ready: true, wake_identity: None, wake_cause: String::new(),
            device_last_stage: String::new(), timings: vec![],
        }
    }

    #[tokio::test]
    async fn cancelled_control_write_poison_survives_until_the_link_is_closed() {
        let mut link = test_link();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let mut write = Box::pin(link.control_write_until(deadline, std::future::pending()));
        assert!(futures::poll!(&mut write).is_pending());
        drop(write);
        assert!(link.control_write_until(deadline, async { panic!("must not write on a poisoned link") }).await.is_err());
        let error = link.command("status", json!({})).await.unwrap_err();
        let proof = bridge_core::rendezvous::command_failure(&error).unwrap();
        assert_eq!(proof.outcome, bridge_core::rendezvous::DispatchOutcome::NotSent);
        assert_eq!(proof.reason, "previous_write_unknown");
        let link = test_link();
        link.control_write_until(deadline, async { Ok(()) }).await.unwrap();
        link.control_write_until(deadline, async { Ok(()) }).await.unwrap();
    }

    #[test]
    fn phase_budget_is_shared_with_rf_and_cannot_be_renewed_by_commands() {
        let mut link = test_link();
        link.rf_deadline = Some(tokio::time::Instant::now() + Duration::from_secs(4));
        link.set_command_budget(Some(bridge_core::rendezvous::Budget::new(Duration::from_secs(1))));
        let phase = link.command_deadline();
        assert!(phase < link.rf_deadline.unwrap());
        assert_eq!(link.command_deadline(), phase);
        // A refreshed, longer RF phase cannot extend the admitted work budget.
        link.rf_deadline = Some(tokio::time::Instant::now() + Duration::from_secs(30));
        assert_eq!(link.command_deadline(), phase);
        link.command_budget_deadline = Some(tokio::time::Instant::now() - Duration::from_secs(1));
        assert!(link.command_deadline() < tokio::time::Instant::now());
        link.set_command_budget(None);
        assert!(link.command_deadline() > phase);
    }

    #[test]
    fn rf_deadline_is_fixed_for_repeated_commands_and_expired_windows() {
        let now = tokio::time::Instant::now();
        let mut link = test_link();
        link.rf_deadline = Some(now + Duration::from_secs(1));
        assert_eq!(link.command_deadline(), link.rf_deadline.unwrap());
        assert_eq!(link.command_deadline(), link.rf_deadline.unwrap());
        link.rf_deadline = Some(now - Duration::from_secs(1));
        assert!(link.command_deadline() < tokio::time::Instant::now());
    }

}
