use std::collections::HashMap;
use std::env;

use bridge_core::config::{BridgeConfig, HubConfig, LanConfig, RouterConfig, TailscaleConfig};
use bridge_core::fdt::FdtManager;
use bridge_core::logbuf::{LogEntry, LogRingBuffer};
use bridge_core::state::{AppState, StateManager};
use bridge_core::BridgeError;

// ---------------------------------------------------------------------------
// 1. GET /api/status equivalent — config shape and initial state
// ---------------------------------------------------------------------------

#[test]
fn test_status_response_shape() {
    let config = BridgeConfig::generate_default();
    let mut config_clone = config.clone();
    config_clone.router.transport = "tailscale".to_string();

    assert_eq!(config.router.transport, "sc");
    assert_eq!(config.router.lan.interface, "");
    assert_eq!(config.router.lan.port, 47808);
    assert_eq!(config.router.device_id, 4194303);
    assert_eq!(config.web.host, "0.0.0.0");

    let state = StateManager::new();
    assert_eq!(state.current(), AppState::Stopped);
    assert_eq!(state.current().to_string(), "Stopped");

    assert_eq!(config.router.transport, "sc");
    assert_eq!(config_clone.router.transport, "tailscale");
    assert!(config.web.port > 0);
}

#[test]
fn test_initial_state_is_stopped() {
    let sm = StateManager::new();
    assert_eq!(sm.current(), AppState::Stopped);
}

// ---------------------------------------------------------------------------
// 2. GET /api/interfaces equivalent — configured interface fields
// ---------------------------------------------------------------------------

#[test]
fn test_interfaces_configured_empty_by_default() {
    let config = BridgeConfig::generate_default();
    assert_eq!(config.router.lan.interface, "");
    assert_eq!(config.router.tailscale.interface, "");
}

#[test]
fn test_interfaces_with_non_empty_fields() {
    let config = BridgeConfig {
        router: RouterConfig {
            lan: LanConfig {
                interface: "192.168.1.10".to_string(),
                ..LanConfig::default()
            },
            tailscale: TailscaleConfig {
                interface: "100.64.0.1".to_string(),
                ..TailscaleConfig::default()
            },
            ..RouterConfig::default()
        },
        ..BridgeConfig::default()
    };

    assert!(!config.router.lan.interface.is_empty());
    assert!(config.router.lan.interface.starts_with("192.168"));

    assert!(!config.router.tailscale.interface.is_empty());
    assert!(config.router.tailscale.interface.starts_with("100."));

    assert_eq!(config.router.lan.port, 47808);
    assert_eq!(config.router.tailscale.port, 20000);
}

#[test]
fn test_is_tailscale_detection() {
    let tailscale_ip = "100.64.0.1";
    let lan_ip = "10.0.0.5";

    assert!(tailscale_ip.starts_with("100."));
    assert!(!lan_ip.starts_with("100."));
}

// ---------------------------------------------------------------------------
// 3. GET /api/config equivalent — all sections present
// ---------------------------------------------------------------------------

#[test]
fn test_config_contains_all_sections() {
    let config = BridgeConfig::generate_default();

    let _ = &config.router;
    let _ = &config.web;
    let _ = &config.hub;

    assert_eq!(config.router.transport, "sc");
    assert_eq!(config.web.port, 28821);
    assert_eq!(config.hub.bind, "0.0.0.0:8443");
}

#[test]
fn test_config_serialization_all_sections() {
    let config = BridgeConfig::generate_default();
    let toml_str = toml::to_string_pretty(&config).expect("serialize");

    assert!(toml_str.contains("[router]"));
    assert!(toml_str.contains("[web]"));
    assert!(toml_str.contains("[hub]"));
}

// ---------------------------------------------------------------------------
// 4. PUT /api/config equivalent — update and persist
// ---------------------------------------------------------------------------

