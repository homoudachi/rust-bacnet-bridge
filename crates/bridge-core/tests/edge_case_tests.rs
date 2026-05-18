use bridge_core::config::BridgeConfig;
use bridge_core::error::BridgeError;
use bridge_core::state::{AppState, StateManager};

// Edge case 1: Maximum valid device_id
#[test]
fn test_validate_max_device_id_passes() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.device_id = 4194303;
    assert!(config.validate().is_ok());
}

// Edge case 2: Just over max device_id fails
#[test]
fn test_validate_device_id_just_over_max_fails() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.device_id = 4194304;
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("device_id"));
}

// Edge case 3: Whitespace-only device_name
#[test]
fn test_validate_whitespace_device_name_fails() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.device_name = "   ".to_string();
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("device_name"));
}

// Edge case 4: CIDR notation in interface field should work
#[test]
fn test_validate_cidr_notation_in_interface() {
    let mut config = BridgeConfig::generate_default();
    config.router.transport = "tailscale".to_string();
    config.router.tailscale.interface = "100.64.0.1/32".to_string();
    assert!(config.validate().is_ok());

    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.lan.interface = "192.168.1.0/24".to_string();
    assert!(config.validate().is_ok());
}

// Edge case 5: Very large reconnect values
#[test]
fn test_validate_large_reconnect_values() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.sc.reconnect_initial_ms = 100000;
    config.router.sc.reconnect_max_ms = 3600000;
    config.router.sc.reconnect_max_attempts = u32::MAX;
    assert!(config.validate().is_ok());
}

// Edge case 6: Transport "sc" is case-sensitive (only lowercase valid)
#[test]
fn test_validate_transport_is_case_sensitive() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.transport = "SC".to_string();
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("router.transport"));
}

// Edge case 7: State transitions - Starting -> Stopped (failure rollback) is valid
#[test]
fn test_starting_to_stopped_transition_is_valid() {
    let sm = StateManager::new();
    sm.try_transition(AppState::Starting).unwrap();
    assert!(sm.try_transition(AppState::Stopped).is_ok());
    assert_eq!(sm.current(), AppState::Stopped);
}

// Edge case 8: Double transition to same state fails
#[test]
fn test_double_transition_to_same_state_fails() {
    let sm = StateManager::new();
    sm.try_transition(AppState::Starting).unwrap();
    assert!(sm.try_transition(AppState::Starting).is_err());

    sm.try_transition(AppState::Running).unwrap();
    assert!(sm.try_transition(AppState::Running).is_err());
}

// Edge case 9: StateManager subscribe works after multiple transitions
#[test]
fn test_subscribe_tracks_multiple_transitions() {
    let sm = StateManager::new();
    let rx = sm.subscribe();

    assert_eq!(*rx.borrow(), AppState::Stopped);

    sm.try_transition(AppState::Starting).unwrap();
    assert_eq!(*rx.borrow(), AppState::Starting);

    sm.try_transition(AppState::Stopped).unwrap();
    assert_eq!(*rx.borrow(), AppState::Stopped);

    sm.try_transition(AppState::Starting).unwrap();
    sm.try_transition(AppState::Running).unwrap();
    assert_eq!(*rx.borrow(), AppState::Running);
}

// Edge case 10: FDT add with duplicate entry overwrites
#[test]
fn test_fdt_add_duplicate_overwrites() {
    let mut fdt = bridge_core::fdt::FdtManager::new();
    fdt.add([10, 0, 0, 1], 47808, 30);
    assert_eq!(fdt.len(), 1);
    fdt.add([10, 0, 0, 1], 47808, 60);
    assert_eq!(fdt.len(), 1);
    let entries = fdt.list();
    assert_eq!(entries[0].ttl, 60);
}

// Edge case 11: FDT remove non-existent entry is no-op
#[test]
fn test_fdt_remove_nonexistent_is_noop() {
    let mut fdt = bridge_core::fdt::FdtManager::new();
    fdt.add([10, 0, 0, 1], 47808, 30);
    assert_eq!(fdt.len(), 1);
    fdt.remove([10, 0, 0, 2], 47808);
    assert_eq!(fdt.len(), 1);
}

