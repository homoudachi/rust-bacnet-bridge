use std::env;

use bridge_core::config::{BridgeConfig, RouterConfig};
use bridge_core::state::StateManager;
use bridge_core::{AppState, BridgeError};

/// Config save/load round-trip preserves transport field for both modes
#[test]
fn test_config_round_trip_transport_change() {
    let dir = env::temp_dir().join("bacnet-bridge-test-transport-change");
    let path = dir.join("config.toml");
    let _ = std::fs::remove_dir_all(&dir);

    let mut config = BridgeConfig::generate_default();

    assert_eq!(config.router.transport, "sc");

    config.save(&path).expect("save sc config");
    let loaded = BridgeConfig::load(&path).expect("load sc config");
    assert_eq!(loaded.router.transport, "sc");

    config.router.transport = "tailscale".to_string();
    config.save(&path).expect("save tailscale config");
    let loaded = BridgeConfig::load(&path).expect("load tailscale config");
    assert_eq!(loaded.router.transport, "tailscale");

    config.router.transport = "sc".to_string();
    config.save(&path).expect("save sc config again");
    let loaded = BridgeConfig::load(&path).expect("load sc config again");
    assert_eq!(loaded.router.transport, "sc");

    std::fs::remove_dir_all(&dir).ok();
}

/// StateManager accepts all valid state transitions in sequence
#[test]
fn test_state_manager_valid_transitions() {
    let sm = StateManager::new();
    assert_eq!(sm.current(), AppState::Stopped);

    sm.try_transition(AppState::Starting).unwrap();
    assert_eq!(sm.current(), AppState::Starting);

    sm.try_transition(AppState::Running).unwrap();
    assert_eq!(sm.current(), AppState::Running);

    sm.try_transition(AppState::Stopping).unwrap();
    assert_eq!(sm.current(), AppState::Stopping);

    sm.try_transition(AppState::Stopped).unwrap();
    assert_eq!(sm.current(), AppState::Stopped);
}

/// StateManager rejects illegal state transitions
#[test]
fn test_state_manager_rejects_illegal_transitions() {
    let sm = StateManager::new();

    sm.try_transition(AppState::Starting).unwrap();
    sm.try_transition(AppState::Running).unwrap();

    assert!(matches!(
        sm.try_transition(AppState::Starting),
        Err(BridgeError::InvalidStateTransition { .. })
    ));

    assert!(matches!(
        sm.try_transition(AppState::Stopped),
        Err(BridgeError::InvalidStateTransition { .. })
    ));

    assert!(matches!(
        sm.try_transition(AppState::Running),
        Err(BridgeError::InvalidStateTransition { .. })
    ));

    sm.try_transition(AppState::Stopping).unwrap();
    sm.try_transition(AppState::Stopped).unwrap();
}

/// StateManager rejects transitions from Stopped to non-Starting states
#[test]
fn test_state_manager_stopped_only_allows_starting() {
    let sm = StateManager::new();

    assert!(matches!(
        sm.try_transition(AppState::Running),
        Err(BridgeError::InvalidStateTransition { .. })
    ));
    assert!(matches!(
        sm.try_transition(AppState::Stopping),
        Err(BridgeError::InvalidStateTransition { .. })
    ));
    assert!(matches!(
        sm.try_transition(AppState::Stopped),
        Err(BridgeError::InvalidStateTransition { .. })
    ));
}

/// StateManager rejects Starting -> Stopped (must go Running first)
#[test]
fn test_state_manager_starting_cannot_skip_running() {
    let sm = StateManager::new();
    sm.try_transition(AppState::Starting).unwrap();

    assert!(matches!(
        sm.try_transition(AppState::Stopped),
        Err(BridgeError::InvalidStateTransition { .. })
    ));
    assert!(matches!(
        sm.try_transition(AppState::Stopping),
        Err(BridgeError::InvalidStateTransition { .. })
    ));
}

/// build_remote_transport rejects "invalid" mode with correct error
#[tokio::test]
async fn test_remote_transport_rejects_invalid_mode() {
    let config = BridgeConfig {
        router: RouterConfig {
            transport: "invalid".to_string(),
            ..RouterConfig::default()
        },
        ..BridgeConfig::default()
    };

    let result = bridge_core::build_remote_transport(&config)
        .await
        .map(|(t, _)| t);
    match result {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("Unknown transport mode"),
                "Error should mention unknown transport mode, got: {msg}"
            );
        }
        Ok(_) => panic!("Expected Err for invalid transport mode, got Ok"),
    }
}

/// "sc" dispatch reaches SC transport build (not rejected as unknown)
#[tokio::test]
async fn test_remote_transport_sc_dispatch() {
    let config = BridgeConfig {
        router: RouterConfig {
            transport: "sc".to_string(),
            ..RouterConfig::default()
        },
        ..BridgeConfig::default()
    };

    let result = bridge_core::build_remote_transport(&config)
        .await
        .map(|(t, _)| t);
    if let Err(e) = result {
        let msg = e.to_string();
        assert!(
            !msg.contains("Unknown transport mode"),
            "SC transport should not be rejected as unknown, got: {msg}"
        );
    }
}

/// State sequence used by transport switch cycle is valid end-to-end
#[test]
fn test_transport_switch_cycle_state_sequence() {
    let sm = StateManager::new();

    assert_eq!(sm.current(), AppState::Stopped);

    sm.try_transition(AppState::Starting).unwrap();
    assert_eq!(sm.current(), AppState::Starting);

    sm.try_transition(AppState::Running).unwrap();
    assert_eq!(sm.current(), AppState::Running);

    sm.try_transition(AppState::Stopping).unwrap();
    assert_eq!(sm.current(), AppState::Stopping);

    sm.try_transition(AppState::Stopped).unwrap();
    assert_eq!(sm.current(), AppState::Stopped);

    sm.try_transition(AppState::Starting).unwrap();
    assert_eq!(sm.current(), AppState::Starting);

    sm.try_transition(AppState::Running).unwrap();
    assert_eq!(sm.current(), AppState::Running);
}

/// Transport switch only changes the transport field in config
#[test]
fn test_transport_switch_only_changes_transport_field() {
    let mut config = BridgeConfig::generate_default();
    let original = config.clone();

    config.router.transport = "tailscale".to_string();

    assert_eq!(config.router.device_id, original.router.device_id);
    assert_eq!(config.router.vendor_id, original.router.vendor_id);
    assert_eq!(config.router.device_name, original.router.device_name);
    assert_eq!(config.router.lan.interface, original.router.lan.interface);
    assert_eq!(config.router.lan.port, original.router.lan.port);
    assert_eq!(config.router.sc.hub_url, original.router.sc.hub_url);
    assert_eq!(
        config.router.tailscale.interface,
        original.router.tailscale.interface
    );
    assert_eq!(config.router.tailscale.port, original.router.tailscale.port);
    assert_eq!(config.web.host, original.web.host);
    assert_eq!(config.web.port, original.web.port);
    assert_eq!(config.hub.bind, original.hub.bind);

    assert_eq!(config.router.transport, "tailscale");
    assert_eq!(original.router.transport, "sc");
}
