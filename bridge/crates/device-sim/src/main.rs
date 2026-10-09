use anyhow::{bail, ensure, Context, Result};
use axum::{
    body::{to_bytes, Body},
    extract::{DefaultBodyLimit, FromRequest, Multipart, State},
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use sha2::{Digest, Sha256};
use bridge_core::compact_frame::{Admission, Header, RequestFence, HEADER_LEN};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env, fs,
    io::Write,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Component, Path, PathBuf},
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BODY_LIMIT: usize = 64 * 1024;
const MAX_RATE_PPM: u64 = 1_000_000_000;
const MAX_STEP_MS: u64 = 86_400_000;
const CAPABILITIES: &[&str] = &["device_status", "clock_control", "claim", "plan_state",
    "bundle_transfer", "bundle_persistence", "data_render", "activate",
    "power_sleep_http", "power_lifecycle", "button_cycle", "ota_catalog", "fake_ble_rendezvous",
    "sync_v1", "wifi_setup_transactions", "wifi_full_snapshot_sync"];
const UNSUPPORTED: &[&str] = &[
    "physical_ble",
    "physical_display",
    "physical_power",
];

#[derive(Clone)]
struct SimState {
    mac: String,
    target: String,
    seed: u64,
    data_dir: PathBuf,
    session: Arc<Mutex<SimSession>>,
    rendezvous: Arc<Mutex<bridge_render::SimulatorRendezvous>>,
    http_frame: Arc<tokio::sync::Mutex<SimHttpFrame>>,
    endpoint_token: Arc<str>,
    device_token: Arc<str>,
    control_token: Arc<str>,
    clock: Arc<Mutex<SimClock>>,
    owner: Arc<Mutex<OwnerStore>>,
    plan: Arc<Mutex<bridge_render::SimulatorPlan>>,
    bundle: Arc<Mutex<bridge_render::SimulatorBundle>>,
    sync: Arc<Mutex<bridge_render::SimulatorSync>>,
    ota: Arc<Mutex<OtaStore>>,
    ota_running: Arc<AtomicBool>,
    power: Arc<Mutex<SimPower>>,
    power_reconcile: Arc<Mutex<()>>,
    stall_ack_after: Arc<Mutex<Option<StallFault>>>,
    status_padding_bytes: Arc<Mutex<usize>>,
    ble_rx: Arc<Mutex<Vec<u8>>>,
    ble: Arc<Mutex<SimBle>>,
    wifi: Arc<Mutex<SimWifi>>,
    report_ip: Ipv4Addr,
    ready_report: Arc<Mutex<Option<Value>>>,
}

struct SimWifi {
    engine: bridge_render::SimulatorWifi,
    peer: String,
    physical_until: u64,
    outcome: i32,
    drop_http_ack: bool,
    unified: bool,
    principal: String,
    status_seq:u64,
    status_key:String,
}

#[derive(Default)]
struct SimHttpFrame {
    status_pages: Option<bridge_render::SimulatorStatusPages>,
    id: [u8; 16],
    open_nonce: [u8; 16],
    token_digest: [u8; 32],
    opens: u64,
    fence: RequestFence,
}

// Connection-local GATT attributes, distinct from business HTTP responses.
#[derive(Default)]
struct SimBle {
    generation: u64,
    connected: bool,
    wake: (u64, u64),
    info: Vec<u8>,
    status: Vec<u8>,
    pending: Option<(u64, Vec<u8>)>,
    fault: Option<BleFault>,
    read_failed: bool,
    events: std::collections::VecDeque<Value>,
    fence: RequestFence,
}
#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BleFault {
    kind: String,
    #[serde(default)] size: usize,
    #[serde(default)] delay_ms: u64,
}
impl SimBle {
    fn store(&mut self, op: &str, mut bytes: Vec<u8>, now: u64) {
        let fault = self.fault.take();
        let kind = fault.as_ref().map(|f| f.kind.as_str()).unwrap_or("none");
        let framed = Header::decode(&bytes, 8192).ok().map(|(header, payload)| (header, payload.to_vec()));
        if let Some(f) = &fault {
            if f.size > 0 {
                // Explicit SYNTHETIC same-length frame; not a captured ROM frame.
                if let Some((header, payload)) = &framed {
                    let mut header = *header;
                    if let Ok(mut value) = serde_json::from_slice::<Value>(payload) {
                        value["padding"] = json!("");
                        let base = serde_json::to_vec(&value).unwrap();
                        if f.size >= HEADER_LEN + base.len() {
                            value["padding"] = json!("x".repeat(f.size - HEADER_LEN - base.len()));
                            let payload = serde_json::to_vec(&value).unwrap();
                            header.payload_len = payload.len() as u16;
                            bytes = header.encode().unwrap().to_vec();
                            bytes.extend_from_slice(&payload);
                        } else { bytes.resize(f.size, b'x'); }
                    } else { bytes.resize(f.size, b'x'); }
                } else if bytes.last() == Some(&b'}') && f.size >= bytes.len() + 13 {
                    bytes.pop(); bytes.extend_from_slice(b",\"padding\":\"");
                    bytes.resize(f.size - 2, b'x'); bytes.extend_from_slice(b"\"}");
                } else { bytes.resize(f.size, b'x'); }
            }
        }
        if matches!(kind, "wrong_request" | "wrong_wake" | "wrong_mac" | "wrong_session" | "wrong_op") {
            let frame_header = Header::decode(&bytes, 8192).ok().map(|(h,p)| (h,p.to_vec()));
            let json_bytes = frame_header.as_ref().map_or(bytes.as_slice(), |(_,p)| p.as_slice());
            if let Ok(mut value) = serde_json::from_slice::<Value>(json_bytes) {
                match kind {
                    "wrong_op" => value["op"] = json!("other"),
                    "wrong_request" => value["rid"] = json!("c3RhbGU="),
                    "wrong_wake" => value["w"][1] = json!(self.wake.1.saturating_add(1)),
                    "wrong_mac" => value["device_mac"] = json!("AA:BB:CC:DD:EE:FF"),
                    "wrong_session" => value["session_nonce"] = json!("f".repeat(32)),
                    _ => unreachable!(),
                }
                let payload = serde_json::to_vec(&value).unwrap();
                if let Some((mut header,_)) = frame_header {
                    header.payload_len = payload.len() as u16;
                    bytes = header.encode().unwrap().to_vec();
                    bytes.extend_from_slice(&payload);
                } else { bytes = payload; }
            }
        }
        let attempted = bytes.len();
        let result = match kind {
            "legacy_overflow" if attempted > 512 => { self.status.clear(); "library_cleared" }
            "guard_overflow" if attempted > 512 => "precheck_failed",
            "write_failed" | "drop_ack" => "write_failed",
            "disconnect" | "window_closed" => { self.connected = false; self.pending = None; "disconnected" }
            "empty" => { self.status.clear(); "empty" }
            "malformed" => { self.status = b"invalid-json".to_vec(); "malformed" }
            "stale" | "overwrite" => "old_value",
            "read_failed" => { self.read_failed = true; "read_failed" }
            "delay" => {
                self.pending = Some((now.saturating_add(fault.as_ref().unwrap().delay_ms), bytes));
                "delayed"
            }
            _ if attempted > 512 => "precheck_failed",
            _ => { self.status = bytes; "written" } // notify_lost is still readable
        };
        self.events.push_back(json!({"op":op,"characteristic":"status","bytes":attempted,
            "max":512,"result":result,"generation":self.generation,"wake_generation":self.wake.0,"wake_seq":self.wake.1}));
        while self.events.len() > 64 { self.events.pop_front(); }
    }
}

#[derive(Clone)]
struct SimSession {
    boot_id: u64,
    nonce: String,
}

#[derive(Clone)]
struct StallFault {
    operation: String,
    duration_ms: u64,
}

#[derive(Clone)]
struct SimPower {
    configured: bool,
    light: bool,
    plugged: bool,
    deep_on_usb: bool,
    manual_ble_hold: bool,
    battery_pct: u8,
    boot_ms: u64,
    provisional: bool,
    safety_deadline_ms: u64,
    next_contact_ms: Option<u64>,
    ble_window_until_ms: Option<u64>,
    wake_count: u64,
    last_sleep_reason: String,
    sync_open: bool,
    sync_open_id: String,
    sync_last_progress_ms: u64,
    sync_visible_skip: u8,
}

impl SimPower {
    fn new(configured: bool, wake_cause: &str) -> Self {
        Self {
            configured, light: true, plugged: false, deep_on_usb: false,
            manual_ble_hold: false, battery_pct: 75, boot_ms: 0,
            provisional: wake_cause == "button",
            safety_deadline_ms: 600_000,
            next_contact_ms: None, ble_window_until_ms: None, wake_count: 0,
            last_sleep_reason: String::new(),
            sync_open: false,
            sync_open_id: String::new(),
            sync_last_progress_ms: 0,
            sync_visible_skip: 0,
        }
    }

    fn enter_deep(&mut self, at_ms: u64, reason: &str) {
        self.light = false;
        self.provisional = false;
        self.next_contact_ms = Some(at_ms.saturating_add(60_000));
        self.ble_window_until_ms = None;
        self.last_sleep_reason = reason.to_owned();
        self.sync_open = false;
        self.sync_open_id.clear();
    }

    fn json(&self, now_ms: u64) -> serde_json::Value {
        json!({"mode":if self.light {"light"} else {"deep"},
            "plugged":self.plugged,"deep_on_usb":self.deep_on_usb,
            "manual_ble_hold":self.manual_ble_hold,"battery_pct":self.battery_pct,
            "boot_ms":self.boot_ms,"provisional":self.provisional,
            "safety_deadline_ms":self.safety_deadline_ms,
            "next_contact_ms":self.next_contact_ms,
            "ble_window_until_ms":self.ble_window_until_ms,
            "wake_count":self.wake_count,
            "last_sleep_reason":self.last_sleep_reason,
            "sync_open":self.sync_open,
            "sync_last_progress_ms":self.sync_last_progress_ms,
            "sync_visible_skip":self.sync_visible_skip,
            "now_ms":now_ms})
    }
}

#[derive(Debug)]
struct Options {
    listen: SocketAddr,
    mac: String,
    target: String,
    seed: u64,
    epoch_ms: Option<u64>,
    data_dir: PathBuf,
    catalog_path: Option<PathBuf>,
    wake_cause: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RomVersion {
    id: String,
    fw: String,
    target: String,
    size: u64,
    sha256: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RomCatalog {
    initial: String,
    versions: Vec<RomVersion>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OtaFile {
    schema: u32,
    catalog_sha256: String,
    active: String,
    pending: Option<String>,
    source: String,
    event_seq: u64,
    #[serde(default)]
    slot: u8,
}

struct OtaStore {
    dir: PathBuf,
    catalog: RomCatalog,
    file: OtaFile,
}

struct OtaUploadGuard(Arc<AtomicBool>);
impl Drop for OtaUploadGuard {
    fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerRec {
    id: String,
    name: String,
    host: String,
    port: u16,
    since: u32,
    last_seen: u32,
    lease: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InstanceMarker {
    schema: u32,
    mac: String,
    #[serde(default)]
    target: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerFile {
    schema: u32,
    mac: String,
    owner: Option<OwnerRec>,
}

struct OwnerStore {
    dir: PathBuf,
    mac: String,
    owner: Option<OwnerRec>,
}

#[derive(Clone)]
struct SimClock {
    logical_ms: u64,
    rate_ppm: u64,
    anchor: Instant,
    epoch_ms: u64,
    wall_offset_ms: i64,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ClockFile {
    schema: u32,
    monotonic_ms: u64,
    rate_ppm: u64,
    epoch_ms: u64,
    wall_offset_ms: i64,
    saved_host_ms: u64,
}

#[derive(Clone, Copy)]
struct ClockSnapshot {
    monotonic_ms: u64,
    wall_ms: u64,
    rate_ppm: u64,
    wall_offset_ms: i64,
}

enum ClockCommand {
    Rate(u64),
    Step(u64),
    Wall(i64),
}

impl SimClock {
    fn logical_at(&self, now: Instant) -> u64 {
        let elapsed = now.saturating_duration_since(self.anchor).as_nanos();
        let advance = elapsed.saturating_mul(u128::from(self.rate_ppm)) / 1_000_000_000_000;
        self.logical_ms
            .saturating_add(advance.min(u128::from(u64::MAX)) as u64)
    }

    fn snapshot_at(&self, now: Instant) -> Option<ClockSnapshot> {
        let monotonic_ms = self.logical_at(now);
        let wall =
            i128::from(self.epoch_ms) + i128::from(monotonic_ms) + i128::from(self.wall_offset_ms);
        Some(ClockSnapshot {
            monotonic_ms,
            wall_ms: u64::try_from(wall).ok()?,
            rate_ppm: self.rate_ppm,
            wall_offset_ms: self.wall_offset_ms,
        })
    }

    fn apply(&self, command: ClockCommand) -> Option<(Self, ClockSnapshot)> {
        let now = Instant::now();
        let mut next = self.clone();
        next.logical_ms = self.logical_at(now);
        next.anchor = now;
        match command {
            ClockCommand::Rate(rate) => next.rate_ppm = rate,
            ClockCommand::Step(delta) if self.rate_ppm == 0 => {
                next.logical_ms = next.logical_ms.saturating_add(delta);
            }
            ClockCommand::Step(_) => return None,
            ClockCommand::Wall(offset) => next.wall_offset_ms = offset,
        }
        let snapshot = next.snapshot_at(now)?;
        Some((next, snapshot))
    }
}

fn host_now_ms() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_millis().min(u128::from(u64::MAX)) as u64)
}

fn load_clock(data_dir: &Path, requested_epoch_ms: Option<u64>) -> Result<SimClock> {
    let now = Instant::now();
    let path = data_dir.join("sim-clock.json");
    if !path.exists() {
        let epoch_ms = requested_epoch_ms.unwrap_or(host_now_ms()?);
        return Ok(SimClock { logical_ms: 0, rate_ppm: 1_000_000,
            anchor: now, epoch_ms, wall_offset_ms: 0 });
    }
    let saved: ClockFile = serde_json::from_slice(&fs::read(&path)
        .context("read simulator clock")?).context("invalid simulator clock")?;
    ensure!(saved.schema == 1 && saved.rate_ppm <= MAX_RATE_PPM,
        "unsupported simulator clock state");
    ensure!(requested_epoch_ms.is_none_or(|epoch| epoch == saved.epoch_ms),
        "simulator epoch changed across restart");
    let elapsed = host_now_ms()?.saturating_sub(saved.saved_host_ms);
    let advance = u128::from(elapsed) * u128::from(saved.rate_ppm) / 1_000_000;
    Ok(SimClock {
        logical_ms: saved.monotonic_ms.saturating_add(
            advance.min(u128::from(u64::MAX)) as u64),
        rate_ppm: saved.rate_ppm, anchor: now, epoch_ms: saved.epoch_ms,
        wall_offset_ms: saved.wall_offset_ms,
    })
}

fn save_clock(data_dir: &Path, clock: &SimClock) -> Result<()> {
    let path = data_dir.join("sim-clock.json");
    let saved = ClockFile { schema: 1, monotonic_ms: clock.logical_at(Instant::now()),
        rate_ppm: clock.rate_ppm, epoch_ms: clock.epoch_ms,
        wall_offset_ms: clock.wall_offset_ms, saved_host_ms: host_now_ms()? };
    let bytes = serde_json::to_vec(&saved)?;
    let mut file = fs::File::create(&path).context("write simulator clock")?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

impl ClockSnapshot {
    fn json(self, uptime_ms: u64) -> serde_json::Value {
        serde_json::json!({
            "monotonic_ms": self.monotonic_ms,
            "uptime_ms": uptime_ms,
            "wall_ms": self.wall_ms,
            "rate_ppm": self.rate_ppm,
            "wall_offset_ms": self.wall_offset_ms
        })
    }
}

fn parse_options() -> Result<Options> {
    let mut listen = "127.0.0.1:0".to_owned();
    let mut mac = None;
    let mut target = "codex-status-154g".to_owned();
    let mut seed = 1u64;
    let mut epoch_ms = None;
    let mut data_dir = None;
    let mut catalog_path = None;
    let mut wake_cause = "cold".to_owned();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .with_context(|| format!("missing value for {arg}"))?;
        match arg.as_str() {
            "--listen" => listen = value,
            "--mac" => {
                ensure!(mac.is_none(), "--mac specified more than once");
                mac = Some(value);
            }
            "--target" => target = value,
            "--seed" => seed = value.parse().context("invalid --seed")?,
            "--epoch-ms" => {
                ensure!(epoch_ms.is_none(), "--epoch-ms specified more than once");
                epoch_ms = Some(value.parse().context("invalid --epoch-ms")?);
            }
            "--data-dir" => {
                ensure!(data_dir.is_none(), "--data-dir specified more than once");
                data_dir = Some(PathBuf::from(value));
            }
            "--catalog" => catalog_path = Some(PathBuf::from(value)),
            "--wake-cause" => wake_cause = value,
            _ => bail!("unknown option: {arg}"),
        }
    }
    let listen: SocketAddr = listen.parse().context("invalid --listen address")?;
    ensure!(
        matches!(listen.ip(), IpAddr::V4(ip) if ip.is_loopback() || ip.is_private()),
        "--listen must use loopback or a private IPv4 address (synthetic MAC only)"
    );
    let mac = normalize_mac(mac.as_deref().context("--mac is required")?)?;
    ensure!(target == "codex-status-154g" || target == "zectrix-note4-400x300",
        "--target must be codex-status-154g or zectrix-note4-400x300");
    ensure!(["cold", "deep", "soft", "button"].contains(&wake_cause.as_str()),
        "--wake-cause must be cold, deep, soft or button");
    if let Some(path) = &catalog_path {
        ensure!(path.is_absolute(), "--catalog must be absolute");
    }
    let data_dir = data_dir.context("--data-dir is required")?;
    Ok(Options {
        listen,
        mac,
        target,
        seed,
        epoch_ms,
        data_dir,
        catalog_path,
        wake_cause,
    })
}

fn normalize_mac(raw: &str) -> Result<String> {
    if raw.len() == 17 {
        ensure!(
            [2, 5, 8, 11, 14].iter().all(|&i| raw.as_bytes()[i] == b':')
                || [2, 5, 8, 11, 14].iter().all(|&i| raw.as_bytes()[i] == b'-'),
            "--mac must use consistent separators"
        );
    } else {
        ensure!(raw.len() == 12, "--mac must contain 12 hexadecimal digits");
    }
    let compact: String = raw.chars().filter(|c| *c != ':' && *c != '-').collect();
    ensure!(
        compact.len() == 12,
        "--mac must contain 12 hexadecimal digits"
    );
    ensure!(
        compact.bytes().all(|b| b.is_ascii_hexdigit()),
        "--mac contains a non-hexadecimal character"
    );
    let bytes: Vec<u8> = (0..6)
        .map(|i| u8::from_str_radix(&compact[i * 2..i * 2 + 2], 16))
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        bytes[0] & 0x03 == 0x02,
        "--mac must be locally administered unicast"
    );
    Ok(bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":"))
}

fn required_token(name: &str) -> Result<Arc<str>> {
    let token = env::var(name).with_context(|| format!("{name} is required"))?;
    ensure!(!token.is_empty(), "{name} must not be empty");
    Ok(Arc::from(token))
}

fn normalized_path(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "--data-dir must be absolute");
    let mut lexical = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                lexical.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                lexical.pop();
            }
        }
    }
    let mut existing = lexical.clone();
    let mut tail = Vec::new();
    while !existing.exists() {
        tail.push(
            existing
                .file_name()
                .context("invalid --data-dir")?
                .to_owned(),
        );
        ensure!(existing.pop(), "invalid --data-dir");
    }
    let mut resolved = fs::canonicalize(existing).context("cannot resolve --data-dir")?;
    for component in tail.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

