//! LAN HTTP service for the device: `GET /usage` and `GET /template?id=&hash=`
//! (Bearer token; template hash match returns 304).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::template::Library;

#[derive(Clone)]
pub struct AppState {
    pub token: Arc<String>,
    pub envelope: Arc<RwLock<Option<Value>>>,
    pub library: Arc<RwLock<Library>>,
}

impl AppState {
    pub fn new(token: String, library: Library) -> Self {
        Self {
            token: Arc::new(token),
            envelope: Arc::new(RwLock::new(None)),
            library: Arc::new(RwLock::new(library)),
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

async fn usage(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let guard = state.envelope.read().await;
    match guard.as_ref() {
        Some(value) => Json(value.clone()).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "no data yet").into_response(),
    }
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
        .route("/template", get(template))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("http listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
