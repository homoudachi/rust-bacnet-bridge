use std::time::Duration;

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use bridge_core::{AppState, BridgeConfig, FdtDisplayEntry};

use crate::web::{RouterCommand, WebAppState};

#[derive(Serialize)]
pub struct StatusResponse {
    pub state: String,
    pub transport: String,
    pub uptime_secs: u64,
    pub connected_url: String,
    pub lan_ip: String,
    pub lan_port: u16,
    pub device_id: u32,
    pub hub_mode: String,
}

pub async fn status(State(state): State<WebAppState>) -> impl IntoResponse {
    let current_state = {
        let rx = state.inner.state_rx.lock().await;
        let val = *rx.borrow();
        val
    };

    let cfg = state.inner.config.read().await;

    let uptime = state
        .inner
        .start_time
        .lock()
        .await
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0);

    let hub_mode = if *state.inner.is_embedded_hub.lock().await {
        "embedded"
    } else {
        "cloud"
    };

    Json(StatusResponse {
        state: current_state.to_string(),
        transport: cfg.router.transport.clone(),
        uptime_secs: uptime,
        connected_url: cfg.router.sc.hub_url.clone(),
        lan_ip: cfg.router.lan.interface.clone(),
        lan_port: cfg.router.lan.port,
        device_id: cfg.router.device_id,
        hub_mode: hub_mode.to_string(),
    })
}

#[derive(Serialize)]
pub struct RouterInfoNetwork {
    pub network: u8,
    #[serde(rename = "type")]
    pub net_type: String,
    pub ip: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hub_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
}

#[derive(Serialize)]
pub struct RouterInfoResponse {
    pub device_id: u32,
    pub vendor_id: u16,
    pub device_name: String,
    pub networks: Vec<RouterInfoNetwork>,
}

pub async fn router_info(State(state): State<WebAppState>) -> impl IntoResponse {
    let cfg = state.inner.config.read().await;

    let mut networks = Vec::new();

    networks.push(RouterInfoNetwork {
        network: 1,
        net_type: "LAN".to_string(),
        ip: cfg.router.lan.interface.clone(),
        port: cfg.router.lan.port,
        hub_url: None,
        adapter: if cfg.router.lan.interface.is_empty() {
            None
        } else {
            Some("lan".to_string())
        },
    });

    if cfg.router.transport == "sc" {
        networks.push(RouterInfoNetwork {
            network: 2,
            net_type: "BACnet/SC".to_string(),
            ip: cfg.router.sc.hub_url.clone(),
            port: 0,
            hub_url: Some(cfg.router.sc.hub_url.clone()),
            adapter: None,
        });
    } else {
        networks.push(RouterInfoNetwork {
            network: 2,
            net_type: "Tailscale".to_string(),
            ip: cfg.router.tailscale.interface.clone(),
            port: cfg.router.tailscale.port,
            hub_url: None,
            adapter: if cfg.router.tailscale.interface.is_empty() {
                None
            } else {
                Some("tailscale".to_string())
            },
        });
    }

    Json(RouterInfoResponse {
        device_id: cfg.router.device_id,
        vendor_id: cfg.router.vendor_id,
        device_name: cfg.router.device_name.clone(),
        networks,
    })
}

pub async fn interfaces(State(state): State<WebAppState>) -> impl IntoResponse {
    let cfg = state.inner.config.read().await;
    let mut interfaces = Vec::new();

    if !cfg.router.lan.interface.is_empty() {
        let ip = &cfg.router.lan.interface;
        interfaces.push(json!({
            "name": "lan",
            "ip": ip,
            "is_tailscale": ip.starts_with("100.")
        }));
    }

    if !cfg.router.tailscale.interface.is_empty() {
        let ip = &cfg.router.tailscale.interface;
        interfaces.push(json!({
            "name": "tailscale",
            "ip": ip,
            "is_tailscale": ip.starts_with("100.")
        }));
    }

    Json(json!({ "interfaces": interfaces }))
}

pub async fn get_config(State(state): State<WebAppState>) -> impl IntoResponse {
    let cfg = state.inner.config.read().await;
    Json(cfg.clone())
}

#[derive(Deserialize)]
pub struct SwitchRequest {
    pub mode: String,
}

pub async fn transport_switch(
    State(state): State<WebAppState>,
    Json(body): Json<SwitchRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if body.mode != "sc" && body.mode != "tailscale" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid mode. Must be 'sc' or 'tailscale'." })),
        ));
    }

    if state.inner.command_tx.is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "Router control not available in this mode" })),
        ));
    }

    let current_state = {
        let rx = state.inner.state_rx.lock().await;
        let val = *rx.borrow();
        val
    };

    if current_state != AppState::Running {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Cannot switch transport. Router must be Running."
            })),
        ));
    }

    let tx = state.inner.command_tx.as_ref().unwrap();
    let _ = tx
        .send(RouterCommand::SwitchTransport(body.mode.clone()))
        .await;
    Ok(Json(json!({ "status": "ok", "transport": body.mode })))
}

