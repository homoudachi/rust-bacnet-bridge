use bacnet_network::router::{BACnetRouter, RouterPort};
use bacnet_transport::loopback::LoopbackTransport;
use tokio::task::JoinHandle;
use tracing;

use crate::config::BridgeConfig;
use crate::error::BridgeError;
use crate::local_device::{self, LocalDeviceConfig};

pub struct RunningRouter {
    router: BACnetRouter,
    _local_device_task: JoinHandle<()>,
}

pub async fn start_router(config: &BridgeConfig) -> Result<RunningRouter, BridgeError> {
    let (lan_router, lan_device) = LoopbackTransport::pair(
        vec![0x01, 0x01],
        vec![0x01, 0x02],
    );
    let (remote_router, _remote_device) = LoopbackTransport::pair(
        vec![0x02, 0x01],
        vec![0x02, 0x02],
    );

    let ports = vec![
        RouterPort {
            transport: lan_router,
            network_number: 1,
        },
        RouterPort {
            transport: remote_router,
            network_number: 2,
        },
    ];

    let (router, local_rx) = BACnetRouter::start(ports)
        .await
        .map_err(|e| BridgeError::Router(e.to_string()))?;

    let device_config = LocalDeviceConfig {
        device_id: config.router.device_id,
        vendor_id: config.router.vendor_id,
        device_name: config.router.device_name.clone(),
    };

    tracing::info!("Starting router with loopback transports (net1=LAN, net2=remote)");

    let local_task = tokio::spawn(async move {
        local_device::handle_local_device(local_rx, lan_device, device_config).await;
    });

    Ok(RunningRouter {
        router,
        _local_device_task: local_task,
    })
}

impl RunningRouter {
    pub async fn stop(mut self) {
        self.router.stop().await;
        self._local_device_task.abort();
        tracing::info!("Router stopped");
    }

    pub fn table(&self) -> &std::sync::Arc<tokio::sync::Mutex<bacnet_network::router_table::RouterTable>> {
        self.router.table()
    }
}
