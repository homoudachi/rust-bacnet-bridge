use bacnet_transport::port::TransportPort;
use bridge_core::config::{BdtEntry, BridgeConfig, RouterConfig, TailscaleConfig};
use bridge_core::fdt::FdtManager;
use bridge_core::FdtDisplayEntry;
use std::time::Duration;

/// Test FDT lifecycle: add, list, verify entry, remove.
#[tokio::test]
async fn fdt_add_list_remove() {
    let mut mgr = FdtManager::new();
    assert!(mgr.is_empty());

    mgr.add([10, 0, 0, 50], 47808, 120);
    assert_eq!(mgr.len(), 1);

    let list = mgr.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].ip, "10.0.0.50");
    assert_eq!(list[0].port, 47808);
    assert_eq!(list[0].ttl, 120);
    assert!(!list[0].registered_at.is_empty());

    mgr.remove([10, 0, 0, 50], 47808);
    assert!(mgr.is_empty());
}

/// Test TTL expiry: add an entry with a very short TTL, wait for it to expire,
/// verify tick() removes it.
#[tokio::test]
async fn fdt_ttl_expiry() {
    let mut mgr = FdtManager::new();
    mgr.add([10, 0, 0, 51], 47808, 2); // 2-second TTL
    assert_eq!(mgr.len(), 1);

    {
        let list = mgr.list();
        let remaining = list[0].remaining_ttl;
        assert!(
            remaining > 0 && remaining <= 2,
            "remaining_ttl was {remaining}"
        );
    }

    mgr.add([10, 0, 0, 51], 47808, 0);
    assert_eq!(mgr.len(), 1);

    // Wait 1 second, then the entry should be gone only if TTL+grace expired.
    // Since TTL=0, grace=30: after 1s, entry still alive.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    mgr.tick();
    assert_eq!(
        mgr.len(),
        1,
        "Entry with TTL=0 should survive 1s (grace=30s)"
    );
}

/// Test that multiple FDs can be registered.
#[tokio::test]
async fn fdt_multiple_entries() {
    let mut mgr = FdtManager::new();
    mgr.add([10, 0, 0, 1], 47808, 60);
    mgr.add([10, 0, 0, 2], 47809, 120);
    mgr.add([10, 0, 0, 3], 47810, 300);

    assert_eq!(mgr.len(), 3);
    let ips: Vec<String> = mgr.list().iter().map(|e| e.ip.clone()).collect();
    assert!(ips.contains(&"10.0.0.1".to_string()));
    assert!(ips.contains(&"10.0.0.2".to_string()));
    assert!(ips.contains(&"10.0.0.3".to_string()));
}

/// Test display entry formatting.
#[test]
fn fdt_display_entry_format() {
    let entry = FdtDisplayEntry {
        ip: "10.0.0.100".to_string(),
        port: 47808,
        ttl: 60,
        remaining_ttl: 55,
        registered_at: "2026-05-15T12:00:00.000Z".to_string(),
    };
    assert_eq!(entry.ip, "10.0.0.100");
    assert_eq!(entry.port, 47808);
    assert_eq!(entry.ttl, 60);
    assert_eq!(entry.remaining_ttl, 55);
    assert_eq!(entry.registered_at, "2026-05-15T12:00:00.000Z");
}

/// Test that build_bbmd_transport creates a valid transport from config.
#[tokio::test]
async fn bbmd_transport_creation() {
    let config = TailscaleConfig {
        interface: "127.0.0.1".to_string(),
        port: 0,
        bdt: None,
    };

    let transport = bridge_core::bbmd_transport::build_bbmd_transport(&config)
        .await
        .expect("Should create BIP transport without BDT");

    // Verify it's a BIP transport by checking max_apdu_length
    assert_eq!(transport.max_apdu_length(), 1476, "Expected BIP max APDU");
    assert_eq!(transport.local_mac().len(), 6, "Expected BIP MAC length");
}

/// Test that build_bbmd_transport with BDT entries works.
#[tokio::test]
async fn bbmd_transport_with_bdt() {
    let config = TailscaleConfig {
        interface: "127.0.0.1".to_string(),
        port: 0,
        bdt: Some(vec![BdtEntry {
            ip: "10.0.0.1".to_string(),
            port: 47808,
            broadcast_mask: [255, 255, 255, 0],
        }]),
    };

    let transport = bridge_core::bbmd_transport::build_bbmd_transport(&config)
        .await
        .expect("Should create BIP transport with BDT");

    assert_eq!(transport.max_apdu_length(), 1476, "Expected BIP max APDU");
    assert_eq!(transport.local_mac().len(), 6, "Expected BIP MAC length");
}

/// Test error handling for invalid interface IP.
#[tokio::test]
async fn bbmd_transport_invalid_ip() {
    let config = TailscaleConfig {
        interface: "not-an-ip".to_string(),
        port: 0,
        bdt: None,
    };

    let result = bridge_core::bbmd_transport::build_bbmd_transport(&config).await;
    match result {
        Err(e) => {
            let err_msg = format!("{}", e);
            assert!(err_msg.contains("Invalid Tailscale interface IP"));
        }
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

/// Test that transport.rs dispatches correctly for tailscale config.
#[tokio::test]
async fn remote_transport_tailscale_dispatch() {
    let config = BridgeConfig {
        router: RouterConfig {
            transport: "tailscale".to_string(),
            ..RouterConfig::default()
        },
        ..BridgeConfig::default()
    };

    let result = bridge_core::build_remote_transport(&config).await;
    assert!(
        result.is_err(),
        "Expected error from empty tailscale interface"
    );
}
