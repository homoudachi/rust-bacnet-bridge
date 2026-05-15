use std::path::PathBuf;
use std::sync::Arc;

use bridge_core::{start_router, AppState, BridgeConfig, StateManager};
use tokio::sync::{mpsc, RwLock};
use tracing;

use crate::web;

pub async fn run_router(
    config_path: Option<String>,
    transport_override: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(BridgeConfig::default_config_path);

    let mut config = BridgeConfig::load(&path)?;

    if let Some(transport) = transport_override {
        if transport == "sc" || transport == "tailscale" {
            tracing::info!("Transport override: {}", transport);
            config.router.transport = transport;
        } else {
            tracing::warn!(
                "Unknown transport override '{}', using config value",
                transport
            );
        }
    }

    let config = Arc::new(RwLock::new(config));
    let state = StateManager::new();
    state.try_transition(AppState::Starting)?;

    {
        let cfg = config.read().await;
        tracing::info!(
            "Starting BACnet Bridge router (device_id={}, transport={}, lan_port={})",
            cfg.router.device_id,
            cfg.router.transport,
            cfg.router.lan.port,
        );
    }

    let cfg_guard = config.read().await;
    let mut running = Some(start_router(&cfg_guard).await?);
    drop(cfg_guard);
    state.try_transition(AppState::Running)?;
    tracing::info!("Router running");

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<web::RouterCommand>(32);
    let state_rx = state.subscribe();

    let web_host = {
        let cfg = config.read().await;
        cfg.web.host.clone()
    };
    let web_port = {
        let cfg = config.read().await;
        cfg.web.port
    };

    let _web_handle = web::run_web_server(
        &web_host,
        web_port,
        false,
        state_rx,
        config.clone(),
        Some(path),
        Some(cmd_tx),
    );

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutdown signal received");
                state.try_transition(AppState::Stopping)?;
                if let Some(r) = running.take() {
                    r.stop().await;
                }
                state.try_transition(AppState::Stopped)?;
                tracing::info!("Router stopped");
                break;
            }
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    web::RouterCommand::Stop => {
                        if state.current() == AppState::Running {
                            tracing::info!("Stopping router via web API");
                            state.try_transition(AppState::Stopping)?;
                            if let Some(r) = running.take() {
                                r.stop().await;
                            }
                            state.try_transition(AppState::Stopped)?;
                            tracing::info!("Router stopped via web API");
                        }
                    }
                    web::RouterCommand::Start => {
                        tracing::warn!("Router restart via web API not yet supported");
                    }

                }
            }
        }
    }

    Ok(())
}
