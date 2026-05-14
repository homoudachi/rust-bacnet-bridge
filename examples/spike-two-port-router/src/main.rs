//! Spike: prove BACnetRouter forwards broadcasts and learns routes between
//! two loopback transports.
//!
//! Topology:
//!   [LAN device] ←→ (lan_a) Router (remote_a) ←→ [Remote device]
//!     net 1                                    net 2
//!
//! Test 1: Who-Is broadcast from remote device → appears on LAN device
//! Test 2: I-Am broadcast from LAN device → forwarded to remote device
//! Test 3: ReadProperty unicast from remote → routed to LAN (strips DNET/DADR)

use bacnet_encoding::npdu::{decode_npdu, encode_npdu, Npdu, NpduAddress};
use bacnet_network::router::{BACnetRouter, RouterPort};
use bacnet_transport::loopback::LoopbackTransport;
use bacnet_transport::port::{ReceivedNpdu, TransportPort};
use bacnet_types::{enums::NetworkPriority, MacAddr};
use bytes::{Bytes, BytesMut};
use tokio::sync::mpsc::Receiver;
use tokio::time::Duration;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tracing::info!("=== SPIKE: Two-port BACnetRouter with loopback transports ===");

    // --- Build transport pairs ---
    let (lan_router, mut lan_device) =
        LoopbackTransport::pair(vec![0x01, 0x01], vec![0x01, 0x02]);
    let (remote_router, mut remote_device) =
        LoopbackTransport::pair(vec![0x02, 0x01], vec![0x02, 0x02]);

    let lan_device_mac = MacAddr::from_slice(lan_device.local_mac());
    let remote_device_mac = MacAddr::from_slice(remote_device.local_mac());

    // Start device-side transports
    let mut lan_rx: Receiver<ReceivedNpdu> = lan_device.start().await.expect("lan_device start");
    let mut remote_rx: Receiver<ReceivedNpdu> =
        remote_device.start().await.expect("remote_device start");

    // --- Start router ---
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

    let (mut router, _local_rx) = BACnetRouter::start(ports)
        .await
        .expect("BACnetRouter::start");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify routing table
    {
        let table = router.table().lock().await;
        assert_eq!(table.len(), 2, "expected 2 direct routes");
        let net1 = table.lookup(1).expect("network 1 not found");
        let net2 = table.lookup(2).expect("network 2 not found");
        assert!(net1.directly_connected);
        assert!(net2.directly_connected);
        tracing::info!("Router table: net1 → port {}, net2 → port {}", net1.port_index, net2.port_index);
    }

    // ==================================================================
    // Test 1: Who-Is global broadcast from remote → appears on LAN
    // ==================================================================
    tracing::info!("--- Test 1: Who-Is global broadcast (DNET=FFFF) ---");

    let who_is_npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        destination: Some(NpduAddress {
            network: 0xFFFF, // global broadcast — forwarded to ALL other ports
            mac_address: MacAddr::new(),
        }),
        source: None, // router fills this in
        hop_count: 255,
        payload: Bytes::from_static(&[0x10, 0x08]), // UnconfirmedReq, Who-Is
        ..Npdu::default()
    };

    let mut buf = BytesMut::new();
    encode_npdu(&mut buf, &who_is_npdu).expect("encode Who-Is");
    remote_device.send_broadcast(&buf).await.expect("send Who-Is");

    // Drain all messages from lan_rx; skip I-Am-Router announcements
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(2), lan_rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");
        let decoded = decode_npdu(msg.npdu.clone()).expect("decode");
        eprintln!(
            "  rx: network_msg={}, src={:?}, dest={:?}, payload_len={}",
            decoded.is_network_message, decoded.source, decoded.destination, msg.npdu.len()
        );
        // Skip I-Am-Router announcements (network messages)
        if !decoded.is_network_message {
            assert!(decoded.source.is_some(), "forwarded NPDU needs SNET/SADR");
            assert_eq!(decoded.source.unwrap().network, 2);
            break;
        }
    }
    tracing::info!("PASS: Who-Is forwarded net2→net1");

    // ==================================================================
    // Test 2: I-Am global broadcast from LAN → forwarded to remote
    // ==================================================================
    tracing::info!("--- Test 2: I-Am global broadcast cross-forwarding ---");

    let i_am_npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        destination: Some(NpduAddress {
            network: 0xFFFF, // global broadcast
            mac_address: MacAddr::new(),
        }),
        source: None,
        hop_count: 255,
        payload: Bytes::from_static(&[
            0x00, 0x00,                         // I-Am (confirmed)
            0xC4, 0x02, 0x00, 0x00, 0x03, 0xE9, // device 1001
            0x22, 0x05, 0xC4,                    // maxAPDU 1476
            0x91, 0x00,                          // no segmentation
            0x22, 0x00, 0x0F,                    // vendor 15
        ]),
        ..Npdu::default()
    };

    let mut buf = BytesMut::new();
    encode_npdu(&mut buf, &i_am_npdu).expect("encode I-Am");
    lan_device.send_broadcast(&buf).await.expect("send I-Am");

    // Drain I-Am-Router announcements from remote_rx
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(2), remote_rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");
        let decoded = decode_npdu(msg.npdu.clone()).expect("decode");
        if !decoded.is_network_message {
            assert!(decoded.source.is_some(), "forwarded I-Am needs SNET/SADR");
            assert_eq!(decoded.source.unwrap().network, 1);
            break;
        }
    }
    tracing::info!("PASS: I-Am forwarded net1→net2");

    // ==================================================================
    // Test 3: ReadProperty unicast remote→LAN (via routing table)
    // ==================================================================
    tracing::info!("--- Test 3: ReadProperty unicast routing ---");

    let rp_npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        destination: Some(NpduAddress {
            network: 1,
            mac_address: lan_device_mac.clone(),
        }),
        source: Some(NpduAddress {
            network: 2,
            mac_address: remote_device_mac.clone(),
        }),
        hop_count: 255,
        payload: Bytes::from_static(&[
            0x00, 0x0C,                 // ConfirmedReq, ReadProperty
            0x02, 0x00, 0x00, 0x0C,     // invoke id 12
            0x0C,                       // opening tag
            0x00, 0x00, 0x03, 0xE9,     // object 1001
            0x19, 0x4D,                 // property PRESENT_VALUE (77)
            0x0F,                       // closing tag
        ]),
        ..Npdu::default()
    };

    let mut buf = BytesMut::new();
    encode_npdu(&mut buf, &rp_npdu).expect("encode ReadProperty");
    remote_device
        .send_unicast(&buf, lan_device.local_mac())
        .await
        .expect("send ReadProperty");

    let fwd = tokio::time::timeout(Duration::from_secs(2), lan_rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");
    let decoded = decode_npdu(fwd.npdu.clone()).expect("decode");

    // Router strips DNET/DADR for direct network, sets SNET/SADR
    assert!(decoded.destination.is_none(), "DNET/DADR stripped for direct net");
    assert!(decoded.source.is_some(), "SNET/SADR set");
    assert_eq!(decoded.source.unwrap().network, 2);
    tracing::info!("PASS: ReadProperty routed remote→LAN (DNET stripped, SNET=2)");

    // Cleanup
    router.stop().await;
    tracing::info!("=== SPIKE: all tests passed ===");
}
