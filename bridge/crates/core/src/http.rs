//! LAN HTTP service for the device: `GET /usage` (envelope; a deep pull with
//! `next_contact_s`/`mode`/`usage_rev` query parameters gets the v0.14
//! mode/next_contact_s/pending decision added), `POST /deep`, and
//! `GET /template?id=&hash=` (Bearer token; template hash match returns 304).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::activity::Activity;
use crate::template::Library;
use crate::{local_offset_minutes, now_secs};

#[derive(Clone)]
pub struct AppState {
    pub token: Arc<String>,
    pub envelope: Arc<RwLock<Option<Value>>>,
    pub library: Arc<RwLock<Library>>,
    pub activity: Arc<Activity>,
}

impl AppState {
    pub fn new(token: String, library: Library) -> Self {
        Self {
            token: Arc::new(token),
            envelope: Arc::new(RwLock::new(None)),
            library: Arc::new(RwLock::new(library)),
            activity: Arc::new(Activity::new()),
        }
    }
}

fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == format!("Bearer {}", state.token))
        .unwrap_or(false)
}

/// True when the query marks a device pull (v0.14) rather than a legacy read.
fn is_pull(query: &HashMap<String, String>) -> bool {
    query.contains_key("next_contact_s")
        || query.contains_key("mode")
        || query.contains_key("usage_rev")
}

/// Stamp a pull response at generation time. The cached envelope's
/// `server_time` is refreshed by the poller and can lag tens of seconds; the
/// device slews its clock to it on every contact, so the response must carry
/// the current time. `tz_offset_min` (local minutes east of UTC) lets the
/// device follow the PC timezone; older firmware ignores the extra field.
fn stamp_pull_response(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("server_time".into(), json!(now_secs()));
        obj.insert("tz_offset_min".into(), json!(local_offset_minutes()));
    }
}

async fn usage(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let guard = state.envelope.read().await;
    let Some(mut value) = guard.clone() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "no data yet").into_response();
    };
    if is_pull(&query) {
        let device_next = query
            .get("next_contact_s")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let device_rev = query
            .get("usage_rev")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let extra = state.activity.note_pull(device_next, device_rev);
        tracing::info!(
            "device pull: requested_next={} rev={} -> mode={} next={} pending_tpl={} pending_ota={}",
            device_next,
            device_rev,
            extra["mode"].as_str().unwrap_or("?"),
            extra["next_contact_s"].as_u64().unwrap_or(0),
            extra["pending"]["templates"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0),
            extra["pending"]["ota"].as_bool().unwrap_or(false),
        );
        if let (Some(obj), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
            for (key, val) in extra {
                obj.insert(key.clone(), val.clone());
            }
        }
        stamp_pull_response(&mut value);
    } else {
        state.activity.note_contact("http-get");
    }
    Json(value).into_response()
}

/// `POST /deep`: the device tells the bridge it is going to sleep (the bridge
/// treats the silence as expected until the next pull/announce/claim).
async fn deep(State(state): State<AppState>, headers: HeaderMap, body: String) -> Response {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let doc: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let next = doc
        .get("next_contact_s")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    state.activity.note_deep(next);
    tracing::info!("device entered deep sleep (next_contact_s={next})");
    Json(serde_json::json!({
        "ok": true,
        "mode": "deep",
        "next_contact_s": state.activity.next_contact_s(),
    }))
    .into_response()
}

async fn template(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let id = query.get("id").cloned().unwrap_or_default();
    let want_hash = query.get("hash").cloned().unwrap_or_default();
    let library = state.library.read().await;
    let Some(entry) = library.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !want_hash.is_empty() && want_hash == entry.hash {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    (
        [(header::CONTENT_TYPE, "application/json")],
        entry.bytes.clone(),
    )
        .into_response()
}

pub async fn serve(addr: SocketAddr, state: AppState) -> Result<()> {
    let app = Router::new()
        .route("/usage", get(usage))
        .route("/deep", post(deep))
        .route("/template", get(template))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("http listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_stamp_refreshes_time_and_adds_tz() {
        let mut value = json!({"server_time": 1, "buckets": []});
        stamp_pull_response(&mut value);
        let stamped = value["server_time"].as_u64().unwrap();
        assert!(stamped > 1_600_000_000, "server_time must be a fresh epoch");
        assert!(stamped <= now_secs());
        let tz = value["tz_offset_min"].as_i64().unwrap();
        assert!((-840..=840).contains(&tz), "utc offset out of range: {tz}");
        assert_eq!(value["buckets"], json!([]));
    }

    #[test]
    fn pull_detection_accepts_v014_queries() {
        let mut query = HashMap::new();
        query.insert("mode".to_string(), "deep".to_string());
        assert!(is_pull(&query));
        query.clear();
        query.insert("id".to_string(), "quad".to_string());
        assert!(!is_pull(&query));
    }
}