impl OwnerStore {
    fn on_boot(&mut self) {
        if let Some(owner) = self.owner.as_mut() {
            owner.since = 0;
            owner.last_seen = 0;
        }
    }

    fn load(dir: &Path, mac: &str, target: &str) -> Result<Self> {
        let dir = normalized_path(dir)?;
        let executable_dir = env::current_exe()?
            .parent()
            .context("simulator executable has no parent")?
            .to_path_buf();
        let production_data = normalized_path(&executable_dir.join("data"))?;
        ensure!(
            !production_data.starts_with(&dir),
            "--data-dir cannot be the simulator executable data directory or an ancestor"
        );
        fs::create_dir_all(&dir).context("cannot create --data-dir")?;
        let dir = fs::canonicalize(dir).context("cannot resolve --data-dir")?;
        ensure!(
            !production_data.starts_with(&dir),
            "--data-dir cannot be the simulator executable data directory or an ancestor"
        );
        let marker_path = dir.join("simulator.json");
        match fs::read(&marker_path) {
            Ok(bytes) => {
                let marker: InstanceMarker =
                    serde_json::from_slice(&bytes).context("invalid simulator instance marker")?;
                ensure!(marker.schema == 1, "unsupported simulator instance schema");
                ensure!(
                    marker.mac == mac,
                    "simulator data directory belongs to another MAC"
                );
                ensure!(marker.target.is_empty() && target == "codex-status-154g"
                    || marker.target == target,
                    "simulator data directory belongs to another target");
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let marker = InstanceMarker {
                    schema: 1,
                    mac: mac.to_owned(),
                    target: target.to_owned(),
                };
                fs::write(&marker_path, serde_json::to_vec(&marker)?)
                    .context("cannot write simulator instance marker")?;
            }
            Err(error) => return Err(error).context("cannot read simulator instance marker"),
        }

        let owner_path = dir.join("owner.json");
        let mut owner = match fs::read(&owner_path) {
            Ok(bytes) => {
                let record: OwnerFile =
                    serde_json::from_slice(&bytes).context("invalid simulator owner file")?;
                ensure!(record.schema == 1, "unsupported simulator owner schema");
                ensure!(
                    record.mac == mac,
                    "simulator owner file belongs to another MAC"
                );
                record.owner
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error).context("cannot read simulator owner file"),
        };
        if let Some(record) = owner.as_mut() {
            if record.since > 0 {
                record.since = 0;
            }
            if record.last_seen > 0 {
                record.last_seen = 0;
            }
            if record.id.is_empty() {
                owner = None;
            }
        }
        Ok(Self {
            dir,
            mac: mac.to_owned(),
            owner,
        })
    }

    fn persist(&self, owner: &Option<OwnerRec>) -> Result<()> {
        let record = OwnerFile {
            schema: 1,
            mac: self.mac.clone(),
            owner: owner.clone(),
        };
        fs::write(self.dir.join("owner.json"), serde_json::to_vec(&record)?)
            .context("cannot persist simulator owner")
    }

    fn get_valid(&mut self, now_s: u32) -> Result<Option<OwnerRec>> {
        let expired = self.owner.as_ref().is_some_and(|owner| {
            owner.lease == 0 || now_s.wrapping_sub(owner.last_seen) > owner.lease
        });
        if expired {
            self.persist(&None)?;
            self.owner = None;
        }
        Ok(self.owner.clone())
    }

    fn replace(&mut self, owner: Option<OwnerRec>) -> Result<()> {
        self.persist(&owner)?;
        self.owner = owner;
        Ok(())
    }
}

fn deterministic_nonce(seed: u64, mac: &str) -> String {
    fn hash(seed: u64, mac: &str, domain: u8) -> u64 {
        let mut value = 0xcbf29ce484222325u64 ^ seed ^ u64::from(domain);
        for byte in mac.bytes() {
            value ^= u64::from(byte);
            value = value.wrapping_mul(0x100000001b3);
        }
        value
    }
    format!("{:016x}{:016x}", hash(seed, mac, 0), hash(seed, mac, 1))
}

fn next_boot_id(dir: &Path) -> Result<u64> {
    let path = dir.join("sim-boot-seq.json");
    let previous = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<u64>(&bytes).context("invalid simulator boot sequence")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error).context("cannot read simulator boot sequence"),
    };
    let boot_id = previous.checked_add(1).context("simulator boot sequence exhausted")?;
    fs::write(path, serde_json::to_vec(&boot_id)?).context("cannot persist simulator boot sequence")?;
    Ok(boot_id)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// A reproducible, inert upload fixture; the simulator only compares bytes.
fn default_rom_bytes(target: &str, fw: &str) -> Vec<u8> {
    let mut bytes = vec![0u8; 1024];
    let marker = format!("codex-status-ota-v1|{target}|{fw}\0");
    bytes[..marker.len()].copy_from_slice(marker.as_bytes());
    bytes
}

fn default_catalog(target: &str) -> RomCatalog {
    let suffix = if target == "codex-status-154g" { "bw" } else { "note4" };
    let versions = ["0.18.24", "0.18.25"].iter().enumerate().map(|(index, prefix)| {
        let fw = format!("{prefix}-{suffix}");
        let bytes = default_rom_bytes(target, &fw);
        RomVersion {
            id: format!("v{}", index + 1), fw,
            target: target.to_owned(), size: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
        }
    }).collect();
    RomCatalog { initial: "v1".into(), versions }
}

impl OtaStore {
    fn load(dir: &Path, target: &str, catalog_path: Option<&Path>) -> Result<Self> {
        let catalog = match catalog_path {
            Some(path) => serde_json::from_slice::<RomCatalog>(&fs::read(path)
                .with_context(|| format!("cannot read OTA catalog {}", path.display()))?)
                .context("invalid OTA catalog")?,
            None => default_catalog(target),
        };
        ensure!((2..=3).contains(&catalog.versions.len()), "OTA catalog must have 2–3 versions");
        ensure!(catalog.versions.iter().any(|v| v.id == catalog.initial),
            "OTA initial version is absent");
        for (index, version) in catalog.versions.iter().enumerate() {
            ensure!(!version.id.is_empty() && version.id.bytes().all(|b|
                b.is_ascii_alphanumeric() || b == b'-') &&
                !version.fw.is_empty() && version.target == target,
                "OTA catalog version target/id invalid");
            ensure!((1024..=0x30_0000).contains(&version.size),
                "OTA catalog version size invalid");
            ensure!(version.sha256.len() == 64 &&
                version.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "OTA catalog SHA256 invalid");
            ensure!(!catalog.versions[..index].iter().any(|prior| prior.id == version.id),
                "OTA catalog version IDs must be unique");
        }
        let catalog_sha256 = sha256_hex(&serde_json::to_vec(&catalog)?);
        let path = dir.join("sim-ota.json");
        let file = match fs::read(&path) {
            Ok(bytes) => {
                let file: OtaFile = serde_json::from_slice(&bytes).context("invalid simulator OTA state")?;
                ensure!(file.schema == 1 && file.catalog_sha256 == catalog_sha256,
                    "simulator OTA catalog changed for this instance");
                ensure!(catalog.versions.iter().any(|v| v.id == file.active),
                    "simulator OTA active version is absent");
                if let Some(pending) = &file.pending {
                    ensure!(catalog.versions.iter().any(|v| &v.id == pending),
                        "simulator OTA pending version is absent");
                }
                file
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => OtaFile {
                schema: 1, catalog_sha256, active: catalog.initial.clone(),
                pending: None, source: "initial".into(), event_seq: 0, slot: 0,
            },
            Err(error) => return Err(error).context("cannot read simulator OTA state"),
        };
        let mut store = Self { dir: dir.to_owned(), catalog, file };
        let active = store.active_version();
        let fixture = default_rom_bytes(target, &active.fw);
        if fixture.len() as u64 == active.size && sha256_hex(&fixture) == active.sha256 &&
            !store.rom_path(&active.id).exists() {
            let mut file = fs::File::create(store.rom_path(&active.id))?;
            file.write_all(&fixture)?;
            file.sync_all()?;
        }
        if store.file.pending.is_some() { store.commit_pending()?; }
        else { store.persist()?; }
        Ok(store)
    }

    fn persist(&self) -> Result<()> {
        fs::write(self.dir.join("sim-ota.json"), serde_json::to_vec(&self.file)?)
            .context("cannot persist simulator OTA state")
    }

    fn active_version(&self) -> &RomVersion {
        self.catalog.versions.iter().find(|v| v.id == self.file.active)
            .expect("validated OTA active version")
    }

    fn rom_path(&self, id: &str) -> PathBuf { self.dir.join(format!("sim-rom-{id}.bin")) }

    fn active_bytes(&self) -> Result<Vec<u8>> {
        let version = self.active_version();
        let bytes = fs::read(self.rom_path(&version.id))?;
        ensure!(bytes.len() as u64 == version.size && sha256_hex(&bytes) == version.sha256,
            "active ROM bytes do not match catalog");
        Ok(bytes)
    }

    fn state_json(&self) -> serde_json::Value {
        json!({"active":self.file.active,"pending":self.file.pending,
            "source":self.file.source,"event_seq":self.file.event_seq,
            "versions":self.catalog.versions})
    }

    fn queue_upload(&mut self, size: u64, sha256: &str) -> Result<Option<String>> {
        if self.file.pending.is_some() { bail!("OTA upload already pending"); }
        let version = self.catalog.versions.iter().find(|v|
            v.size == size && v.sha256.eq_ignore_ascii_case(sha256));
        let Some(version) = version else { return Ok(None); };
        let destination = self.rom_path(&version.id);
        let upload = self.dir.join("sim-upload.tmp");
        if destination.exists() {
            let existing = fs::read(&destination)?;
            ensure!(existing.len() as u64 == size && sha256_hex(&existing) == sha256,
                "catalogued ROM bytes conflict");
            fs::remove_file(&upload)?;
        } else {
            fs::rename(&upload, &destination)?;
            fs::OpenOptions::new().write(true).open(&destination)?.sync_all()?;
        }
        self.file.pending = Some(version.id.clone());
        self.file.event_seq += 1;
        self.persist()?;
        Ok(Some(version.id.clone()))
    }

    fn commit_pending(&mut self) -> Result<bool> {
        let Some(pending) = self.file.pending.clone() else { return Ok(false); };
        let version = self.catalog.versions.iter().find(|v| v.id == pending).context("pending ROM absent")?;
        let bytes = fs::read(self.rom_path(&pending))?;
        ensure!(bytes.len() as u64 == version.size && sha256_hex(&bytes) == version.sha256,
            "pending ROM bytes do not match catalog");
        self.file.pending = None;
        self.file.active = pending;
        self.file.slot ^= 1;
        self.file.source = "ota_upload".into();
        self.file.event_seq += 1;
        self.persist()?;
        Ok(true)
    }

    fn override_to(&mut self, id: &str) -> Result<bool> {
        if !self.catalog.versions.iter().any(|v| v.id == id) { return Ok(false); }
        self.file.active = id.to_owned();
        self.file.pending = None;
        self.file.source = "test_override".into();
        self.file.event_seq += 1;
        self.persist()?;
        Ok(true)
    }
}

fn session_snapshot(state: &SimState) -> SimSession {
    state.session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
}

fn rotate_session(state: &SimState) -> Result<SimSession> {
    let boot_id = next_boot_id(&state.data_dir)?;
    let session = SimSession {
        boot_id,
        nonce: deterministic_nonce(state.seed ^ boot_id, &state.mac),
    };
    *state.session.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = session.clone();
    Ok(session)
}

fn reboot_device(state: &SimState, cause: &str) -> Result<SimSession> {
    *state.ready_report.lock().unwrap() = None;
    // Deep/button wakes retain the ROM's last-contact RTC fact. Resetting it
    // here would make every no-Data Sleep rendezvous forget Bridge freshness.
    let contact = if matches!(cause,"deep"|"button") {
        state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
            .status()?["bridge_contact_epoch"].as_i64().unwrap_or(0)
    } else {0};
    let session = rotate_session(state)?;
    let mut bundle = bridge_render::SimulatorBundle::new(&state.data_dir, &state.target,
        session.boot_id, cause)?;
    if contact > 0 {bundle.set_wall_secs(contact)?;bundle.note_bridge_contact()?;}
    *state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = bundle;
    let sync = bridge_render::SimulatorSync::new(&state.data_dir, &state.mac,
        &state.target, state.seed, session.boot_id, cause)?;
    *state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = sync;
    *state.plan.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
        bridge_render::SimulatorPlan::new()?;
    Ok(session)
}

fn reset_power_after_reboot(state: &SimState, now_ms: u64, cause: &str) -> Result<()> {
    reboot_device(state, cause)?;
    state.ble_rx.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
    { let mut ble = state.ble.lock().unwrap();
      let next = ble.generation.saturating_add(1); *ble = SimBle::default(); ble.generation = next; }
    let configured = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .status()?["configured"] == true;
    let mut power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    power.configured = configured;
    power.light = true;
    power.boot_ms = now_ms;
    power.provisional = cause == "button";
    power.safety_deadline_ms = now_ms.saturating_add(600_000);
    power.next_contact_ms = None;
    power.ble_window_until_ms = None;
    power.wake_count += 1;
    power.sync_open = false;
    power.sync_open_id.clear();
    power.sync_visible_skip = 0;
    if cause == "deep" {
        let mut sync = state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        sync.deep_round(power.wake_count as u32, 0)?;
        power.sync_visible_skip = sync.status()?["retry_skip"].as_u64().unwrap_or(0) as u8;
        if power.sync_visible_skip != 0 { sync.consume_skip()?; }
    }
    drop(power);
    state.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner).on_boot();
    Ok(())
}

