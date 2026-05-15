use crate::bbmd_transport::build_bbmd_transport;
use crate::config::BridgeConfig;
use crate::error::BridgeError;
use crate::sc_transport::build_sc_transport;
use bacnet_transport::any::AnyTransport;
use bacnet_transport::mstp::NoSerial;

pub async fn build_remote_transport(
    config: &BridgeConfig,
) -> Result<AnyTransport<NoSerial>, BridgeError> {
    match config.router.transport.as_str() {
        "sc" => {
            let sc = &config.router.sc;
            let cert = sc.client_cert.as_deref();
            let key = sc.client_key.as_deref();
            build_sc_transport(sc, cert, key).await
        }
        "tailscale" => build_bbmd_transport(&config.router.tailscale).await,
        other => Err(BridgeError::Transport(format!(
            "Unknown transport mode: '{other}'. Expected 'sc' or 'tailscale'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BridgeConfig;

    #[tokio::test]
    async fn test_build_sc_transport_from_config() {
        let mut config = BridgeConfig::generate_default();
        config.router.transport = "sc".to_string();
        config.router.sc.hub_url = "wss://localhost:1".to_string();
        let result = build_remote_transport(&config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_build_tailscale_transport_from_config() {
        let mut config = BridgeConfig::generate_default();
        config.router.transport = "tailscale".to_string();
        config.router.tailscale.interface = "127.0.0.1".to_string();
        config.router.tailscale.port = 20000;
        let result = build_remote_transport(&config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_unknown_transport_mode_is_rejected() {
        let mut config = BridgeConfig::generate_default();
        config.router.transport = "invalid".to_string();
        let result = build_remote_transport(&config).await;
        assert!(result.is_err());
        match result {
            Err(BridgeError::Transport(msg)) => {
                assert!(msg.contains("Unknown transport mode"));
            }
            _ => panic!("Expected Transport error with 'Unknown transport mode'"),
        }
    }

    #[tokio::test]
    async fn test_sc_transport_uses_sc_config() {
        let mut config = BridgeConfig::generate_default();
        config.router.transport = "sc".to_string();
        config.router.sc.hub_url = "wss://localhost:1".to_string();
        config.router.sc.reconnect_initial_ms = 500;
        config.router.sc.reconnect_max_ms = 10000;
        config.router.sc.reconnect_max_attempts = 3;
        let result = build_remote_transport(&config).await;
        assert!(result.is_err());
    }
}
