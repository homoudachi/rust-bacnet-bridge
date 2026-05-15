use std::path::PathBuf;
use std::sync::Arc;

use bridge_core::{AppState, BridgeConfig, FdtManager, LogRingBuffer};
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tracing;

use crate::web;

pub async fn run_serve(
    host: &str,
    port: u16,
    config_path: Option<String>,
    dev: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(BridgeConfig::default_config_path);

    let config = if path.exists() {
        BridgeConfig::load(&path)?
    } else {
        let cfg = BridgeConfig::generate_default();
        cfg.save(&path)?;
        cfg
    };

    let config = Arc::new(RwLock::new(config));
    let (_state_tx, state_rx) = watch::channel(AppState::Stopped);
    let (_cmd_tx, _cmd_rx): (
        mpsc::Sender<web::RouterCommand>,
        mpsc::Receiver<web::RouterCommand>,
    ) = mpsc::channel(32);

    tracing::info!(
        "Starting web server in {} mode on {}:{}",
        if dev { "development" } else { "production" },
        host,
        port,
    );
    tracing::info!("Config path: {}", path.display());

    let fdt = Arc::new(Mutex::new(FdtManager::new()));
    let logbuf = Arc::new(LogRingBuffer::new(1000));

    let _web_handle = web::run_web_server(
        host,
        port,
        dev,
        state_rx,
        config,
        Some(path),
        Some(_cmd_tx),
        fdt,
        logbuf,
    );

    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutting down web server");
    _web_handle.abort();
    Ok(())
}