fn controlled_reboot(state: &SimState, now_ms: u64, cause: &str) -> Result<()> {
    let _serial = state.power_reconcile.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    reset_power_after_reboot(state, now_ms, cause)
}

fn reconcile_power(state: &SimState, now_ms: u64) -> Result<SimPower> {
    let _serial = state.power_reconcile.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let configured = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .status()?["configured"] == true;
    let before = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let local_now = now_ms.saturating_sub(before.boot_ms);
    let (reason, plan_deadline, plan_was_light) = {
        let plan = state.plan.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (_, mode, _, granted_s, accepted_at_ms) = plan.status_fields();
        // PowerPlan now governs devices awaiting their first Bundle too.
        (plan.sleep_reason(true, before.light, before.plugged,
            before.deep_on_usb, before.manual_ble_hold,
            before.provisional, 0,
            before.safety_deadline_ms.saturating_sub(before.boot_ms), local_now),
         if mode == "light" {before.boot_ms.saturating_add(accepted_at_ms)
             .saturating_add(u64::from(granted_s) * 1000)}
         else {now_ms}, mode == "light")
    };
    let mut power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if power.sync_open &&
        now_ms.saturating_sub(power.sync_last_progress_ms) >= 90_000 {
        state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
            .fail(now_ms.saturating_sub(power.boot_ms) as u32)?;
        power.enter_deep(now_ms, "sync incomplete");
    }
    if configured && !power.configured {
        power.configured = true;
        power.safety_deadline_ms = now_ms.saturating_add(600_000);
    }
    if power.light {
        // The low-battery cut-off is a separate boot/loop decision in the ROM.
        if bridge_render::battery_power_off(power.plugged, power.battery_pct) {
            power.enter_deep(now_ms, "low battery");
            power.next_contact_ms = None;
            return Ok(power.clone());
        }
        let transition = match reason {
            1 => Some((plan_deadline, "plan")),
            2 => Some((power.boot_ms.saturating_add(300_000), "provisional")),
            3 => Some((power.safety_deadline_ms, "safety")),
            _ => None,
        };
        if let Some((at, why)) = transition {
            let light_exit_sync = reason == 1 && plan_was_light && state.sync.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .status()?["enabled"] == true;
            if light_exit_sync {
                state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
                    .add_reason(2, local_now as u32)?;
                power.light = false;
                power.sync_open = true;
                power.sync_open_id = "light-exit".into();
                power.sync_last_progress_ms = now_ms;
                power.next_contact_ms = None;
                power.ble_window_until_ms = None;
                power.last_sleep_reason = "sync light_exit".into();
            } else {
                power.enter_deep(at, why);
            }
        }
    }
    if power.ble_window_until_ms.is_some_and(|until| now_ms >= until) {
        power.ble_window_until_ms = None;
        power.last_sleep_reason = "rendezvous timeout".to_owned();
    }
    for _ in 0..10_000 {
        let Some(first) = power.next_contact_ms.filter(|first| !power.light && now_ms >= *first)
            else {
                if power.ble_window_until_ms.is_some_and(|until| now_ms >= until) {
                    power.ble_window_until_ms = None;
                    power.last_sleep_reason = "rendezvous timeout".to_owned();
                }
                return Ok(power.clone());
            };
        drop(power);
        // A timer wake runs the BLE rendezvous. Without a fake BLE peer
        // there is no accepted light plan, so HTTP stays unavailable.
        let result = reset_power_after_reboot(state, first, "deep");
        power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        result?;
        power.light = false;
        power.ble_window_until_ms = Some(first.saturating_add(5_000));
        power.next_contact_ms = first.checked_add(60_000);
        power.last_sleep_reason = "ble rendezvous".to_owned();
    }
    anyhow::bail!("too many timer wakes in one clock step")
}

async fn device_gate(state: &SimState) -> Option<Response> {
    let Some(clock) = clock_snapshot(state) else {
        return Some(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    };
    match reconcile_power(state, clock.monotonic_ms) {
        Ok(power) if power.light || power.sync_open => None,
        Ok(_) => {
            // The device surface stalls past the Bridge's I/O timeout while the
            // simulator control surface remains usable.
            tokio::time::sleep(Duration::from_secs(10)).await;
            Some(StatusCode::SERVICE_UNAVAILABLE.into_response())
        }
        Err(_) => Some(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    }
}

fn bearer(request: &Request<Body>, expected: &str) -> bool {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.strip_prefix("Bearer ") == Some(expected))
}

fn clock_snapshot(state: &SimState) -> Option<ClockSnapshot> {
    state
        .clock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .snapshot_at(Instant::now())
}

fn parse_clock_command(body: &[u8]) -> Option<ClockCommand> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let object = value.as_object()?;
    let op = object.get("op")?.as_str()?;
    match op {
        "rate" if object.len() == 2 => {
            let rate = object.get("rate_ppm")?.as_u64()?;
            (rate <= MAX_RATE_PPM).then_some(ClockCommand::Rate(rate))
        }
        "step" if object.len() == 2 => {
            let delta = object.get("delta_ms")?.as_u64()?;
            (delta <= MAX_STEP_MS).then_some(ClockCommand::Step(delta))
        }
        "wall" if object.len() == 2 => Some(ClockCommand::Wall(object.get("offset_ms")?.as_i64()?)),
        _ => None,
    }
}

fn device_uptime_ms(state: &SimState, clock: ClockSnapshot) -> u64 {
    let boot_ms = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner).boot_ms;
    clock.monotonic_ms.saturating_sub(boot_ms)
}

fn now_seconds(state: &SimState, clock: ClockSnapshot) -> u32 {
    (device_uptime_ms(state, clock) / 1000) as u32
}

fn owner_json(owner: Option<&OwnerRec>, now_s: u32) -> serde_json::Value {
    let Some(owner) = owner else {
        return serde_json::Value::Null;
    };
    let elapsed = now_s.wrapping_sub(owner.last_seen);
    let expires_in = if elapsed >= owner.lease {
        0
    } else {
        owner.lease - elapsed
    };
    json!({
        "id": owner.id,
        "name": owner.name,
        "host": owner.host,
        "port": owner.port,
        "since_s": owner.since,
        "last_seen_s": owner.last_seen,
        "lease_s": owner.lease,
        "expires_in_s": expires_in
    })
}

fn claim_current(owner: Option<&OwnerRec>) -> serde_json::Value {
    match owner {
        Some(owner) => json!({
            "id": owner.id,
            "name": owner.name,
            "host": owner.host,
            "port": owner.port,
            "since": owner.since,
            "last_seen": owner.last_seen,
            "lease": owner.lease
        }),
        None => json!({
            "id": "", "name": "", "host": "", "port": 0,
            "since": 0, "last_seen": 0, "lease": 300
        }),
    }
}

async fn consume_body(request: Request<Body>) -> std::result::Result<(), Response> {
    to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map(|_| ())
        .map_err(|_| {
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error":"body_too_large"})),
            )
                .into_response()
        })
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"result":"unauthorized"})),
    )
        .into_response()
}

async fn public_status(State(state): State<SimState>) -> Response {
    if let Some(response) = device_gate(&state).await { return response; }
    let Some(clock) = clock_snapshot(&state) else {
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"clock_unavailable"}))).into_response();
    };
    let bundle = match state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status() {
        Ok(bundle) => bundle,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"bundle_status_failed"}))).into_response(),
    };
    let note4 = state.target == "zectrix-note4-400x300";
    let fw = state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .active_version().fw.clone();
    let power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let boot_ms = power.boot_ms;
    Json(json!({
        "mac": state.mac,
        "fw": fw,
        "fw_target": state.target,
        "render_target": bundle["render_target"],
        "compiler_abi": 2,
        "width": if note4 {400} else {200},
        "height": if note4 {300} else {200},
        "pixel_format": "1bpp", "colors": "bw", "partial": true,
        "hardware_verified": false,
        "max_templates": 8, "max_bundle_bytes": 262144,
        "asset_publish_protocol": 0,
        "free_bytes": bundle["free_bytes"],
        "uptime_s": clock.monotonic_ms.saturating_sub(boot_ms) / 1000,
        "mode": "light",
        "battery": power.battery_pct,
        "bundle_configured": bundle["configured"],
        "template_count": bundle["template_ids"].as_array().map_or(0, Vec::len),
        "commit_seq": bundle["commit_seq"],
        "active_context_id": bundle["context"],
        "active_template_id": bundle["active_template_id"],
        "committed_job_id": bundle["job_id"],
        "data_seq": bundle["applied_seq"],
        "applied_seq": bundle["applied_seq"],
        "display_state": match bundle["display_state_code"].as_u64().unwrap_or(0) {
            1 => "displayed", 2 => "pending", 3 => "failed", _ => "unchanged"
        }
    })).into_response()
}

async fn status(State(state): State<SimState>, request: Request<Body>) -> Response {
    status_impl(state, request, true).await
}

async fn sync_endpoint(state: SimState, request: Request<Body>, op: &'static str) -> Response {
    if let Some(response) = device_gate(&state).await { return response; }
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), 4096).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let Ok(message) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let nonce = session_snapshot(&state).nonce;
    let bridge = message["bridge_id"].as_str().unwrap_or("");
    let id = message["request_id"].as_str().unwrap_or("");
    let reject = |error: &str, status: StatusCode| {
        (status, Json(json!({"op":op,"result":"rejected","error":error,
            "request_id":id,"device_mac":state.mac,"session_nonce":nonce,
            "sync_version":1}))).into_response()
    };
    if message.get("protocol").is_some() || message.get("rv").is_some() ||
        message["sync_version"] != 1 ||
        message["device_mac"] != state.mac || bridge.is_empty() || id.is_empty() || id.len() > 64 {
        return reject("shape", StatusCode::BAD_REQUEST);
    }
    if message["session_nonce"] != nonce {
        return reject("session", StatusCode::BAD_REQUEST);
    }
    let Some(clock) = clock_snapshot(&state) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let owner = match state.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_valid(now_seconds(&state, clock)) {
        Ok(owner) => owner,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if owner.is_none() { return reject("claim_required", StatusCode::CONFLICT); }
    if owner.as_ref().is_some_and(|owner| owner.id != bridge) {
        return reject("occupied", StatusCode::CONFLICT);
    }
    let bundle = match state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status() {
        Ok(bundle) => bundle,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let fw = state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .active_version().fw.clone();
    let snapshot = json!({"target":state.target,"fw":fw,"slot":"ota_0",
        "reset_reason":0,"active_context_id":bundle["context"],
        "active_template_id":bundle["active_template_id"],
        "committed_job_id":bundle["job_id"],"commit_seq":bundle["commit_seq"],
        "display_state":bundle["display_state_code"],"applied_seq":bundle["applied_seq"],
        "heap_free":0,"battery":power.battery_pct,"plan_id":0,
        "plan_remaining_s":0,"rounds":state.sync.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner).status()
            .unwrap_or(serde_json::Value::Null)["rounds"],
        "owner_id":bridge});
    let before_ack = state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .status().unwrap_or(Value::Null)["pending_batch"]["acked_offset"]
        .as_u64().unwrap_or(0);
    let result = state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .command(op, &message.to_string(), &snapshot);
    let Ok(mut reply) = result else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let status = reply["http_status"].as_u64()
        .and_then(|n| StatusCode::from_u16(n as u16).ok())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    reply.as_object_mut().map(|object| object.remove("http_status"));
    reply["session_nonce"] = json!(nonce);
    if reply["result"] == "applied" && (op == "sync_begin" ||
        (op == "sync_ack" && reply["acked_offset"].as_u64().unwrap_or(0) > before_ack)) {
        state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
            .sync_last_progress_ms = clock.monotonic_ms;
    }
    if op == "sync_complete" && reply["result"] == "applied" {
        let sync = state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
            .status().unwrap_or(serde_json::Value::Null);
        if sync["due"] != true && sync["pending_batch"].is_null() {
            let mut power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            power.sync_open = false;
            power.sync_open_id.clear();
            if !power.light { power.enter_deep(clock.monotonic_ms, "sync complete"); }
        }
    }
    (status, Json(reply)).into_response()
}

async fn sync_begin(State(state): State<SimState>, request: Request<Body>) -> Response {
    sync_endpoint(state, request, "sync_begin").await
}
async fn sync_page(State(state): State<SimState>, request: Request<Body>) -> Response {
    sync_endpoint(state, request, "sync_page").await
}
async fn sync_ack(State(state): State<SimState>, request: Request<Body>) -> Response {
    sync_endpoint(state, request, "sync_ack").await
}
async fn sync_complete(State(state): State<SimState>, request: Request<Body>) -> Response {
    sync_endpoint(state, request, "sync_complete").await
}

async fn status_impl(state: SimState, request: Request<Body>, gate: bool) -> Response {
    if gate { if let Some(response) = device_gate(&state).await { return response; } }
    if !bearer(&request, &state.endpoint_token) {
        return unauthorized();
    }
    let body=match to_bytes(request.into_body(),4096).await {
        Ok(body)=>body, Err(_)=>return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let compact=serde_json::from_slice::<Value>(&body).ok()
        .is_some_and(|options|options["compact"]==true);
    let Some(clock) = clock_snapshot(&state) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"clock_unavailable"})),
        )
            .into_response();
    };
    let plan = state
        .plan
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (plan_accepted, plan_mode, plan_id, granted_s, accepted_at_ms) = plan.status_fields();
    let plan_mode = plan_mode.to_owned();
    drop(plan);
    let bundle = match state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status() {
        Ok(bundle) => bundle,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"bundle_status_failed"}))).into_response(),
    };
    let fw = state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .active_version().fw.clone();
    let power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let input = json!({
        "mac": state.mac,
        "firmware_version": fw,
        "session_nonce": session_snapshot(&state).nonce,
        "context": bundle["context"],
        "job_id": bundle["job_id"],
        "active_template_id": bundle["active_template_id"],
        "template_ids": bundle["template_ids"],
        "configured": bundle["configured"],
        "data_contract": bundle["data_contract"],
        "applied_seq": bundle["applied_seq"],
        "data_crc": bundle["data_crc"],
        "display_state_code": bundle["display_state_code"],
        "commit_seq": bundle["commit_seq"],
        "deep_sleep": false,
        "plan_accepted": plan_accepted,
        "plan_mode": plan_mode,
        "plan_id": plan_id,
        "granted_s": granted_s,
        "plan_accepted_at_ms": accepted_at_ms,
        "provisional": power.provisional,
        "boot_ms": power.boot_ms,
        "now_ms": device_uptime_ms(&state, clock),
        "battery": state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
            .battery_pct
    });
    match bridge_render::simulator_status_snapshot(&input) {
        Ok(mut status) => {
            let mut sync = match state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status() {
                Ok(sync) => sync,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            let owner = match state.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
                .get_valid(now_seconds(&state, clock)) {
                Ok(owner) => owner,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            status["owner_id"] = json!(owner.as_ref().map(|owner| owner.id.as_str()).unwrap_or(""));
            sync["enabled"] = json!(sync["enabled"] == true &&
                owner.as_ref().is_some_and(|owner|
                    sync["config_owner"].as_str() == Some(owner.id.as_str())));
            sync.as_object_mut().map(|object| object.remove("config_owner"));
            let mut wifi=state.wifi.lock().unwrap();let continuous=power.plugged && !power.deep_on_usb;
            status["wifi_management"] = json!(if wifi.unified {3}else{2});
            status["wifi_storage"]=match wifi.engine.storage_status() {Ok(value)=>value,Err(_)=>return (StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"ok":false,"error":"wifi_storage_status_unavailable","stage":"status"}))).into_response()};
            let radio=match wifi.engine.observe() {Ok(value)=>value,Err(_)=>return (StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"ok":false,"error":"wifi_radio_unavailable","stage":"status"}))).into_response()};
            let radio_ready=radio["connected"]==true && radio["has_ip"]==true;
            let ble_connected=state.ble.lock().unwrap().connected;
            let mut rv=state.rendezvous.lock().unwrap();
            rv.tick(clock.monotonic_ms);
            if rv.phase()==2 && !ble_connected && radio_ready {rv.http_ready();}
            let http_ready=radio_ready && rv.phase()!=2;
            status["wake_generation"]=json!(session_snapshot(&state).boot_id);
            status["wake_seq"]=json!(power.wake_count);
            status["rendezvous"]=json!({"v":1,"id":session_snapshot(&state).nonce,
                "ready":rv.phase()==3 && http_ready,
                "remaining_ms":rv.deadline().saturating_sub(clock.monotonic_ms)});
            drop(rv);
            if wifi.unified {
                let until=wifi_control_until(&state,&power,wifi.physical_until,clock.monotonic_ms);
                let remaining=if continuous {Value::Null}else{json!(until.saturating_sub(clock.monotonic_ms))};
                let claimed=owner.as_ref().is_some_and(|o|o.id==wifi.principal);
                let key=format!("{}:{continuous}:{}:{claimed}:{until}:{http_ready}:{}",session_snapshot(&state).boot_id,power.light,wifi.physical_until)+&format!(":{}:{}",status["wifi_storage"]["revision"],status["wifi_storage"]["receipts"]);
                if key!=wifi.status_key {wifi.status_seq+=1;wifi.status_key=key;}status["status_seq"]=json!(wifi.status_seq);
                status["presence"]=json!({"mode":if continuous {"continuous"}else{"window"},"remaining_ms":remaining,"reasons":[if continuous {"usb"}else{"timer"}]});
                status["transports"]=json!({"http":{"ready":http_ready},"ble":{"ready":power.ble_window_until_ms.is_some()}});
                status["wifi_control"]=json!({"mode":if continuous {"continuous"}else{"window"},"read_allowed":until>clock.monotonic_ms,"write_allowed":claimed && until>clock.monotonic_ms,"remaining_ms":remaining,"principal":wifi.principal,"namespace":wifi.peer,"reason":if claimed {Value::Null}else{json!("claim_required")}});
            }
            drop(wifi);
            status["sync_v1"] = json!(1);
            status["diag_format"] = json!(1);
            status["diag_capacity"] = json!(4096);
            status["firmware_target"] = json!(state.target);
            status["boot_id"] = json!(session_snapshot(&state).nonce);
            status["uptime_ms"] = json!(device_uptime_ms(&state, clock));
            status["heap_free"] = json!(0);
            status["heap_min"] = json!(0);
            status["radio"] = json!({"wifi_connected":radio_ready,"rssi":0,
                "ble_connected":power.ble_window_until_ms.is_some()});
            status["display"] = json!({"epd_writes":bundle["display_writes"],
                "epd_busy_fails":0});
            let ota = state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            status["running_slot"] = json!(format!("ota_{}", ota.file.slot));
            status["image_identity"] = json!(if ota.active_bytes().is_ok() {
                vec!["sha256-running-prefix-v1"]
            } else { vec![] });
            status["sync"] = sync;
            let padding = *state.status_padding_bytes.lock().unwrap();
            if padding > 0 { status["synthetic_status_padding"] = json!("x".repeat(padding)); }
            status["sync"]["phase"] = json!(if power.sync_open {"WIFI_SYNC_ONCE"}
                else if power.light {"WIFI_LIGHT"} else {"DEEP"});
            if compact {
                status["status_projection"]=json!("compact");
                for field in ["data_contract","wifi_storage","clock_rtc","sync","radio"] {
                    status.as_object_mut().unwrap().remove(field);
                }
            }
            Json(status).into_response()
        },
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"status_builder_failed"})),
        )
            .into_response(),
    }
}