pub async fn transport_stop(
    State(state): State<WebAppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if state.inner.command_tx.is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "Router control not available in this mode" })),
        ));
    }

    let current_state = {
        let rx = state.inner.state_rx.lock().await;
        let val = *rx.borrow();
        val
    };

    if current_state != AppState::Running {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Cannot stop router unless it is in Running state"
            })),
        ));
    }

    let tx = state.inner.command_tx.as_ref().unwrap();
    let _ = tx.send(RouterCommand::Stop).await;
    Ok(Json(json!({ "status": "ok" })))
}

pub async fn transport_start(
    State(state): State<WebAppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let current_state = {
        let rx = state.inner.state_rx.lock().await;
        let val = *rx.borrow();
        val
    };

    if current_state != AppState::Stopped {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Cannot start router unless it is in Stopped state"
            })),
        ));
    }

    match &state.inner.command_tx {
        Some(tx) => {
            let _ = tx.send(RouterCommand::Start).await;
            Ok(Json(json!({ "status": "ok" })))
        }
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "Router control not available in this mode" })),
        )),
    }
}

pub async fn update_config(
    State(state): State<WebAppState>,
    Json(body): Json<BridgeConfig>,
) -> impl IntoResponse {
    {
        let mut cfg = state.inner.config.write().await;
        *cfg = body;
        if let Some(path) = &state.inner.config_path {
            if let Err(e) = cfg.save(std::path::Path::new(path)) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Failed to save config: {}", e) })),
                );
            }
        }
    }

    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

pub async fn get_fdt(State(state): State<WebAppState>) -> impl IntoResponse {
    let transport = {
        let cfg = state.inner.config.read().await;
        cfg.router.transport.clone()
    };
    if transport != "tailscale" {
        return Json(Vec::<FdtDisplayEntry>::new());
    }
    let entries = state.inner.fdt.lock().await.list();
    Json(entries)
}

#[derive(Deserialize)]
pub struct LogQuery {
    pub limit: Option<usize>,
    pub level: Option<String>,
}

pub async fn get_logs(
    State(state): State<WebAppState>,
    Query(params): Query<LogQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(500);
    let level = params.level.as_deref();
    let entries = state.inner.logbuf.recent(limit, level);
    Json(entries)
}

pub async fn hub_status(State(state): State<WebAppState>) -> impl IntoResponse {
    let listen_addr = state.inner.hub_listen_addr.clone();
    let spoke_count = *state.inner.hub_spoke_count.lock().await;
    let mode = if *state.inner.is_embedded_hub.lock().await {
        "embedded"
    } else {
        "cloud"
    };

    Json(json!({
        "mode": mode,
        "listen_addr": listen_addr,
        "spoke_count": spoke_count,
    }))
}

#[derive(Deserialize)]
pub struct HubModeRequest {
    pub mode: String,
}

pub async fn hub_mode_switch(
    State(state): State<WebAppState>,
    Json(body): Json<HubModeRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if body.mode != "cloud" && body.mode != "embedded" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid mode. Must be 'cloud' or 'embedded'." })),
        ));
    }

    let current_state = {
        let rx = state.inner.state_rx.lock().await;
        let val = *rx.borrow();
        val
    };

    if current_state == AppState::Running {
        return Err((
            StatusCode::CONFLICT,
            Json(
                json!({ "error": "Cannot switch hub mode while router is running. Stop the router first." }),
            ),
        ));
    }

    {
        let mut cfg = state.inner.config.write().await;
        if body.mode == "embedded" {
            let cloud_url = cfg.router.sc.hub_url.clone();
            cfg.router.sc.hub_url = "wss://localhost:8443".to_string();
            if let Some(ref path) = state.inner.config_path {
                let _ = cfg.save(std::path::Path::new(path));
            }
            *state.inner.cloud_hub_url.lock().await = Some(cloud_url);
            *state.inner.is_embedded_hub.lock().await = true;
        } else {
            if let Some(ref cloud_url) = *state.inner.cloud_hub_url.lock().await {
                cfg.router.sc.hub_url = cloud_url.clone();
            }
            if let Some(ref path) = state.inner.config_path {
                let _ = cfg.save(std::path::Path::new(path));
            }
            *state.inner.is_embedded_hub.lock().await = false;
        }
    }

    Ok(Json(json!({ "status": "ok", "mode": body.mode })))
}

pub async fn ws_logs(State(state): State<WebAppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    let logbuf = state.inner.logbuf.clone();
    ws.on_upgrade(move |mut ws: WebSocket| async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let entries = logbuf.recent(50, None);
                    if let Ok(text) = serde_json::to_string(&entries) {
                        if ws.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                }
                msg = ws.recv() => {
                    let _ = msg;
                    break;
                }
            }
        }
    })
}
