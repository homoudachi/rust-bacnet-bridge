use bacnet_encoding::apdu::{encode_apdu, Apdu, UnconfirmedRequest};
use bacnet_encoding::npdu::{encode_npdu, Npdu, NpduAddress};
use bacnet_encoding::primitives::{
    encode_ctx_enumerated, encode_ctx_object_id, encode_ctx_unsigned,
};
use bacnet_network::layer::ReceivedApdu;
use bacnet_transport::port::TransportPort;
use bacnet_types::enums::ObjectType;
use bacnet_types::enums::{NetworkPriority, UnconfirmedServiceChoice};
use bacnet_types::primitives::ObjectIdentifier;
use bytes::BytesMut;
use tokio::sync::mpsc;
use tracing;

pub struct LocalDeviceConfig {
    pub device_id: u32,
    pub vendor_id: u16,
    pub device_name: String,
    pub transport_mode: String,
}

pub async fn handle_local_device(
    mut local_rx: mpsc::Receiver<ReceivedApdu>,
    mut lan_transport: impl TransportPort,
    config: LocalDeviceConfig,
) {
    tracing::info!(
        "Local BACnet device started (instance {})",
        config.device_id
    );

    while let Some(msg) = local_rx.recv().await {
        let apdu = &msg.apdu;

        if apdu.len() < 2 {
            continue;
        }

        let pdu_type = apdu[0] >> 4;
        let service = apdu[1];

        match (pdu_type, service) {
            (0x01, 0x08) => {
                send_iam(&mut lan_transport, &msg, &config).await;
            }
            (0x00, 0x0C) => {
                handle_read_property(&mut lan_transport, &msg, &config).await;
            }
            _ => {
                tracing::debug!(
                    "Unhandled local APDU: type=0x{pdu_type:02X} service=0x{service:02X}"
                );
            }
        }
    }
}

async fn send_iam(
    lan_transport: &mut impl TransportPort,
    _msg: &ReceivedApdu,
    config: &LocalDeviceConfig,
) {
    let oid = match ObjectIdentifier::new(ObjectType::DEVICE, config.device_id) {
        Ok(oid) => oid,
        Err(e) => {
            tracing::warn!("Invalid device ID {}: {e}", config.device_id);
            return;
        }
    };

    let mut service_request = BytesMut::with_capacity(32);
    encode_ctx_object_id(&mut service_request, 0, &oid);
    encode_ctx_unsigned(&mut service_request, 1, 1476);
    encode_ctx_enumerated(&mut service_request, 2, 0);
    encode_ctx_unsigned(&mut service_request, 3, config.vendor_id as u64);

    let apdu = Apdu::UnconfirmedRequest(UnconfirmedRequest {
        service_choice: UnconfirmedServiceChoice::I_AM,
        service_request: service_request.freeze(),
    });

    let mut apdu_buf = BytesMut::with_capacity(64);
    if let Err(e) = encode_apdu(&mut apdu_buf, &apdu) {
        tracing::warn!("Failed to encode I-Am APDU: {e}");
        return;
    }

    let npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        source: Some(NpduAddress {
            network: 1,
            mac_address: bacnet_types::MacAddr::from_slice(lan_transport.local_mac()),
        }),
        destination: Some(NpduAddress {
            network: 0xFFFF,
            mac_address: bacnet_types::MacAddr::from_slice(&[]),
        }),
        hop_count: 255,
        payload: apdu_buf.freeze(),
        ..Npdu::default()
    };

    let mut buf = BytesMut::with_capacity(64);
    if let Err(e) = encode_npdu(&mut buf, &npdu) {
        tracing::warn!("Failed to encode I-Am NPDU: {e}");
        return;
    }

    if let Err(e) = lan_transport.send_broadcast(&buf).await {
        tracing::warn!("Failed to send I-Am broadcast: {e}");
    } else {
        tracing::info!("Sent I-Am for device {}", config.device_id);
    }
}

async fn handle_read_property(
    lan_transport: &mut impl TransportPort,
    msg: &ReceivedApdu,
    config: &LocalDeviceConfig,
) {
    let apdu_payload = &msg.apdu;
    if apdu_payload.len() < 8 {
        return;
    }

    let invoke_id = apdu_payload[2];

    let device_id = config.device_id;
    let id_bytes = device_id.to_be_bytes();
    let start_byte = 4 - id_bytes.len();

    let prop_id_pos = 6 + start_byte;
    let prop_id = if apdu_payload.len() > prop_id_pos + 1 {
        ((apdu_payload[prop_id_pos] as u16) << 8) | (apdu_payload[prop_id_pos + 1] as u16)
    } else {
        return;
    };

    let mut rp_apdu = BytesMut::with_capacity(64);
    rp_apdu.extend_from_slice(&[0x30, invoke_id]);
    rp_apdu.extend_from_slice(&[0x0C]);
    for _ in 0..start_byte {
        rp_apdu.extend_from_slice(&[0x00]);
    }
    rp_apdu.extend_from_slice(&id_bytes);

    match prop_id {
        75 => {
            rp_apdu.extend_from_slice(&[0x19, 0x4B]);
            let name = config.device_name.as_bytes();
            rp_apdu.extend_from_slice(&[0x75, name.len() as u8]);
            rp_apdu.extend_from_slice(name);
        }
        76 => {
            rp_apdu.extend_from_slice(&[0x19, 0x4C]);
            let vid = config.vendor_id;
            rp_apdu.extend_from_slice(&[0x22, (vid >> 8) as u8, vid as u8]);
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
        512 => {
            rp_apdu.extend_from_slice(&[0x1A, 0x02, 0x00]);
            let mode = config.transport_mode.as_bytes();
            rp_apdu.extend_from_slice(&[0x75, mode.len() as u8]);
            rp_apdu.extend_from_slice(mode);
        }
        _ => {
            rp_apdu.extend_from_slice(&[0x19, 0x00]);
            rp_apdu.extend_from_slice(&[0x5F]);
        }
    }

    rp_apdu.extend_from_slice(&[0x0F]);

    let npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        source: Some(NpduAddress {
            network: 1,
            mac_address: bacnet_types::MacAddr::from_slice(lan_transport.local_mac()),
        }),
        destination: msg.source_network.clone(),
        hop_count: 255,
        payload: rp_apdu.freeze(),
        ..Npdu::default()
    };

    let mut buf = BytesMut::with_capacity(64);
    if let Err(e) = encode_npdu(&mut buf, &npdu) {
        tracing::warn!("Failed to encode ReadProperty response NPDU: {e}");
        return;
    }

    if let Err(e) = lan_transport.send_unicast(&buf, &[]).await {
        tracing::warn!("Failed to send ReadProperty response: {e}");
    }
}
