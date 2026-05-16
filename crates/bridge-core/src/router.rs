use std::net::Ipv4Addr;
use std::sync::Arc;

use bacnet_network::router::{BACnetRouter, RouterPort};
use bacnet_transport::any::AnyTransport;
use bacnet_transport::bbmd::BbmdState;
use bacnet_transport::bip::BipTransport;
use bacnet_transport::loopback::LoopbackTransport;
use bacnet_transport::port::TransportPort;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing;

use crate::config::BridgeConfig;
use crate::error::BridgeError;
use crate::local_device::{self, LocalDeviceConfig};
use crate::transport::build_remote_transport;

const LOOPBACK_NETWORK: u16 = 65520;

pub struct RunningRouter {
    router: BACnetRouter,
    _local_device_task: JoinHandle<()>,
    pub bbmd_state: Option<Arc<Mutex<BbmdState>>>,
}

fn parse_lan_ip(interface: &str) -> Ipv4Addr {
    if interface.is_empty() {
        Ipv4Addr::UNSPECIFIED
    } else {
        interface.parse().unwrap_or(Ipv4Addr::UNSPECIFIED)
    }
}

fn broadcast_from_ip(ip: Ipv4Addr) -> Ipv4Addr {
    if ip.is_unspecified() {
        Ipv4Addr::BROADCAST
    } else {
        let octets = ip.octets();
        Ipv4Addr::new(octets[0], octets[1], octets[2], 255)
    }
}

fn encode_bip_mac(ip: Ipv4Addr, port: u16) -> Vec<u8> {
    let octets = ip.octets();
    let port_bytes = port.to_be_bytes();
    vec![
        octets[0],
        octets[1],
        octets[2],
        octets[3],
        port_bytes[0],
        port_bytes[1],
    ]
}

fn resolve_local_ip() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    if let std::net::IpAddr::V4(ip) = socket.local_addr().ok()?.ip() {
        Some(ip)
    } else {
        None
    }
}

fn resolved_lan_ip(lan_ip: Ipv4Addr) -> Ipv4Addr {
    if lan_ip.is_unspecified() {
        resolve_local_ip().unwrap_or(Ipv4Addr::new(127, 0, 0, 1))
    } else {
        lan_ip
    }
}

pub async fn start_router(config: &BridgeConfig) -> Result<RunningRouter, BridgeError> {
    let lan_ip = parse_lan_ip(&config.router.lan.interface);
    let lan_port = config.router.lan.port;
    let broadcast_addr = broadcast_from_ip(lan_ip);
    let actual_lan_ip = resolved_lan_ip(lan_ip);

    let mut lan_bip = BipTransport::new(Ipv4Addr::UNSPECIFIED, lan_port, broadcast_addr);
    lan_bip.enable_bbmd(vec![]);
    let lan_bbmd_state = lan_bip.bbmd_state().cloned();
    let (remote, _remote_bbmd_state) = build_remote_transport(config).await?;

    let local_loop_mac = vec![0x01, 0x02];
    let (local_loop_router, mut local_loop_device) =
        LoopbackTransport::pair(vec![0x01, 0x01], local_loop_mac.clone());

    let mut loopback_rx = local_loop_device
        .start()
        .await
        .map_err(|e| BridgeError::Router(format!("loopback device start: {e}")))?;
    tokio::spawn(async move {
        while let Some(_) = loopback_rx.recv().await {}
    });

    let lan_mac = encode_bip_mac(actual_lan_ip, lan_port);

    let ports = vec![
        RouterPort {
            transport: AnyTransport::from(lan_bip),
            network_number: 1,
        },
        RouterPort {
            transport: remote,
            network_number: 2,
        },
        RouterPort {
            transport: AnyTransport::from(local_loop_router),
            network_number: LOOPBACK_NETWORK,
        },
    ];

    let (router, local_rx) = BACnetRouter::start(ports)
        .await
        .map_err(|e| BridgeError::Router(e.to_string()))?;

    let device_config = LocalDeviceConfig {
        device_id: config.router.device_id,
        vendor_id: config.router.vendor_id,
        device_name: config.router.device_name.clone(),
        transport_mode: config.router.transport.clone(),
        lan_mac,
        local_network: LOOPBACK_NETWORK,
        local_mac: local_loop_mac,
    };

    tracing::info!(
        "Router started: LAN {} (resolved {}) :{}, broadcast {}, remote transport={}",
        lan_ip,
        actual_lan_ip,
        lan_port,
        broadcast_addr,
        config.router.transport,
    );

    let local_task = tokio::spawn(async move {
        local_device::handle_local_device(local_rx, local_loop_device, device_config).await;
    });

    Ok(RunningRouter {
        router,
        _local_device_task: local_task,
        bbmd_state: lan_bbmd_state,
    })
}

impl RunningRouter {
    pub async fn stop(mut self) {
        self.router.stop().await;
        self._local_device_task.abort();
        tracing::info!("Router stopped");
    }

    pub fn table(
        &self,
    ) -> &std::sync::Arc<tokio::sync::Mutex<bacnet_network::router_table::RouterTable>> {
        self.router.table()
    }
}
