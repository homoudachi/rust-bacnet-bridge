use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::error::BridgeError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BridgeConfig {
    pub router: RouterConfig,
    pub web: WebConfig,
    pub hub: HubConfig,
}

impl BridgeConfig {
    pub fn generate_default() -> Self {
        Self::default()
    }

    pub fn load(path: &Path) -> Result<Self, BridgeError> {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let mut config: Self = toml::from_str(&content)?;
            let overrides = collect_env_overrides();
            apply_overrides(&mut config, &overrides);
            Ok(config)
        } else {
            let config = Self::generate_default();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            config.save(path)?;
            let overrides = collect_env_overrides();
            let mut result = config;
            apply_overrides(&mut result, &overrides);
            Ok(result)
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), BridgeError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn default_config_path() -> PathBuf {
        let base = dirs::config_dir().expect("Could not determine config directory");
        base.join("bacnet-bridge").join("config.toml")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RouterConfig {
    pub transport: String,
    pub device_id: u32,
    pub vendor_id: u16,
    pub device_name: String,
    pub lan: LanConfig,
    pub sc: ScConfig,
    pub tailscale: TailscaleConfig,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            transport: "sc".to_string(),
            device_id: 4194303,
            vendor_id: 15,
            device_name: "BACnet-Bridge".to_string(),
            lan: LanConfig::default(),
            sc: ScConfig::default(),
            tailscale: TailscaleConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LanConfig {
    pub interface: String,
    pub port: u16,
}

impl Default for LanConfig {
    fn default() -> Self {
        Self {
            interface: String::new(),
            port: 47808,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScConfig {
    pub hub_url: String,
    pub reconnect_initial_ms: u64,
    pub reconnect_max_ms: u64,
    pub reconnect_max_attempts: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_cert: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_cert: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub danger_accept_invalid_certs: bool,
}

impl Default for ScConfig {
    fn default() -> Self {
        Self {
            hub_url: String::new(),
            reconnect_initial_ms: 1000,
            reconnect_max_ms: 30000,
            reconnect_max_attempts: 0,
            client_cert: None,
            client_key: None,
            ca_cert: None,
            danger_accept_invalid_certs: false,
        }
    }
}

fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TailscaleConfig {
    pub interface: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bdt: Option<Vec<BdtEntry>>,
}

impl Default for TailscaleConfig {
    fn default() -> Self {
        Self {
            interface: String::new(),
            port: 20000,
            bdt: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BdtEntry {
    pub ip: String,
    pub port: u16,
    pub broadcast_mask: [u8; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    pub host: String,
    pub port: u16,
    pub open_browser: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 28821,
            open_browser: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HubConfig {
    pub bind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub acme_domain: String,
    pub acme_cache: String,
    pub acme_production: bool,
}

impl HubConfig {
    pub fn tls_strategy(&self) -> &'static str {
        if self.cert.is_some() && self.key.is_some() {
            "static"
        } else if !self.acme_domain.is_empty() {
            "acme"
        } else {
            "self-signed"
        }
    }
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8443".to_string(),
            cert: None,
            key: None,
            acme_domain: String::new(),
            acme_cache: "./acme-cache".to_string(),
            acme_production: false,
        }
    }
}

fn collect_env_overrides() -> HashMap<String, String> {
    env::vars()
        .filter(|(k, _)| k.starts_with("BACNET_BRIDGE_"))
        .map(|(k, v)| {
            let path = k["BACNET_BRIDGE_".len()..]
                .to_lowercase()
                .replace("__", ".");
            (path, v)
        })
        .collect()
}

fn apply_overrides(config: &mut BridgeConfig, overrides: &HashMap<String, String>) {
    for (path, value) in overrides {
        let parts: Vec<&str> = path.splitn(2, '.').collect();
        if parts.len() < 2 {
            warn!("Config override has no section prefix: {}", path);
            continue;
        }
        match parts[0] {
            "router" => apply_router_override(&mut config.router, parts[1], value),
            "web" => apply_web_override(&mut config.web, parts[1], value),
            "hub" => apply_hub_override(&mut config.hub, parts[1], value),
            _ => warn!("Unknown config section: {}", parts[0]),
        }
    }
}

fn apply_router_override(router: &mut RouterConfig, path: &str, value: &str) {
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    match parts[0] {
        "transport" => router.transport = value.to_string(),
        "device_id" => {
            if let Ok(v) = value.parse::<u32>() {
                router.device_id = v;
            }
        }
        "vendor_id" => {
            if let Ok(v) = value.parse::<u16>() {
                router.vendor_id = v;
            }
        }
        "device_name" => router.device_name = value.to_string(),
        "lan" => {
            if parts.len() < 2 {
                return;
            }
            match parts[1] {
                "interface" => router.lan.interface = value.to_string(),
                "port" => {
                    if let Ok(v) = value.parse::<u16>() {
                        router.lan.port = v;
                    }
                }
                _ => warn!("Unknown config key: router.lan.{}", parts[1]),
            }
        }
        "sc" => {
            if parts.len() < 2 {
                return;
            }
            match parts[1] {
                "hub_url" => router.sc.hub_url = value.to_string(),
                "reconnect_initial_ms" => {
                    if let Ok(v) = value.parse::<u64>() {
                        router.sc.reconnect_initial_ms = v;
                    }
                }
                "reconnect_max_ms" => {
                    if let Ok(v) = value.parse::<u64>() {
                        router.sc.reconnect_max_ms = v;
                    }
                }
                "reconnect_max_attempts" => {
                    if let Ok(v) = value.parse::<u32>() {
                        router.sc.reconnect_max_attempts = v;
                    }
                }
                "client_cert" => router.sc.client_cert = Some(value.to_string()),
                "client_key" => router.sc.client_key = Some(value.to_string()),
                "ca_cert" => router.sc.ca_cert = Some(value.to_string()),
                "danger_accept_invalid_certs" => {
                    router.sc.danger_accept_invalid_certs = value == "true" || value == "1";
                }
                _ => warn!("Unknown config key: router.sc.{}", parts[1]),
            }
        }
        "tailscale" => {
            if parts.len() < 2 {
                return;
            }
            match parts[1] {
                "interface" => router.tailscale.interface = value.to_string(),
                "port" => {
                    if let Ok(v) = value.parse::<u16>() {
                        router.tailscale.port = v;
                    }
                }
                "bdt" => {} // BDT array overrides not supported via env vars
                _ => warn!("Unknown config key: router.tailscale.{}", parts[1]),
            }
        }
        _ => warn!("Unknown config key: router.{}", parts[0]),
    }
}

fn apply_web_override(web: &mut WebConfig, path: &str, value: &str) {
    match path {
        "host" => web.host = value.to_string(),
        "port" => {
            if let Ok(v) = value.parse::<u16>() {
                web.port = v;
            }
        }
        "open_browser" => {
            if let Ok(v) = value.parse::<bool>() {
                web.open_browser = v;
            }
        }
        _ => warn!("Unknown config key: web.{}", path),
    }
}

fn apply_hub_override(hub: &mut HubConfig, path: &str, value: &str) {
    match path {
        "bind" => hub.bind = value.to_string(),
        "cert" => hub.cert = Some(value.to_string()),
        "key" => hub.key = Some(value.to_string()),
        "acme_domain" => hub.acme_domain = value.to_string(),
        "acme_cache" => hub.acme_cache = value.to_string(),
        "acme_production" => {
            if let Ok(v) = value.parse::<bool>() {
                hub.acme_production = v;
            }
        }
        _ => warn!("Unknown config key: hub.{}", path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BridgeConfig::generate_default();
        assert_eq!(config.router.transport, "sc");
        assert_eq!(config.router.device_id, 4194303);
        assert_eq!(config.router.vendor_id, 15);
        assert_eq!(config.router.device_name, "BACnet-Bridge");
        assert_eq!(config.router.lan.interface, "");
        assert_eq!(config.router.lan.port, 47808);
        assert_eq!(config.router.sc.hub_url, "");
        assert_eq!(config.router.sc.reconnect_initial_ms, 1000);
        assert_eq!(config.router.sc.reconnect_max_ms, 30000);
        assert_eq!(config.router.sc.reconnect_max_attempts, 0);
        assert!(config.router.sc.client_cert.is_none());
        assert!(config.router.sc.client_key.is_none());
        assert_eq!(config.router.tailscale.interface, "");
        assert_eq!(config.router.tailscale.port, 20000);
        assert!(config.router.tailscale.bdt.is_none());
        assert_eq!(config.web.host, "0.0.0.0");
        assert_eq!(config.web.port, 28821);
        assert!(config.web.open_browser);
        assert_eq!(config.hub.bind, "0.0.0.0:8443");
        assert!(config.hub.cert.is_none());
        assert!(config.hub.key.is_none());
        assert_eq!(config.hub.acme_domain, "");
        assert_eq!(config.hub.acme_cache, "./acme-cache");
        assert!(!config.hub.acme_production);
    }

    #[test]
    fn test_config_round_trip() {
        let config = BridgeConfig::generate_default();
        let toml_str = toml::to_string_pretty(&config).expect("serialize");
        let parsed: BridgeConfig = toml::from_str(&toml_str).expect("deserialize");

        assert_eq!(config.router.transport, parsed.router.transport);
        assert_eq!(config.router.device_id, parsed.router.device_id);
        assert_eq!(config.router.vendor_id, parsed.router.vendor_id);
        assert_eq!(config.router.device_name, parsed.router.device_name);
        assert_eq!(config.router.lan.interface, parsed.router.lan.interface);
        assert_eq!(config.router.lan.port, parsed.router.lan.port);
        assert_eq!(config.router.sc.hub_url, parsed.router.sc.hub_url);
        assert_eq!(
            config.router.sc.reconnect_initial_ms,
            parsed.router.sc.reconnect_initial_ms
        );
        assert_eq!(
            config.router.sc.reconnect_max_ms,
            parsed.router.sc.reconnect_max_ms
        );
        assert_eq!(
            config.router.sc.reconnect_max_attempts,
            parsed.router.sc.reconnect_max_attempts
        );
        assert_eq!(config.router.sc.client_cert, parsed.router.sc.client_cert);
        assert_eq!(config.router.sc.client_key, parsed.router.sc.client_key);
        assert_eq!(
            config.router.tailscale.interface,
            parsed.router.tailscale.interface
        );
        assert_eq!(config.router.tailscale.port, parsed.router.tailscale.port);
        assert!(parsed.router.tailscale.bdt.is_none());
        assert_eq!(config.web.host, parsed.web.host);
        assert_eq!(config.web.port, parsed.web.port);
        assert_eq!(config.web.open_browser, parsed.web.open_browser);
        assert_eq!(config.hub.bind, parsed.hub.bind);
        assert_eq!(config.hub.cert, parsed.hub.cert);
        assert_eq!(config.hub.key, parsed.hub.key);
        assert_eq!(config.hub.acme_domain, parsed.hub.acme_domain);
        assert_eq!(config.hub.acme_cache, parsed.hub.acme_cache);
        assert_eq!(config.hub.acme_production, parsed.hub.acme_production);
    }

    #[test]
    fn test_acme_production_round_trip() {
        let mut config = BridgeConfig::generate_default();
        assert!(!config.hub.acme_production);

        config.hub.acme_production = true;
        let toml_str = toml::to_string_pretty(&config).expect("serialize");
        assert!(toml_str.contains("acme_production = true"));

        let parsed: BridgeConfig = toml::from_str(&toml_str).expect("deserialize");
        assert!(parsed.hub.acme_production);
    }

    #[test]
    fn test_env_overrides_transport() {
        let mut config = BridgeConfig::generate_default();
        let mut overrides = HashMap::new();
        overrides.insert("router.transport".to_string(), "tailscale".to_string());
        apply_overrides(&mut config, &overrides);
        assert_eq!(config.router.transport, "tailscale");
    }

    #[test]
    fn test_env_overrides_nested() {
        let mut config = BridgeConfig::generate_default();
        let mut overrides = HashMap::new();
        overrides.insert(
            "router.lan.interface".to_string(),
            "192.168.1.50".to_string(),
        );
        overrides.insert(
            "router.sc.hub_url".to_string(),
            "wss://test.example.com:443".to_string(),
        );
        overrides.insert(
            "router.tailscale.interface".to_string(),
            "100.64.0.1".to_string(),
        );
        apply_overrides(&mut config, &overrides);
        assert_eq!(config.router.lan.interface, "192.168.1.50");
        assert_eq!(config.router.sc.hub_url, "wss://test.example.com:443");
        assert_eq!(config.router.tailscale.interface, "100.64.0.1");
    }

    #[test]
    fn test_env_overrides_integer() {
        let mut config = BridgeConfig::generate_default();
        let mut overrides = HashMap::new();
        overrides.insert("router.device_id".to_string(), "12345".to_string());
        overrides.insert("router.lan.port".to_string(), "47809".to_string());
        overrides.insert("web.port".to_string(), "3000".to_string());
        apply_overrides(&mut config, &overrides);
        assert_eq!(config.router.device_id, 12345);
        assert_eq!(config.router.lan.port, 47809);
        assert_eq!(config.web.port, 3000);
    }

    #[test]
    fn test_env_overrides_bool() {
        let mut config = BridgeConfig::generate_default();
        let mut overrides = HashMap::new();
        overrides.insert("web.open_browser".to_string(), "false".to_string());
        apply_overrides(&mut config, &overrides);
        assert!(!config.web.open_browser);
    }

    #[test]
    fn test_env_overrides_optional_some() {
        let mut config = BridgeConfig::generate_default();
        let mut overrides = HashMap::new();
        overrides.insert(
            "router.sc.client_cert".to_string(),
            "certs/client.pem".to_string(),
        );
        overrides.insert(
            "router.sc.client_key".to_string(),
            "certs/client-key.pem".to_string(),
        );
        apply_overrides(&mut config, &overrides);
        assert_eq!(
            config.router.sc.client_cert,
            Some("certs/client.pem".to_string())
        );
        assert_eq!(
            config.router.sc.client_key,
            Some("certs/client-key.pem".to_string())
        );
    }

    #[test]
    fn test_file_save_and_load() {
        let dir = env::temp_dir().join("bacnet-bridge-test-save-load");
        let path = dir.join("config.toml");

        let original = BridgeConfig::generate_default();
        original.save(&path).expect("save");
        let loaded = BridgeConfig::load(&path).expect("load");

        assert_eq!(original.router.transport, loaded.router.transport);
        assert_eq!(original.router.device_id, loaded.router.device_id);
        assert_eq!(original.router.lan.port, loaded.router.lan.port);
        assert_eq!(original.web.port, loaded.web.port);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_auto_generate_on_missing() {
        let dir = env::temp_dir().join("bacnet-bridge-test-autogen");
        let path = dir.join("config.toml");

        assert!(!path.exists());
        let config = BridgeConfig::load(&path).expect("load should auto-generate");
        assert!(path.exists());
        assert_eq!(config.router.transport, "sc");
        assert_eq!(config.router.device_id, 4194303);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_default_config_path_is_not_empty() {
        let path = BridgeConfig::default_config_path();
        assert!(path.to_string_lossy().contains("bacnet-bridge"));
        assert!(path.to_string_lossy().ends_with("config.toml"));
    }

    #[test]
    fn test_toml_serialization_skips_none_optionals() {
        let config = BridgeConfig::generate_default();
        let toml_str = toml::to_string_pretty(&config).expect("serialize");
        assert!(!toml_str.contains("client_cert"));
        assert!(!toml_str.contains("client_key"));
        assert!(!toml_str.contains("bdt"));
        assert!(!toml_str.contains("cert"));
        assert!(!toml_str.contains("key"));
    }
}
