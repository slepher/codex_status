//! Local liveness listener. Device business traffic uses the authenticated
//! device-side /api/* endpoints.

use std::net::SocketAddr;

use anyhow::Result;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;

fn router() -> Router {
    Router::new().route("/health", get(|| async { StatusCode::NO_CONTENT }))
}

pub async fn serve(addr: SocketAddr) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("bridge health listening on http://{addr}");
    axum::serve(listener, router()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn listener_has_no_retired_device_routes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router()).await;
        });
        for (path, expected) in [
            ("/health", "204 No Content"),
            ("/usage", "404 Not Found"),
            ("/template", "404 Not Found"),
            ("/deep", "404 Not Found"),
        ] {
            let mut socket = tokio::net::TcpStream::connect(addr).await.unwrap();
            socket.write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").as_bytes())
                .await.unwrap();
            let mut response = Vec::new();
            tokio::time::timeout(std::time::Duration::from_secs(2), socket.read_to_end(&mut response))
                .await.unwrap().unwrap();
            let status = String::from_utf8(response).unwrap();
            assert!(status.starts_with(&format!("HTTP/1.1 {expected}")), "{path}: {status}");
        }
        task.abort();
    }
}
