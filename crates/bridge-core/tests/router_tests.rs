use bacnet_encoding::npdu::{decode_npdu, encode_npdu, Npdu, NpduAddress};
use bacnet_network::router::{BACnetRouter, RouterPort};
use bacnet_transport::loopback::LoopbackTransport;
use bacnet_transport::port::{ReceivedNpdu, TransportPort};
use bacnet_types::enums::NetworkPriority;
use bacnet_types::MacAddr;
use bytes::{Bytes, BytesMut};
use tokio::sync::mpsc::Receiver;
use tokio::time::Duration;

struct TestHarness {
    _router: BACnetRouter,
    lan_device: LoopbackTransport,
    remote_device: LoopbackTransport,
    lan_rx: Receiver<ReceivedNpdu>,
    remote_rx: Receiver<ReceivedNpdu>,
    lan_mac: MacAddr,
    remote_mac: MacAddr,
}

async fn setup() -> TestHarness {
    let (lan_router, mut lan_device) =
        LoopbackTransport::pair(vec![0x01, 0x01], vec![0x01, 0x02]);
    let (remote_router, mut remote_device) =
        LoopbackTransport::pair(vec![0x02, 0x01], vec![0x02, 0x02]);

    let lan_mac = MacAddr::from_slice(lan_device.local_mac());
    let remote_mac = MacAddr::from_slice(remote_device.local_mac());

    let lan_rx = lan_device.start().await.expect("lan_device start");
    let remote_rx = remote_device.start().await.expect("remote_device start");

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

    let (router, _local_rx) = BACnetRouter::start(ports).await.expect("BACnetRouter::start");
    tokio::time::sleep(Duration::from_millis(100)).await;

    TestHarness {
        _router: router,
        lan_device,
        remote_device,
        lan_rx,
        remote_rx,
        lan_mac,
        remote_mac,
    }
}

impl TestHarness {
    async fn drain_lan(&mut self) -> ReceivedNpdu {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(2), self.lan_rx.recv())
                .await
                .expect("timeout")
                .expect("lan channel closed");
            let decoded = decode_npdu(msg.npdu.clone()).expect("decode");
            if !decoded.is_network_message {
                return msg;
            }
        }
    }

    async fn drain_remote(&mut self) -> ReceivedNpdu {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(2), self.remote_rx.recv())
                .await
                .expect("timeout")
                .expect("remote channel closed");
            let decoded = decode_npdu(msg.npdu.clone()).expect("decode");
            if !decoded.is_network_message {
                return msg;
            }
        }
    }

    async fn send_broadcast_from_remote(&mut self, buf: &[u8]) {
        self.remote_device
            .send_broadcast(buf)
            .await
            .expect("send_broadcast from remote");
    }

    async fn send_broadcast_from_lan(&mut self, buf: &[u8]) {
        self.lan_device
            .send_broadcast(buf)
            .await
            .expect("send_broadcast from lan");
    }

    async fn send_unicast_from_remote(&mut self, buf: &[u8], mac: &MacAddr) {
        self.remote_device
            .send_unicast(buf, mac)
            .await
            .expect("send_unicast from remote");
    }

    async fn stop(mut self) {
        self._router.stop().await;
    }
}

#[tokio::test]
async fn test_who_is_forwarded_net2_to_net1() {
    let mut h = setup().await;

    let who_is_npdu = Npdu {
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

    let mut buf = BytesMut::new();
    encode_npdu(&mut buf, &who_is_npdu).expect("encode Who-Is");
    h.send_broadcast_from_remote(&buf).await;

    let msg = h.drain_lan().await;
    let decoded = decode_npdu(msg.npdu.clone()).expect("decode");

    assert!(decoded.source.is_some(), "forwarded NPDU needs SNET/SADR");
    assert_eq!(decoded.source.unwrap().network, 2, "source network should be 2");

    h.stop().await;
}

#[tokio::test]
async fn test_iam_forwarded_net1_to_net2() {
    let mut h = setup().await;

    let i_am_npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        destination: Some(NpduAddress {
            network: 0xFFFF,
            mac_address: MacAddr::new(),
        }),
        source: None,
        hop_count: 255,
        payload: Bytes::from_static(&[
            0x00, 0x00,
            0xC4, 0x02, 0x00, 0x00, 0x03, 0xE9,
            0x22, 0x05, 0xC4,
            0x91, 0x00,
            0x22, 0x00, 0x0F,
        ]),
        ..Npdu::default()
    };

    let mut buf = BytesMut::new();
    encode_npdu(&mut buf, &i_am_npdu).expect("encode I-Am");
    h.send_broadcast_from_lan(&buf).await;

    let msg = h.drain_remote().await;
    let decoded = decode_npdu(msg.npdu.clone()).expect("decode");

    assert!(decoded.source.is_some(), "forwarded I-Am needs SNET/SADR");
    assert_eq!(decoded.source.unwrap().network, 1, "source network should be 1");

    h.stop().await;
}

#[tokio::test]
async fn test_read_property_unicast_routed() {
    let mut h = setup().await;

    let rp_npdu = Npdu {
        is_network_message: false,
        expecting_reply: false,
        priority: NetworkPriority::NORMAL,
        destination: Some(NpduAddress {
            network: 1,
            mac_address: h.lan_mac.clone(),
        }),
        source: Some(NpduAddress {
            network: 2,
            mac_address: h.remote_mac.clone(),
        }),
        hop_count: 255,
        payload: Bytes::from_static(&[
            0x00, 0x0C,
            0x02, 0x00, 0x00, 0x0C,
            0x0C,
            0x00, 0x00, 0x03, 0xE9,
            0x19, 0x4D,
            0x0F,
        ]),
        ..Npdu::default()
    };

    let mut buf = BytesMut::new();
    encode_npdu(&mut buf, &rp_npdu).expect("encode ReadProperty");
    let lan_mac = h.lan_mac.clone();
    h.send_unicast_from_remote(&buf, &lan_mac).await;

    let fwd = h.drain_lan().await;
    let decoded = decode_npdu(fwd.npdu.clone()).expect("decode");

    assert!(decoded.destination.is_none(), "DNET/DADR stripped for direct net");
    assert!(decoded.source.is_some(), "SNET/SADR set");
    assert_eq!(decoded.source.unwrap().network, 2, "source network should be 2");

    h.stop().await;
}