async fn sim_state(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) {
        return unauthorized();
    }
    if let Err(response) = consume_body(request).await {
        return response;
    }
    let clock_guard = state
        .clock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(clock) = clock_guard.snapshot_at(Instant::now()) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"clock_unavailable"})),
        )
            .into_response();
    };
    drop(clock_guard);
    let power = match reconcile_power(&state, clock.monotonic_ms) {
        Ok(power) => power,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"power_reconcile_failed"}))).into_response(),
    };
    let now_s = now_seconds(&state, clock);
    let owner = match state
        .owner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_valid(now_s)
    {
        Ok(owner) => owner,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"owner_storage_failed"})),
            )
                .into_response()
        }
    };
    let plan = state
        .plan
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (accepted, mode, plan_id, granted_s, accepted_at_ms) = plan.status_fields();
    let mode = mode.to_owned();
    drop(plan);
    let bundle = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .status().unwrap_or(serde_json::Value::Null);
    let ota = state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner).state_json();
    let sync = state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .status().unwrap_or(serde_json::Value::Null);
    Json(json!({
        "mac": state.mac,
        "target": state.target,
        "boot_id": session_snapshot(&state).boot_id,
        "capabilities": CAPABILITIES,
        "unsupported": UNSUPPORTED,
        "uptime_ms": device_uptime_ms(&state, clock),
        "clock": clock.json(device_uptime_ms(&state, clock)),
        "clock_persistence": "instance_file",
        "owner": owner_json(owner.as_ref(), now_s),
        "bundle": bundle,
        "ota": ota,
        "sync": sync,
        "fault": {"stall_ack_after":state.stall_ack_after.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref().map(|fault| fault.operation.clone())},
        "power": power.json(clock.monotonic_ms),
        "plan": {"accepted": accepted, "mode": mode, "plan_id": plan_id,
                 "granted_s": granted_s, "accepted_at_ms": accepted_at_ms}
    }))
    .into_response()
}

async fn sim_frame(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    if let Err(response) = consume_body(request).await { return response; }
    let Some(clock) = clock_snapshot(&state) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    let Ok(power) = reconcile_power(&state, clock.monotonic_ms) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    let wifi_up = {
        let wifi = state.wifi.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if wifi.unified { wifi.engine.observe().is_ok_and(|s| s["connected"] == true) }
        else { power.light }
    };
    let mut bundle = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if bundle.set_wall_secs((clock.wall_ms / 1000) as i64).is_err() { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
    if bundle.display_state(!power.light, wifi_up, false).is_err() { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
    match bundle.frame_bits() {
        Ok(bits) if !bits.is_empty() =>
            ([(header::CONTENT_TYPE, "application/octet-stream")], bits).into_response(),
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn sim_diagnostics(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), 8192).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(object) = value.as_object() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let operation = match value["op"].as_str() {
        Some("append_text") if object.len() == 2 =>
            value["text"].as_str().filter(|text| text.len() <= 4096)
                .map(|text| (0, text.to_owned(), 0)),
        Some("corrupt_record") if object.len() == 2 =>
            value["seq"].as_str().and_then(|seq| seq.parse::<u64>().ok())
                .map(|seq| (1, String::new(), seq)),
        Some("corrupt_header") if object.len() == 1 =>
            Some((2, String::new(), 0)),
        _ => None,
    };
    let Some((kind, text, seq)) = operation else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(clock) = clock_snapshot(&state) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut sync = state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let result = match kind {
        0 => sync.append_text(&text, device_uptime_ms(&state, clock) as u32),
        1 => sync.corrupt(false, seq),
        _ => sync.corrupt(true, 0),
    };
    if result.is_err() { return StatusCode::BAD_REQUEST.into_response(); }
    match sync.status() {
        Ok(status) => Json(status).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn sim_time_get(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) {
        return unauthorized();
    }
    if let Err(response) = consume_body(request).await {
        return response;
    }
    match clock_snapshot(&state) {
        Some(clock) => Json(clock.json(device_uptime_ms(&state, clock))).into_response(),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"clock_unavailable"})),
        )
            .into_response(),
    }
}

async fn sim_time_post(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) {
        return unauthorized();
    }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error":"body_too_large"})),
            )
                .into_response()
        }
    };
    let Some(command) = parse_clock_command(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_clock_command"})),
        )
            .into_response();
    };
    let mut stored = state.clock.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let result = stored.apply(command);
    match result {
        Some((next, clock)) => {
            if save_clock(&state.data_dir, &next).is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"clock_storage_failed"}))).into_response();
            }
            *stored = next;
            drop(stored);
            match reconcile_power(&state, clock.monotonic_ms) {
            Ok(_) => Json(clock.json(device_uptime_ms(&state, clock))).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"power_reconcile_failed"}))).into_response(),
            }
        },
        None => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_clock_command"})),
        )
            .into_response(),
    }
}

async fn sim_wake(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return (StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error":"body_too_large"}))).into_response(),
    };
    let value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_wake_command"}))).into_response(),
    };
    let Some(cause) = value["cause"].as_str() else {
        return (StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_wake_command"}))).into_response();
    };
    if !["button", "cold", "soft"].contains(&cause)
        || value.as_object().is_none_or(|object| object.len() != 1) {
        return (StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_wake_command"}))).into_response();
    }
    let Some(clock) = clock_snapshot(&state) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let power = match reconcile_power(&state, clock.monotonic_ms) {
        Ok(power) => power,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if cause == "button" && power.light {
        return (StatusCode::CONFLICT, Json(json!({"error":"already_awake"}))).into_response();
    }
    match controlled_reboot(&state, clock.monotonic_ms, cause) {
        Ok(()) => Json(json!({"boot_id":session_snapshot(&state).boot_id,
            "power":state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
                .json(clock.monotonic_ms)})).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn sim_display(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if value["fail_next"] != true || value.as_object().is_none_or(|object| object.len() != 1) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .fail_next_display() {
        Ok(()) => Json(json!({"fail_next":true})).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn sim_power(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(object) = value.as_object() else { return StatusCode::BAD_REQUEST.into_response(); };
    if object.is_empty() || object.keys().any(|key| ![
        "plugged", "deep_on_usb", "manual_ble_hold", "battery_pct"
    ].contains(&key.as_str()))
        || object.iter().any(|(key, value)| if key == "battery_pct" {
            value.as_u64().is_none_or(|pct| pct > 100)
        } else { !value.is_boolean() }) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(clock) = clock_snapshot(&state) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    {
        let mut power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(value) = object.get("plugged") { power.plugged = value.as_bool().unwrap(); }
        if let Some(value) = object.get("deep_on_usb") {
            power.deep_on_usb = value.as_bool().unwrap();
        }
        if let Some(value) = object.get("manual_ble_hold") {
            power.manual_ble_hold = value.as_bool().unwrap();
        }
        if let Some(value) = object.get("battery_pct") {
            power.battery_pct = value.as_u64().unwrap() as u8;
        }
    }
    match reconcile_power(&state, clock.monotonic_ms) {
        Ok(power) => Json(power.json(clock.monotonic_ms)).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn sim_fault(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if let Some(size) = value["status_padding_bytes"].as_u64() {
        if size > 20000 || value.as_object().is_none_or(|v| v.len() != 1) {
            return StatusCode::BAD_REQUEST.into_response();
        }
        *state.status_padding_bytes.lock().unwrap() = size as usize;
        return Json(json!({"status_padding_bytes":size})).into_response();
    }
    let Some(after) = value["stall_ack_after"].as_str() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let duration_ms = value.get("duration_ms").and_then(serde_json::Value::as_u64)
        .unwrap_or(10_000);
    if !["none", "bundle_commit", "data", "activate", "ota_upload"].contains(&after)
        || (value.get("duration_ms").is_some() && value["duration_ms"].as_u64().is_none())
        || !(1_000..=120_000).contains(&duration_ms)
        || value.as_object().is_none_or(|object| object.len() > 2
            || object.keys().any(|key| key != "stall_ack_after" && key != "duration_ms")) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    *state.stall_ack_after.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
        (after != "none").then(|| StallFault {operation: after.to_owned(), duration_ms});
    Json(json!({"stall_ack_after":after,"duration_ms":duration_ms})).into_response()
}

fn take_stalled_ack(state: &SimState, operation: &str, applied: bool) -> Option<u64> {
    if !applied { return None; }
    let mut fault = state.stall_ack_after.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if fault.as_ref().map(|fault| fault.operation.as_str()) != Some(operation) { return None; }
    let duration_ms = fault.as_ref().map(|fault| fault.duration_ms);
    *fault = None;
    duration_ms
}

async fn sim_button(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if value["hold_ms"].as_u64().is_none_or(|hold| !(2000..15000).contains(&hold))
        || value.as_object().is_none_or(|object| object.len() != 1) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(clock) = clock_snapshot(&state) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    match reconcile_power(&state, clock.monotonic_ms) {
        Ok(power) if power.light => {},
        Ok(_) => return (StatusCode::CONFLICT,
            Json(json!({"error":"asleep"}))).into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
    let uptime_ms = device_uptime_ms(&state, clock);
    let mut bundle = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if bundle.set_wall_secs((clock.wall_ms / 1000).min(i64::MAX as u64) as i64).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let count = bundle.status().ok().and_then(|status|
        status["commit_seq"].as_u64()).unwrap_or(0).saturating_add(1);
    let context = format!("{:016x}{:016x}", clock.monotonic_ms, count);
    match bundle.button_next(uptime_ms, &context) {
        Ok(result) => Json(result).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn sim_storage(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return (StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error":"body_too_large"}))).into_response(),
    };
    let value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_storage_command"}))).into_response(),
    };
    if value.as_object().is_some_and(|object| object.len() == 2 &&
        object.contains_key("crash_after_sync") && object.contains_key("count")) {
        let kind = value["crash_after_sync"].as_str().unwrap_or("");
        let count = value["count"].as_i64().unwrap_or(0);
        if !(1..=100).contains(&count) {
            return (StatusCode::BAD_REQUEST,
                Json(json!({"error":"invalid_storage_command"}))).into_response();
        }
        let mut bundle = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        return match bundle.crash_after_sync(kind, count as i32) {
            Ok(()) => Json(json!({"crash_after_sync":kind,"count":count})).into_response(),
            Err(_) => (StatusCode::BAD_REQUEST,
                Json(json!({"error":"invalid_storage_command"}))).into_response(),
        };
    }
    if value.as_object().is_none_or(|object| object.len() != 1 || !object.contains_key("write_budget")) {
        return (StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_storage_command"}))).into_response();
    }
    let budget = if value["write_budget"].is_null() { -1 } else {
        match value["write_budget"].as_i64() {
            Some(budget) if (0..=1048576).contains(&budget) => budget,
            _ => return (StatusCode::BAD_REQUEST,
                Json(json!({"error":"invalid_storage_command"}))).into_response(),
        }
    };
    let mut bundle = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match bundle.set_write_budget(budget).and_then(|_| bundle.status()) {
        Ok(status) => Json(json!({"write_budget":status["write_budget"]})).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"storage_command_failed"}))).into_response(),
    }
}

fn query_args(request: &Request<Body>) -> HashMap<String, String> {
    let mut args = HashMap::new();
    for (key, value) in form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
        args.entry(key.into_owned()).or_insert_with(|| value.into_owned());
    }
    args
}

async fn sim_versions_get(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    Json(state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .state_json()).into_response()
}

async fn sim_versions_post(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return (StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error":"body_too_large"}))).into_response(),
    };
    let value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_version_command"}))).into_response(),
    };
    let Some(id) = value["id"].as_str() else {
        return (StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_version_command"}))).into_response();
    };
    if value["reboot"] != true || value.as_object().is_none_or(|object| object.len() != 2) {
        return (StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_version_command"}))).into_response();
    }
    let mut ota = state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match ota.override_to(id) {
        Ok(true) => {
            let Some(clock) = clock_snapshot(&state) else {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            };
            if controlled_reboot(&state, clock.monotonic_ms, "soft").is_err() {
                return (StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"session_rotation_failed"}))).into_response();
            }
            Json(ota.state_json()).into_response()
        }
        Ok(false) => (StatusCode::BAD_REQUEST,
            Json(json!({"error":"unknown_version"}))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"version_storage_failed"}))).into_response(),
    }
}

async fn ota_token_route(State(state): State<SimState>, request: Request<Body>) -> Response {
    if let Some(response) = device_gate(&state).await { return response; }
    if query_args(&request).get("token").map(String::as_str) != Some(state.device_token.as_ref()) {
        return unauthorized();
    }
    (StatusCode::OK, "Fake ROM OTA").into_response()
}

async fn receive_ota(state: &SimState, request: Request<Body>) -> Result<(u64, String)> {
    let mut form = Multipart::from_request(request, state).await
        .map_err(|_| anyhow::anyhow!("invalid multipart upload"))?;
    let mut field = form.next_field().await.context("invalid multipart field")?
        .context("missing firmware field")?;
    ensure!(field.name() == Some("firmware"), "missing firmware field");
    let mut file = fs::File::create(state.data_dir.join("sim-upload.tmp"))
        .context("cannot create OTA upload file")?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    while let Some(chunk) = field.chunk().await.context("truncated OTA upload")? {
        size += chunk.len() as u64;
        ensure!(size <= 0x30_0000, "OTA upload too large");
        file.write_all(&chunk).context("cannot write OTA upload file")?;
        hasher.update(&chunk);
    }
    drop(field);
    file.flush().context("cannot flush OTA upload file")?;
    ensure!(size >= 1024, "OTA upload too small");
    ensure!(form.next_field().await.context("invalid multipart tail")?.is_none(),
        "multiple OTA fields are unsupported");
    Ok((size, format!("{:x}", hasher.finalize())))
}

async fn ota_upload(State(state): State<SimState>, request: Request<Body>) -> Response {
    if let Some(response) = device_gate(&state).await { return response; }
    let args = query_args(&request);
    if args.get("token").map(String::as_str) != Some(state.device_token.as_ref()) {
        return unauthorized();
    }
    if args.get("target").is_some_and(|target| target != &state.target) {
        return unauthorized();
    }
    if state.ota_running.swap(true, Ordering::SeqCst) {
        return (StatusCode::CONFLICT, "UPDATE FAILED").into_response();
    }
    let _guard = OtaUploadGuard(state.ota_running.clone());
    let received = receive_ota(&state, request).await;
    let (size, sha256) = match received {
        Ok(received) => received,
        Err(error) => {
            let _ = fs::remove_file(state.data_dir.join("sim-upload.tmp"));
            eprintln!("sim OTA upload rejected: {error:#}");
            return (StatusCode::OK, "UPDATE FAILED").into_response();
        }
    };
    let accepted = state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .queue_upload(size, &sha256);
    let _ = fs::remove_file(state.data_dir.join("sim-upload.tmp"));
    match accepted {
        Ok(Some(_)) => {
            let reboot_state = state.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let changed = reboot_state.ota.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .commit_pending().unwrap_or(false);
                if changed {
                    if let Some(clock) = clock_snapshot(&reboot_state) {
                        let _ = controlled_reboot(&reboot_state, clock.monotonic_ms, "soft");
                    }
                }
            });
            if let Some(duration_ms) = take_stalled_ack(&state, "ota_upload", true) {
                tokio::time::sleep(Duration::from_millis(duration_ms)).await;
            }
            (StatusCode::OK, "UPDATE OK").into_response()
        }
        Ok(None) => {
            eprintln!("sim OTA upload is outside the version catalog: {size} bytes, SHA256 {sha256}");
            (StatusCode::OK, "UPDATE FAILED").into_response()
        }
        Err(error) => {
            eprintln!("sim OTA storage failed: {error:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "UPDATE FAILED").into_response()
        },
    }
}

