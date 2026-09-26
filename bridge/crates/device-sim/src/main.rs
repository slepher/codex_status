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
use serde_json::json;
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
const CAPABILITIES: &[&str] = &["v2_status", "clock_control", "claim", "plan_state",
    "bundle_transfer", "bundle_persistence", "data_render", "activate",
    "power_sleep_http", "button_cycle", "ota_catalog"];
const UNSUPPORTED: &[&str] = &[
    "BLE",
    "physical_display",
    "power_lifecycle",
];

#[derive(Clone)]
struct SimState {
    mac: String,
    target: String,
    seed: u64,
    data_dir: PathBuf,
    session: Arc<Mutex<SimSession>>,
    endpoint_token: Arc<str>,
    device_token: Arc<str>,
    control_token: Arc<str>,
    clock: Arc<Mutex<SimClock>>,
    owner: Arc<Mutex<OwnerStore>>,
    plan: Arc<Mutex<bridge_render::SimulatorPlan>>,
    bundle: Arc<Mutex<bridge_render::SimulatorBundle>>,
    ota: Arc<Mutex<OtaStore>>,
    ota_running: Arc<AtomicBool>,
    power: Arc<Mutex<SimPower>>,
    power_reconcile: Arc<Mutex<()>>,
    stall_ack_after: Arc<Mutex<Option<StallFault>>>,
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
    wake_count: u64,
    last_sleep_reason: String,
}

impl SimPower {
    fn new(configured: bool, wake_cause: &str) -> Self {
        Self {
            configured, light: true, plugged: false, deep_on_usb: false,
            manual_ble_hold: false, battery_pct: 75, boot_ms: 0,
            provisional: wake_cause == "button",
            safety_deadline_ms: if configured { 600_000 } else { 0 },
            next_contact_ms: None, wake_count: 0,
            last_sleep_reason: String::new(),
        }
    }

    fn enter_deep(&mut self, at_ms: u64, reason: &str) {
        self.light = false;
        self.provisional = false;
        self.next_contact_ms = Some(at_ms.saturating_add(60_000));
        self.last_sleep_reason = reason.to_owned();
    }

    fn json(&self, now_ms: u64) -> serde_json::Value {
        json!({"mode":if self.light {"light"} else {"deep"},
            "plugged":self.plugged,"deep_on_usb":self.deep_on_usb,
            "manual_ble_hold":self.manual_ble_hold,"battery_pct":self.battery_pct,
            "boot_ms":self.boot_ms,"provisional":self.provisional,
            "safety_deadline_ms":self.safety_deadline_ms,
            "next_contact_ms":self.next_contact_ms,
            "wake_count":self.wake_count,
            "last_sleep_reason":self.last_sleep_reason,
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
        listen.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST),
        "--listen must use 127.0.0.1"
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
    let suffix = if target == "codex-status-154g" { "bw" } else { "note4-b" };
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
            ensure!(!version.id.is_empty() && !version.fw.is_empty() && version.target == target,
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
                pending: None, source: "initial".into(), event_seq: 0,
            },
            Err(error) => return Err(error).context("cannot read simulator OTA state"),
        };
        let mut store = Self { dir: dir.to_owned(), catalog, file };
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
        self.file.pending = Some(version.id.clone());
        self.file.event_seq += 1;
        self.persist()?;
        Ok(Some(version.id.clone()))
    }

    fn commit_pending(&mut self) -> Result<bool> {
        let Some(pending) = self.file.pending.take() else { return Ok(false); };
        self.file.active = pending;
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
    let session = rotate_session(state)?;
    let bundle = bridge_render::SimulatorBundle::new(&state.data_dir, &state.target,
        session.boot_id, cause)?;
    *state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = bundle;
    *state.plan.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
        bridge_render::SimulatorPlan::new()?;
    Ok(session)
}

fn reset_power_after_reboot(state: &SimState, now_ms: u64, cause: &str) -> Result<()> {
    reboot_device(state, cause)?;
    let configured = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .status()?["configured"] == true;
    let mut power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    power.configured = configured;
    power.light = true;
    power.boot_ms = now_ms;
    power.provisional = cause == "button";
    power.safety_deadline_ms = now_ms.saturating_add(600_000);
    power.next_contact_ms = None;
    power.wake_count += 1;
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
    let (reason, plan_deadline) = {
        let plan = state.plan.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (_, mode, _, granted_s, accepted_at_ms) = plan.status_fields();
        (plan.sleep_reason(configured, before.light, before.plugged,
            before.deep_on_usb, before.manual_ble_hold,
            before.provisional, 0,
            before.safety_deadline_ms.saturating_sub(before.boot_ms), local_now),
         if mode == "light" {before.boot_ms.saturating_add(accepted_at_ms)
             .saturating_add(u64::from(granted_s) * 1000)}
         else {now_ms})
    };
    let mut power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
            1 => Some((plan_deadline, "v2 plan")),
            2 => Some((power.boot_ms.saturating_add(300_000), "v2 provisional")),
            3 => Some((power.safety_deadline_ms, "v2 safety")),
            _ => None,
        };
        if let Some((at, why)) = transition { power.enter_deep(at, why); }
    }
    for _ in 0..10_000 {
        let Some(first) = power.next_contact_ms.filter(|first| !power.light && now_ms >= *first)
            else { return Ok(power.clone()); };
        drop(power);
        // A timer wake runs the BLE rendezvous. Without a fake BLE peer
        // there is no accepted light plan, so HTTP stays unavailable.
        let result = reset_power_after_reboot(state, first, "deep");
        power = state.power.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        result?;
        power.light = false;
        power.next_contact_ms = first.checked_add(60_000);
        power.last_sleep_reason = "rendezvous timeout".to_owned();
    }
    anyhow::bail!("too many timer wakes in one clock step")
}

