pub mod api;
pub mod routes;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use tokio::sync::{mpsc, oneshot, watch, Mutex, RwLock};
use tokio::task::JoinHandle;
use tower_http::services::ServeDir;

use bridge_core::{AppState, BridgeConfig, FdtManager, LogRingBuffer};

pub struct WebAppStateInner {
    pub state_rx: Mutex<watch::Receiver<AppState>>,
    pub config: Arc<RwLock<BridgeConfig>>,
    pub config_path: Option<String>,
    pub start_time: Mutex<Option<Instant>>,
    pub command_tx: Option<mpsc::Sender<RouterCommand>>,
    pub fdt: Arc<tokio::sync::Mutex<FdtManager>>,
    pub logbuf: Arc<LogRingBuffer>,
    pub is_embedded_hub: tokio::sync::Mutex<bool>,
    pub cloud_hub_url: tokio::sync::Mutex<Option<String>>,
    pub hub_listen_addr: Option<String>,
    pub hub_spoke_count: tokio::sync::Mutex<u32>,
}

#[derive(Clone)]
pub struct WebAppState {
    pub inner: Arc<WebAppStateInner>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum RouterCommand {
    Stop,
    Start(oneshot::Sender<Result<(), String>>),
    SwitchTransport(String, oneshot::Sender<Result<(), String>>),
    Exit,
}

pub struct WebServerConfig {
    pub host: String,
    pub port: u16,
    pub dev: bool,
    pub state_rx: watch::Receiver<AppState>,
    pub config: Arc<RwLock<BridgeConfig>>,
    pub config_path: Option<PathBuf>,
    pub command_tx: Option<mpsc::Sender<RouterCommand>>,
    pub fdt: Arc<tokio::sync::Mutex<FdtManager>>,
    pub logbuf: Arc<LogRingBuffer>,
    pub is_embedded_hub: bool,
    pub cloud_hub_url: Option<String>,
    pub hub_listen_addr: Option<String>,
}

pub fn run_web_server(cfg: WebServerConfig) -> JoinHandle<()> {
    let shared = WebAppState {
        inner: Arc::new(WebAppStateInner {
            state_rx: Mutex::new(cfg.state_rx),
            config: cfg.config,
            config_path: cfg.config_path.map(|p| p.to_string_lossy().to_string()),
            start_time: Mutex::new(Some(Instant::now())),
            command_tx: cfg.command_tx,
            fdt: cfg.fdt,
            logbuf: cfg.logbuf,
            is_embedded_hub: tokio::sync::Mutex::new(cfg.is_embedded_hub),
            cloud_hub_url: tokio::sync::Mutex::new(cfg.cloud_hub_url),
            hub_listen_addr: cfg.hub_listen_addr,
            hub_spoke_count: tokio::sync::Mutex::new(0),
        }),
    };

    let app = Router::new()
        .route("/api/status", axum::routing::get(api::status))
        .route("/api/router-info", axum::routing::get(api::router_info))
        .route("/api/interfaces", axum::routing::get(api::interfaces))
        .route(
            "/api/config",
            axum::routing::get(api::get_config).put(api::update_config),
        )
        .route(
            "/api/transport/switch",
            axum::routing::post(api::transport_switch),
        )
        .route(
            "/api/transport/stop",
            axum::routing::post(api::transport_stop),
        )
        .route(
            "/api/transport/start",
            axum::routing::post(api::transport_start),
        )
        .route("/api/hub/status", axum::routing::get(api::hub_status))
        .route("/api/hub/mode", axum::routing::post(api::hub_mode_switch))
        .route("/api/fdt", axum::routing::get(api::get_fdt))
        .route("/api/logs", axum::routing::get(api::get_logs))
        .route("/api/log", axum::routing::post(api::post_log))
        .route("/ws/logs", axum::routing::get(api::ws_logs));

    let app = if cfg.dev {
        let assets_path = format!("{}/assets", env!("CARGO_MANIFEST_DIR"));
        Router::new().fallback_service(ServeDir::new(&assets_path))
    } else {
        app.route("/", axum::routing::get(routes::index))
            .route("/style.css", axum::routing::get(routes::style_css))
            .route("/app.js", axum::routing::get(routes::app_js))
            .route("/favicon.ico", axum::routing::get(routes::favicon))
    };

    let app = app.with_state(shared);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .expect("bind web server");
        tracing::info!("Web dashboard listening on http://{}", addr);
        axum::serve(listener, app).await.expect("serve web server");
    })
}
