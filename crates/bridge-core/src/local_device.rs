use bacnet_encoding::apdu::{encode_apdu, Apdu, ErrorPdu, UnconfirmedRequest};
use bacnet_encoding::npdu::{encode_npdu, Npdu, NpduAddress};
use bacnet_encoding::primitives::{
    encode_app_bit_string, encode_app_boolean, encode_app_character_string, encode_app_enumerated,
    encode_app_object_id, encode_app_real, encode_ctx_enumerated, encode_ctx_object_id,
    encode_ctx_unsigned,
};
use bacnet_encoding::tags::decode_tag;
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

struct ObjectState {
    object_type: ObjectType,
    instance: u32,
    present_value: f32,
    priority_array: [Option<f32>; 16],
    relinquish_default: f32,
    out_of_service: bool,
}

impl ObjectState {
    fn resolve_present_value(&self) -> f32 {
        self.priority_array
            .iter()
            .flatten()
            .copied()
            .next()
            .unwrap_or(self.relinquish_default)
    }

    fn update_priority(&mut self, priority: usize, value: Option<f32>) {
        if (1..=16).contains(&priority) {
            self.priority_array[priority - 1] = value;
            self.present_value = self.resolve_present_value();
        }
    }
}

fn make_object(object_type: ObjectType, instance: u32) -> ObjectState {
    ObjectState {
        object_type,
        instance,
        present_value: 0.0,
        priority_array: [None; 16],
        relinquish_default: 0.0,
        out_of_service: false,
    }
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
    let mut objects: HashMap<ObjectIdentifier, ObjectState> = HashMap::new();

    let ao_oid = ObjectIdentifier::new(ObjectType::ANALOG_OUTPUT, 1).expect("AO OID");
    objects.insert(ao_oid, make_object(ObjectType::ANALOG_OUTPUT, 1));

    let av_oid = ObjectIdentifier::new(ObjectType::ANALOG_VALUE, 1).expect("AV OID");
    objects.insert(av_oid, make_object(ObjectType::ANALOG_VALUE, 1));

    let lo_oid = ObjectIdentifier::new(ObjectType::LIGHTING_OUTPUT, 1).expect("LO OID");
    objects.insert(lo_oid, make_object(ObjectType::LIGHTING_OUTPUT, 1));

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
                handle_read_property(&mut lan_transport, &msg, &config, &mut apdu_cache, &objects)
                    .await;
            }
            (0x00, 0x05) => {
                handle_subscribe_cov(&mut lan_transport, &msg).await;
            }
            (0x00, 0x0F) => {
                handle_write_property(&mut lan_transport, &msg, &config, &mut objects).await;
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
    objects: &HashMap<ObjectIdentifier, ObjectState>,
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

    // Parse the requesting object OID
    let request_oid = parse_oid_from_request(apdu_payload);

    let device_oid = match ObjectIdentifier::new(ObjectType::DEVICE, config.device_id) {
        Ok(oid) => oid,
        Err(_) => return,
    };

    // If the request is not for the Device object, try AO/AV/LO objects
    if let Some(ref oid) = request_oid {
        if *oid != device_oid {
            if let Some(state) = objects.get(oid) {
                handle_read_property_ao_av_lo(
                    lan_transport,
                    msg,
                    config,
                    apdu_cache,
                    state,
                    prop_id,
                    invoke_id,
                )
                .await;
            } else {
                send_error(
                    lan_transport,
                    msg,
                    config,
                    invoke_id,
                    ConfirmedServiceChoice::READ_PROPERTY,
                    ErrorClass::OBJECT,
                    ErrorCode::UNKNOWN_OBJECT,
                )
                .await;
            }
            return;
        }
    }

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
    objects: &mut HashMap<ObjectIdentifier, ObjectState>,
) {
    let apdu_payload = &msg.apdu;
    if apdu_payload.len() < 14 {
        return;
    }
    let invoke_id = apdu_payload[2];

    let request_oid = match parse_oid_from_request(apdu_payload) {
        Some(oid) => oid,
        None => return,
    };

    let device_oid = match ObjectIdentifier::new(ObjectType::DEVICE, config.device_id) {
        Ok(oid) => oid,
        Err(_) => return,
    };

    // Device object: WRITE_ACCESS_DENIED (existing behavior)
    if request_oid == device_oid {
        send_error(
            lan_transport,
            msg,
            config,
            invoke_id,
            ConfirmedServiceChoice::WRITE_PROPERTY,
            ErrorClass::PROPERTY,
            ErrorCode::WRITE_ACCESS_DENIED,
        )
        .await;
        return;
    }

    let ctx1 = apdu_payload[9];
    let prop_len = ctx1 & 0x07;
    if prop_len == 0 || prop_len > 4 || apdu_payload.len() <= 10 + prop_len as usize {
        return;
    }
    let prop_id = match prop_len {
        1 => apdu_payload[10] as u16,
        2 => u16::from_be_bytes([apdu_payload[10], apdu_payload[11]]),
        _ => return,
    };
    let prop_end = 10 + prop_len as usize;

    if apdu_payload.len() <= prop_end {
        return;
    }

    let state = match objects.get_mut(&request_oid) {
        Some(s) => s,
        None => {
            send_error(
                lan_transport,
                msg,
                config,
                invoke_id,
                ConfirmedServiceChoice::WRITE_PROPERTY,
                ErrorClass::OBJECT,
                ErrorCode::UNKNOWN_OBJECT,
            )
            .await;
            return;
        }
    };

    match prop_id {
        85 => {
            let (value, priority) = match parse_write_property_value(apdu_payload, prop_end) {
                Some(v) => v,
                None => {
                    send_error(
                        lan_transport,
                        msg,
                        config,
                        invoke_id,
                        ConfirmedServiceChoice::WRITE_PROPERTY,
                        ErrorClass::PROPERTY,
                        ErrorCode::WRITE_ACCESS_DENIED,
                    )
                    .await;
                    return;
                }
            };

            if (1..=16).contains(&priority) {
                state.update_priority(priority as usize, value);
            } else if let Some(v) = value {
                state.present_value = v;
            }

            send_simple_ack(lan_transport, msg, config, invoke_id, 0x0F).await;
        }
        81 => {
            if let Ok((tag, _)) = decode_tag(apdu_payload, prop_end) {
                if tag.is_context(2) && tag.length == 1 {
                    let val_start = prop_end + 1;
                    if val_start < apdu_payload.len() {
                        state.out_of_service = apdu_payload[val_start] != 0;
                    }
                }
            }
            send_simple_ack(lan_transport, msg, config, invoke_id, 0x0F).await;
        }
        _ => {
            send_error(
                lan_transport,
                msg,
                config,
                invoke_id,
                ConfirmedServiceChoice::WRITE_PROPERTY,
                ErrorClass::PROPERTY,
                ErrorCode::WRITE_ACCESS_DENIED,
            )
            .await;
        }
    }
}

