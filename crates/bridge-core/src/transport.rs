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
        "tailscale" => Err(BridgeError::Transport(
            "Tailscale BBMD transport not yet implemented".into(),
        )),
        other => Err(BridgeError::Transport(format!(
            "Unknown transport mode: '{other}'. Expected 'sc' or 'tailscale'"
        ))),
    }
}