async fn ota_image(State(state): State<SimState>, request: Request<Body>) -> Response {
    if let Some(response) = device_gate(&state).await { return response; }
    if !bearer(&request, &state.device_token) { return unauthorized(); }
    let Ok(bytes)=to_bytes(request.into_body(),4096).await else {return StatusCode::PAYLOAD_TOO_LARGE.into_response();};
    let input:Value=serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let Some(length) = input["image_bytes"].as_u64().filter(|n|*n<=u32::MAX as u64).map(|n|n as usize) else {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"range"}))).into_response();
    };
    let ota = state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Ok(bytes) = ota.active_bytes() else {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"storage"}))).into_response();
    };
    if length == 0 || length > bytes.len() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"range"}))).into_response();
    }
    Json(json!({"algorithm":"sha256-running-prefix-v1", "image_bytes":length,
        "sha256":sha256_hex(&bytes[..length]), "device_mac":state.mac,
        "running_slot":format!("ota_{}", ota.file.slot), "fw_target":state.target,
        "fw":ota.active_version().fw})).into_response()
}

async fn device_command(State(state): State<SimState>, request: Request<Body>, operation: &str) -> Response {
    device_command_impl(state, request, operation, true).await
}

async fn device_command_impl(state: SimState, request: Request<Body>, operation: &str,
    gate: bool) -> Response {
    if gate { if let Some(response) = device_gate(&state).await { return response; } }
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return (StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error":"body_too_large"}))).into_response(),
    };
    let Ok(message) = std::str::from_utf8(&body) else {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"json"}))).into_response();
    };
    let parsed = match bridge_render::simulator_command_parse(message) {
        Ok(value) if value["parsed"] == true => value,
        _ => return (StatusCode::BAD_REQUEST, Json(json!({"error":"json"}))).into_response(),
    };
    let bridge_id = parsed["bridge_id"].as_str().unwrap_or("");
    let Some(clock) = clock_snapshot(&state) else {
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"clock_unavailable"}))).into_response();
    };
    {
        let mut owners = state.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let owner = match owners.get_valid(now_seconds(&state, clock)) {
            Ok(owner) => owner,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"owner_storage_failed"}))).into_response(),
        };
        if owner.as_ref().is_some_and(|owner| owner.id != bridge_id) {
            return (StatusCode::CONFLICT, Json(json!({"result":"rejected","error":"occupied"}))).into_response();
        }
    }
    let nonce = session_snapshot(&state).nonce;
    let uptime_ms = device_uptime_ms(&state, clock);
    let session = match bridge_render::simulator_command_check(message, &state.mac, &nonce) {
        Ok(session) => session,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"session_check_failed"}))).into_response(),
    };
    if session["accepted"] != true {
        return Json(json!({"op":"bundle","result":"rejected","display_state":"unchanged",
            "error":session["error"]})).into_response();
    }
    if let Some(current) = state.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .owner.as_mut() { current.last_seen = now_seconds(&state, clock); }
    let result = {
        let mut bundle = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if bundle.set_wall_secs((clock.wall_ms / 1000).min(i64::MAX as u64) as i64).is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if operation == "commit" {
            let count = bundle.status().ok().and_then(|status|
                status["commit_seq"].as_u64()).unwrap_or(0).saturating_add(1);
            let context = format!("{:016x}{:016x}", clock.monotonic_ms, count);
            bundle.commit(message, &nonce, uptime_ms, &context)
        } else if operation == "activate" {
            let count = bundle.status().ok().and_then(|status|
                status["commit_seq"].as_u64()).unwrap_or(0).saturating_add(1);
            let context = format!("{:016x}{:016x}", clock.monotonic_ms, count);
            bundle.activate(message, uptime_ms, &context)
        } else if operation == "data" {
            bundle.data(message)
        } else { bundle.begin(message, &nonce, uptime_ms) }
    };
    match result {
        Ok(ack) => {
            let fault_name = if operation == "commit" { "bundle_commit" } else { operation };
            if let Some(duration_ms) = take_stalled_ack(&state, fault_name, ack["result"] == "applied") {
                tokio::time::sleep(Duration::from_millis(duration_ms)).await;
            }
            Json(ack).into_response()
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"bundle_command_failed"}))).into_response(),
    }
}

async fn bundle_begin(state: State<SimState>, request: Request<Body>) -> Response {
    device_command(state, request, "begin").await
}

async fn bundle_commit(state: State<SimState>, request: Request<Body>) -> Response {
    device_command(state, request, "commit").await
}

async fn data(state: State<SimState>, request: Request<Body>) -> Response {
    device_command(state, request, "data").await
}

async fn activate(state: State<SimState>, request: Request<Body>) -> Response {
    device_command(state, request, "activate").await
}

async fn bundle_chunk(State(state): State<SimState>, request: Request<Body>) -> Response {
    if let Some(response) = device_gate(&state).await { return response; }
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    let query = request.uri().query().unwrap_or("").to_owned();
    let mut args = HashMap::new();
    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        args.entry(key.into_owned()).or_insert_with(|| value.into_owned());
    }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return (StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error":"body_too_large"}))).into_response(),
    };
    let Some(clock) = clock_snapshot(&state) else {
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"clock_unavailable"}))).into_response();
    };
    let uptime_ms = device_uptime_ms(&state, clock);
    let now_s = now_seconds(&state, clock);
    let rx_owner = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .status().ok().and_then(|status|
        status["rx_owner"].as_str().map(str::to_owned)).unwrap_or_default();
    let mut owners = state.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let owner = match owners.get_valid(now_s) {
        Ok(owner) => owner,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"owner_storage_failed"}))).into_response(),
    };
    if owner.as_ref().is_some_and(|owner| owner.id != rx_owner) {
        return (StatusCode::CONFLICT, Json(json!({"result":"rejected","error":"occupied"}))).into_response();
    }
    if owner.is_some() {
        if let Some(current) = owners.owner.as_mut() { current.last_seen = now_s; }
    }
    drop(owners);
    let result = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner).chunk(
        args.get("request_id").map(String::as_str).unwrap_or(""),
        args.get("session_nonce").map(String::as_str).unwrap_or(""),
        args.get("offset").map(String::as_str).unwrap_or(""),
        &body, uptime_ms);
    match result {
        Ok(ack) => Json(ack).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"bundle_chunk_failed"}))).into_response(),
    }
}

async fn plan(State(state): State<SimState>, request: Request<Body>) -> Response {
    plan_impl(state, request, true).await
}

async fn plan_impl(state: SimState, request: Request<Body>, gate: bool) -> Response {
    if gate { if let Some(response) = device_gate(&state).await { return response; } }
    if !bearer(&request, &state.endpoint_token) {
        return unauthorized();
    }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error":"body_too_large"})),
            )
                .into_response()
        }
    };
    let Ok(message) = std::str::from_utf8(&body) else {
        return match bridge_render::simulator_plan_ack(
            "command",
            "rejected",
            "unchanged",
            Some("json"),
            0,
            None,
        ) {
            Ok(ack) => Json(ack).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"plan_ack_failed"})),
            )
                .into_response(),
        };
    };
    let parsed = match bridge_render::simulator_command_parse(message) {
        Ok(parsed) if parsed["parsed"] == true => parsed,
        _ => {
            return match bridge_render::simulator_plan_ack(
                "command",
                "rejected",
                "unchanged",
                Some("json"),
                0,
                None,
            ) {
                Ok(ack) => Json(ack).into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"plan_ack_failed"})),
                )
                    .into_response(),
            }
        }
    };
    let Some(bridge_id) = parsed["bridge_id"].as_str() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"command_parse_failed"})),
        )
            .into_response();
    };
    let Some(clock) = clock_snapshot(&state) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"clock_unavailable"})),
        )
            .into_response();
    };
    let now_s = now_seconds(&state, clock);
    {
        let mut owners = state
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let owner = match owners.get_valid(now_s) {
            Ok(owner) => owner,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"owner_storage_failed"})),
                )
                    .into_response()
            }
        };
        if let Some(owner) = owner {
            if owner.id != bridge_id {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({"result":"rejected","error":"occupied","owner":owner_json(Some(&owner), now_s)})),
                )
                    .into_response();
            }
        }
    }
    let session = match bridge_render::simulator_command_check(message, &state.mac,
        &session_snapshot(&state).nonce) {
        Ok(session) => session,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"session_check_failed"})),
            )
                .into_response()
        }
    };
    if session["accepted"] != true {
        let error = session["error"].as_str().unwrap_or("session");
        return match bridge_render::simulator_plan_ack(
            "command",
            "rejected",
            "unchanged",
            Some(error),
            0,
            None,
        ) {
            Ok(ack) => Json(ack).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"plan_ack_failed"})),
            )
                .into_response(),
        };
    }
    if let Some(current) = state.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .owner.as_mut() { current.last_seen = now_s; }
    let decision = match state
        .plan
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .decide(message, device_uptime_ms(&state, clock))
    {
        Ok(decision) => decision,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"plan_decision_failed"})),
            )
                .into_response()
        }
    };
    let error = decision["error"].as_str();
    let granted = decision["granted_s"].as_u64().map(|value| value as u32);
    match bridge_render::simulator_plan_ack(
        "plan",
        decision["result"].as_str().unwrap_or("rejected"),
        decision["display"].as_str().unwrap_or("unchanged"),
        error,
        decision["plan_id"].as_u64().unwrap_or(0),
        granted,
    ) {
        Ok(mut ack) => {
            if ack["result"] == "applied" {
                let mut bundle = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if bundle.set_wall_secs((clock.wall_ms / 1000) as i64).is_err()
                    || bundle.note_bridge_contact().is_err() {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"result":"unknown",
                        "error":"display_contact","stage":"plan_applied"}))).into_response();
                }
                drop(bundle);
                let _ = reconcile_power(&state, clock.monotonic_ms);
                if let Some(data) = serde_json::from_str::<Value>(message).ok()
                    .and_then(|payload| payload.get("data").cloned()) {
                    let data_reply = state.bundle.lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .data(&data.to_string());
                    ack["data"] = data_reply.unwrap_or_else(|_| json!({"result":"rejected",
                        "error":"data_processing"}));
                }
            }
            Json(ack).into_response()
        },
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"plan_ack_failed"})),
        )
            .into_response(),
    }
}

