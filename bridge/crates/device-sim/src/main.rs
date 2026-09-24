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
    sync::Arc,
    time::Instant,
};

const BODY_LIMIT: usize = 64 * 1024;
const CAPABILITIES: &[&str] = &["v2_status"];
const UNSUPPORTED: &[&str] = &[
    "data",
    "plan",
    "bundle",
    "activate",
    "claim",
    "BLE",
    "persistence",
    "display",
    "clock-control",
];

#[derive(Clone)]
struct SimState {
    mac: String,
    endpoint_token: Arc<str>,
    device_token: Arc<str>,
    control_token: Arc<str>,
    nonce: String,
    started: Instant,
}

#[derive(Debug)]
struct Options {
    listen: SocketAddr,
    mac: String,
    seed: u64,
}

fn parse_options() -> Result<Options> {
    let mut listen = "127.0.0.1:0".to_owned();
    let mut mac = None;
    let mut seed = 1u64;
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
            _ => bail!("unknown option: {arg}"),
        }
    }
    let listen: SocketAddr = listen.parse().context("invalid --listen address")?;
    ensure!(
        listen.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST),
        "--listen must use 127.0.0.1"
    );
    let mac = normalize_mac(mac.as_deref().context("--mac is required")?)?;
    Ok(Options { listen, mac, seed })
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
    let now_ms = state.started.elapsed().as_millis().min(u64::MAX as u128) as u64;
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
        "now_ms": now_ms,
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
    Json(json!({
        "mac": state.mac,
        "capabilities": CAPABILITIES,
        "unsupported": UNSUPPORTED,
        "uptime_ms": state.started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }))
    .into_response()
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
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"unauthorized","owner":null})))
            .into_response();
    }
    if let Err(response) = consume_body(request).await {
        return response;
    }
    (StatusCode::NOT_IMPLEMENTED, Json(json!({"error":"unsupported"}))).into_response()
}

fn app(state: SimState) -> Router {
    Router::new()
        .route("/v2/status", get(status))
        .route("/sim/state", get(sim_state))
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
    let state = SimState {
        mac: options.mac.clone(),
        endpoint_token,
        device_token,
        control_token,
        nonce: deterministic_nonce(options.seed, &options.mac),
        started: Instant::now(),
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
