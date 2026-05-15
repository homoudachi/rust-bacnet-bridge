use std::env;
use std::net::Ipv4Addr;

use bacnet_encoding::npdu::{decode_npdu, encode_npdu, Npdu, NpduAddress};
use bacnet_transport::bip::BipTransport;
use bacnet_transport::port::{ReceivedNpdu, TransportPort};
use bacnet_types::enums::NetworkPriority;
use bacnet_types::MacAddr;
use bytes::{Bytes, BytesMut};

fn resolve_ip(s: &str) -> Ipv4Addr {
    if s.is_empty() || s == "0.0.0.0" {
        Ipv4Addr::UNSPECIFIED
    } else {
        s.parse().unwrap_or(Ipv4Addr::UNSPECIFIED)
    }
}

fn object_id_bytes(instance: u32) -> [u8; 4] {
    let type_bits: u32 = 8;
    let encoded = (type_bits << 22) | (instance & 0x3F_FFFF);
    encoded.to_be_bytes()
}

fn is_who_is(apdu: &[u8]) -> bool {
    apdu.len() >= 2 && (apdu[0] >> 4) == 0x01 && apdu[1] == 0x08
}

fn is_read_property(apdu: &[u8]) -> bool {
    apdu.len() >= 2 && (apdu[0] >> 4) == 0x00 && apdu[1] == 0x0C
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let port: u16 = env::var("BACNET_DEVICE_PORT")
        .unwrap_or_else(|_| "47808".into())
        .parse()
        .expect("BACNET_DEVICE_PORT must be a u16");
    let device_id: u32 = env::var("BACNET_DEVICE_ID")
        .unwrap_or_else(|_| "1001".into())
        .parse()
        .expect("BACNET_DEVICE_ID must be a u32");
    let device_name = env::var("BACNET_DEVICE_NAME").unwrap_or_else(|_| "Test-Device-01".into());
    let vendor_id: u16 = env::var("BACNET_VENDOR_ID")
        .unwrap_or_else(|_| "15".into())
        .parse()
        .expect("BACNET_VENDOR_ID must be a u16");
    let bind_ip = resolve_ip(&env::var("BACNET_DEVICE_BIND").unwrap_or_else(|_| "0.0.0.0".into()));
    let broadcast_ip =
        resolve_ip(&env::var("BACNET_DEVICE_BROADCAST").unwrap_or_else(|_| "255.255.255.255".into()));

    let mut transport = BipTransport::new(bind_ip, port, broadcast_ip);
    let mut rx: tokio::sync::mpsc::Receiver<ReceivedNpdu> = transport
        .start()
        .await
        .expect("Failed to start BIP transport");

    tracing::info!(
        "LAN device simulator started: instance={}, name={}, port={}",
        device_id,
        device_name,
        port,
    );

    while let Some(msg) = rx.recv().await {
        if let Err(e) = handle_message(&transport, &msg, device_id, vendor_id, &device_name).await {
            tracing::warn!("Error handling message: {e}");
        }
    }
}

async fn handle_message(
    transport: &BipTransport,
    msg: &ReceivedNpdu,
    device_id: u32,
    vendor_id: u16,
    device_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let npdu = decode_npdu(msg.npdu.clone())?;
    let apdu = &npdu.payload;

    if apdu.len() < 2 {
        return Ok(());
    }

    if is_who_is(apdu) {
        send_iam(transport, msg, device_id, vendor_id).await?;
    } else if is_read_property(apdu) {
        send_rp_response(transport, msg, device_id, vendor_id, device_name).await?;
    } else {
        tracing::debug!(
            "Unhandled APDU: pdu_type=0x{:X} service=0x{:X}",
            apdu[0] >> 4,
            apdu[1]
        );
    }

    Ok(())
}

