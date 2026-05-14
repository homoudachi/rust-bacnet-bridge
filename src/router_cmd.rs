use std::path::PathBuf;

use bridge_core::{start_router, AppState, BridgeConfig, StateManager};
use tracing;

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
            tracing::warn!("Unknown transport override '{}', using config value", transport);
        }
    }

    let state = StateManager::new();
    state.try_transition(AppState::Starting)?;

    tracing::info!(
        "Starting BACnet Bridge router (device_id={}, transport={}, lan_port={})",
        config.router.device_id,
        config.router.transport,
        config.router.lan.port,
    );

    let running = start_router(&config).await?;
    state.try_transition(AppState::Running)?;
    tracing::info!("Router running");

    tracing::info!("Router running (Ctrl-C to stop)");

    tokio::signal::ctrl_c().await?;

    tracing::info!("Shutting down...");
    state.try_transition(AppState::Stopping)?;
    running.stop().await;
    state.try_transition(AppState::Stopped)?;
    tracing::info!("Router stopped");

    Ok(())
}
