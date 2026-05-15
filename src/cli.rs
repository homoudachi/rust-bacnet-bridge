use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bacnet-bridge")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Router {
        #[arg(long)]
        config: Option<String>,
        #[arg(long)]
        transport: Option<String>,
        #[arg(long)]
        with_hub: bool,
        #[arg(long, default_value = "info")]
        log_level: String,
    },
    Hub {
        #[arg(long)]
        bind: Option<String>,
        #[arg(long)]
        cert: Option<String>,
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        acme_domain: Option<String>,
        #[arg(long, default_value = "./acme-cache")]
        acme_cache: String,
        #[arg(long)]
        acme_production: bool,
    },
    Serve {
        #[arg(long)]
        config: Option<String>,
        #[arg(long)]
        dev: bool,
        #[arg(long, default_value = "28821")]
        port: u16,
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
    },
}
