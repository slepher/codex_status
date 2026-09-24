use anyhow::{bail, ensure, Context, Result};
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::{
    collections::HashMap,
    env, fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const BODY_LIMIT: usize = 64 * 1024;
const MAX_RATE_PPM: u64 = 1_000_000_000;
const MAX_STEP_MS: u64 = 86_400_000;
const CAPABILITIES: &[&str] = &["v2_status", "clock_control", "claim", "plan_state"];
const UNSUPPORTED: &[&str] = &[
    "data",
    "bundle",
    "activate",
    "BLE",
    "persistence",
    "display",
    "power_lifecycle",
];

#[derive(Clone)]
struct SimState {
    mac: String,
    endpoint_token: Arc<str>,
    device_token: Arc<str>,
    control_token: Arc<str>,
    nonce: String,
    clock: Arc<Mutex<SimClock>>,
    owner: Arc<Mutex<OwnerStore>>,
    plan: Arc<Mutex<bridge_render::SimulatorPlan>>,
}

#[derive(Debug)]
struct Options {
    listen: SocketAddr,
    mac: String,
    seed: u64,
    epoch_ms: Option<u64>,
    data_dir: PathBuf,
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

    fn apply(&mut self, command: ClockCommand) -> Option<ClockSnapshot> {
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
        *self = next;
        Some(snapshot)
    }
}

impl ClockSnapshot {
    fn json(self) -> serde_json::Value {
        serde_json::json!({
            "monotonic_ms": self.monotonic_ms,
            "uptime_ms": self.monotonic_ms,
            "wall_ms": self.wall_ms,
            "rate_ppm": self.rate_ppm,
            "wall_offset_ms": self.wall_offset_ms
        })
    }
}

fn parse_options() -> Result<Options> {
    let mut listen = "127.0.0.1:0".to_owned();
    let mut mac = None;
    let mut seed = 1u64;
    let mut epoch_ms = None;
    let mut data_dir = None;
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
            "--seed" => seed = value.parse().context("invalid --seed")?,
            "--epoch-ms" => {
                ensure!(epoch_ms.is_none(), "--epoch-ms specified more than once");
                epoch_ms = Some(value.parse().context("invalid --epoch-ms")?);
            }
            "--data-dir" => {
                ensure!(data_dir.is_none(), "--data-dir specified more than once");
                data_dir = Some(PathBuf::from(value));
            }
            _ => bail!("unknown option: {arg}"),
        }
    }
    let listen: SocketAddr = listen.parse().context("invalid --listen address")?;
    ensure!(
        listen.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST),
        "--listen must use 127.0.0.1"
    );
    let mac = normalize_mac(mac.as_deref().context("--mac is required")?)?;
    let data_dir = data_dir.context("--data-dir is required")?;
    Ok(Options {
        listen,
        mac,
        seed,
        epoch_ms,
        data_dir,
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
    fn load(dir: &Path, mac: &str) -> Result<Self> {
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
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let marker = InstanceMarker {
                    schema: 1,
                    mac: mac.to_owned(),
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

fn now_seconds(clock: ClockSnapshot) -> u32 {
    (clock.monotonic_ms / 1000) as u32
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

async fn status(State(state): State<SimState>, request: Request<Body>) -> Response {
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
    let input = json!({
        "mac": state.mac,
        "session_nonce": state.nonce,
        "context": "",
        "job_id": "",
        "active_template_id": "",
        "template_ids": [],
        "configured": false,
        "applied_seq": 0,
        "data_crc": 0,
        "display_state_code": 0,
        "commit_seq": 0,
        "deep_sleep": false,
        "plan_accepted": plan_accepted,
        "plan_mode": plan_mode,
        "plan_id": plan_id,
        "granted_s": granted_s,
        "plan_accepted_at_ms": accepted_at_ms,
        "provisional": false,
        "boot_ms": 0,
        "now_ms": clock.monotonic_ms,
        "battery": 75
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
    let now_s = now_seconds(clock);
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
    Json(json!({
        "mac": state.mac,
        "capabilities": CAPABILITIES,
        "unsupported": UNSUPPORTED,
        "uptime_ms": clock.monotonic_ms,
        "clock": clock.json(),
        "clock_persistence": "unsupported",
        "owner": owner_json(owner.as_ref(), now_s),
        "plan": {"accepted": accepted, "mode": mode, "plan_id": plan_id,
                 "granted_s": granted_s, "accepted_at_ms": accepted_at_ms}
    }))
    .into_response()
}

async fn sim_time_get(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.control_token) {
        return unauthorized();
    }
    if let Err(response) = consume_body(request).await {
        return response;
    }
    match clock_snapshot(&state) {
        Some(clock) => Json(clock.json()).into_response(),
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
    let result = state
        .clock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .apply(command);
    match result {
        Some(clock) => Json(clock.json()).into_response(),
        None => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_clock_command"})),
        )
            .into_response(),
    }
}

async fn unsupported(
    State(state): State<SimState>,
    request: Request<Body>,
    device_token: bool,
) -> Response {
    let token = if device_token {
        &state.device_token
    } else {
        &state.endpoint_token
    };
    if !bearer(&request, token) {
        return unauthorized();
    }
    if let Err(response) = consume_body(request).await {
        return response;
    }
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({"error":"unsupported"})),
    )
        .into_response()
}

async fn endpoint_write(State(state): State<SimState>, request: Request<Body>) -> Response {
    unsupported(State(state), request, false).await
}

async fn plan(State(state): State<SimState>, request: Request<Body>) -> Response {
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
    let now_s = now_seconds(clock);
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
    let session = match bridge_render::simulator_command_check(message, &state.mac, &state.nonce) {
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
        .decide(message, clock.monotonic_ms)
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
        let now_s = now_seconds(clock);
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
    let now_s = now_seconds(clock);
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
        .route("/v2/status", get(status))
        .route("/sim/state", get(sim_state))
        .route("/sim/time", get(sim_time_get).post(sim_time_post))
        .route("/v2/data", post(endpoint_write))
        .route("/v2/plan", post(plan))
        .route("/v2/activate", post(endpoint_write))
        .route("/v2/bundle/begin", post(endpoint_write))
        .route("/v2/bundle/chunk", post(endpoint_write))
        .route("/v2/bundle/commit", post(endpoint_write))
        .route("/claim", post(claim))
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
    let epoch_ms = match options.epoch_ms {
        Some(epoch_ms) => epoch_ms,
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before Unix epoch")?
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    };
    let owner = OwnerStore::load(&options.data_dir, &options.mac)?;
    let plan = bridge_render::SimulatorPlan::new()?;
    let state = SimState {
        mac: options.mac.clone(),
        endpoint_token,
        device_token,
        control_token,
        nonce: deterministic_nonce(options.seed, &options.mac),
        clock: Arc::new(Mutex::new(SimClock {
            logical_ms: 0,
            rate_ppm: 1_000_000,
            anchor: Instant::now(),
            epoch_ms,
            wall_offset_ms: 0,
        })),
        owner: Arc::new(Mutex::new(owner)),
        plan: Arc::new(Mutex::new(plan)),
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
