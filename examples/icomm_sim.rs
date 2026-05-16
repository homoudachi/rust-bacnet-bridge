use std::env;
use std::net::Ipv4Addr;
use std::time::Duration;

use bacnet_encoding::npdu::{decode_npdu, encode_npdu, Npdu, NpduAddress};
use bacnet_transport::bip::BipTransport;
use bacnet_transport::port::{ReceivedNpdu, TransportPort};
use bacnet_types::enums::NetworkPriority;
use bacnet_types::MacAddr;
use bytes::{Bytes, BytesMut};
use tokio::time;

fn resolve_ip(s: &str) -> Ipv4Addr {
    if s.is_empty() || s == "0.0.0.0" {
        Ipv4Addr::UNSPECIFIED
    } else {
        s.parse().unwrap_or(Ipv4Addr::UNSPECIFIED)
    }
}

fn mac_to_ip_port(mac: &[u8]) -> (Ipv4Addr, u16) {
    if mac.len() >= 6 {
        let ip = Ipv4Addr::new(mac[0], mac[1], mac[2], mac[3]);
        let port = ((mac[4] as u16) << 8) | (mac[5] as u16);
        (ip, port)
    } else {
        (Ipv4Addr::UNSPECIFIED, 0)
    }
}

fn is_iam_apdu(apdu: &[u8]) -> bool {
    if apdu.len() < 2 {
        return false;
    }
    let pdu_type = apdu[0] >> 4;
    let service = apdu[1];
    (pdu_type == 0x00 || pdu_type == 0x01) && service == 0x00
}

fn extract_device_id(apdu: &[u8]) -> Option<u32> {
    if apdu.len() < 7 {
        return None;
    }
    let tag = apdu[2];
    if tag == 0x0C || tag == 0xC4 {
        let bytes: [u8; 4] = [apdu[3], apdu[4], apdu[5], apdu[6]];
        let raw = u32::from_be_bytes(bytes);
        Some(raw & 0x3F_FFFF)
    } else {
        None
    }
}

async fn send_who_is(transport: &BipTransport) -> Result<(), Box<dyn std::error::Error>> {
    let npdu = Npdu {
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

    let mut buf = BytesMut::with_capacity(64);
    encode_npdu(&mut buf, &npdu)?;
    transport.send_broadcast(&buf).await?;
    tracing::info!("Sent Who-Is broadcast");
    Ok(())
}

async fn send_who_is_unicast(
    transport: &BipTransport,
    target_ip: Ipv4Addr,
    target_port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let npdu = Npdu {
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

    let mut buf = BytesMut::with_capacity(64);
    encode_npdu(&mut buf, &npdu)?;

    let ip = target_ip.octets();
    let port_bytes = target_port.to_be_bytes();
    let mut mac = [0u8; 6];
    mac[..4].copy_from_slice(&ip);
    mac[4..].copy_from_slice(&port_bytes);

    transport.send_unicast(&buf, &mac).await?;
    tracing::info!("Sent Who-Is unicast to {}:{}", target_ip, target_port);
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let port: u16 = env::var("BACNET_DEVICE_PORT")
        .unwrap_or_else(|_| "47810".into())
        .parse()
        .expect("BACNET_DEVICE_PORT must be a u16");
    let bind_ip = resolve_ip(&env::var("BACNET_DEVICE_BIND").unwrap_or_else(|_| "0.0.0.0".into()));
    let broadcast_ip = resolve_ip(
        &env::var("BACNET_DEVICE_BROADCAST").unwrap_or_else(|_| "255.255.255.255".into()),
    );

    let target_mode = env::var("BACNET_TEST_TARGET").unwrap_or_else(|_| "sc".into());
    let target_ip =
        resolve_ip(&env::var("BACNET_TEST_TARGET_IP").unwrap_or_else(|_| "0.0.0.0".into()));
    let target_port: u16 = env::var("BACNET_TEST_TARGET_PORT")
        .unwrap_or_else(|_| "47809".into())
        .parse()
        .expect("BACNET_TEST_TARGET_PORT must be a u16");

    let timeout_secs: u64 = env::var("BACNET_TEST_TIMEOUT")
        .unwrap_or_else(|_| "15".into())
        .parse()
        .expect("BACNET_TEST_TIMEOUT must be a u64");

    let mut transport = BipTransport::new(bind_ip, port, broadcast_ip);
    let mut rx: tokio::sync::mpsc::Receiver<ReceivedNpdu> = transport
        .start()
        .await
        .expect("Failed to start BIP transport");

    tracing::info!(
        "iComm simulator started: port={}, mode={}, timeout={}s",
        port,
        target_mode,
        timeout_secs,
    );

    tokio::time::sleep(Duration::from_millis(500)).await;

    if target_mode == "unicast" && !target_ip.is_unspecified() {
        send_who_is_unicast(&transport, target_ip, target_port)
            .await
            .expect("Failed to send unicast Who-Is");
    } else {
        send_who_is(&transport)
            .await
            .expect("Failed to send Who-Is broadcast");
    }

    let deadline = time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut found_devices: Vec<u32> = Vec::new();

    loop {
        let remaining = deadline.saturating_duration_since(time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        let msg = tokio::time::timeout(remaining, rx.recv()).await;

        match msg {
            Ok(Some(received)) => {
                if let Ok(npdu) = decode_npdu(received.npdu.clone()) {
                    let apdu = &npdu.payload;
                    if is_iam_apdu(apdu) {
                        if let Some(id) = extract_device_id(apdu) {
                            let (ip, port) = mac_to_ip_port(received.source_mac.as_ref());
                            if !found_devices.contains(&id) {
                                found_devices.push(id);
                                tracing::info!("I-Am from device {} (source {}:{})", id, ip, port);
                            }
                        }
                    }
                }
            }
            Ok(None) => {
                tracing::warn!("BIP transport channel closed");
                break;
            }
            Err(_elapsed) => {
                break;
            }
        }
    }

    if found_devices.is_empty() {
        tracing::error!(
            "FAIL: No I-Am responses received within {}s timeout",
            timeout_secs
        );
        std::process::exit(1);
    } else {
        tracing::info!(
            "PASS: Discovered {} device(s): {:?}",
            found_devices.len(),
            found_devices
        );
        std::process::exit(0);
    }
}
