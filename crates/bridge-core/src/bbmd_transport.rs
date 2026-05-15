use std::net::Ipv4Addr;

use bacnet_transport::any::AnyTransport;
use bacnet_transport::bbmd::BdtEntry as BbmdBdtEntry;
use bacnet_transport::bip::BipTransport;
use bacnet_transport::mstp::NoSerial;

use crate::config::{BdtEntry, TailscaleConfig};
use crate::error::BridgeError;

fn parse_ipv4(s: &str) -> Result<[u8; 4], BridgeError> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return Err(BridgeError::Transport(format!(
            "Invalid IP address format: {s}"
        )));
    }
    let mut ip = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        ip[i] = part
            .parse()
            .map_err(|_| BridgeError::Transport(format!("Invalid IP octet: {part}")))?;
    }
    Ok(ip)
}

fn convert_bdt_entry(entry: &BdtEntry) -> Result<BbmdBdtEntry, BridgeError> {
    let ip = parse_ipv4(&entry.ip)?;
    Ok(BbmdBdtEntry {
        ip,
        port: entry.port,
        broadcast_mask: entry.broadcast_mask,
    })
}

pub async fn build_bbmd_transport(
    config: &TailscaleConfig,
) -> Result<AnyTransport<NoSerial>, BridgeError> {
    let interface: Ipv4Addr = config.interface.parse().map_err(|e| {
        BridgeError::Transport(format!(
            "Invalid Tailscale interface IP '{}': {e}",
            config.interface
        ))
    })?;

    let broadcast = Ipv4Addr::new(255, 255, 255, 255);
    let mut transport = BipTransport::new(interface, config.port, broadcast);

    if let Some(bdt) = &config.bdt {
        let bbmd_bdt: Vec<BbmdBdtEntry> = bdt
            .iter()
            .map(convert_bdt_entry)
            .collect::<Result<Vec<_>, _>>()?;

        transport.enable_bbmd(bbmd_bdt);
        tracing::info!(
            "BBMD enabled on {}:{} with {} BDT entries",
            config.interface,
            config.port,
            bdt.len()
        );
    } else {
        tracing::info!(
            "BIP transport created on {}:{} (no BBMD mode)",
            config.interface,
            config.port
        );
    }

    Ok(AnyTransport::Bip(transport))
}
