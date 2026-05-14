use crate::config::BridgeConfig;
use crate::error::BridgeError;
use bacnet_transport::any::AnyTransport;
use bacnet_transport::loopback::LoopbackTransport;
use bacnet_transport::mstp::NoSerial;

pub fn build_remote_transport(_config: &BridgeConfig) -> Result<AnyTransport<NoSerial>, BridgeError> {
    let (transport, _) = LoopbackTransport::pair(vec![0x02, 0x01], vec![0x02, 0x02]);
    Ok(AnyTransport::Loopback(transport))
}
