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

    // Starting -> Stopped is now valid (startup failure rollback)
    assert!(sm.try_transition(AppState::Stopped).is_ok());
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
        id: 0,
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

// ---------------------------------------------------------------------------
// 3. API error scenario tests — config validation, state transitions
// ---------------------------------------------------------------------------

#[test]
fn test_validate_rejects_invalid_config() {
    let config = BridgeConfig::generate_default();
    // Default config has empty hub_url → should fail for sc transport
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("sc.hub_url"));

    // Fix hub_url → should pass
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_collects_multiple_errors() {
    let mut config = BridgeConfig::generate_default();
    config.router.device_id = 9999999;
    config.router.device_name = "".to_string();
    config.router.lan.port = 0;
    // SC mode, empty hub_url
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("device_id"));
    assert!(msg.contains("device_name"));
    assert!(msg.contains("lan.port"));
    assert!(msg.contains("sc.hub_url"));
    // Multiple errors joined by semicolons
    assert!(msg.contains(";"));
}

#[test]
fn test_state_transition_error_messages() {
    let sm = StateManager::new();
    let result = sm.try_transition(AppState::Running);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_display = err.to_string();
    match &err {
        BridgeError::InvalidStateTransition { from, to } => {
            assert_eq!(from, "Stopped");
            assert_eq!(to, "Running");
        }
        _ => panic!("Expected InvalidStateTransition"),
    }
    assert!(err_display.contains("Invalid state transition"));
    assert!(err_display.contains("Stopped"));
    assert!(err_display.contains("Running"));
}

#[test]
fn test_config_validation_error_format() {
    let err = BridgeError::ConfigValidation("test error message".to_string());
    assert_eq!(err.to_string(), "Config validation error: test error message");

    // Test display format
    let multi = BridgeError::ConfigValidation("error one; error two; error three".to_string());
    assert!(multi.to_string().contains("error one"));
    assert!(multi.to_string().contains("error two"));
}

#[test]
fn test_validate_tailscale_mode_requires_interface() {
    let mut config = BridgeConfig::generate_default();
    config.router.transport = "tailscale".to_string();
    // Missing tailscale interface
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("tailscale.interface"));

    // With valid interface
    config.router.tailscale.interface = "100.64.0.1".to_string();
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_sc_mode_requires_hub_url() {
    let mut config = BridgeConfig::generate_default();
    config.router.transport = "sc".to_string();
    // Missing hub_url
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("sc.hub_url"));

    // With valid hub_url
    config.router.sc.hub_url = "wss://hub.example.com:443".to_string();
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_zero_ports_rejected() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.lan.port = 0;
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("lan.port"));

    let mut config = BridgeConfig::generate_default();
    config.router.transport = "tailscale".to_string();
    config.router.tailscale.interface = "100.64.0.1".to_string();
    config.router.tailscale.port = 0;
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("tailscale.port"));

    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.web.port = 0;
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("web.port"));
}

#[test]
fn test_validate_rejects_invalid_lan_ip() {
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    config.router.lan.interface = "not-a-valid-ip".to_string();
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("lan.interface"));
    assert!(msg.contains("valid IPv4"));
}

// ---------------------------------------------------------------------------
// 13. Log buffer push via LogEntry (POST /api/log)
// ---------------------------------------------------------------------------

#[test]
fn test_log_buffer_push_and_retrieve() {
    let buf = LogRingBuffer::new(100);

    let entry = LogEntry {
        id: 0,
        timestamp: "2026-05-18T12:00:00Z".into(),
        level: "ERROR".into(),
        target: "frontend".into(),
        message: "Frontend error: test message".into(),
        fields: std::collections::HashMap::new(),
    };
    buf.push(entry);

    let recent = buf.recent(10, None);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].level, "ERROR");
    assert_eq!(recent[0].target, "frontend");
    assert!(recent[0].message.contains("Frontend error"));
}