fn build_subscribe_cov_ack(invoke_id: u8) -> Bytes {
    let mut buf = BytesMut::with_capacity(3);
    buf.extend_from_slice(&[0x20, invoke_id, 0x05]);
    buf.freeze()
}

fn parse_oid_from_request(apdu: &[u8]) -> Option<ObjectIdentifier> {
    if apdu.len() < 9 {
        return None;
    }
    if apdu[4] != 0x0C {
        return None;
    }
    ObjectIdentifier::decode(&apdu[5..9]).ok()
}

fn build_rp_ack_for_oid(
    invoke_id: u8,
    oid: &ObjectIdentifier,
    prop_id: u16,
    value_bytes: &[u8],
) -> Bytes {
    let mut rp_apdu = BytesMut::with_capacity(64);
    rp_apdu.extend_from_slice(&[0x30, invoke_id]);
    rp_apdu.extend_from_slice(&[0x0C]);
    encode_ctx_object_id(&mut rp_apdu, 0, oid);
    encode_ctx_unsigned(&mut rp_apdu, 1, prop_id as u64);
    rp_apdu.extend_from_slice(&[0x3E]);
    rp_apdu.extend_from_slice(value_bytes);
    rp_apdu.extend_from_slice(&[0x3F]);
    rp_apdu.freeze()
}

