//! Locate the real `codex-cli` binary and speak the app-server JSON-RPC protocol.
//!
//! Spawn spec mirrors the reference implementation (token-monitor):
//! `codex -s read-only -a never app-server`, line-delimited JSON-RPC on stdio.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};

pub const CLI_MARKER: &str = "codex-cli";
pub const RPC_TIMEOUT: Duration = Duration::from_secs(20);

pub fn candidate_paths(explicit: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(p) = explicit {
        out.push(p.to_path_buf());
    }
    if let Ok(p) = std::env::var("CODEX_STATUS_CODEX") {
        out.push(PathBuf::from(p));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(&local)
            .join("Codex Minibar")
            .join("desktop-cli");
        if let Ok(rd) = std::fs::read_dir(&base) {
            let mut dirs: Vec<PathBuf> = rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with("OpenAI.Codex_"))
                        .unwrap_or(false)
                })
                .collect();
            dirs.sort();
            for d in dirs.into_iter().rev() {
                let exe = d.join("codex.exe");
                if exe.exists() {
                    out.push(exe);
                }
            }
        }
        out.push(
            PathBuf::from(&local)
                .join("Packages")
                .join("OpenAI.Codex_2p2nqsd0c76g0")
                .join("LocalCache")
                .join("Local")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
                .join("codex.exe"),
        );
    }
    out.push(PathBuf::from("codex.exe"));
    out.push(PathBuf::from("codex"));
    out
}

fn version_matches(exe: &Path) -> bool {
    let output = std::process::Command::new(exe).arg("--version").output();
    match output {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            text.contains(CLI_MARKER)
        }
        Err(_) => false,
    }
}

/// Pick the first candidate whose `--version` reports `codex-cli`.
pub fn locate_codex(explicit: Option<&Path>) -> Result<PathBuf> {
    for cand in candidate_paths(explicit) {
        if cand.is_absolute() && !cand.exists() {
            continue;
        }
        if version_matches(&cand) {
            return Ok(cand);
        }
    }
    bail!("codex CLI not found (need `--version` to contain `{CLI_MARKER}`)")
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// JSON-RPC client over a spawned `codex app-server` process.
pub struct CodexClient {
    child: Child,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    next_id: AtomicU64,
    pending: Pending,
    timeout: Duration,
}

impl CodexClient {
    pub async fn spawn(exe: &Path) -> Result<Self> {
        let mut child = Command::new(exe)
            .args(["-s", "read-only", "-a", "never", "app-server"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawn {}", exe.display()))?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr"))?;
        let stdin = Arc::new(Mutex::new(stdin));
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));

        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "codex.stderr", "{line}");
            }
        });

        let pending_reader = pending.clone();
        let stdin_reader = stdin.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        let msg: Value = match serde_json::from_str(line) {
                            Ok(v) => v,
                            Err(_) => {
                                tracing::debug!(target: "codex.rpc", "non-json: {line}");
                                continue;
                            }
                        };
                        let Some(id) = msg.get("id").and_then(|v| v.as_u64()) else {
                            if let Some(m) = msg.get("method").and_then(|v| v.as_str()) {
                                tracing::debug!(target: "codex.notify", "{m}");
                            }
                            continue;
                        };
                        if msg.get("method").is_some() {
                            // Server -> client request; answer with null to keep it unstuck.
                            let reply = json!({"id": id, "result": null});
                            let mut w = stdin_reader.lock().await;
                            let _ = w.write_all(format!("{reply}\n").as_bytes()).await;
                            let _ = w.flush().await;
                            continue;
                        }
                        if let Some(tx) = pending_reader.lock().await.remove(&id) {
                            let payload = match msg.get("error") {
                                Some(err) => Err(err.to_string()),
                                None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                            };
                            let _ = tx.send(payload);
                        }
                    }
                    Ok(None) => {
                        tracing::warn!("codex app-server stdout closed");
                        let mut p = pending_reader.lock().await;
                        for (_, tx) in p.drain() {
                            let _ = tx.send(Err("app-server closed".into()));
                        }
                        break;
                    }
                    Err(e) => {
                        tracing::warn!("codex stdout error: {e}");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            next_id: AtomicU64::new(1),
            pending,
            timeout: RPC_TIMEOUT,
        })
    }

    async fn write_line(&self, msg: &Value) -> Result<()> {
        let mut w = self.stdin.lock().await;
        w.write_all(format!("{msg}\n").as_bytes())
            .await
            .context("write rpc")?;
        w.flush().await.context("flush rpc")?;
        Ok(())
    }

    pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let mut msg = json!({"method": method, "id": id});
        if let Some(p) = params {
            msg["params"] = p;
        }
        self.write_line(&msg).await?;
        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => bail!("rpc {method} error: {e}"),
            Ok(Err(_)) => bail!("rpc {method} channel closed"),
            Err(_) => bail!("rpc {method} timed out"),
        }
    }

    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        let mut msg = json!({"method": method});
        if let Some(p) = params {
            msg["params"] = p;
        }
        self.write_line(&msg).await
    }

    pub async fn initialize(&self) -> Result<Value> {
        let result = self
            .call(
                "initialize",
                Some(json!({
                    "clientInfo": {
                        "name": "codex-status",
                        "title": "Codex Status",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                })),
            )
            .await?;
        self.notify("initialized", Some(json!({}))).await?;
        Ok(result)
    }

    pub async fn read_rate_limits(&self) -> Result<Value> {
        self.call("account/rateLimits/read", None).await
    }

    pub async fn read_account(&self) -> Result<Value> {
        self.call("account/read", Some(json!({}))).await
    }

    pub fn kill(&mut self) {
        let _ = self.child.start_kill();
    }
}
