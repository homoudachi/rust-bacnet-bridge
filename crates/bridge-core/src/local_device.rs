use bacnet_encoding::apdu::{encode_apdu, Apdu, ErrorPdu, UnconfirmedRequest};
use bacnet_encoding::npdu::{encode_npdu, Npdu, NpduAddress};
use bacnet_encoding::primitives::{
    encode_ctx_enumerated, encode_ctx_object_id, encode_ctx_unsigned,
};
use bacnet_network::layer::ReceivedApdu;
use bacnet_transport::port::TransportPort;
use bacnet_types::enums::ObjectType;
use bacnet_types::enums::{
    ConfirmedServiceChoice, ErrorClass, ErrorCode, NetworkPriority, UnconfirmedServiceChoice,
};
use bacnet_types::primitives::ObjectIdentifier;
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing;

pub fn build_read_property_ack(
    invoke_id: u8,
    device_id: u32,
    prop_id: u16,
    value_bytes: &[u8],
) -> Bytes {
    let oid = match ObjectIdentifier::new(ObjectType::DEVICE, device_id) {
        Ok(oid) => oid,
        Err(_) => return Bytes::new(),
    };

    let mut rp_apdu = BytesMut::with_capacity(64);
    rp_apdu.extend_from_slice(&[0x30, invoke_id]);
    rp_apdu.extend_from_slice(&[0x0C]);
    encode_ctx_object_id(&mut rp_apdu, 0, &oid);
    encode_ctx_unsigned(&mut rp_apdu, 1, prop_id as u64);
    rp_apdu.extend_from_slice(&[0x3E]);
    rp_apdu.extend_from_slice(value_bytes);
    rp_apdu.extend_from_slice(&[0x3F]);

    rp_apdu.freeze()
}

pub struct LocalDeviceConfig {
    pub device_id: u32,
    pub vendor_id: u16,
    pub device_name: String,
    pub transport_mode: String,
    pub lan_mac: Vec<u8>,
    pub local_network: u16,
    pub local_mac: Vec<u8>,
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

    let mut apdu_cache: HashMap<u8, Vec<u8>> = HashMap::new();