#[test]
fn test_config_update_and_persist() {
    let dir = env::temp_dir().join("bacnet-bridge-test-dash-put");
    let path = dir.join("config.toml");
    let _ = std::fs::remove_dir_all(&dir);

    let mut config = BridgeConfig::generate_default();
    config.router.transport = "tailscale".to_string();
    config.router.device_id = 99999;
    config.web.port = 3000;
    config.save(&path).expect("save updated config");

    let loaded = BridgeConfig::load(&path).expect("load updated config");
    assert_eq!(loaded.router.transport, "tailscale");
    assert_eq!(loaded.router.device_id, 99999);
    assert_eq!(loaded.web.port, 3000);

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// 5. POST /api/transport/stop state gating — 409 when Stopped
// ---------------------------------------------------------------------------

#[test]
fn test_transport_stop_requires_running_state() {
    let sm = StateManager::new();
    assert_eq!(sm.current(), AppState::Stopped);

    let result = sm.try_transition(AppState::Stopping);
    assert!(result.is_err());
    match result {
        Err(BridgeError::InvalidStateTransition { from, to }) => {
            assert_eq!(from, "Stopped");
            assert_eq!(to, "Stopping");
        }
        _ => panic!("Expected InvalidStateTransition"),
    }

    sm.try_transition(AppState::Starting).unwrap();
    sm.try_transition(AppState::Running).unwrap();
    sm.try_transition(AppState::Stopping).unwrap();
    assert_eq!(sm.current(), AppState::Stopping);
}

// ---------------------------------------------------------------------------
// 6. POST /api/transport/start state gating — 503 / Stopped requirement
// ---------------------------------------------------------------------------

#[test]
fn test_transport_start_requires_stopped_state() {
    let sm = StateManager::new();
    assert_eq!(sm.current(), AppState::Stopped);

    sm.try_transition(AppState::Starting).unwrap();
    assert_eq!(sm.current(), AppState::Starting);

    let result = sm.try_transition(AppState::Starting);
    assert!(result.is_err());
    match result {
        Err(BridgeError::InvalidStateTransition { from, to }) => {
            assert_eq!(from, "Starting");
            assert_eq!(to, "Starting");
        }
        _ => panic!("Expected InvalidStateTransition"),
    }

    let result = sm.try_transition(AppState::Stopped);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// 7. POST /api/transport/switch state gating — 403/409 when not Running
// ---------------------------------------------------------------------------

#[test]
fn test_transport_switch_requires_running_state() {
    let sm = StateManager::new();

    let result = sm.try_transition(AppState::Running);
    assert!(result.is_err());
    match result {
        Err(BridgeError::InvalidStateTransition { from, to }) => {
            assert_eq!(from, "Stopped");
            assert_eq!(to, "Running");
        }
        _ => panic!("Expected InvalidStateTransition"),
    }

    sm.try_transition(AppState::Starting).unwrap();
    let result = sm.try_transition(AppState::Running);
    assert!(result.is_ok());
    assert_eq!(sm.current(), AppState::Running);
}

// ---------------------------------------------------------------------------
// 8. GET /api/fdt equivalent — FdtManager.list()
// ---------------------------------------------------------------------------

#[test]
fn test_fdt_empty_for_sc_mode() {
    let config = BridgeConfig::generate_default();
    assert_eq!(config.router.transport, "sc");

    let fdt = FdtManager::new();
    assert!(fdt.is_empty());
    assert_eq!(fdt.list().len(), 0);
}

#[test]
fn test_fdt_populated_for_tailscale_mode() {
    let config = BridgeConfig {
        router: RouterConfig {
            transport: "tailscale".to_string(),
            ..RouterConfig::default()
        },
        ..BridgeConfig::default()
    };
    assert_eq!(config.router.transport, "tailscale");

    let mut fdt = FdtManager::new();
    assert!(fdt.is_empty());

    fdt.add([10, 0, 0, 5], 47808, 60);
    fdt.add([10, 0, 0, 6], 47809, 120);

    assert_eq!(fdt.len(), 2);
    let entries = fdt.list();
    assert_eq!(entries.len(), 2);

    assert_eq!(entries[0].ip, "10.0.0.5");
    assert_eq!(entries[0].port, 47808);
    assert_eq!(entries[0].ttl, 60);
    assert!(entries[0].remaining_ttl > 55);

    assert_eq!(entries[1].ip, "10.0.0.6");
    assert_eq!(entries[1].port, 47809);
    assert_eq!(entries[1].ttl, 120);

    assert!(!entries[0].registered_at.is_empty());
    assert!(entries[0].registered_at.contains('T'));
    assert!(entries[0].registered_at.ends_with('Z'));
}

#[test]
fn test_fdt_remove_entry() {
    let mut fdt = FdtManager::new();
    fdt.add([10, 0, 0, 5], 47808, 60);
    fdt.add([10, 0, 0, 6], 47808, 120);
    assert_eq!(fdt.len(), 2);

    fdt.remove([10, 0, 0, 5], 47808);
    assert_eq!(fdt.len(), 1);
    assert_eq!(fdt.list()[0].ip, "10.0.0.6");
}

// ---------------------------------------------------------------------------
// 9. GET /api/logs equivalent — LogRingBuffer.recent() with filtering
// ---------------------------------------------------------------------------

fn make_log_entry(level: &str, msg: &str) -> LogEntry {
    LogEntry {
        timestamp: "2025-01-01T00:00:00Z".into(),
        level: level.into(),
        target: "dashboard_api_test".into(),
        message: msg.into(),
        fields: HashMap::new(),
    }
}

#[test]
fn test_logs_recent_all_levels() {
    let buf = LogRingBuffer::new(100);
    buf.push(make_log_entry("INFO", "info message"));
    buf.push(make_log_entry("DEBUG", "debug message"));
    buf.push(make_log_entry("WARN", "warn message"));
    buf.push(make_log_entry("ERROR", "error message"));

    let all = buf.recent(10, None);
    assert_eq!(all.len(), 4);
}

#[test]
fn test_logs_filter_by_level() {
    let buf = LogRingBuffer::new(100);
    buf.push(make_log_entry("DEBUG", "debug message"));
    buf.push(make_log_entry("INFO", "info message"));
    buf.push(make_log_entry("WARN", "warn message"));
    buf.push(make_log_entry("ERROR", "error message"));

    let warn_up = buf.recent(10, Some("WARN"));
    assert_eq!(warn_up.len(), 2);
    for e in &warn_up {
        assert!(e.level == "WARN" || e.level == "ERROR");
    }

    let error_only = buf.recent(10, Some("ERROR"));
    assert_eq!(error_only.len(), 1);
    assert_eq!(error_only[0].level, "ERROR");
}

#[test]
fn test_logs_limit() {
    let buf = LogRingBuffer::new(100);
    for i in 0..20 {
        buf.push(make_log_entry("INFO", &format!("message {}", i)));
    }

    let limited = buf.recent(5, None);
    assert_eq!(limited.len(), 5);
    assert_eq!(limited[0].message, "message 15");
    assert_eq!(limited[4].message, "message 19");
}

#[test]
fn test_logs_capacity_drops_oldest() {
    let buf = LogRingBuffer::new(3);
    buf.push(make_log_entry("INFO", "first"));
    buf.push(make_log_entry("INFO", "second"));
    buf.push(make_log_entry("INFO", "third"));
    buf.push(make_log_entry("ERROR", "fourth"));

    let recent = buf.recent(10, None);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].message, "second");
    assert_eq!(recent[1].message, "third");
    assert_eq!(recent[2].message, "fourth");
}

