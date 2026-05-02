//! ClawSeed — Android AI agent framework.

use clap::Parser;

#[derive(Parser)]
#[command(name = "clawseed", version, about = "ClawSeed AI agent framework")]
enum Cli {
    /// Start the HTTP/WebSocket gateway
    Gateway {
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to listen on (overrides config)
        #[arg(long)]
        port: Option<u16>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli {
        Cli::Gateway { host, port } => {
            tracing::info!("Starting ClawSeed gateway on {host}...");
            let config = clawseed_config::load_config()?;
            let port = port.unwrap_or(config.gateway.port);
            clawseed_gateway::run_gateway(&host, port, config, None).await?;
        }
    }
    Ok(())
}