async fn sim_ble_info(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    let Some(clock) = clock_snapshot(&state) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(power) = reconcile_power(&state, clock.monotonic_ms) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if !power.light && power.ble_window_until_ms.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let generation = session_snapshot(&state).boot_id;
    let fw = state.ota.lock().unwrap().active_version().fw.clone();
    let bytes = match bridge_render::simulator_ble_wire("info", &json!({
        "mac":state.mac,"ip":"127.0.0.1","fw":fw,"peerBonded":true,"peerEncrypted":true,
        "wake_generation":generation,"wake_seq":power.wake_count,"bundle_configured":power.configured,
        "wifi_setup_protocol":1,"wifi_management":if state.wifi.lock().unwrap().unified {3}else{2},"wifi_connection":1,"rf_remaining_ms":power.ble_window_until_ms.map(|until| until.saturating_sub(clock.monotonic_ms))})) {
        Ok(bytes) => bytes, Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let connect = request.uri().query().is_some_and(|q| q == "connect=1");
    let mut ble = state.ble.lock().unwrap();
    if connect || !ble.connected || ble.wake != (generation, power.wake_count) {
        ble.generation = ble.generation.saturating_add(1); ble.connected = true;
        ble.wake = (generation, power.wake_count); ble.status.clear(); ble.pending = None; ble.read_failed = false;
        ble.fence = RequestFence::default();
        state.ble_rx.lock().unwrap().clear();
        let mut rv=state.rendezvous.lock().unwrap();
        rv.begin(clock.monotonic_ms,4000);
        rv.connected(clock.monotonic_ms);
    }
    ble.info = bytes.clone();
    ([("x-sim-ble-generation", ble.generation.to_string())], bytes).into_response()
}

fn ble_generation_ok(state: &SimState, request: &Request<Body>) -> bool {
    // Header-less calls remain a test carrier; DeviceConnection always stamps it.
    request.headers().get("x-sim-ble-generation").map_or(true, |header|
        header.to_str().ok().and_then(|g| g.parse::<u64>().ok()) == Some(state.ble.lock().unwrap().generation))
}
async fn sim_ble_status(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    if !ble_generation_ok(&state, &request) { return StatusCode::CONFLICT.into_response(); }
    let Some(clock) = clock_snapshot(&state) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    let Ok(power) = reconcile_power(&state, clock.monotonic_ms) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    let mut ble = state.ble.lock().unwrap();
    if !ble.connected || (!power.light && power.ble_window_until_ms.is_none()) ||
        ble.wake != (session_snapshot(&state).boot_id, power.wake_count) {
        ble.connected = false; ble.pending = None; state.ble_rx.lock().unwrap().clear();
        return StatusCode::NOT_FOUND.into_response();
    }
    if ble.read_failed { return StatusCode::SERVICE_UNAVAILABLE.into_response(); }
    if ble.pending.as_ref().is_some_and(|(at, _)| clock.monotonic_ms >= *at) {
        ble.status = ble.pending.take().unwrap().1;
    }
    ble.status.clone().into_response()
}
async fn sim_ble_aux(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    if !ble_generation_ok(&state, &request) { return StatusCode::CONFLICT.into_response(); }
    let token = request.uri().path().ends_with("token");
    let Some(clock) = clock_snapshot(&state) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    let Ok(power) = reconcile_power(&state, clock.monotonic_ms) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    if !power.light && power.ble_window_until_ms.is_none() { return StatusCode::NOT_FOUND.into_response(); }
    let Ok(bytes) = to_bytes(request.into_body(), if token {256} else {1024}).await else { return StatusCode::PAYLOAD_TOO_LARGE.into_response(); };
    let Ok(body) = serde_json::from_slice::<Value>(&bytes) else { return StatusCode::BAD_REQUEST.into_response(); };
    let valid = if token { body["cmd"] == "token" && body.get("rotate").is_none() }
        else { matches!(body["schema"].as_u64(),Some(1|2)) && body["host"].is_string() && body["port"].as_u64().is_some_and(|p| p > 0 && p <= 65535) && body["token"] == state.endpoint_token.as_ref() };
    let op = if token {"auth"} else {"endpoint"};
    let mut response = json!({"ack":op,"ok":valid});
    if token && valid { response["token"] = json!(state.device_token.as_ref()); }
    let Ok(raw) = bridge_render::simulator_ble_wire(op, &response) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    state.ble.lock().unwrap().store(op, raw, clock.monotonic_ms);
    Json(json!({"result":"accepted"})).into_response()
}
// Network adapter around the shared ROM rendezvous/Wi-Fi engines. Real RF and
// Arduino HTTPClient are intentionally outside the host simulator.
async fn sim_ready_reports(state: SimState) {
    let client = reqwest::Client::builder().no_proxy().local_address(IpAddr::V4(state.report_ip))
        .timeout(Duration::from_secs(1)).build().unwrap();
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let intent = state.ready_report.lock().unwrap().clone();
        let Some(intent) = intent else { continue; };
        if clock_snapshot(&state).is_some_and(|clock| state.rendezvous.lock().unwrap().deadline()<=clock.monotonic_ms) {
            *state.ready_report.lock().unwrap()=None;
            continue;
        }
        let request = Request::builder().uri("/api/status")
            .header(header::AUTHORIZATION, format!("Bearer {}", state.endpoint_token))
            .body(Body::empty()).unwrap();
        let response = status_impl(state.clone(), request, false).await;
        let Ok(bytes) = to_bytes(response.into_body(), 16384).await else {continue;};
        let Ok(status) = serde_json::from_slice::<Value>(&bytes) else {continue;};
        if status["rendezvous"]["ready"] != true {continue;}
        *state.ready_report.lock().unwrap() = None;
        let principal = intent["bridge_id"].as_str().unwrap_or("");
        let base = format!("http://{}:{}/api/device", intent["bridge_addr"].as_str().unwrap_or(""), intent["bridge_port"]);
        let boot = session_snapshot(&state).nonce;
        let hello = json!({"device_mac":state.mac,"boot_id":boot,
            "principal":principal,"instance_id":principal});
        let mut received = false;
        for attempt in 0..3 {
            let Some(clock) = clock_snapshot(&state) else {break;};
            let deadline = state.rendezvous.lock().unwrap().deadline();
            if deadline <= clock.monotonic_ms + 1000 {break;}
            let result: Result<()> = async {
                let session:Value = client.post(format!("{base}/session")).bearer_auth(state.endpoint_token.as_ref())
                    .json(&hello).send().await?.error_for_status()?.json().await?;
                let id=session["session_id"].as_str().context("missing report session")?;
                ensure!(id.len()==32,"invalid report session");
                let mut session_bytes=[0u8;16];
                for (index,byte) in session_bytes.iter_mut().enumerate() {
                    *byte=u8::from_str_radix(&id[index*2..index*2+2],16)?;
                }
                let report=json!({"device_mac":state.mac,"boot_id":boot,"principal":principal,
                    "instance_id":principal,"origin_session":intent["session_nonce"],
                    "origin_request_no":intent["origin_request_no"],"business_id":intent["plan_id"].to_string(),
                    "context_id":"","ip":state.report_ip.to_string(),"port":80,"http_ready":true,
                    "plan_result":"applied","data_result":"none","network_result":"ready",
                    "remaining_ms":(deadline-clock.monotonic_ms).min(90000)});
                let payload=serde_json::to_vec(&report)?;
                let frame=Header{kind:2,op:20,result:0,payload_len:payload.len() as u16,
                    session:session_bytes,request_no:1,context:[0;16]};
                let mut wire=frame.encode().map_err(anyhow::Error::msg)?.to_vec();wire.extend_from_slice(&payload);
                let response=client.post(format!("{base}/report")).bearer_auth(state.endpoint_token.as_ref())
                    .header(header::CONTENT_TYPE,bridge_core::compact_frame::CONTENT_TYPE).body(wire).send().await?;
                ensure!(response.status()==StatusCode::ACCEPTED,"report rejected");
                let bytes=response.bytes().await?;
                let (ack,payload)=Header::decode(&bytes,256).map_err(anyhow::Error::msg)?;
                ensure!(ack.kind==1 && ack.op==20 && ack.result==1 && ack.request_no==1 && ack.session==frame.session,"report ACK binding");
                let body:Value=serde_json::from_slice(payload)?;
                ensure!(body["result"]=="report_received","report ACK result");
                Ok(())
            }.await;
            if result.is_ok() {received=true;break;}
            eprintln!("synthetic ready report attempt {}: {}",attempt+1,result.unwrap_err());
            tokio::time::sleep(Duration::from_millis(if attempt==0 {250}else{1000})).await;
        }
        state.ble.lock().unwrap().events.push_back(json!({"event":"ready_report","received":received}));
    }
}

async fn sim_ble_close(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    if !ble_generation_ok(&state, &request) { return StatusCode::CONFLICT.into_response(); }
    let mut ble = state.ble.lock().unwrap();
    ble.connected = false; ble.pending = None; ble.status.clear();
    state.ble_rx.lock().unwrap().clear();
    Json(json!({"result":"closed"})).into_response()
}
async fn sim_ble_fault(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let Ok(bytes) = to_bytes(request.into_body(), 1024).await else { return StatusCode::PAYLOAD_TOO_LARGE.into_response(); };
    let Ok(fault) = serde_json::from_slice::<BleFault>(&bytes) else { return StatusCode::BAD_REQUEST.into_response(); };
    if !matches!(fault.kind.as_str(), "none" | "legacy_overflow" | "guard_overflow" | "write_failed" |
        "disconnect" | "window_closed" | "empty" | "malformed" | "stale" | "overwrite" | "read_failed" | "delay" | "notify_lost" | "drop_ack" | "wrong_request" | "wrong_wake" | "wrong_mac" | "wrong_session" | "wrong_op") ||
        fault.size > 8192 || fault.delay_ms > 86_400_000 { return StatusCode::BAD_REQUEST.into_response(); }
    state.ble.lock().unwrap().fault = Some(fault);
    Json(json!({"result":"armed"})).into_response()
}
async fn sim_ble_attributes(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let ble = state.ble.lock().unwrap();
    Json(json!({"generation":ble.generation,"connected":ble.connected,"info_bytes":ble.info.len(),
        "status_bytes":ble.status.len(),"events":ble.events})).into_response()
}

