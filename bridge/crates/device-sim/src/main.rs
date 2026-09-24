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
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const BODY_LIMIT: usize = 64 * 1024;
const MAX_RATE_PPM: u64 = 1_000_000_000;
const MAX_STEP_MS: u64 = 86_400_000;
const CAPABILITIES: &[&str] = &["v2_status", "clock_control"];
const UNSUPPORTED: &[&str] = &[
    "data",
    "plan",
    "bundle",
    "activate",
    "claim",
    "BLE",
    "persistence",
    "display",
];

#[derive(Clone)]
struct SimState {
    mac: String,
    endpoint_token: Arc<str>,
    device_token: Arc<str>,
    control_token: Arc<str>,
    nonce: String,
    clock: Arc<Mutex<SimClock>>,
}

#[derive(Debug)]
struct Options {
    listen: SocketAddr,
    mac: String,
    seed: u64,
    epoch_ms: Option<u64>,
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
            _ => bail!("unknown option: {arg}"),
        }
    }
    let listen: SocketAddr = listen.parse().context("invalid --listen address")?;
    ensure!(
        listen.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST),
        "--listen must use 127.0.0.1"
    );
    let mac = normalize_mac(mac.as_deref().context("--mac is required")?)?;
    Ok(Options {
        listen,
        mac,
        seed,
        epoch_ms,
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
        "plan_accepted": false,
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
    let Some(clock) = clock_snapshot(&state) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"clock_unavailable"})),
        )
            .into_response();
    };
    Json(json!({
        "mac": state.mac,
        "capabilities": CAPABILITIES,
        "unsupported": UNSUPPORTED,
        "uptime_ms": clock.monotonic_ms,
        "clock": clock.json(),
        "clock_persistence": "unsupported"
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

async fn claim(State(state): State<SimState>, request: Request<Body>) -> Response {
    if !bearer(&request, &state.device_token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"unauthorized","owner":null})),
        )
            .into_response();
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

fn app(state: SimState) -> Router {
    Router::new()
        .route("/v2/status", get(status))
        .route("/sim/state", get(sim_state))
        .route("/sim/time", get(sim_time_get).post(sim_time_post))
        .route("/v2/data", post(endpoint_write))
        .route("/v2/plan", post(endpoint_write))
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