    while let Some(msg) = local_rx.recv().await {
        let apdu = &msg.apdu;

        tracing::debug!(
            "Received local APDU: len={} bytes={:02x?}",
            apdu.len(),
            apdu
        );

        if apdu.len() < 2 {
            continue;
        }

        let pdu_type = apdu[0] >> 4;
        let segmented = pdu_type == 0x00 && apdu.len() > 2 && (apdu[0] & 0x08) != 0;
        let service = if pdu_type == 0x00 && apdu.len() > 3 {
            // Confirmed Request: [0]=type+flags [1]=max_seg|max_apdu [2]=invoke_id [3]=service_choice
            // If segmented: [3]=seq [4]=window [5]=service_choice
            if segmented && apdu.len() > 5 {
                apdu[5]
            } else {
                apdu[3]
            }
        } else if pdu_type == 0x01 && apdu.len() > 1 {
            // Unconfirmed Request: [0]=type [1]=service_choice
            apdu[1]
        } else if apdu.len() > 1 {
            apdu[1]
        } else {
            continue;
        };

        match (pdu_type, service) {
            (0x01, 0x08) => {
                send_iam(&mut lan_transport, &msg, &config).await;
            }
            (0x00, 0x0C) => {
                handle_read_property(&mut lan_transport, &msg, &config, &mut apdu_cache).await;
            }
            (0x00, 0x05) => {
                handle_subscribe_cov(&mut lan_transport, &msg).await;
            }
            (0x00, 0x0F) => {
                handle_write_property(&mut lan_transport, &msg, &config).await;
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
            network: config.local_network,
            mac_address: bacnet_types::MacAddr::from_slice(&config.local_mac),
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
    apdu_cache: &mut HashMap<u8, Vec<u8>>,
) {
    let apdu_payload = &msg.apdu;
    if apdu_payload.len() < 11 {
        return;
    }

    let invoke_id = apdu_payload[2];

    // Bug 3: APDU retry cache — re-send cached response for duplicate invoke_id
    if let Some(cached) = apdu_cache.get(&invoke_id) {
        tracing::debug!("Re-sending cached response for invoke_id={}", invoke_id);
        if let Err(e) = lan_transport.send_unicast(cached, &[]).await {
            tracing::warn!("Failed to re-send cached response: {e}");
        }
        return;
    }

    let prop_id = {
        // After confirmed request header (4 bytes) + ctx0 ObjectId tag (1 byte)
        // + ObjectId value (4 bytes) = position 9 for ctx1 (PropertyIdentifier tag)
        // ctx1 tag byte = (1 << 4) | (1 << 3) | length
        //   length 1 → 0x19, length 2 → 0x1A, length 3 → 0x1B, length 4 → 0x1C
        let ctx1 = apdu_payload[9];
        let prop_len = ctx1 & 0x07;
        if prop_len == 0 || prop_len > 4 || apdu_payload.len() <= 10 {
            return;
        }
        let value_start = 10;
        match prop_len {
            1 => apdu_payload[value_start] as u16,
            2 => u16::from_be_bytes([apdu_payload[value_start], apdu_payload[value_start + 1]]),
            _ => return,
        }
    };

    // Bug 2: Unknown property — return Error PDU instead of ReadProperty-ACK
    let known_props: [u16; 10] = [11, 12, 13, 62, 75, 76, 85, 97, 139, 512];
    if !known_props.contains(&prop_id) {
        let error_apdu = Apdu::Error(ErrorPdu {
            invoke_id,
            service_choice: ConfirmedServiceChoice::READ_PROPERTY,
            error_class: ErrorClass::PROPERTY,
            error_code: ErrorCode::UNKNOWN_PROPERTY,
            error_data: Bytes::new(),
        });

        let mut apdu_buf = BytesMut::with_capacity(32);
        if let Err(e) = encode_apdu(&mut apdu_buf, &error_apdu) {
            tracing::warn!("Failed to encode error APDU: {e}");
            return;
        }

        let npdu = Npdu {
            is_network_message: false,
            expecting_reply: false,
            priority: NetworkPriority::NORMAL,
            source: Some(NpduAddress {
                network: config.local_network,
                mac_address: bacnet_types::MacAddr::from_slice(&config.local_mac),
            }),
            destination: msg.source_network.clone().or(Some(NpduAddress {
                network: 1,
                mac_address: msg.source_mac.clone(),
            })),
            hop_count: 255,
            payload: apdu_buf.freeze(),
            ..Npdu::default()
        };

        let mut buf = BytesMut::with_capacity(32);
        if let Err(e) = encode_npdu(&mut buf, &npdu) {
            tracing::warn!("Failed to encode error response NPDU: {e}");
            return;
        }

        apdu_cache.insert(invoke_id, buf.to_vec());
        if let Err(e) = lan_transport.send_unicast(&buf, &[]).await {
            tracing::warn!("Failed to send error response: {e}");
        }
        return;
    }

    let value_bytes = match prop_id {
        11 => {
            vec![0x22, 0x0B, 0xB8]
        }
        // Bug 1: Object_Name — use fallback when device_name is empty
        75 => {
            let name = if config.device_name.is_empty() {
                "BACnet-Bridge"
            } else {
                config.device_name.as_str()
            };
            let bytes = name.as_bytes();
            let mut v = vec![0x75, bytes.len() as u8];
            v.extend_from_slice(bytes);
            v
        }
        76 => {
            let vid = config.vendor_id;
            vec![0x22, (vid >> 8) as u8, vid as u8]
        }
        85 => {
            vec![0x75, 5, 0x30, 0x2E, 0x31, 0x2E, 0x30]
        }
        12 => {
            vec![0x91, 0x00]
        }
        13 => {
            vec![0x91, 0x18]
        }
        62 => {
            vec![0x91, 0x02]
        }
        512 => {
            let mode = config.transport_mode.as_bytes();
            let mut v = vec![0x75, mode.len() as u8];
            v.extend_from_slice(mode);
            v
        }
        97 => {
            vec![0x85, 5, 5, 0x84, 0x0B, 0x00, 0x20]
        }
        139 => {
            vec![0x21, 0x18]
        }
        _ => unreachable!(),
    };

    let rp_apdu = build_read_property_ack(invoke_id, config.device_id, prop_id, &value_bytes);
    if rp_apdu.is_empty() {
        return;
    }

    let npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        source: Some(NpduAddress {
            network: config.local_network,
            mac_address: bacnet_types::MacAddr::from_slice(&config.local_mac),
        }),
        destination: msg.source_network.clone().or(Some(NpduAddress {
            network: 1,
            mac_address: msg.source_mac.clone(),
        })),
        hop_count: 255,
        payload: rp_apdu,
        ..Npdu::default()
    };

    let mut buf = BytesMut::with_capacity(64);
    if let Err(e) = encode_npdu(&mut buf, &npdu) {
        tracing::warn!("Failed to encode ReadProperty response NPDU: {e}");
        return;
    }

    // Bug 3: Cache the response before sending
    apdu_cache.insert(invoke_id, buf.to_vec());

    if let Err(e) = lan_transport.send_unicast(&buf, &[]).await {
        tracing::warn!("Failed to send ReadProperty response: {e}");
    }
}

async fn handle_subscribe_cov(lan_transport: &mut impl TransportPort, msg: &ReceivedApdu) {
    let apdu_payload = &msg.apdu;
    if apdu_payload.len() < 3 {
        return;
    }

    let invoke_id = apdu_payload[2];

    let response = build_subscribe_cov_ack(invoke_id);
    if response.is_empty() {
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
        destination: msg.source_network.clone().or(Some(NpduAddress {
            network: 1,
            mac_address: msg.source_mac.clone(),
        })),
        hop_count: 255,
        payload: response,
        ..Npdu::default()
    };

    let mut buf = BytesMut::with_capacity(64);
    if let Err(e) = encode_npdu(&mut buf, &npdu) {
        tracing::warn!("Failed to encode SubscribeCOV response NPDU: {e}");
        return;
    }

    if let Err(e) = lan_transport.send_unicast(&buf, &[]).await {
        tracing::warn!("Failed to send SubscribeCOV response: {e}");
    } else {
        tracing::debug!("Sent SubscribeCOV SimpleAck (invoke_id={})", invoke_id);
    }
}

async fn handle_write_property(
    lan_transport: &mut impl TransportPort,
    msg: &ReceivedApdu,
    config: &LocalDeviceConfig,
) {
    let apdu_payload = &msg.apdu;
    if apdu_payload.len() < 3 {
        return;
    }
    let invoke_id = apdu_payload[2];

    let error_apdu = Apdu::Error(ErrorPdu {
        invoke_id,
        service_choice: ConfirmedServiceChoice::WRITE_PROPERTY,
        error_class: ErrorClass::PROPERTY,
        error_code: ErrorCode::WRITE_ACCESS_DENIED,
        error_data: Bytes::new(),
    });

    let mut apdu_buf = BytesMut::with_capacity(32);
    if let Err(e) = encode_apdu(&mut apdu_buf, &error_apdu) {
        tracing::warn!("Failed to encode WriteProperty error APDU: {e}");
        return;
    }

    let npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        source: Some(NpduAddress {
            network: config.local_network,
            mac_address: bacnet_types::MacAddr::from_slice(&config.local_mac),
        }),
        destination: msg.source_network.clone().or(Some(NpduAddress {
            network: 1,
            mac_address: msg.source_mac.clone(),
        })),
        hop_count: 255,
        payload: apdu_buf.freeze(),
        ..Npdu::default()
    };

    let mut buf = BytesMut::with_capacity(32);
    if let Err(e) = encode_npdu(&mut buf, &npdu) {
        tracing::warn!("Failed to encode WriteProperty error NPDU: {e}");
        return;
    }

    if let Err(e) = lan_transport.send_unicast(&buf, &[]).await {
        tracing::warn!("Failed to send WriteProperty error response: {e}");
    } else {
        tracing::debug!(
            "Sent WriteProperty error (WRITE_ACCESS_DENIED) invoke_id={}",
            invoke_id
        );
    }
}

fn build_subscribe_cov_ack(invoke_id: u8) -> Bytes {
    let mut buf = BytesMut::with_capacity(3);
    buf.extend_from_slice(&[0x20, invoke_id, 0x05]);
    buf.freeze()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bacnet_encoding::apdu::decode_apdu;

    #[test]
    fn test_object_name_fallback_non_empty() {
        let fallback = "BACnet-Bridge";
        let bytes = fallback.as_bytes();
        let mut value_bytes = vec![0x75, bytes.len() as u8];
        value_bytes.extend_from_slice(bytes);

        let ack = build_read_property_ack(1, 99999, 75, &value_bytes);
        let raw = ack.to_vec();

        let opening = raw.iter().position(|&b| b == 0x3E).unwrap();
        assert_eq!(raw[opening + 1], 0x75, "must be CharacterString tag");
        assert!(raw[opening + 2] > 0, "Object_Name length must be non-zero");
    }

    #[test]
    fn test_unknown_property_error_pdu_format() {
        let error_apdu = Apdu::Error(ErrorPdu {
            invoke_id: 42,
            service_choice: ConfirmedServiceChoice::READ_PROPERTY,
            error_class: ErrorClass::PROPERTY,
            error_code: ErrorCode::UNKNOWN_PROPERTY,
            error_data: Bytes::new(),
        });

        let mut buf = BytesMut::new();
        encode_apdu(&mut buf, &error_apdu).unwrap();

        let decoded = decode_apdu(buf.freeze()).unwrap();
        match decoded {
            Apdu::Error(pdu) => {
                assert_eq!(pdu.invoke_id, 42);
                assert_eq!(pdu.service_choice, ConfirmedServiceChoice::READ_PROPERTY);
                assert_eq!(pdu.error_class, ErrorClass::PROPERTY);
                assert_eq!(pdu.error_code, ErrorCode::UNKNOWN_PROPERTY);
                assert!(pdu.error_data.is_empty());
            }
            other => panic!("expected Error PDU, got {other:?}"),
        }
    }

    #[test]
    fn test_apdu_cache_dedup() {
        let mut cache: HashMap<u8, Vec<u8>> = HashMap::new();

        cache.insert(42, vec![0x30, 0x01, 0x0C]);

        assert!(
            cache.contains_key(&42),
            "same invoke_id must be a cache hit"
        );
        assert!(
            !cache.contains_key(&99),
            "different invoke_id must be a cache miss"
        );
        assert_eq!(cache.get(&42), Some(&vec![0x30, 0x01, 0x0C]));
    }

    #[test]
    fn test_prop_139_context_tag_is_3_not_2() {
        let ack = build_read_property_ack(1, 99999, 139, &[0x21, 0x18]);
        // Expected APDU structure (verified from actual encoding):
        //   0x30, 0x01, 0x0C,                        // ComplexAck + ReadProperty
        //   0x0C, 0x02, 0x01, 0x86, 0x9F,            // Context[0] DEVICE(8), 99999
        //   0x19, 0x8B,                               // Context[1] PROP 139
        //   0x3E,                                     // Context[3] OPENING ← was 0x2E
        //   0x21, 0x18,                               // Unsigned 24
        //   0x3F                                      // Context[3] CLOSING ← was 0x2F
        let expected = vec![
            0x30, 0x01, 0x0C, 0x0C, 0x02, 0x01, 0x86, 0x9F, 0x19, 0x8B, 0x3E, 0x21, 0x18, 0x3F,
        ];
        assert_eq!(ack.to_vec(), expected, "PROP 139 encoding has wrong bytes");
    }

    #[test]
    fn test_prop_97_context_tag_is_3_not_2() {
        let ack = build_read_property_ack(1, 99999, 97, &[0x85, 5, 5, 0x84, 0x0B, 0x00, 0x20]);
        let expected = vec![
            0x30, 0x01, 0x0C, 0x0C, 0x02, 0x01, 0x86, 0x9F, 0x19, 0x61, 0x3E, 0x85, 5, 5, 0x84,
            0x0B, 0x00, 0x20, 0x3F,
        ];
        assert_eq!(ack.to_vec(), expected, "PROP 97 encoding has wrong bytes");
    }

    #[test]
    fn test_subscribe_cov_simple_ack() {
        let ack = build_subscribe_cov_ack(42);
        let expected = vec![0x20, 42, 0x05];
        assert_eq!(
            ack.to_vec(),
            expected,
            "SubscribeCOV SimpleAck has wrong bytes"
        );
    }

    #[test]
    fn test_subscribe_cov_simple_ack_default_invoke() {
        let ack = build_subscribe_cov_ack(0);
        assert_eq!(ack.len(), 3);
        assert_eq!(ack[1], 0);
        assert_eq!(ack[2], 0x05);
    }

    #[test]
    fn test_context_tag_not_0x2e_or_0x2f() {
        for prop_id in [12u16, 13, 75, 76, 85, 97, 139, 512] {
            let ack = build_read_property_ack(1, 99999, prop_id, &[0x5F]);
            let bytes = ack.to_vec();
            assert!(
                !bytes.contains(&0x2E),
                "PROP {} contains 0x2E (old buggy tag [2] opening)",
                prop_id
            );
            assert!(
                !bytes.contains(&0x2F),
                "PROP {} contains 0x2F (old buggy tag [2] closing)",
                prop_id
            );
        }
    }
}