async fn sim_ble_business(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    let body = match to_bytes(request.into_body(), 8192).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let Ok(message) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if message["token"] != state.endpoint_token.as_ref() {
        return unauthorized();
    }
    let Some(clock) = clock_snapshot(&state) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(power) = reconcile_power(&state, clock.monotonic_ms) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if !power.light && power.ble_window_until_ms.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(op) = message["op"].as_str() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let request_id = message["request_id"].clone();
    if request_id.as_str().is_none_or(|id| id.is_empty() || id.len() > 64 || id.contains('\0')) { return StatusCode::BAD_REQUEST.into_response(); }
    if message.get("protocol").is_some() || message.get("rv").is_some() {
        return Json(json!({"ack":"command","op":"command","request_id":request_id,
            "result":"rejected","error":"retired_protocol"})).into_response();
    }
    let response = match op {
        "status" => {
            let request = Request::builder().uri("/api/status")
                .header(header::AUTHORIZATION, format!("Bearer {}", state.endpoint_token))
                .body(Body::empty()).unwrap();
            status_impl(state.clone(), request, false).await
        }
        "plan" => {
            let request = Request::builder().uri("/api/plan")
                .header(header::AUTHORIZATION, format!("Bearer {}", state.endpoint_token))
                .body(Body::from(body.to_vec())).unwrap();
            plan_impl(state.clone(), request, false).await
        }
        "data" => {
            let request = Request::builder().uri("/api/data")
                .header(header::AUTHORIZATION, format!("Bearer {}", state.endpoint_token))
                .body(Body::from(body.to_vec())).unwrap();
            device_command_impl(state.clone(), request, "data", false).await
        }
        "network_start" => {
            let session = session_snapshot(&state);
            let (_, _, plan_id, _, _) = state.plan.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
                .status_fields();
            let owner = state.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
                .get_valid(now_seconds(&state, clock)).ok().flatten();
            let address = message["bridge_addr"].as_str().and_then(|s| s.parse::<std::net::Ipv4Addr>().ok());
            let valid = message["device_mac"] == state.mac &&
                message["session_nonce"] == session.nonce &&
                owner.as_ref().is_some_and(|o| message["bridge_id"] == o.id) &&
                message["plan_id"].as_u64().is_some_and(|id| id > 0 && id == plan_id) &&
                message["origin_request_no"].as_u64().is_some_and(|id| id > 0) &&
                message["bridge_port"].as_u64().is_some_and(|p| (1..=65535).contains(&p)) &&
                address.is_some_and(|ip| !ip.is_unspecified() && !ip.is_multicast()) &&
                (power.light || state.power.lock().unwrap().sync_open);
            let admitted=if valid {
                let (_,_,_,granted,accepted_at)=state.plan.lock().unwrap().status_fields();
                let legal_until=accepted_at.saturating_add((granted as u64).saturating_mul(1000));
                let mut rv=state.rendezvous.lock().unwrap();
                rv.admit_http(clock.monotonic_ms,legal_until);
                rv.phase()==2
            } else {false};
            if admitted { *state.ready_report.lock().unwrap() = Some(message.clone()); }
            Json(json!({"op":"network_start","result":if admitted {"accepted"} else {"rejected"},
                "error":if admitted {None} else {Some("plan_window")}})).into_response()
        }
        "history" => {
            if message["device_mac"]!=state.mac||message["session_nonce"]!=session_snapshot(&state).nonce {
                Json(json!({"op":"history","result":"rejected","error":"session"})).into_response()
            } else {
                let reply=state.sync.lock().unwrap().history(message["since"].as_u64().unwrap_or(0) as u32,
                    message["limit"].as_u64().unwrap_or(8).min(64) as u32);
                match reply {Ok(mut reply)=>{reply["op"]=json!("history");reply["result"]=json!("applied");Json(reply).into_response()},Err(_)=>StatusCode::INTERNAL_SERVER_ERROR.into_response()}
            }
        }
        "sync_config" | "sync_open" => {
            let now_s = now_seconds(&state, clock);
            let owner = match state.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
                .get_valid(now_s) {
                Ok(owner) => owner,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            let bridge = message["bridge_id"].as_str().unwrap_or("");
            let current_nonce = session_snapshot(&state).nonce;
            let error = if message["device_mac"].as_str() != Some(state.mac.as_str()) ||
                message["session_nonce"].as_str() != Some(current_nonce.as_str()) {
                Some("session")
            } else if owner.as_ref().is_none() { Some("claim_required") }
            else if owner.as_ref().is_some_and(|owner| owner.id != bridge) { Some("occupied") }
            else { None };
            if let Some(error) = error {
                Json(json!({"op":op,"result":"rejected","error":error})).into_response()
            } else if op == "sync_config" {
                let enabled = message["enabled"].as_bool();
                match enabled.and_then(|enabled| state.sync.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .set_owner(bridge, enabled).ok().map(|_| enabled)) {
                    Some(enabled) => Json(json!({"op":op,"result":"applied",
                        "enabled":enabled})).into_response(),
                    None => Json(json!({"op":op,"result":"rejected","error":"storage"}))
                        .into_response(),
                }
            } else {
                let sync = state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
                    .status().unwrap_or(serde_json::Value::Null);
                let reason = message["reason"].as_str().unwrap_or("");
                let open_id = message["open_id"].as_str().unwrap_or("");
                let valid = sync["enabled"] == true && sync["retry_skip"] == 0 &&
                    !open_id.is_empty() && open_id.len() <= 64 &&
                    message["wake_generation"] == session_snapshot(&state).boot_id &&
                    message["wake_seq"] == power.wake_count &&
                    ((reason == "periodic" && sync["rounds"] == 15 && sync["due"] == true) ||
                     (reason == "retry" && (sync["due"] == true ||
                       !sync["pending_batch"].is_null())));
                let mut live = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let valid = valid && live.sync_visible_skip == 0 &&
                    (!live.sync_open || live.sync_open_id == open_id);
                if valid {
                    if !live.sync_open { live.sync_last_progress_ms = clock.monotonic_ms; }
                    live.sync_open = true;
                    live.sync_open_id = open_id.to_owned();
                    live.next_contact_ms = None;
                    Json(json!({"op":op,"result":"applied","wifi_open":true})).into_response()
                } else {
                    Json(json!({"op":op,"result":"rejected","error":"range"})).into_response()
                }
            }
        }
        _ => Json(json!({"op":"command","result":"rejected","error":"http_required"})).into_response(),
    };
    if response.status() != StatusCode::OK { return response; }
    let Ok(bytes) = to_bytes(response.into_body(), BODY_LIMIT).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(mut ack) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if op == "status" {
        // Runtime input for the shared C++ BLE projection below. No HTTP
        // document is stored as an attribute, and no firmware bytes re-encoded.
        ack["result"] = "applied".into();
        ack["sync"]["v"] = json!(1);
        ack["sync"]["pending"] = json!(!ack["sync"]["pending_batch"].is_null());
        ack["sync"]["retry_skip"] = json!(power.sync_visible_skip);
    }
    ack["ack"] = "command".into();
    ack["request_id"] = request_id;
    ack["wake_generation"] = session_snapshot(&state).boot_id.into();
    ack["wake_seq"] = power.wake_count.into();
    if op == "plan" && ack["result"] == "applied" {
        let light_plan = {
            let plan = state.plan.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            plan.status_fields().1 == "light"
        };
        if light_plan {
            let mut power = state.power.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let was_light = power.light;
            power.light = true;
            power.ble_window_until_ms = None;
            power.next_contact_ms = None;
            drop(power);
            if !was_light && state.sync.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .status().is_ok_and(|sync| sync["enabled"] == true) {
                if state.sync.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
                    .add_reason(1, device_uptime_ms(&state, clock) as u32).is_err() {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
    }
    Json(ack).into_response()
}

// Control-only fault/physical actions; business callers cannot self-assert a bond.
async fn sim_history_control(State(state):State<SimState>,request:Request<Body>)->Response {
    if !bearer(&request,&state.control_token){return unauthorized();}
    let Ok(bytes)=to_bytes(request.into_body(),1024).await else{return StatusCode::BAD_REQUEST.into_response();};
    let Ok(value)=serde_json::from_slice::<Value>(&bytes) else{return StatusCode::BAD_REQUEST.into_response();};
    match state.sync.lock().unwrap().append_wake(&value){Ok(())=>Json(json!({"ok":true,"synthetic":true})).into_response(),Err(_)=>StatusCode::BAD_REQUEST.into_response()}
}
async fn sim_wifi_control(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) { return unauthorized(); }
    let Some(clock) = clock_snapshot(&state) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    let Ok(bytes) = to_bytes(request.into_body(), 1024).await else { return StatusCode::BAD_REQUEST.into_response(); };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else { return StatusCode::BAD_REQUEST.into_response(); };
    let generation = state.ble.lock().unwrap().generation as u32;
    let mut wifi = state.wifi.lock().unwrap();
    if let Some(ms) = value["maintenance_ms"].as_u64() {
        if ms > 120000 { return StatusCode::BAD_REQUEST.into_response(); }
        wifi.physical_until = clock.monotonic_ms + ms;
    }
    if let Some(peer) = value["bond"].as_str() {
        if peer.len() != 19 || !peer.ends_with("/1") { return StatusCode::BAD_REQUEST.into_response(); }
        wifi.peer = peer.into();
        let mut ble = state.ble.lock().unwrap(); ble.connected = false; ble.status.clear(); ble.pending = None;
    }
    if let Some(legacy)=value.get("legacy") { if wifi.engine.legacy(legacy).is_err() { return StatusCode::BAD_REQUEST.into_response(); } }
    if let Some(outcome) = value["outcome"].as_i64() {
        if !(0..=2).contains(&outcome) { return StatusCode::BAD_REQUEST.into_response(); }
        wifi.outcome = outcome as i32;
    }
    if let Some(fail) = value["storage_failure"].as_bool() { wifi.engine.storage_failure(fail); }
    if let Some(drop) = value["drop_http_ack"].as_bool() { wifi.drop_http_ack = drop; }
    if let Some(window)=value.get("window_ms") {
        let Some(window)=window.as_u64().filter(|w|*w<=120000) else{return StatusCode::BAD_REQUEST.into_response();};
        wifi.engine.constrain(clock.monotonic_ms,clock.monotonic_ms.saturating_add(window),true);
    }
    let peer = wifi.peer.clone(); let outcome = wifi.outcome;
    let busy = wifi.engine.poll(clock.monotonic_ms, &peer, generation, true, outcome).unwrap_or(false);
    match wifi.engine.observe() {
        Ok(radio)=>Json(json!({"busy":busy,"radio":radio})).into_response(),
        Err(_)=>StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
fn wifi_control_until(state:&SimState,power:&SimPower,physical:u64,now:u64)->u64 {
    if power.plugged && !power.deep_on_usb {return u64::MAX;}
    let plan=state.plan.lock().unwrap();let (_,mode,_,seconds,accepted)=plan.status_fields();
    let formal=if mode=="light" {power.boot_ms.saturating_add(accepted).saturating_add(u64::from(seconds)*1000)}else{0};
    let sync=if power.sync_open {power.sync_last_progress_ms.saturating_add(90000)}else{0};
    let until=physical.max(formal).max(sync);if until>now {until}else{0}
}
async fn wifi_http(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    if let Some(response) = device_gate(&state).await { return response; }
    let op = match request.uri().path() { "/api/wifi/snapshot" => "snapshot", "/api/wifi/apply" => "apply", "/api/wifi/result_ack"=>"result_ack", "/api/wifi/command"=>"command", _ => "result" };
    let Some(clock) = clock_snapshot(&state) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    let Ok(bytes) = to_bytes(request.into_body(), 8192).await else { return StatusCode::PAYLOAD_TOO_LARGE.into_response(); };
    let Ok(mut message) = serde_json::from_slice::<Value>(&bytes) else { return StatusCode::BAD_REQUEST.into_response(); };
    if message["device_mac"] != state.mac { return StatusCode::BAD_REQUEST.into_response(); }
    if op!="command" {message["op"] = json!(op);}
    let generation = state.ble.lock().unwrap().generation as u32;
    let mut wifi = state.wifi.lock().unwrap(); let peer = wifi.peer.clone(); let physical = wifi.physical_until;
    let reply = if wifi.unified {
        let power=state.power.lock().unwrap().clone();let principal=wifi.principal.clone();
        let allowed=state.owner.lock().unwrap().get_valid(now_seconds(&state,clock)).ok().flatten().is_some_and(|o|o.id==principal);
        let until=wifi_control_until(&state,&power,physical,clock.monotonic_ms);
        let session_valid=message["session_nonce"]==session_snapshot(&state).nonce;
        wifi.engine.control(&message,&principal,&peer,0,clock.monotonic_ms,until,physical,allowed,session_valid)
    } else {wifi.engine.command(&message, &peer, generation, clock.monotonic_ms, physical, 0, true)};
    if message["op"] == "apply" && wifi.drop_http_ack { wifi.drop_http_ack = false; return StatusCode::GATEWAY_TIMEOUT.into_response(); }
    match reply { Ok(reply) => Json(reply).into_response(), Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response() }
}

fn ble_frame_session(state: &SimState) -> [u8; 16] {
    let generation = state.ble.lock().unwrap().generation;
    let boot = session_snapshot(state).boot_id;
    let digest = Sha256::digest(format!("{}:{boot}:{generation}", state.mac));
    digest[..16].try_into().unwrap()
}

fn ble_frame_reply(state: &SimState, request: Header, op: &str, ack: &Value,
                   original: &[u8], now: u64) -> Result<()> {
    let wire_kind = if op == "status" && ack["result"] == "applied" { "status" } else { "reply" };
    let mut payload = bridge_render::simulator_ble_wire(wire_kind, ack)?;
    if payload.len() > 512 - HEADER_LEN {
        payload = serde_json::to_vec(&json!({"ack":"command","op":op,
            "request_id":ack["request_id"],"result":"unknown","error":"reply_limit",
            "stage":"frame","query":"original_business_id"}))?;
    }
    ensure!(payload.len() <= 512 - HEADER_LEN, "BLE reply limit");
    let result = match ack["result"].as_str() {
        Some("accepted") => 1, Some("rejected") => 2, Some("unknown") => 3,
        _ if ack["ok"] == false => 2, _ => 0,
    };
    let header = Header { kind: 1, op: request.op, result,
        payload_len: payload.len().try_into()?, session: ble_frame_session(state),
        request_no: request.request_no, context: request.context };
    let mut bytes = header.encode().map_err(anyhow::Error::msg)?.to_vec();
    bytes.extend_from_slice(&payload);
    let mut ble = state.ble.lock().unwrap();
    ble.fence.retain(request.request_no, original, &bytes);
    ble.store(op, bytes, now);
    Ok(())
}

async fn sim_ble_command(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    if !ble_generation_ok(&state, &request) { return StatusCode::CONFLICT.into_response(); }
    let Some(clock) = clock_snapshot(&state) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    let Ok(power) = reconcile_power(&state, clock.monotonic_ms) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    if !state.ble.lock().unwrap().connected || (!power.light && power.ble_window_until_ms.is_none()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(bytes) = to_bytes(request.into_body(), 8192).await else { return StatusCode::PAYLOAD_TOO_LARGE.into_response(); };
    let Ok((frame, payload)) = Header::decode(&bytes, 8192) else { return StatusCode::BAD_REQUEST.into_response(); };
    if frame.kind != 0 { return StatusCode::BAD_REQUEST.into_response(); }
    let Ok(message) = serde_json::from_slice::<Value>(payload) else { return StatusCode::BAD_REQUEST.into_response(); };
    let op = message["op"].as_str().unwrap_or("command");
    if bridge_core::compact_frame::operation(op) != Some(frame.op) { return StatusCode::BAD_REQUEST.into_response(); }
    if (op == "status" && frame.session != [0; 16]) ||
        (op != "status" && frame.session != ble_frame_session(&state)) {
        return StatusCode::CONFLICT.into_response();
    }
    if frame.context != [0; 16] {
        let current = state.bundle.lock().unwrap().status().ok()
            .and_then(|v| v["context"].as_str().map(str::to_owned));
        if current.as_deref().map(str::as_bytes) != Some(frame.context.as_slice()) {
            return StatusCode::CONFLICT.into_response();
        }
    }
    let admission = {
        let ble = state.ble.lock().unwrap();
        match ble.fence.inspect(frame.request_no, &bytes) {
            Admission::New => None,
            Admission::Replay(reply) => Some((None, Some(reply.to_vec()))),
            Admission::Retired => Some((Some("retired"), None)),
            Admission::Conflict => Some((Some("request_conflict"), None)),
        }
    };
    if let Some((error, replay)) = admission {
        if let Some(replay) = replay { state.ble.lock().unwrap().store(op, replay, clock.monotonic_ms); }
        else {
            let reject = json!({"ack":"command","op":op,"request_id":message["request_id"],
                "result":"rejected","error":error.unwrap()});
            let mut payload = serde_json::to_vec(&reject).unwrap();
            let header = Header {kind:1,op:frame.op,result:2,payload_len:payload.len() as u16,
                session:ble_frame_session(&state),request_no:frame.request_no,context:frame.context};
            let mut raw = header.encode().unwrap().to_vec(); raw.append(&mut payload);
            state.ble.lock().unwrap().store(op,raw,clock.monotonic_ms);
        }
        return Json(json!({"result":"accepted"})).into_response();
    }
    if op.starts_with("wifi_") {
        if bytes.len() > if state.wifi.lock().unwrap().unified {4096}else{1024} { return StatusCode::PAYLOAD_TOO_LARGE.into_response(); }
        if message["device_mac"] != state.mac { return StatusCode::BAD_REQUEST.into_response(); }
        let generation = state.ble.lock().unwrap().generation as u32;
        if op=="wifi_status" && state.wifi.lock().unwrap().unified {
            let authenticated=Request::builder().uri("/api/status").header(header::AUTHORIZATION,format!("Bearer {}",state.endpoint_token)).body(Body::empty()).unwrap();
            let response=status_impl(state.clone(),authenticated,false).await;
            let Ok(bytes)=to_bytes(response.into_body(),16384).await else{return StatusCode::INTERNAL_SERVER_ERROR.into_response();};
            let Ok(text)=std::str::from_utf8(&bytes) else{return StatusCode::INTERNAL_SERVER_ERROR.into_response();};
            let mut wifi=state.wifi.lock().unwrap();let peer=wifi.peer.clone();
            let Ok(mut reply)=wifi.engine.read_page(&message,&peer,generation,text) else{return StatusCode::INTERNAL_SERVER_ERROR.into_response();};drop(wifi);
            reply["wake_generation"]=json!(session_snapshot(&state).boot_id);reply["wake_seq"]=json!(power.wake_count);
            let _ = ble_frame_reply(&state,frame,op,&reply,&bytes,clock.monotonic_ms);
            return Json(json!({"result":"accepted"})).into_response();
        }
        let now = clock.monotonic_ms;
        let bridge = message["bridge_id"].as_str().unwrap_or("");
        let allowed = state.owner.lock().unwrap().get_valid(now_seconds(&state, clock)).ok().flatten()
            .is_none_or(|owner| owner.id == bridge);
        let reply = {
            let mut wifi = state.wifi.lock().unwrap();
            let peer = wifi.peer.clone(); let outcome = wifi.outcome; let physical = wifi.physical_until;
            let light = if power.light { now + 25000 } else { 0 };
            let _ = wifi.engine.poll(now, &peer, generation, true, outcome);
            if wifi.unified {
                let principal=wifi.principal.clone();let claimed=state.owner.lock().unwrap().get_valid(now_seconds(&state,clock)).ok().flatten().is_some_and(|o|o.id==principal);
                let until=wifi_control_until(&state,&power,physical,now);
                wifi.engine.control(&message,&principal,&peer,generation,now,until,physical,claimed,message["session_nonce"]==session_snapshot(&state).nonce)
            } else {wifi.engine.command(&message, &peer, generation, now, physical, light, allowed)}
        };
        let Ok(mut reply) = reply else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
        reply["ack"]=json!("wifi");reply["op"]=json!("wifi");reply["request_id"]=message["request_id"].clone();
        reply["wake_generation"] = json!(session_snapshot(&state).boot_id);
        reply["wake_seq"] = json!(power.wake_count);
        let _ = ble_frame_reply(&state,frame,op,&reply,&bytes,now);
        return Json(json!({"result":"accepted"})).into_response();
    }
    let request = Request::builder().uri("/sim/ble/business")
        .header(header::AUTHORIZATION, format!("Bearer {}", state.endpoint_token))
        .body(Body::from(payload.to_vec())).unwrap();
    let response = sim_ble_business(State(state.clone()), request).await;
    // A business refusal is a device ACK too, not an HTTP upload failure.
    let business_status = response.status();
    let Ok(business_bytes) = to_bytes(response.into_body(), BODY_LIMIT).await else { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); };
    let mut ack = serde_json::from_slice::<Value>(&business_bytes).unwrap_or_else(|_| json!({
        "op":op,"result":"rejected","error":"invalid_request","stage":"request_validation",
        "http_status":business_status.as_u16()}));
    if op=="plan" && ack["result"]=="applied" {
        let (mode,granted,accepted_at)={
            let plan=state.plan.lock().unwrap();
            let (_,mode,_,granted,accepted_at)=plan.status_fields();
            (mode.to_owned(),granted,accepted_at)
        };
        let legal_until=if mode=="light" {
            accepted_at.saturating_add((granted as u64).saturating_mul(1000))
        } else {clock.monotonic_ms};
        let mut rv=state.rendezvous.lock().unwrap();
        rv.plan_received();
        rv.await_network(clock.monotonic_ms,legal_until);
    }
    ack["request_id"] = message["request_id"].as_str().filter(|id| !id.is_empty())
        .map(|id| json!(id)).unwrap_or_else(|| json!(format!("frame-{:016x}",frame.request_no)));
    ack["wake_generation"] = json!(session_snapshot(&state).boot_id);
    ack["wake_seq"] = json!(state.power.lock().unwrap().wake_count);
    let now = clock_snapshot(&state).unwrap().monotonic_ms;
    if ble_frame_reply(&state,frame,
        if matches!(op,"status"|"plan"|"data"|"sync_config"|"sync_open") {op} else {"command"},
        &ack,&bytes,now).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR,Json(json!({"result":"unknown",
            "error":"ack_encoding","stage":"business_complete"}))).into_response();
    }
    Json(json!({"result":"accepted"})).into_response()
}

async fn sim_ble_fragment(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.endpoint_token) { return unauthorized(); }
    if !ble_generation_ok(&state, &request) { return StatusCode::CONFLICT.into_response(); }
    let Some(clock) = clock_snapshot(&state) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(power) = reconcile_power(&state, clock.monotonic_ms) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if !power.light && power.ble_window_until_ms.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let args: HashMap<String, String> = form_urlencoded::parse(
        request.uri().query().unwrap_or("").as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned())).collect();
    let Some(offset) = args.get("offset").and_then(|value| value.parse::<usize>().ok()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(final_chunk) = args.get("final").map(String::as_str) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !matches!(final_chunk, "0" | "1") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let generation = request.headers().get("x-sim-ble-generation").cloned();
    let body = match to_bytes(request.into_body(), 180).await {
        Ok(body) if !body.is_empty() => body,
        _ => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let completed = {
        let mut rx = state.ble_rx.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if offset != rx.len() || rx.len().saturating_add(body.len()) > 8192 {
            rx.clear();
            return StatusCode::CONFLICT.into_response();
        }
        rx.extend_from_slice(&body);
        // Match the production fixed frame boundary, independent of JSON text.
        if rx.len() >= HEADER_LEN && rx.len() == HEADER_LEN +
            (usize::from(rx[6]) << 8 | usize::from(rx[7])) { Some(std::mem::take(&mut *rx)) }
        else if final_chunk == "1" { rx.clear(); None } else { None }
    };
    let Some(completed) = completed else {
        return Json(json!({"result":if final_chunk == "1" {"accepted"} else {"more"}})).into_response();
    };
    let mut builder = Request::builder().uri("/sim/ble/command")
        .header(header::AUTHORIZATION, format!("Bearer {}", state.endpoint_token));
    if let Some(generation) = generation { builder = builder.header("x-sim-ble-generation", generation); }
    let request = builder.body(Body::from(completed)).unwrap();
    let response = sim_ble_command(State(state), request).await;
    if response.status() != StatusCode::OK { return response; }
    Json(json!({"result":if final_chunk == "1" {"accepted"} else {"more"}})).into_response()
}

async fn claim(State(state): State<SimState>, request: Request<Body>) -> Response {
    if let Some(response) = device_gate(&state).await { return response; }
    if request.headers().get("x-device-token").and_then(|v|v.to_str().ok())
        != Some(state.device_token.as_ref()) {
        let clock_guard = state
            .clock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(clock) = clock_guard.snapshot_at(Instant::now()) else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"clock_unavailable"})),
            )
                .into_response();
        };
        let now_s = now_seconds(&state, clock);
        let owner = match state
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_valid(now_s)
        {
            Ok(owner) => owner,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"owner_storage_failed"})),
                )
                    .into_response()
            }
        };
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error":"unauthorized",
                "owner":owner_json(owner.as_ref(), now_s)
            })),
        )
            .into_response();
    }
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error":"body_too_large"})),
            )
                .into_response()
        }
    };
    let input:Value = match serde_json::from_slice(&body) {
        Ok(input) => input,
        Err(_) => return (StatusCode::BAD_REQUEST,Json(json!({"error":"payload"}))).into_response(),
    };
    let args = json!({"id":input["id"],"name":input["name"],"host":input["host"],
        "port":input["port"],"lease":input["lease"],"has_lease":!input["lease"].is_null(),
        "force":input["force"],"release":input["release"]});
    let empty_current = claim_current(None);
    let prepared = match bridge_render::simulator_claim_decision(&args, false, &empty_current) {
        Ok(decision) => decision,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"claim_decision_failed"})),
            )
                .into_response()
        }
    };
    if prepared["valid_id"] != true {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"args"}))).into_response();
    }

    let clock_guard = state
        .clock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(clock) = clock_guard.snapshot_at(Instant::now()) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"clock_unavailable"})),
        )
            .into_response();
    };
    let now_s = now_seconds(&state, clock);
    let mut owners = state
        .owner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let current = match owners.get_valid(now_s) {
        Ok(owner) => owner,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"owner_storage_failed"})),
            )
                .into_response()
        }
    };
    let current_json = claim_current(current.as_ref());
    let decision =
        match bridge_render::simulator_claim_decision(&args, current.is_some(), &current_json) {
            Ok(decision) => decision,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"claim_decision_failed"})),
                )
                    .into_response()
            }
        };
    match decision["action"].as_str() {
        Some("release_empty") => Json(json!({"owner":null,"released":false})).into_response(),
        Some("occupied") => (
            StatusCode::CONFLICT,
            Json(json!({"error":"occupied","owner":owner_json(current.as_ref(), now_s)})),
        )
            .into_response(),
        Some("release") => match owners.replace(None) {
            Ok(()) => Json(json!({"owner":null,"released":true})).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"owner_storage_failed"})),
            )
                .into_response(),
        },
        Some("claim") => {
            let Some(port) = decision["request_port"]
                .as_u64()
                .and_then(|v| u16::try_from(v).ok())
            else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"claim_decision_failed"})),
                )
                    .into_response();
            };
            let Some(lease) = decision["request_lease"]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
            else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"claim_decision_failed"})),
                )
                    .into_response();
            };
            let Some(id) = decision["request_id"].as_str() else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"claim_decision_failed"})),
                )
                    .into_response();
            };
            let Some(name) = decision["request_name"].as_str() else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"claim_decision_failed"})),
                )
                    .into_response();
            };
            let Some(host) = decision["request_host"].as_str() else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"claim_decision_failed"})),
                )
                    .into_response();
            };
            let keep_since = decision["keep_since"].as_bool().unwrap_or(false);
            let new_claim = decision["new_claim"].as_bool().unwrap_or(true);
            let since = if keep_since {
                current.as_ref().map_or(now_s, |owner| owner.since)
            } else {
                now_s
            };
            let owner = OwnerRec {
                id: id.to_owned(),
                name: name.to_owned(),
                host: host.to_owned(),
                port,
                since,
                last_seen: now_s,
                lease,
            };
            match owners.replace(Some(owner.clone())) {
                Ok(()) => Json(json!({"owner":owner_json(Some(&owner), now_s),"renew":!new_claim}))
                    .into_response(),
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error":"owner_storage_failed"})),
                )
                    .into_response(),
            }
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"claim_decision_failed"})),
        )
            .into_response(),
    }
}

