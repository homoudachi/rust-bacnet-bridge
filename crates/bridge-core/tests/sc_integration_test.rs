use std::sync::Arc;
use std::time::Duration;

use bacnet_encoding::npdu::{encode_npdu, Npdu, NpduAddress};
use bacnet_network::router::{BACnetRouter, RouterPort};
use bacnet_transport::any::AnyTransport;
use bacnet_transport::loopback::LoopbackTransport;
use bacnet_transport::port::TransportPort;
use bacnet_transport::sc_frame::Vmac;
use bacnet_transport::sc_hub::ScHub;
use bacnet_types::enums::NetworkPriority;
use bacnet_types::MacAddr;
use bridge_core::config::{
    BridgeConfig, HubConfig, LanConfig, RouterConfig, ScConfig, TailscaleConfig, WebConfig,
};
use bridge_core::transport::build_remote_transport;
use bytes::{Bytes, BytesMut};
use tokio_rustls::rustls;
use tokio_rustls::TlsAcceptor;

#[tokio::test]
async fn test_router_connects_as_sc_spoke() {
    let dir = std::env::temp_dir().join("bacnet-bridge-sc-int-test");
    std::fs::create_dir_all(&dir).unwrap();
    let cert_path = dir.join("test-cert.pem");
    let key_path = dir.join("test-key.pem");

    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_pem = cert.cert.pem();
    let key_pem = cert.signing_key.serialize_pem();
    std::fs::write(&cert_path, cert_pem.as_bytes()).unwrap();
    std::fs::write(&key_path, key_pem.as_bytes()).unwrap();

    let cert_der: Vec<rustls::pki_types::CertificateDer> =
        rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
    let key_der = rustls_pemfile::private_key(&mut key_pem.as_bytes())
        .unwrap()
        .unwrap();

    let server_config =
        rustls::ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_no_client_auth()
            .with_single_cert(cert_der, key_der)
            .unwrap();
    let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

    let hub_vmac: Vmac = [0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
    let mut hub = ScHub::start("127.0.0.1:0", tls_acceptor, hub_vmac)
        .await
        .expect("ScHub::start");
    let hub_port = hub.local_addr().unwrap().port();
    let hub_url = format!("wss://localhost:{hub_port}");

    let config = BridgeConfig {
        router: RouterConfig {
            transport: "sc".to_string(),
            device_id: 42,
            vendor_id: 15,
            device_name: "Test-Router".to_string(),
            lan: LanConfig {
                interface: String::new(),
                port: 47808,
            },
            sc: ScConfig {
                hub_url: hub_url.clone(),
                reconnect_initial_ms: 100,
                reconnect_max_ms: 1000,
                reconnect_max_attempts: 5,
                client_cert: None,
                client_key: None,
                ca_cert: Some(cert_path.to_string_lossy().to_string()),
            },
            tailscale: TailscaleConfig::default(),
        },
        web: WebConfig::default(),
        hub: HubConfig::default(),
    };

    let (remote, _bbmd) = build_remote_transport(&config)
        .await
        .expect("build_remote_transport SC");

    let (lan_router, mut lan_device) = LoopbackTransport::pair(vec![0x01, 0x01], vec![0x01, 0x02]);
    let _lan_rx = lan_device.start().await.unwrap();

    let ports = vec![
        RouterPort {
            transport: AnyTransport::from(lan_router),
            network_number: 1,
        },
        RouterPort {
            transport: remote,
            network_number: 2,
        },
    ];

    let (mut router, _local_rx) = BACnetRouter::start(ports)
        .await
        .expect("BACnetRouter::start");

    tokio::time::sleep(Duration::from_millis(500)).await;

    let who_is = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        destination: Some(NpduAddress {
            network: 0xFFFF,
            mac_address: MacAddr::new(),
        }),
        source: None,
        hop_count: 255,
        payload: Bytes::from_static(&[0x10, 0x08]),
        ..Npdu::default()
    };
    let mut buf = BytesMut::new();
    encode_npdu(&mut buf, &who_is).unwrap();
    lan_device.send_broadcast(&buf).await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    router.stop().await;
    hub.stop().await;
    std::fs::remove_dir_all(&dir).ok();
}