fn build_iam_apdu(device_id: u32, vendor_id: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(20);
    buf.extend_from_slice(&[0x10, 0x00]);
    buf.push(0xC4);
    buf.extend_from_slice(&object_id_bytes(device_id));
    buf.extend_from_slice(&[0x22, 0x05, 0xC4]);
    buf.extend_from_slice(&[0x91, 0x00]);
    buf.extend_from_slice(&[0x22, (vendor_id >> 8) as u8, vendor_id as u8]);
    buf
}

async fn send_iam(
    transport: &BipTransport,
    _msg: &ReceivedNpdu,
    device_id: u32,
    vendor_id: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let apdu_bytes = build_iam_apdu(device_id, vendor_id);

    let npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        source: None,
        destination: Some(NpduAddress {
            network: 0xFFFF,
            mac_address: MacAddr::new(),
        }),
        hop_count: 255,
        payload: Bytes::from(apdu_bytes),
        ..Npdu::default()
    };

    let mut buf = BytesMut::with_capacity(64);
    encode_npdu(&mut buf, &npdu)?;

    transport.send_broadcast(&buf).await?;
    tracing::info!("Sent I-Am (instance={})", device_id);
    Ok(())
}

async fn send_rp_response(
    transport: &BipTransport,
    msg: &ReceivedNpdu,
    device_id: u32,
    vendor_id: u16,
    device_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let decoded = decode_npdu(msg.npdu.clone())?;
    let req_apdu = &decoded.payload;

    if req_apdu.len() < 8 {
        return Ok(());
    }

    let invoke_id = req_apdu[2];
    let id_bytes = device_id.to_be_bytes();
    let start_bytes = 4 - id_bytes.len();
    let prop_pos = 6 + start_bytes;
    let prop_id = if req_apdu.len() > prop_pos + 1 {
        ((req_apdu[prop_pos] as u16) << 8) | (req_apdu[prop_pos + 1] as u16)
    } else {
        return Ok(());
    };

    let mut rp_apdu = Vec::with_capacity(64);
    rp_apdu.extend_from_slice(&[0x30, invoke_id]);
    rp_apdu.push(0x0C);
    for _ in 0..start_bytes {
        rp_apdu.push(0x00);
    }
    rp_apdu.extend_from_slice(&id_bytes);

    match prop_id {
        75 => {
            rp_apdu.extend_from_slice(&[0x19, 0x4B]);
            let name = device_name.as_bytes();
            rp_apdu.extend_from_slice(&[0x75, name.len() as u8]);
            rp_apdu.extend_from_slice(name);
        }
        76 => {
            rp_apdu.extend_from_slice(&[0x19, 0x4C]);
            rp_apdu.extend_from_slice(&[0x22, (vendor_id >> 8) as u8, vendor_id as u8]);
        }
        85 => {
            rp_apdu.extend_from_slice(&[0x19, 0x55]);
            rp_apdu.extend_from_slice(&[0x75, 5, 0x30, 0x2E, 0x31, 0x2E, 0x30]);
        }
        12 => {
            rp_apdu.extend_from_slice(&[0x19, 0x0C]);
            rp_apdu.extend_from_slice(&[0x91, 0x00]);
        }
        13 => {
            rp_apdu.extend_from_slice(&[0x19, 0x0D]);
            rp_apdu.extend_from_slice(&[0x91, 0x18]);
        }
        _ => {
            rp_apdu.extend_from_slice(&[0x19, 0x00]);
            rp_apdu.push(0x5F);
        }
    }

    rp_apdu.push(0x0F);

    let response_npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        source: None,
        destination: Some(NpduAddress {
            network: 0xFFFF,
            mac_address: MacAddr::from_slice(msg.source_mac.as_ref()),
        }),
        hop_count: 255,
        payload: Bytes::from(rp_apdu),
        ..Npdu::default()
    };

    let mut buf = BytesMut::with_capacity(64);
    encode_npdu(&mut buf, &response_npdu)?;

    transport
        .send_unicast(&buf, msg.source_mac.as_ref())
        .await?;
    tracing::info!("Sent ReadProperty response (prop={})", prop_id);
    Ok(())
}
