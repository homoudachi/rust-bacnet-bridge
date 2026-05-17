use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bacnet_transport::bbmd::BbmdState;
use bacnet_transport::sc_frame::Vmac;
use bacnet_transport::sc_hub::ScHub;
use bridge_core::{
    start_router, AppState, BridgeConfig, FdtManager, LogRingBuffer, RunningRouter, StateManager,
};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_rustls::TlsAcceptor;

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

    let config_path_str = path.to_string_lossy().to_string();

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

        let tls_config = hub_cmd::build_self_signed_tls(&[])
            .map_err(|e| format!("Failed to build self-signed TLS for embedded hub: {e}"))?;
        let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let hub_bind = hub_config.bind.clone();

        let hub_vmac: Vmac = rand::random();

        tracing::info!("Starting embedded Hub on {}", hub_bind);

        let mut hub = ScHub::start(&hub_bind, tls_acceptor, hub_vmac)
            .await
            .map_err(|e| format!("Embedded hub start failed: {e}"))?;

        hub_listen_addr = Some(
            hub.local_addr()
                .map(|a| a.to_string())
                .unwrap_or_else(|| hub_bind.clone()),
        );
        tracing::info!(
            "Embedded Hub listening on {}",
            hub_listen_addr.as_ref().unwrap()
        );

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
    let mut running: Option<RunningRouter> = match start_router(&cfg_guard).await {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!(
                "Initial router start failed: {}. Dashboard available for configuration.",
                e
            );
            state.try_transition(AppState::Stopped).ok();
            None
        }
    };
    drop(cfg_guard);
    if running.is_some() {
        state.try_transition(AppState::Running)?;
        tracing::info!("Router running");
    }

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<web::RouterCommand>(32);
    let state_rx = state.subscribe();

    let fdt = Arc::new(Mutex::new(FdtManager::new()));
    let logbuf = Arc::new(LogRingBuffer::new(1000));

    let bbmd_handle: Arc<RwLock<Option<Arc<Mutex<BbmdState>>>>> = Arc::new(RwLock::new(
        running.as_ref().and_then(|r| r.bbmd_state.clone()),
    ));

    {
        let sync_fdt = fdt.clone();
        let sync_bbmd = bbmd_handle.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            loop {
                interval.tick().await;
                if let Some(bbmd) = sync_bbmd.read().await.as_ref() {
                    let mut state = bbmd.lock().await;
                    let entries: Vec<([u8; 4], u16, u16)> =
                        state.fdt().iter().map(|e| (e.ip, e.port, e.ttl)).collect();
                    drop(state);
                    let mut fdt_mgr = sync_fdt.lock().await;
                    for (ip, port, ttl) in entries {
                        fdt_mgr.add(ip, port, ttl);
                    }
                }
                sync_fdt.lock().await.tick();
            }
        });
    }

    let web_host = {
        let cfg = config.read().await;
        cfg.web.host.clone()
    };
    let web_port = {
        let cfg = config.read().await;
        cfg.web.port
    };

    #[cfg(feature = "windows-tray")]
    let tray_url = format!("http://{}:{}", web_host, web_port);

    let _web_handle = web::run_web_server(web::WebServerConfig {
        host: web_host,
        port: web_port,
        dev: false,
        state_rx,
        config: config.clone(),
        config_path: Some(path),
        command_tx: Some(cmd_tx.clone()),
        fdt: fdt.clone(),
        logbuf: logbuf.clone(),
        is_embedded_hub,
        cloud_hub_url,
        hub_listen_addr,
    });

    #[cfg(feature = "windows-tray")]
    let (tray_shutdown_tx, tray_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    #[cfg(feature = "windows-tray")]
    {
        let tray_state_rx = state.subscribe();
        let tray_cmd_tx = cmd_tx.clone();
        std::thread::spawn(move || {
            crate::tray::run_tray(tray_state_rx, tray_cmd_tx, tray_url, tray_shutdown_rx);
        });
    }

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
                            tracing::info!("Stopping router via command");
                            state.try_transition(AppState::Stopping)?;
                            if let Some(r) = running.take() {
                                r.stop().await;
                            }
                            state.try_transition(AppState::Stopped)?;
                            tracing::info!("Router stopped via command");
                        }
                    }
                    web::RouterCommand::Start => {
                        if state.current() != AppState::Stopped {
                            tracing::warn!(
                                "Cannot start router: current state is {:?}",
                                state.current()
                            );
                            continue;
                        }
                        tracing::info!("Starting router via command");
                        if state.try_transition(AppState::Starting).is_err() {
                            tracing::error!("State transition to Starting failed");
                            continue;
                        }
                        let cfg_guard = config.read().await;
                        match start_router(&cfg_guard).await {
                            Ok(r) => {
                                drop(cfg_guard);
                                *bbmd_handle.write().await = r.bbmd_state.clone();
                                running = Some(r);
                                state.try_transition(AppState::Running).ok();
                                tracing::info!("Router started via command");
                            }
                            Err(e) => {
                                drop(cfg_guard);
                                state.try_transition(AppState::Stopped).ok();
                                tracing::error!("Failed to start router via command: {e}");
                            }
                        }
                    }
                    web::RouterCommand::SwitchTransport(mode) => {
                        if mode != "sc" && mode != "tailscale" {
                            tracing::warn!("Invalid transport mode: {}", mode);
                            continue;
                        }
                        tracing::info!("Transport switch requested: {}", mode);

                        if let Some(r) = running.take() {
                            tracing::info!("Stopping router for transport switch");
                            state.try_transition(AppState::Stopping).ok();
                            r.stop().await;
                            state.try_transition(AppState::Stopped).ok();
                            tracing::info!("Router stopped");
                        }

                        {
                            let mut cfg = config.write().await;
                            cfg.router.transport = mode.clone();
                            cfg.save(Path::new(&config_path_str)).ok();
                            tracing::info!("Transport config saved: {}", mode);
                        }

                        state.try_transition(AppState::Starting).ok();
                        let cfg_guard = config.read().await;
                        match start_router(&cfg_guard).await {
                            Ok(r) => {
                                drop(cfg_guard);
                                *bbmd_handle.write().await = r.bbmd_state.clone();
                                running = Some(r);
                                state.try_transition(AppState::Running).ok();
                                tracing::info!("Router started with transport: {}", mode);
                            }
                            Err(e) => {
                                drop(cfg_guard);
                                state.try_transition(AppState::Stopped).ok();
                                tracing::error!(
                                    "Failed to start router with transport {}: {e}",
                                    mode
                                );
                            }
                        }
                    }
                    web::RouterCommand::Exit => {
                        tracing::info!("Exit requested from tray");
                        state.try_transition(AppState::Stopping)?;
                        if let Some(r) = running.take() {
                            r.stop().await;
                        }
                        state.try_transition(AppState::Stopped)?;
                        #[cfg(feature = "windows-tray")]
                        drop(tray_shutdown_tx);
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
