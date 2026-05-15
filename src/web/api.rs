use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use bridge_core::{AppState, BridgeConfig};

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

    Json(StatusResponse {
        state: current_state.to_string(),
        transport: cfg.router.transport.clone(),
        uptime_secs: uptime,
        connected_url: cfg.router.sc.hub_url.clone(),
        lan_ip: cfg.router.lan.interface.clone(),
        lan_port: cfg.router.lan.port,
        device_id: cfg.router.device_id,
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

    if current_state != AppState::Stopped {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Cannot switch transport while router is running. Stop the router first."
            })),
        ));
    }

    {
        let mut cfg = state.inner.config.write().await;
        cfg.router.transport = body.mode.clone();
        if let Some(path) = &state.inner.config_path {
            let _ = cfg.save(std::path::Path::new(path));
        }
    }

    Ok(Json(json!({ "status": "ok", "transport": body.mode })))
}

pub async fn transport_stop(
    State(state): State<WebAppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match &state.inner.command_tx {
        Some(tx) => {
            let _ = tx.send(RouterCommand::Stop).await;
            Ok(Json(json!({ "status": "ok" })))
        }
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "Router control not available in this mode" })),
        )),
    }
}

pub async fn transport_start(
    State(state): State<WebAppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
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
