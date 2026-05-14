use std::sync::Arc;

use bacnet_transport::any::AnyTransport;
use bacnet_transport::mstp::NoSerial;
use bacnet_transport::sc::ScTransport;
use bacnet_transport::sc_frame::Vmac;
use bacnet_transport::sc_tls::TlsWebSocket;
use rand::RngExt;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls;
use tracing;

use crate::config::ScConfig;
use crate::error::BridgeError;

pub fn build_client_tls_config(
    client_cert: Option<&str>,
    client_key: Option<&str>,
) -> Result<Arc<rustls::ClientConfig>, BridgeError> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let builder = rustls::ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13]);
    let builder = builder.with_root_certificates(root_store);

    let config = match (client_cert, client_key) {
        (Some(cert_path), Some(key_path)) => {
            let certs = load_certs(cert_path)?;
            let key = load_private_key(key_path)?;
            builder
                .with_client_auth_cert(certs, key)
                .map_err(|e| BridgeError::Transport(format!("TLS client auth config: {e}")))?
        }
        _ => builder.with_no_client_auth(),
    };

    Ok(Arc::new(config))
}

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, BridgeError> {
    let cert_bytes = std::fs::read(path)?;
    rustls_pemfile::certs(&mut &cert_bytes[..])
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| BridgeError::Transport(format!("Failed to load certs from {path}: {e}")))
}

fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>, BridgeError> {
    let key_bytes = std::fs::read(path)?;
    let key = rustls_pemfile::private_key(&mut &key_bytes[..])
        .map_err(|e| BridgeError::Transport(format!("Failed to load key from {path}: {e}")))?
        .ok_or_else(|| BridgeError::Transport(format!("No private key found in {path}")))?;
    Ok(key)
}

pub async fn build_sc_transport(
    sc_config: &ScConfig,
    client_cert: Option<&str>,
    client_key: Option<&str>,
) -> Result<AnyTransport<NoSerial>, BridgeError> {
    let tls_config = build_client_tls_config(client_cert, client_key)?;

    let hub_url = if sc_config.hub_url.is_empty() {
        return Err(BridgeError::Transport(
            "SC hub_url is empty, cannot connect".into(),
        ));
    } else {
        &sc_config.hub_url
    };

    tracing::info!("Connecting BACnet/SC spoke to {}", hub_url);

    let ws = TlsWebSocket::connect(hub_url, tls_config)
        .await
        .map_err(|e| BridgeError::Transport(format!("SC WebSocket connect to {hub_url}: {e}")))?;

    let mut rng = rand::rng();
    let vmac_bytes: [u8; 6] = rng.random();
    let local_vmac: Vmac = vmac_bytes;

    let uuid: [u8; 16] = rng.random();

    let reconnect_config = bacnet_transport::sc::ScReconnectConfig {
        initial_delay_ms: sc_config.reconnect_initial_ms,
        max_delay_ms: sc_config.reconnect_max_ms,
        max_retries: sc_config.reconnect_max_attempts,
    };

    let sc = ScTransport::new(ws, local_vmac)
        .with_device_uuid(uuid)
        .with_reconnect(reconnect_config);

    Ok(AnyTransport::Sc(Box::new(sc)))
}
