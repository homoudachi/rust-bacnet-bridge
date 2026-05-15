mod cli;
mod hub_cmd;
mod router_cmd;
mod serve_cmd;
mod web;

use clap::Parser;
use cli::{Cli, Command};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Command::Router {
            config,
            transport,
            with_hub,
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

            if let Err(e) = hub_cmd::run_hub(&hub_config).await {
                eprintln!("Hub error: {e}");
                std::process::exit(1);
            }
        }
        Command::Serve { config, dev } => {
            let host = "0.0.0.0";
            let port = 28821;
            if let Err(e) = serve_cmd::run_serve(host, port, config, dev).await {
                eprintln!("Serve error: {e}");
                std::process::exit(1);
            }
        }
    }
}