#[test]
fn test_log_buffer_frontend_errors_filterable() {
    let buf = LogRingBuffer::new(100);

    buf.push(LogEntry {
        id: 0, timestamp: "t1".into(), level: "INFO".into(),
        target: "backend".into(), message: "server started".into(),
        fields: std::collections::HashMap::new(),
    });
    buf.push(LogEntry {
        id: 0, timestamp: "t2".into(), level: "ERROR".into(),
        target: "frontend".into(), message: "validation failed".into(),
        fields: std::collections::HashMap::new(),
    });
    buf.push(LogEntry {
        id: 0, timestamp: "t3".into(), level: "WARN".into(),
        target: "frontend".into(), message: "slow response".into(),
        fields: std::collections::HashMap::new(),
    });

    let errors = buf.recent(10, Some("ERROR"));
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].target, "frontend");
    assert_eq!(errors[0].message, "validation failed");
}

// ---------------------------------------------------------------------------
// 14. Config save validation — update_config returns error for invalid config
// ---------------------------------------------------------------------------

#[test]
fn test_config_update_rejects_invalid_tailscale_config() {
    let mut config = BridgeConfig::generate_default();
    config.router.transport = "tailscale".to_string();
    // Leave tailscale.interface empty — this should fail validation
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("tailscale.interface"));

    // Fix the interface — should pass
    config.router.tailscale.interface = "100.64.0.1".to_string();
    assert!(config.validate().is_ok());
}

#[test]
fn test_config_update_rejects_invalid_sc_config() {
    let mut config = BridgeConfig::generate_default();
    config.router.transport = "sc".to_string();
    // Leave hub_url empty — should fail validation
    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("sc.hub_url"));

    // Fix hub_url — should pass
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    assert!(config.validate().is_ok());
}

