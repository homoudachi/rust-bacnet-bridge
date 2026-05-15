use std::net::Ipv4Addr;
use std::sync::Arc;

use bacnet_network::router::{BACnetRouter, RouterPort};
use bacnet_transport::any::AnyTransport;
use bacnet_transport::bbmd::BbmdState;
use bacnet_transport::bip::BipTransport;
use bacnet_transport::loopback::LoopbackTransport;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing;

use crate::config::BridgeConfig;
use crate::error::BridgeError;
use crate::local_device::{self, LocalDeviceConfig};
use crate::transport::build_remote_transport;

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

pub async fn start_router(config: &BridgeConfig) -> Result<RunningRouter, BridgeError> {
    let lan_ip = parse_lan_ip(&config.router.lan.interface);
    let lan_port = config.router.lan.port;
    let broadcast_addr = broadcast_from_ip(lan_ip);

    let lan_bip = BipTransport::new(lan_ip, lan_port, broadcast_addr);
    let (remote, bbmd_state) = build_remote_transport(config).await?;

    let (local_loop_router, local_loop_device) =
        LoopbackTransport::pair(vec![0x01, 0x01], vec![0x01, 0x02]);

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
            network_number: 0,
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
    };

    tracing::info!(
        "Router started: LAN {}:{} (broadcast {}), remote transport={}",
        lan_ip,
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
        bbmd_state,
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