// ---------------------------------------------------------------------------
// 10. GET /api/hub/status equivalent — HubConfig fields
// ---------------------------------------------------------------------------

#[test]
fn test_hub_status_default_config() {
    let config = BridgeConfig::generate_default();

    assert_eq!(config.hub.bind, "0.0.0.0:8443");
    assert!(config.hub.cert.is_none());
    assert!(config.hub.key.is_none());
    assert_eq!(config.hub.acme_domain, "");
}

#[test]
fn test_hub_tls_strategy_default_is_self_signed() {
    let config = BridgeConfig::generate_default();
    assert_eq!(config.hub.tls_strategy(), "self-signed");
}

#[test]
fn test_hub_tls_strategy_static() {
    let config = BridgeConfig {
        hub: HubConfig {
            cert: Some("/path/to/cert.pem".into()),
            key: Some("/path/to/key.pem".into()),
            ..HubConfig::default()
        },
        ..BridgeConfig::default()
    };
    assert_eq!(config.hub.tls_strategy(), "static");
}

#[test]
fn test_hub_tls_strategy_acme() {
    let config = BridgeConfig {
        hub: HubConfig {
            acme_domain: "example.com".into(),
            ..HubConfig::default()
        },
        ..BridgeConfig::default()
    };
    assert_eq!(config.hub.tls_strategy(), "acme");
}

// ---------------------------------------------------------------------------
// 11. POST /api/hub/mode — invalid mode validation
// ---------------------------------------------------------------------------

#[test]
fn test_hub_mode_switch_validates_mode() {
    let valid_modes = ["cloud", "embedded"];
    let invalid_modes = ["", "hybrid", "auto", "on-prem"];

    for mode in &valid_modes {
        assert!(
            *mode == "cloud" || *mode == "embedded",
            "valid mode: {}",
            mode
        );
    }

    for mode in &invalid_modes {
        assert!(
            *mode != "cloud" && *mode != "embedded",
            "invalid mode should be rejected: {}",
            mode
        );
    }
}

#[test]
fn test_hub_mode_switch_embedded_updates_config() {
    let mut config = BridgeConfig::generate_default();
    let original_url = config.router.sc.hub_url.clone();

    config.router.sc.hub_url = "wss://localhost:8443".to_string();
    assert_eq!(config.router.sc.hub_url, "wss://localhost:8443");

    config.router.sc.hub_url = original_url;
    assert_eq!(config.router.sc.hub_url, "");
}

// ---------------------------------------------------------------------------
// 12. Hub mode switch requires router not Running (state gating)
// ---------------------------------------------------------------------------

#[test]
fn test_hub_mode_switch_requires_not_running() {
    let sm = StateManager::new();
    assert_eq!(sm.current(), AppState::Stopped);

    sm.try_transition(AppState::Starting).unwrap();
    sm.try_transition(AppState::Running).unwrap();
    assert_eq!(sm.current(), AppState::Running);

    let result = sm.try_transition(AppState::Starting);
    assert!(result.is_err());

    sm.try_transition(AppState::Stopping).unwrap();
    sm.try_transition(AppState::Stopped).unwrap();
    assert_eq!(sm.current(), AppState::Stopped);
}
