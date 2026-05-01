//! ClawSeed — Android AI agent framework.

use clap::Parser;

#[derive(Parser)]
#[command(name = "clawseed", version, about = "ClawSeed AI agent framework")]
enum Cli {
    /// Start the HTTP/WebSocket gateway
    Gateway,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli {
        Cli::Gateway => {
            tracing::info!("Starting ClawSeed gateway...");
            // TODO: load config, build agent, start gateway
        }
    }
    Ok(())
}