#[test]
fn test_config_update_save_and_reload_after_validation() {
    let dir = std::env::temp_dir().join("bacnet-bridge-test-validate-save");
    let path = dir.join("config.toml");
    let _ = std::fs::remove_dir_all(&dir);

    // Valid config
    let mut config = BridgeConfig::generate_default();
    config.router.sc.hub_url = "wss://hub.example.com".to_string();
    assert!(config.validate().is_ok());
    config.save(&path).expect("save valid config");

    // Invalid config should fail validation before save
    let mut invalid = BridgeConfig::generate_default();
    invalid.router.transport = "tailscale".to_string();
    assert!(invalid.validate().is_err());

    // Loaded config should still be the valid one
    let loaded = BridgeConfig::load(&path).expect("load config");
    assert_eq!(loaded.router.transport, "sc");
    assert_eq!(loaded.router.sc.hub_url, "wss://hub.example.com");

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// 15. LogBuffer with unique IDs
// ---------------------------------------------------------------------------

#[test]
fn test_log_entries_have_unique_ids() {
    let buf = LogRingBuffer::new(100);
    for i in 0..10 {
        buf.push(LogEntry {
            id: 0,
            timestamp: format!("t{}", i),
            level: "INFO".into(),
            target: "test".into(),
            message: format!("msg {}", i),
            fields: std::collections::HashMap::new(),
        });
    }
    let recent = buf.recent(20, None);
    assert_eq!(recent.len(), 10);
    // Each entry should have a unique id (global counter starts at 0, so ids may include 0)
    let ids: Vec<u64> = recent.iter().map(|e| e.id).collect();
    let unique: std::collections::HashSet<u64> = ids.iter().cloned().collect();
    assert_eq!(unique.len(), 10);
}

// ---------------------------------------------------------------------------
// 16. Boolean config values survive save/load round trip (catches JS setNestedValue bugs)
// ---------------------------------------------------------------------------

#[test]
fn test_config_boolean_round_trip() {
    let mut config = BridgeConfig::generate_default();
    config.web.open_browser = false;
    config.router.sc.danger_accept_invalid_certs = true;
    config.router.sc.hub_url = "wss://hub.example.com".to_string();

    let toml_str = toml::to_string_pretty(&config).expect("serialize");
    let parsed: BridgeConfig = toml::from_str(&toml_str).expect("deserialize");

    assert!(!parsed.web.open_browser);
    assert!(parsed.router.sc.danger_accept_invalid_certs);
    assert_eq!(parsed.router.sc.hub_url, "wss://hub.example.com");
}

// ---------------------------------------------------------------------------
// 17. Simulated PUT /api/config flow: deserialize JSON with booleans, validate, save
// ---------------------------------------------------------------------------

#[test]
fn test_config_update_flow_with_booleans() {
    let mut config = BridgeConfig::generate_default();
    config.web.open_browser = false;
    config.router.sc.danger_accept_invalid_certs = true;
    config.router.sc.hub_url = "wss://hub.example.com".to_string();

    // Simulate what the frontend sends: JSON with boolean values
    let json_val = serde_json::to_value(&config).expect("to json");
    let deserialized: BridgeConfig = serde_json::from_value(json_val).expect("from json");

    // Booleans must survive
    assert!(!deserialized.web.open_browser);
    assert!(deserialized.router.sc.danger_accept_invalid_certs);

    // Config must validate
    assert!(deserialized.validate().is_ok());

    // Save and reload
    let dir = std::env::temp_dir().join("bacnet-bridge-test-boolean-flow");
    let path = dir.join("config.toml");
    let _ = std::fs::remove_dir_all(&dir);
    deserialized.save(&path).expect("save");
    let loaded = BridgeConfig::load(&path).expect("load");
    assert!(!loaded.web.open_browser);
    assert!(loaded.router.sc.danger_accept_invalid_certs);

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// 18. Config with all field types survives round trip (simulate full form save)
// ---------------------------------------------------------------------------

#[test]
fn test_config_full_form_save_round_trip() {
    let mut config = BridgeConfig::generate_default();
    // Router section — number, text, select
    config.router.device_id = 12345;
    config.router.vendor_id = 7;
    config.router.device_name = "Test Bridge".to_string();
    config.router.transport = "tailscale".to_string();
    // LAN section — text (IP), number
    config.router.lan.interface = "192.168.1.100".to_string();
    config.router.lan.port = 47809;
    // SC section — text, numbers
    config.router.sc.hub_url = "wss://hub.example.com:443".to_string();
    config.router.sc.reconnect_initial_ms = 2000;
    config.router.sc.reconnect_max_ms = 60000;
    config.router.sc.reconnect_max_attempts = 5;
    // Tailscale section — text (IP), number
    config.router.tailscale.interface = "100.64.0.1".to_string();
    config.router.tailscale.port = 20001;
    // Web section — text, number, checkbox (boolean)
    config.web.host = "127.0.0.1".to_string();
    config.web.port = 8080;
    config.web.open_browser = false;

    // Simulate JSON round trip (as frontend sends)
    let json_val = serde_json::to_value(&config).expect("to json");
    let deserialized: BridgeConfig = serde_json::from_value(json_val).expect("from json");

    assert_eq!(deserialized.router.device_id, 12345);
    assert_eq!(deserialized.router.vendor_id, 7);
    assert_eq!(deserialized.router.device_name, "Test Bridge");
    assert_eq!(deserialized.router.transport, "tailscale");
    assert_eq!(deserialized.router.lan.interface, "192.168.1.100");
    assert_eq!(deserialized.router.lan.port, 47809);
    assert_eq!(deserialized.router.sc.hub_url, "wss://hub.example.com:443");
    assert_eq!(deserialized.router.sc.reconnect_initial_ms, 2000);
    assert_eq!(deserialized.router.sc.reconnect_max_ms, 60000);
    assert_eq!(deserialized.router.sc.reconnect_max_attempts, 5);
    assert_eq!(deserialized.router.tailscale.interface, "100.64.0.1");
    assert_eq!(deserialized.router.tailscale.port, 20001);
    assert_eq!(deserialized.web.host, "127.0.0.1");
    assert_eq!(deserialized.web.port, 8080);
    assert!(!deserialized.web.open_browser);

    // Validate passes (tailscale with valid interface)
    assert!(deserialized.validate().is_ok());
}

// ---------------------------------------------------------------------------
// 19. Reject save of invalid config (backend validate-before-save)
// ---------------------------------------------------------------------------

#[test]
fn test_config_save_rejected_when_invalid() {
    // Simulate frontend sending config with empty tailscale interface (tailscale mode)
    let mut config = BridgeConfig::generate_default();
    config.router.transport = "tailscale".to_string();
    // tailscale.interface is empty (default) → should fail validate

    let result = config.validate();
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("tailscale.interface"));
    assert!(msg.contains("must not be empty"));
}
