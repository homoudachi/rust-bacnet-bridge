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
    },
    Serve {
        #[arg(long)]
        config: Option<String>,
        #[arg(long)]
        dev: bool,
    },
}
