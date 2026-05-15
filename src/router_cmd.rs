use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bacnet_transport::sc_frame::Vmac;
use bacnet_transport::sc_hub::ScHub;
use bridge_core::{start_router, AppState, BridgeConfig, FdtManager, LogRingBuffer, StateManager};
use rand::Rng;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_rustls::TlsAcceptor;
use tracing;

use crate::hub_cmd;
use crate::web;

pub async fn run_router(
    config_path: Option<String>,
    transport_override: Option<String>,
    with_hub: bool,
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

    let is_embedded_hub = with_hub;
    let mut cloud_hub_url: Option<String> = None;
    let mut hub_listen_addr: Option<String> = None;

    if with_hub {
        let hub_config = {
            let cfg = config.read().await;
            cfg.hub.clone()
        };

        let tls_config = hub_cmd::build_self_signed_tls()
            .map_err(|e| format!("Failed to build self-signed TLS for embedded hub: {e}"))?;
        let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let hub_bind = hub_config.bind.clone();

        let hub_vmac: Vmac = rand::thread_rng().gen();

        tracing::info!("Starting embedded Hub on {}", hub_bind);

        let mut hub = ScHub::start(&hub_bind, tls_acceptor, hub_vmac)
            .await
            .map_err(|e| format!("Embedded hub start failed: {e}"))?;

        hub_listen_addr = Some(
            hub.local_addr()
                .map(|a| a.to_string())
                .unwrap_or_else(|| hub_bind.clone()),
        );
        tracing::info!("Embedded Hub listening on {}", hub_listen_addr.as_ref().unwrap());

        {
            let mut cfg = config.write().await;
            cloud_hub_url = Some(cfg.router.sc.hub_url.clone());
            cfg.router.sc.hub_url = "wss://localhost:8443".to_string();
        }

        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("Shutdown signal received, stopping embedded hub");
            hub.stop().await;
            tracing::info!("Embedded hub stopped");
        });
    }

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

    let fdt = Arc::new(Mutex::new(FdtManager::new()));
    let logbuf = Arc::new(LogRingBuffer::new(1000));

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
        fdt.clone(),
        logbuf.clone(),
        is_embedded_hub,
        cloud_hub_url,
        hub_listen_addr,
    );

    let ticker_fdt = fdt.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            ticker_fdt.lock().await.tick();
        }
    });

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