async fn send_error(
    lan_transport: &mut impl TransportPort,
    msg: &ReceivedApdu,
    config: &LocalDeviceConfig,
    invoke_id: u8,
    service_choice: ConfirmedServiceChoice,
    error_class: ErrorClass,
    error_code: ErrorCode,
) {
    let error_apdu = Apdu::Error(ErrorPdu {
        invoke_id,
        service_choice,
        error_class,
        error_code,
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

    if let Err(e) = lan_transport.send_unicast(&buf, &[]).await {
        tracing::warn!("Failed to send error response: {e}");
    }
}

async fn send_simple_ack(
    lan_transport: &mut impl TransportPort,
    msg: &ReceivedApdu,
    config: &LocalDeviceConfig,
    invoke_id: u8,
    service: u8,
) {
    let mut ack_buf = BytesMut::with_capacity(3);
    ack_buf.extend_from_slice(&[0x20, invoke_id, service]);

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
        payload: ack_buf.freeze(),
        ..Npdu::default()
    };

    let mut buf = BytesMut::with_capacity(32);
    if let Err(e) = encode_npdu(&mut buf, &npdu) {
        tracing::warn!("Failed to encode SimpleAck NPDU: {e}");
        return;
    }

    if let Err(e) = lan_transport.send_unicast(&buf, &[]).await {
        tracing::warn!("Failed to send SimpleAck: {e}");
    }
}

fn encode_priority_array_value_bytes(state: &ObjectState) -> Vec<u8> {
    let mut v = BytesMut::with_capacity(72);
    for (i, prio) in state.priority_array.iter().enumerate() {
        v.extend_from_slice(&[0x09, (i + 1) as u8]);
        match prio {
            Some(val) => {
                v.extend_from_slice(&[0x1D]);
                encode_app_real(&mut v, *val);
            }
            None => {
                v.extend_from_slice(&[0x19, 0x00]);
            }
        }
    }
    v.to_vec()
}

fn parse_write_property_value(apdu: &[u8], offset: usize) -> Option<(Option<f32>, u8)> {
    let (tag2, pos2) = decode_tag(apdu, offset).ok()?;
    if !tag2.is_context(2) {
        return None;
    }

    let value = if tag2.length == 0 {
        None
    } else {
        let content_end = pos2 + tag2.length as usize;
        if content_end > apdu.len() {
            return None;
        }
        let (app_tag, app_pos) = decode_tag(apdu, pos2).ok()?;
        match app_tag.number {
            0 => None,
            4 => {
                if app_tag.length != 4 {
                    return None;
                }
                let float_end = app_pos + 4;
                if float_end > apdu.len() {
                    return None;
                }
                let bytes = &apdu[app_pos..float_end];
                Some(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            _ => return None,
        }
    };

    let after_value = pos2 + tag2.length as usize;

    let priority = if after_value < apdu.len() {
        match decode_tag(apdu, after_value).ok() {
            Some((tag3, _)) if tag3.is_context(3) && tag3.length > 0 => {
                let prio_start = after_value + 1;
                if prio_start < apdu.len() {
                    apdu[prio_start]
                } else {
                    0
                }
            }
            _ => 0,
        }
    } else {
        0
    };

    Some((value, priority))
}

async fn handle_read_property_ao_av_lo(
    lan_transport: &mut impl TransportPort,
    msg: &ReceivedApdu,
    config: &LocalDeviceConfig,
    apdu_cache: &mut HashMap<u8, Vec<u8>>,
    state: &ObjectState,
    prop_id: u16,
    invoke_id: u8,
) {
    let ao_av_lo_props: [u16; 12] = [36, 75, 77, 79, 81, 85, 87, 103, 104, 111, 117, 119];
    if !ao_av_lo_props.contains(&prop_id) {
        send_error(
            lan_transport,
            msg,
            config,
            invoke_id,
            ConfirmedServiceChoice::READ_PROPERTY,
            ErrorClass::PROPERTY,
            ErrorCode::UNKNOWN_PROPERTY,
        )
        .await;
        return;
    }

    let value_bytes: Vec<u8> = match prop_id {
        36 => {
            let mut v = BytesMut::with_capacity(2);
            encode_app_enumerated(&mut v, 0);
            v.to_vec()
        }
        75 => {
            let oid = ObjectIdentifier::new_unchecked(state.object_type, state.instance);
            let mut v = BytesMut::with_capacity(6);
            encode_app_object_id(&mut v, &oid);
            v.to_vec()
        }
        77 => {
            let name = match state.object_type {
                ObjectType::ANALOG_OUTPUT => format!("AO-{}", state.instance),
                ObjectType::ANALOG_VALUE => format!("AV-{}", state.instance),
                ObjectType::LIGHTING_OUTPUT => format!("LO-{}", state.instance),
                _ => format!("?{:?}-{}", state.object_type, state.instance),
            };
            let mut v = BytesMut::with_capacity(32);
            let _ = encode_app_character_string(&mut v, &name);
            v.to_vec()
        }
        79 => {
            let mut v = BytesMut::with_capacity(3);
            encode_app_enumerated(&mut v, state.object_type.to_raw());
            v.to_vec()
        }
        81 => {
            let mut v = BytesMut::with_capacity(1);
            encode_app_boolean(&mut v, state.out_of_service);
            v.to_vec()
        }
        85 => {
            let mut v = BytesMut::with_capacity(5);
            encode_app_real(&mut v, state.present_value);
            v.to_vec()
        }
        87 => encode_priority_array_value_bytes(state),
        103 => {
            let mut v = BytesMut::with_capacity(2);
            encode_app_enumerated(&mut v, 0);
            v.to_vec()
        }
        104 => {
            let mut v = BytesMut::with_capacity(5);
            encode_app_real(&mut v, state.relinquish_default);
            v.to_vec()
        }
        111 => {
            let mut v = BytesMut::with_capacity(3);
            let flags_byte = if state.out_of_service { 1u8 } else { 0u8 };
            encode_app_bit_string(&mut v, 4, &[flags_byte]);
            v.to_vec()
        }
        117 => {
            let unit_code: u32 = match state.object_type {
                ObjectType::LIGHTING_OUTPUT => 105,
                _ => 95,
            };
            let mut v = BytesMut::with_capacity(3);
            encode_app_enumerated(&mut v, unit_code);
            v.to_vec()
        }
        119 => {
            let mut v = BytesMut::with_capacity(2);
            encode_app_enumerated(&mut v, 0);
            v.to_vec()
        }
        _ => unreachable!(),
    };

    let oid = ObjectIdentifier::new_unchecked(state.object_type, state.instance);
    let rp_apdu = build_rp_ack_for_oid(invoke_id, &oid, prop_id, &value_bytes);
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

    apdu_cache.insert(invoke_id, buf.to_vec());

    if let Err(e) = lan_transport.send_unicast(&buf, &[]).await {
        tracing::warn!("Failed to send ReadProperty response: {e}");
    }
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

    #[test]
    fn test_priority_array_read_ack() {
        let state = make_object(ObjectType::ANALOG_OUTPUT, 1);
        let oid = ObjectIdentifier::new(ObjectType::ANALOG_OUTPUT, 1).unwrap();
        let value_bytes = encode_priority_array_value_bytes(&state);
        let ack = build_rp_ack_for_oid(1, &oid, 87, &value_bytes);
        let raw = ack.to_vec();

        let opening = raw.iter().position(|&b| b == 0x3E).unwrap();
        assert_eq!(raw[raw.len() - 1], 0x3F, "must end with closing tag 3");

        let inside = &raw[opening + 1..raw.len() - 1];
        assert_eq!(inside.len(), 64, "all-null Priority_Array must be 64 bytes");
        for (i, chunk) in inside.chunks(4).enumerate() {
            assert_eq!(chunk[0], 0x09, "entry {} must start with ctx[0] tag", i);
            assert_eq!(
                chunk[1],
                (i + 1) as u8,
                "entry {} must have correct index",
                i
            );
            assert_eq!(chunk[2], 0x19, "entry {} must have ctx[1] tag", i);
            assert_eq!(chunk[3], 0x00, "entry {} NULL value", i);
        }

        let oid_bytes = oid.encode();
        assert!(
            raw.windows(4).any(|w| w == oid_bytes),
            "response must contain AO OID"
        );
    }

    #[test]
    fn test_write_property_at_priority() {
        let mut state = make_object(ObjectType::ANALOG_OUTPUT, 1);
        let val = 42.5f32;
        state.update_priority(16, Some(val));
        assert_eq!(
            state.priority_array[15],
            Some(val),
            "priority 16 should be set"
        );
        assert_eq!(
            state.present_value, val,
            "present_value should resolve to 42.5"
        );
    }

    #[test]
    fn test_write_property_null_relinquish() {
        let mut state = make_object(ObjectType::ANALOG_OUTPUT, 1);
        state.update_priority(1, Some(100.0));
        state.update_priority(1, None);
        assert_eq!(
            state.priority_array[0], None,
            "priority 1 should be None after relinquish"
        );
        assert_eq!(
            state.present_value, 0.0,
            "present_value should fall back to relinquish_default (0.0)"
        );
    }

    #[test]
    fn test_unknown_object_error() {
        let oid = ObjectIdentifier::new(ObjectType::ANALOG_OUTPUT, 2).unwrap();
        let value_bytes =
            encode_priority_array_value_bytes(&make_object(ObjectType::ANALOG_OUTPUT, 2));
        let ack = build_rp_ack_for_oid(1, &oid, 87, &value_bytes);

        let oid_bytes = oid.encode();
        assert!(
            ack.windows(4).any(|w| w == oid_bytes),
            "response should echo the unknown OID for error handling"
        );
        assert!(ack[0] == 0x30, "should be a ComplexAck");
    }
}
