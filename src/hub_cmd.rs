use std::fs;
use std::io::BufReader;
use std::sync::Arc;

use bridge_core::BridgeError;
use bridge_core::HubConfig;
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;
use tokio::signal;
use tokio_rustls::TlsAcceptor;
use tracing::info;

use bacnet_transport::sc_frame::Vmac;
use bacnet_transport::sc_hub::ScHub;

pub async fn run_hub(config: &HubConfig) -> Result<(), BridgeError> {
    let hub_vmac: Vmac = rand::random();
    let tls_config = build_tls_config(config).await?;
    let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));
    let bind_addr = config.bind.clone();

    let tls_mode = match config.tls_strategy() {
        "static" => "static TLS",
        "acme" => "ACME TLS",
        _ => "self-signed TLS",
    };
    info!("Starting BACnet/SC Hub on {bind_addr} ({tls_mode})");

    let mut hub = ScHub::start(&bind_addr, tls_acceptor, hub_vmac)
        .await
        .map_err(|e| BridgeError::Hub(format!("hub start failed: {e}")))?;

    let local_addr = hub
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|| bind_addr.clone());
    info!("Hub listening on {local_addr}");

    signal::ctrl_c()
        .await
        .map_err(|e| BridgeError::Hub(format!("ctrl-c handler failed: {e}")))?;
    info!("Shutdown signal received, stopping hub");
    hub.stop().await;
    info!("Hub stopped");
    Ok(())
}

async fn build_tls_config(config: &HubConfig) -> Result<ServerConfig, BridgeError> {
    match config.tls_strategy() {
        "static" => {
            let cert = config.cert.as_ref().expect("cert path");
            let key = config.key.as_ref().expect("key path");
            build_static_tls(cert, key)
        }
        "acme" => {
            #[cfg(feature = "acme")]
            {
                build_acme_tls(
                    &config.acme_domain,
                    &config.acme_cache,
                    config.acme_production,
                )
                .await
            }
            #[cfg(not(feature = "acme"))]
            {
                let _ = (
                    &config.acme_domain,
                    &config.acme_cache,
                    config.acme_production,
                );
                Err(BridgeError::Hub(
                    "ACME support not compiled in (enable 'acme' feature)".into(),
                ))
            }
        }
        _ => build_self_signed_tls(&[]),
    }
}

pub(crate) fn build_self_signed_tls(extra_sans: &[&str]) -> Result<ServerConfig, BridgeError> {
    let mut sans: Vec<String> = vec!["localhost".into()];
    for s in extra_sans {
        let s = s.trim();
        if !s.is_empty() && *s != *"localhost" {
            sans.push(s.to_string());
        }
    }
    if let Ok(hostname) = std::env::var("HOSTNAME") {
        let h = hostname.trim().to_string();
        if !h.is_empty() && h != "localhost" && !sans.contains(&h) {
            sans.push(h);
        }
    }
    let cert = generate_simple_self_signed(sans)
        .map_err(|e| BridgeError::Hub(format!("self-signed cert generation failed: {e}")))?;

    let cert_der: CertificateDer<'static> = cert.cert.der().clone();
    let key_der: PrivateKeyDer<'static> =
        PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into();

    let config = ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .map_err(|e| BridgeError::Hub(format!("TLS config build failed: {e}")))?;

    Ok(config)
}

fn build_static_tls(cert_path: &str, key_path: &str) -> Result<ServerConfig, BridgeError> {
    let certs = load_certs(cert_path)?;
    let key = load_private_key(key_path)?;

    let config = ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| BridgeError::Hub(format!("TLS config build failed: {e}")))?;

    Ok(config)
}

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, BridgeError> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| BridgeError::Hub(format!("failed to parse certs from {path}: {e}")))?;
    Ok(certs)
}

fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>, BridgeError> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);

    if let Some(key) = rustls_pemfile::private_key(&mut reader)
        .map_err(|e| BridgeError::Hub(format!("failed to parse key from {path}: {e}")))?
    {
        return Ok(key);
    }

    Err(BridgeError::Hub(format!("no private key found in {path}")))
}

#[cfg(feature = "acme")]
async fn build_acme_tls(
    domain: &str,
    cache_dir: &str,
    use_production: bool,
) -> Result<ServerConfig, BridgeError> {
    use futures::StreamExt;
    use std::path::PathBuf;
    use tokio_rustls_acme::{caches::DirCache, AcmeConfig};

    let cache = DirCache::new(PathBuf::from(cache_dir));
    let config = AcmeConfig::new([domain])
        .contact_push("mailto:admin@example.com")
        .cache(cache)
        .directory_lets_encrypt(use_production);

    let mut state = config.state();
    let resolver = state.resolver();

    tokio::spawn(async move {
        while let Some(event) = state.next().await {
            match event {
                Ok(ok) => tracing::info!("ACME event: {:?}", ok),
                Err(e) => tracing::error!("ACME error: {}", e),
            }
        }
    });

    let server_config = ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_no_client_auth()
        .with_cert_resolver(resolver);

    Ok(server_config)
}