async fn device_gate(state: &SimState) -> Option<Response> {
    let Some(clock) = clock_snapshot(state) else {
        return Some(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    };
    match reconcile_power(state, clock.monotonic_ms) {
        Ok(power) if power.light => None,
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

fn claim_message(query: Option<&str>) -> serde_json::Value {
    let mut params = HashMap::new();
    if let Some(query) = query {
        for (key, value) in form_urlencoded::parse(query.as_bytes()) {
            params
                .entry(key.into_owned())
                .or_insert_with(|| value.into_owned());
        }
    }
    let flag = |key: &str| params.get(key).is_some_and(|value| value != "0");
    json!({
        "id": params.get("id").map(String::as_str).unwrap_or(""),
        "name": params.get("name").map(String::as_str).unwrap_or(""),
        "host": params.get("host").map(String::as_str).unwrap_or(""),
        "port": params.get("port").map(String::as_str).unwrap_or(""),
        "lease": params.get("lease").map(String::as_str).unwrap_or(""),
        "has_lease": params.contains_key("lease"),
        "force": flag("force"),
        "release": flag("release")
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
        "v2_bundle": bundle["configured"],
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
    if let Some(response) = device_gate(&state).await { return response; }
    if !bearer(&request, &state.endpoint_token) {
        return unauthorized();
    }
    if let Err(response) = consume_body(request).await {
        return response;
    }
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
        Ok(status) => Json(status).into_response(),
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
    match state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .frame_bits() {
        Ok(bits) if !bits.is_empty() =>
            ([(header::CONTENT_TYPE, "application/octet-stream")], bits).into_response(),
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
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
    let _ = fs::remove_file(state.data_dir.join("sim-upload.tmp"));
    let (size, sha256) = match received {
        Ok(received) => received,
        Err(error) => {
            eprintln!("sim OTA upload rejected: {error:#}");
            return (StatusCode::OK, "UPDATE FAILED").into_response();
        }
    };
    let accepted = state.ota.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        .queue_upload(size, &sha256);
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
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "UPDATE FAILED").into_response(),
    }
}

async fn device_command(State(state): State<SimState>, request: Request<Body>, operation: &str) -> Response {
    if let Some(response) = device_gate(&state).await { return response; }
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
        if owner.is_some() {
            if let Some(current) = owners.owner.as_mut() { current.last_seen = now_seconds(&state, clock); }
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
    let result = {
        let mut bundle = state.bundle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
    if let Some(response) = device_gate(&state).await { return response; }
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
            if let Some(current) = owners.owner.as_mut() {
                current.last_seen = now_s;
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
        Ok(ack) => Json(ack).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"plan_ack_failed"})),
        )
            .into_response(),
    }
}

async fn claim(State(state): State<SimState>, request: Request<Body>) -> Response {
    if let Some(response) = device_gate(&state).await { return response; }
    if !bearer(&request, &state.device_token) {
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
    let query = request.uri().query().map(str::to_owned);
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
    let _ = body;
    let args = claim_message(query.as_deref());
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

fn app(state: SimState) -> Router {
    Router::new()
        .route("/status.json", get(public_status))
        .route("/v2/status", get(status))
        .route("/sim/state", get(sim_state))
        .route("/sim/frame", get(sim_frame))
        .route("/sim/time", get(sim_time_get).post(sim_time_post))
        .route("/sim/wake", post(sim_wake))
        .route("/sim/display", post(sim_display))
        .route("/sim/power", post(sim_power))
        .route("/sim/fault", post(sim_fault))
        .route("/sim/button", post(sim_button))
        .route("/sim/storage", post(sim_storage))
        .route("/sim/versions", get(sim_versions_get).post(sim_versions_post))
        .route("/update", get(ota_token_route))
        .route("/diag", post(ota_token_route))
        .route("/doUpdate", post(ota_upload))
        .route("/v2/data", post(data))
        .route("/v2/plan", post(plan))
        .route("/v2/activate", post(activate))
        .route("/v2/bundle/begin", post(bundle_begin))
        .route("/v2/bundle/chunk", post(bundle_chunk))
        .route("/v2/bundle/commit", post(bundle_commit))
        .route("/claim", post(claim))
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
        endpoint_token,
        device_token,
        control_token,
        clock: Arc::new(Mutex::new(clock)),
        owner: Arc::new(Mutex::new(owner)),
        plan: Arc::new(Mutex::new(plan)),
        bundle: Arc::new(Mutex::new(bundle)),
        ota: Arc::new(Mutex::new(ota)),
        ota_running: Arc::new(AtomicBool::new(false)),
        power: Arc::new(Mutex::new(power)),
        power_reconcile: Arc::new(Mutex::new(())),
        stall_ack_after: Arc::new(Mutex::new(None)),
    };
    let listener = tokio::net::TcpListener::bind(options.listen)
        .await
        .context("failed to bind simulator listener")?;
    let address = listener.local_addr()?;
    println!(
        "{}",
        json!({
            "schema_version": 1,
            "mac": state.mac,
            "target": state.target,
            "boot_id": session_snapshot(&state).boot_id,
            "http": format!("http://{address}"),
            "capabilities": CAPABILITIES
        })
    );
    use std::io::Write;
    std::io::stdout().flush()?;
    axum::serve(listener, app(state))
        .await
        .context("simulator server failed")?;
    Ok(())
}