// Edge case 12: FDT tick with no expired entries
#[test]
fn test_fdt_tick_preserves_fresh_entries() {
    let mut fdt = bridge_core::fdt::FdtManager::new();
    fdt.add([10, 0, 0, 1], 47808, 300);
    assert_eq!(fdt.len(), 1);
    fdt.tick(); // 2s off the 300s TTL, should still be alive
    assert_eq!(fdt.len(), 1);
}

// Edge case 13: FDT with max entries
#[test]
fn test_fdt_many_entries() {
    let mut fdt = bridge_core::fdt::FdtManager::new();
    for i in 0..50 {
        let a = (i / 256) as u8;
        let b = (i % 256) as u8;
        fdt.add([10, 0, a, b], 47808, 60);
    }
    assert_eq!(fdt.len(), 50);
}

// Edge case 14: Config validation - valid config passes completely
#[test]
fn test_validate_fully_valid_sc_config() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com:443".to_string();
    config.router.device_id = 123;
    config.router.device_name = "My Bridge".to_string();
    config.router.lan.interface = "192.168.1.50".to_string();
    config.router.lan.port = 47808;
    config.web.port = 8080;
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_fully_valid_tailscale_config() {
    let mut config = BridgeConfig::generate_default();
    config.router.transport = "tailscale".to_string();
    config.router.tailscale.interface = "100.64.0.1".to_string();
    config.router.tailscale.port = 20000;
    config.router.device_id = 999;
    config.router.device_name = "Tailscale Bridge".to_string();
    config.router.lan.port = 47809;
    config.web.port = 28821;
    assert!(config.validate().is_ok());
}

// Edge case 15: Error enums implement Debug and Display
#[test]
fn test_bridge_error_debug_and_display() {
    let errors = vec![
        BridgeError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "test")),
        BridgeError::TomlDeserialize(toml::from_str::<toml::Value>("invalid").unwrap_err()),
        BridgeError::Hub("hub down".to_string()),
        BridgeError::InvalidStateTransition {
            from: "X".into(),
            to: "Y".into(),
        },
        BridgeError::StateSync,
        BridgeError::Router("router error".to_string()),
        BridgeError::Transport("transport error".to_string()),
        BridgeError::ConfigValidation("bad config".to_string()),
    ];
    for err in &errors {
        let debug_str = format!("{:?}", err);
        assert!(!debug_str.is_empty());
        let display_str = format!("{}", err);
        assert!(!display_str.is_empty());
    }
}

// Edge case 16: ScConfig reconnect_max_attempts = 0 means infinite
#[test]
fn test_sc_reconnect_attempts_zero_is_infinite() {
    let config = BridgeConfig::generate_default();
    assert_eq!(config.router.sc.reconnect_max_attempts, 0);
    // 0 should pass validation (means infinite retries)
    let mut c = config.clone();
    c.router.sc.hub_url = "wss://hub.example.com".to_string();
    assert!(c.validate().is_ok());
}

// Edge case 17: Reconnect initial_ms = 0 should fail validation
#[test]
fn test_sc_reconnect_initial_ms_zero_fails() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.sc.reconnect_initial_ms = 0;
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("reconnect_initial_ms"));
}

// Edge case 18: danger_accept_invalid_certs flag with sc mode
#[test]
fn test_sc_danger_accept_invalid_certs_validates() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.sc.danger_accept_invalid_certs = true;
    assert!(config.validate().is_ok());
}

// Edge case 19: State display strings
#[test]
fn test_app_state_display() {
    assert_eq!(AppState::Stopped.to_string(), "Stopped");
    assert_eq!(AppState::Starting.to_string(), "Starting");
    assert_eq!(AppState::Running.to_string(), "Running");
    assert_eq!(AppState::Stopping.to_string(), "Stopping");
}

// Edge case 20: LanConfig default port is standard BACnet
#[test]
fn test_lan_config_default_port_is_bacnet_standard() {
    let config = BridgeConfig::generate_default();
    assert_eq!(config.router.lan.port, 47808); // 0xBAC0
}

// Edge case 21: WebConfig default port
#[test]
fn test_web_default_port_is_28821() {
    let config = BridgeConfig::generate_default();
    assert_eq!(config.web.port, 28821);
    // Should be valid
    config.validate().unwrap_err(); // fails only because of missing hub_url
}
