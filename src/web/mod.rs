pub mod api;
pub mod routes;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tokio::task::JoinHandle;
use tower_http::services::ServeDir;

use bridge_core::{AppState, BridgeConfig};

pub struct WebAppStateInner {
    pub state_rx: Mutex<watch::Receiver<AppState>>,
    pub config: Arc<RwLock<BridgeConfig>>,
    pub config_path: Option<String>,
    pub start_time: Mutex<Option<Instant>>,
    pub command_tx: Option<mpsc::Sender<RouterCommand>>,
}

#[derive(Clone)]
pub struct WebAppState {
    pub inner: Arc<WebAppStateInner>,
}

#[derive(Debug, Clone)]
pub enum RouterCommand {
    Stop,
    Start,
}

pub fn run_web_server(
    host: &str,
    port: u16,
    dev: bool,
    state_rx: watch::Receiver<AppState>,
    config: Arc<RwLock<BridgeConfig>>,
    config_path: Option<PathBuf>,
    command_tx: Option<mpsc::Sender<RouterCommand>>,
) -> JoinHandle<()> {
    let shared = WebAppState {
        inner: Arc::new(WebAppStateInner {
            state_rx: Mutex::new(state_rx),
            config,
            config_path: config_path.map(|p| p.to_string_lossy().to_string()),
            start_time: Mutex::new(Some(Instant::now())),
            command_tx,
        }),
    };

    let app = Router::new()
        .route("/api/status", axum::routing::get(api::status))
        .route("/api/interfaces", axum::routing::get(api::interfaces))
        .route(
            "/api/config",
            axum::routing::get(api::get_config).put(api::update_config),
        )
        .route("/api/transport/switch", axum::routing::post(api::transport_switch))
        .route("/api/transport/stop", axum::routing::post(api::transport_stop))
        .route("/api/transport/start", axum::routing::post(api::transport_start));

    let app = if dev {
        let assets_path = format!("{}/assets", env!("CARGO_MANIFEST_DIR"));
        app.nest_service("/", ServeDir::new(&assets_path))
    } else {
        app.route("/", axum::routing::get(routes::index))
            .route("/style.css", axum::routing::get(routes::style_css))
            .route("/app.js", axum::routing::get(routes::app_js))
    };

    let app = app.with_state(shared);

    let addr = format!("{}:{}", host, port);
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .expect("bind web server");
        tracing::info!("Web dashboard listening on http://{}", addr);
        axum::serve(listener, app).await.expect("serve web server");
    })
}
