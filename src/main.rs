#![cfg_attr(feature = "windows-tray", windows_subsystem = "windows")]

#[cfg(feature = "windows-tray")]
mod tray;

mod cli;
mod hub_cmd;
mod router_cmd;
mod serve_cmd;
mod web;

use clap::Parser;
use cli::{Cli, Command};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Router {
        config: None,
        transport: None,
        with_hub: false,
        log_level: "info".to_string(),
    }) {
        Command::Router {
            config,
            transport,
            with_hub,
            log_level: _,
        } => {
            if let Err(e) = router_cmd::run_router(config, transport, with_hub).await {
                eprintln!("Router error: {e}");
                std::process::exit(1);
            }
        }
        Command::Hub {
            bind,
            cert,
            key,
            acme_domain,
            acme_cache,
            acme_production,
        } => {
            let mut hub_config = {
                let bridge = bridge_core::BridgeConfig::generate_default();
                bridge.hub
            };

            hub_config.bind = bind.unwrap_or_else(|| hub_config.bind.clone());
            if let Some(c) = cert {
                hub_config.cert = Some(c);
            }
            if let Some(k) = key {
                hub_config.key = Some(k);
            }
            if let Some(d) = acme_domain {
                hub_config.acme_domain = d;
            }
            hub_config.acme_cache = acme_cache;
            hub_config.acme_production = acme_production;

            if let Err(e) = hub_cmd::run_hub(&hub_config).await {
                eprintln!("Hub error: {e}");
                std::process::exit(1);
            }
        }
        Command::Serve {
            config,
            dev,
            port,
            host,
        } => {
            if let Err(e) = serve_cmd::run_serve(&host, port, config, dev).await {
                eprintln!("Serve error: {e}");
                std::process::exit(1);
            }
        }
    }
}
