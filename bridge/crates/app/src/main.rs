#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bridge_ble::{lan_ip, BleConfig, Pusher};
use bridge_core::codex::locate_codex;
use bridge_core::http::{serve, AppState};
use bridge_core::runtime::{run_poller, PollerConfig};
use bridge_core::template::Library;
use bridge_core::short_id;
use serde_json::{json, Value};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, State};
use tokio::sync::{Notify, RwLock};

struct AppConfig {
    port: u16,
    token: String,
    templates_dir: PathBuf,
    interval_secs: u64,
}

struct AppCtx {
    config: AppConfig,
    envelope: Arc<RwLock<Option<Value>>>,
    library: Arc<RwLock<Library>>,
    force_ble: Arc<Notify>,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn host_label() -> String {
    let raw = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "bridge".to_string());
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '?' })
        .take(16)
        .collect();
    if cleaned.is_empty() { "bridge".into() } else { cleaned }
}

#[tauri::command]
async fn get_status(state: State<'_, AppCtx>) -> Result<Value, String> {
    let usage = state.envelope.read().await.clone();
    let library = state.library.read().await;
    let templates: Vec<Value> = library
        .entries
        .values()
        .map(|e| json!({"id": e.id, "hash": e.hash, "version": e.version}))
        .collect();
    Ok(json!({
        "http": format!("http://0.0.0.0:{}", state.config.port),
        "lan_ip": lan_ip(),
        "interval_secs": state.config.interval_secs,
        "usage": usage,
        "templates": templates,
        "updated": usage.as_ref().and_then(|u| u.get("server_time")).and_then(|v| v.as_i64()),
    }))
}

#[tauri::command]
async fn force_sync(state: State<'_, AppCtx>) -> Result<(), String> {
    state.force_ble.notify_one();
    Ok(())
}

#[tauri::command]
async fn reload_templates(state: State<'_, AppCtx>) -> Result<usize, String> {
    let library = Library::load(&state.config.templates_dir).map_err(|e| e.to_string())?;
    let count = library.entries.len();
    *state.library.write().await = library;
    Ok(count)
}

async fn run_services(ctx: Arc<AppCtx>) {
    let exe = match locate_codex(None) {
        Ok(exe) => exe,
        Err(e) => {
            tracing::error!("codex cli not found: {e}");
            return;
        }
    };
    tracing::info!("codex cli: {}", exe.display());

    let addr = format!("0.0.0.0:{}", ctx.config.port).parse().expect("addr");
    let http_state = AppState {
        token: Arc::new(ctx.config.token.clone()),
        envelope: ctx.envelope.clone(),
        library: ctx.library.clone(),
    };
    tokio::spawn(async move {
        if let Err(e) = serve(addr, http_state).await {
            tracing::error!("http server: {e}");
        }
    });

    let label = host_label();
    let poller = PollerConfig {
        exe,
        label: label.clone(),
        host_id: short_id(&label),
        interval_secs: ctx.config.interval_secs,
        templates: ctx.library.clone(),
    };
    tokio::spawn(run_poller(poller, ctx.envelope.clone()));

    let ble_cfg = BleConfig {
        name_prefix: "CodexStatus-".to_string(),
        host: lan_ip(),
        port: ctx.config.port,
        token: ctx.config.token.clone(),
        template_ids: Vec::new(),
    };
    let pusher = Pusher::new(
        ble_cfg,
        ctx.library.clone(),
        format!("http://127.0.0.1:{}", ctx.config.port),
    );
    loop {
        let adapter = match Pusher::adapter().await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("bluetooth adapter: {e}");
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(ctx.config.interval_secs)) => {}
            _ = ctx.force_ble.notified() => {}
        }
        match pusher.cycle_once(&adapter).await {
            Ok(()) => tracing::info!("ble cycle done"),
            Err(e) => tracing::warn!("ble cycle: {e}"),
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = AppConfig {
        port: env_or("CODEX_STATUS_PORT", "8765").parse().unwrap_or(8765),
        token: env_or("CODEX_STATUS_TOKEN", "test-token-123"),
        templates_dir: PathBuf::from(env_or(
            "CODEX_STATUS_TEMPLATES",
            "tools/test-bridge/templates",
        )),
        interval_secs: env_or("CODEX_STATUS_INTERVAL", "300").parse().unwrap_or(300),
    };
    let library = Library::load(&config.templates_dir).unwrap_or_default();
    tracing::info!(
        "templates: {:?} from {}",
        library.ids(),
        config.templates_dir.display()
    );
    let ctx = Arc::new(AppCtx {
        config,
        envelope: Arc::new(RwLock::new(None)),
        library: Arc::new(RwLock::new(library)),
        force_ble: Arc::new(Notify::new()),
    });

    tauri::Builder::default()
        .setup(move |app| {
            app.manage(ctx.clone());
            tauri::async_runtime::spawn(run_services(ctx.clone()));

            let show = MenuItem::with_id(app, "show", "Show window", true, None::<&str>)?;
            let sync = MenuItem::with_id(app, "sync", "Force BLE sync", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &sync, &quit])?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?;
            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Codex Status Bridge")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "sync" => {
                        if let Some(state) = app.try_state::<AppCtx>() {
                            state.force_ble.notify_one();
                        }
                    }
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_status, force_sync, reload_templates])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