fn framed_http_response(status: StatusCode, header: Header, session: [u8; 16],
                        result: u16, payload: &[u8]) -> Response {
    let mut reply = Header {kind:1,op:header.op,result,payload_len:payload.len() as u16,
        session,request_no:header.request_no,context:header.context}.encode().unwrap().to_vec();
    reply.extend_from_slice(payload);
    (status, [(header::CONTENT_TYPE, bridge_core::compact_frame::CONTENT_TYPE)], reply).into_response()
}

async fn framed_http<F, Fut>(state: SimState, request: Request<Body>, op: u16, handler: F) -> Response
where F: FnOnce(State<SimState>, Request<Body>) -> Fut,
      Fut: std::future::Future<Output = Response>,
{
    let allowed = if op == 19 { bearer(&request, &state.device_token) }
        else { bearer(&request, &state.endpoint_token) };
    if !allowed { return unauthorized(); }
    if request.headers().get(header::CONTENT_TYPE).and_then(|v|v.to_str().ok())
        != Some(bridge_core::compact_frame::CONTENT_TYPE) {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({"error":"content_type","stage":"frame"}))).into_response();
    }
    let token = request.headers().get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
    let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    let (parts, body) = request.into_parts();
    let Ok(raw) = to_bytes(body, 4096).await else { return StatusCode::PAYLOAD_TOO_LARGE.into_response(); };
    let Ok((frame, payload)) = Header::decode(&raw, 4096) else {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"protocol_mismatch","stage":"frame"}))).into_response();
    };
    if frame.kind != 0 || frame.op != op {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"operation","stage":"frame"}))).into_response();
    }
    let mut session = state.http_frame.lock().await;
    if op == 1 || (op == 19 && serde_json::from_slice::<Value>(payload).ok().is_some_and(|v|v["open"]==true)) {
        if frame.session == [0; 16] {
            return framed_http_response(StatusCode::CONFLICT,frame,session.id,2,
                br#"{"result":"rejected","error":"session"}"#);
        }
        if frame.session == session.open_nonce && digest == session.token_digest {
            return match session.fence.inspect(frame.request_no,&raw) {
                Admission::Replay(reply) => (StatusCode::OK,
                    [(header::CONTENT_TYPE,bridge_core::compact_frame::CONTENT_TYPE)],
                    reply.to_vec()).into_response(),
                admission => framed_http_response(StatusCode::CONFLICT,frame,session.id,2,
                    if matches!(admission,Admission::Retired) {
                        br#"{"result":"rejected","error":"retired","query":"original_business_id"}"#
                    } else {
                        br#"{"result":"rejected","error":"request_conflict","query":"original_business_id"}"#
                    }),
            };
        }
        session.opens += 1;
        let boot = session_snapshot(&state).boot_id;
        let seed = Sha256::digest(format!("http:{}:{boot}:{}",state.mac,session.opens));
        session.id.copy_from_slice(&seed[..16]);
        session.open_nonce = frame.session;
        session.token_digest = digest;
        session.fence = RequestFence::default();
    } else if frame.session != session.id || session.id == [0; 16] || digest != session.token_digest {
        return framed_http_response(StatusCode::CONFLICT,frame,session.id,2,
            br#"{"result":"rejected","error":"session"}"#);
    }
    if frame.context != [0; 16] {
        let context = state.bundle.lock().unwrap().status().ok()
            .and_then(|s| s["context"].as_str().map(str::to_owned));
        if context.as_deref().map(str::as_bytes) != Some(frame.context.as_slice()) {
            return framed_http_response(StatusCode::CONFLICT,frame,session.id,2,
                br#"{"result":"rejected","error":"context"}"#);
        }
    }
    match session.fence.inspect(frame.request_no, &raw) {
        Admission::Replay(reply) => {
            return (StatusCode::OK, [(header::CONTENT_TYPE, bridge_core::compact_frame::CONTENT_TYPE)], reply.to_vec()).into_response();
        }
        Admission::Retired | Admission::Conflict => {
            let error = if matches!(session.fence.inspect(frame.request_no, &raw), Admission::Retired) {
                "retired" } else { "request_conflict" };
            let payload = serde_json::to_vec(&json!({"result":"rejected","error":error,
                "query":"original_business_id"})).unwrap();
            return framed_http_response(StatusCode::CONFLICT,frame,session.id,2,&payload);
        }
        Admission::New => {}
    }
    if op==1 {
        if let Some(id)=serde_json::from_slice::<Value>(payload).ok().and_then(|v|v["status_read"].as_str().map(str::to_owned)) {
            let input:Value=serde_json::from_slice(payload).unwrap();
            let offset=input["offset"].as_u64().unwrap_or(u64::MAX);
            let reply=session.status_pages.as_mut().and_then(|pages|
                pages.page(None,&token,&id,offset as usize,device_uptime_ms(&state,clock_snapshot(&state).unwrap())).ok())
                .unwrap_or(json!({"result":"rejected","error":"status_read_expired","stage":"status_read"}));
            let rejected=reply["result"]=="rejected";
            return framed_http_response(if rejected {StatusCode::CONFLICT}else{StatusCode::OK},frame,session.id,if rejected{2}else{0},&serde_json::to_vec(&reply).unwrap());
        }
        session.status_pages = None;
    }
    let request = Request::from_parts(parts, Body::from(payload.to_vec()));
    let response = handler(State(state.clone()),request).await;
    let status = response.status();
    let Ok(bytes) = to_bytes(response.into_body(), BODY_LIMIT).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let result = serde_json::from_slice::<Value>(&bytes).ok().map(|value| {
        match value["result"].as_str() {
            Some("accepted") => 1, Some("rejected") => 2, Some("unknown") => 3,
            _ if value["ok"] == false || !status.is_success() => 2, _ => 0,
        }
    }).unwrap_or(if status.is_success() {0} else {2});
    let page=if op==1 && bytes.len()+HEADER_LEN>4096 {
        let pages=match bridge_render::SimulatorStatusPages::new(){Ok(p)=>p,Err(_)=>return StatusCode::INTERNAL_SERVER_ERROR.into_response()};
        session.status_pages=Some(pages);
        let id=Sha256::digest(&raw)[..16].iter().map(|b|format!("{b:02x}")).collect::<String>();
        let reply=session.status_pages.as_mut().unwrap().page(Some(std::str::from_utf8(&bytes).unwrap()),&token,&id,0,device_uptime_ms(&state,clock_snapshot(&state).unwrap())).unwrap();
        Some(serde_json::to_vec(&reply).unwrap())
    }else{None};
    let payload = if let Some(page)=page.as_ref() {page.clone()} else if bytes.len() + HEADER_LEN <= 4096 { bytes.to_vec() } else {
        br#"{"result":"unknown","error":"reply_limit","stage":"frame","query":"original_business_id"}"#.to_vec()
    };
    let result = if page.is_some(){0}else if bytes.len() + HEADER_LEN > 4096 {3} else {result};
    let mut reply = Header {kind:1,op,result,payload_len:payload.len() as u16,
        session:session.id,request_no:frame.request_no,context:frame.context}.encode().unwrap().to_vec();
    reply.extend_from_slice(&payload);
    session.fence.retain(frame.request_no,&raw,&reply);
    (status, [(header::CONTENT_TYPE, bridge_core::compact_frame::CONTENT_TYPE)], reply).into_response()
}

fn app(state: SimState) -> Router {
    macro_rules! framed { ($op:expr, $handler:ident) => {
        post(|State(state): State<SimState>, request: Request<Body>| async move {
            framed_http(state, request, $op, $handler).await
        })
    }; }
    Router::new()
        .route("/status.json", get(public_status))
        .route("/api/status", framed!(1, status))
        .route("/api/wifi/snapshot", framed!(14, wifi_http))
        .route("/api/wifi/apply", framed!(15, wifi_http))
        .route("/api/wifi/result", framed!(16, wifi_http))
        .route("/api/wifi/result_ack", framed!(17, wifi_http))
        .route("/api/wifi/command", framed!(18, wifi_http))
        .route("/sim/wifi", post(sim_wifi_control))
        .route("/sim/history", post(sim_history_control))
        .route("/api/sync/begin", framed!(6, sync_begin))
        .route("/api/sync/page", framed!(7, sync_page))
        .route("/api/sync/ack", framed!(8, sync_ack))
        .route("/api/sync/complete", framed!(9, sync_complete))
        .route("/sim/state", get(sim_state))
        .route("/sim/frame", get(sim_frame))
        .route("/sim/diagnostics", post(sim_diagnostics))
        .route("/sim/time", get(sim_time_get).post(sim_time_post))
        .route("/sim/wake", post(sim_wake))
        .route("/sim/display", post(sim_display))
        .route("/sim/power", post(sim_power))
        .route("/sim/fault", post(sim_fault))
        .route("/sim/button", post(sim_button))
        .route("/sim/storage", post(sim_storage))
        .route("/sim/versions", get(sim_versions_get).post(sim_versions_post))
        .route("/sim/ble/info", get(sim_ble_info))
        .route("/sim/ble/token", post(sim_ble_aux))
        .route("/sim/ble/endpoint", post(sim_ble_aux))
        .route("/sim/ble/status", get(sim_ble_status))
        .route("/sim/ble/close", post(sim_ble_close))
        .route("/sim/ble/fault", post(sim_ble_fault))
        .route("/sim/ble/attributes", get(sim_ble_attributes))
        .route("/sim/ble/command", post(sim_ble_command))
        .route("/sim/ble/fragment", post(sim_ble_fragment))
        .route("/update", get(ota_token_route))
        .route("/diag", post(ota_token_route))
        .route("/doUpdate", post(ota_upload))
        .route("/api/ota/image", framed!(19, ota_image))
        .route("/api/data", framed!(2, data))
        .route("/api/plan", framed!(3, plan))
        .route("/api/activate", framed!(13, activate))
        .route("/api/bundle/begin", framed!(10, bundle_begin))
        .route("/api/bundle/chunk", framed!(11, bundle_chunk))
        .route("/api/bundle/commit", framed!(12, bundle_commit))
        .route("/claim", framed!(4, claim))
        .layer(DefaultBodyLimit::max(0x30_0000 + 65536))
        .with_state(state)
}

#[tokio::main]
async fn main() -> Result<()> {
    let options = parse_options()?;
    let endpoint_token = required_token("CODEX_STATUS_SIM_ENDPOINT_TOKEN")?;
    let device_token = required_token("CODEX_STATUS_SIM_DEVICE_TOKEN")?;
    let control_token = required_token("CODEX_STATUS_SIM_CONTROL_TOKEN")?;
    ensure!(
        endpoint_token != device_token
            && endpoint_token != control_token
            && device_token != control_token,
        "simulator tokens must be pairwise distinct"
    );
    let owner = OwnerStore::load(&options.data_dir, &options.mac, &options.target)?;
    let instance_lock = fs::OpenOptions::new().read(true).write(true).create(true)
        .open(options.data_dir.join("sim.lock"))
        .context("cannot open simulator instance lock")?;
    instance_lock.try_lock().context("simulator data directory is already in use")?;
    let resumed_clock = options.data_dir.join("sim-clock.json").exists();
    let clock = load_clock(&options.data_dir, options.epoch_ms)?;
    save_clock(&options.data_dir, &clock)?;
    let boot_ms = if resumed_clock { clock.logical_at(Instant::now()) } else { 0 };
    let boot_id = next_boot_id(&options.data_dir)?;
    let plan = bridge_render::SimulatorPlan::new()?;
    let bundle = bridge_render::SimulatorBundle::new(&options.data_dir, &options.target,
        boot_id, &options.wake_cause)?;
    let sync = bridge_render::SimulatorSync::new(&options.data_dir, &options.mac,
        &options.target, options.seed, boot_id, &options.wake_cause)?;
    let ota = OtaStore::load(&options.data_dir, &options.target, options.catalog_path.as_deref())?;
    let mut power = SimPower::new(bundle.status()?["configured"] == true, &options.wake_cause);
    power.boot_ms = boot_ms;
    power.safety_deadline_ms = boot_ms.saturating_add(power.safety_deadline_ms);
    let state = SimState {
        mac: options.mac.clone(),
        target: options.target.clone(),
        seed: options.seed,
        data_dir: options.data_dir.clone(),
        session: Arc::new(Mutex::new(SimSession {
            boot_id,
            nonce: deterministic_nonce(options.seed ^ boot_id, &options.mac),
        })),
        rendezvous: Arc::new(Mutex::new(bridge_render::SimulatorRendezvous::new(boot_ms,4000)?)),
        http_frame: Arc::new(tokio::sync::Mutex::new(SimHttpFrame::default())),
        endpoint_token,
        device_token,
        control_token,
        clock: Arc::new(Mutex::new(clock)),
        owner: Arc::new(Mutex::new(owner)),
        plan: Arc::new(Mutex::new(plan)),
        bundle: Arc::new(Mutex::new(bundle)),
        sync: Arc::new(Mutex::new(sync)),
        ota: Arc::new(Mutex::new(ota)),
        ota_running: Arc::new(AtomicBool::new(false)),
        power: Arc::new(Mutex::new(power)),
        power_reconcile: Arc::new(Mutex::new(())),
        stall_ack_after: Arc::new(Mutex::new(None)),
        status_padding_bytes: Arc::new(Mutex::new(0)),
        ble_rx: Arc::new(Mutex::new(Vec::new())),
        ble: Arc::new(Mutex::new(SimBle::default())),
        wifi: Arc::new(Mutex::new(SimWifi {
            engine: bridge_render::SimulatorWifi::new(&options.data_dir)?, peer: "02:00:00:00:00:02/1".into(),
            physical_until: 0, outcome: 1, drop_http_ack: false,
            unified: std::env::var("CODEX_STATUS_SIM_WIFI_PROTOCOL").as_deref()==Ok("3"),
            principal: std::env::var("CODEX_STATUS_SIM_WIFI_PRINCIPAL").unwrap_or("bridge".into()),status_seq:0,status_key:String::new(),
        })),
        report_ip: match options.listen.ip() { IpAddr::V4(ip) => ip, _ => unreachable!() },
        ready_report: Arc::new(Mutex::new(None)),
    };
    let listener = tokio::net::TcpListener::bind(options.listen)
        .await
        .context("failed to bind simulator listener")?;
    let address = listener.local_addr()?;
    // Fake GATT stays loopback-only even when business HTTP/report traffic is
    // tested on a private interface with production source-address checks.
    let ble_address = if !state.report_ip.is_loopback() {
        let ble_listener=tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST,0)).await?;
        let ble_address=ble_listener.local_addr()?;
        let ble_state=state.clone();
        tokio::spawn(async move {axum::serve(ble_listener,app(ble_state)).await.unwrap();});
        ble_address
    } else {address};
    println!(
        "{}",
        json!({
            "schema_version": 1,
            "mac": state.mac,
            "target": state.target,
            "boot_id": session_snapshot(&state).boot_id,
            "http": format!("http://{address}"),
            "ble_http": format!("http://{ble_address}"),
            "capabilities": CAPABILITIES
        })
    );
    use std::io::Write;
    std::io::stdout().flush()?;
    tokio::spawn(sim_ready_reports(state.clone()));
    axum::serve(listener, app(state))
        .await
        .context("simulator server failed")?;
    Ok(())
}
