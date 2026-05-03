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
    /// Start a local interactive chat session
    Chat {
        /// Override model name from config
        #[arg(long)]
        model: Option<String>,
        /// Override temperature (0.0 - 2.0)
        #[arg(long)]
        temperature: Option<f64>,
        /// Override system prompt
        #[arg(long)]
        system_prompt: Option<String>,
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
        Cli::Chat {
            model,
            temperature,
            system_prompt,
        } => {
            run_chat(model, temperature, system_prompt).await?;
        }
    }
    Ok(())
}

async fn run_chat(
    model: Option<String>,
    temperature: Option<f64>,
    system_prompt: Option<String>,
) -> anyhow::Result<()> {
    use clawseed_agent::agent::{Agent, TurnEvent};
    use std::io::Write;

    let mut config = clawseed_config::load_config()?;

    if let Some(ref m) = model {
        if let Some(fallback) = config.providers.fallback.as_ref() {
            if let Some(entry) = config.providers.models.get_mut(fallback) {
                entry.model = Some(m.clone());
            }
        }
    }
    if let Some(t) = temperature {
        config.agent.temperature = Some(t);
    }
    if let Some(ref sp) = system_prompt {
        config.agent.system_prompt = Some(sp.clone());
    }

    let mut agent = Agent::from_config(&config).await?;

    let fallback_name = config.providers.fallback.as_deref().unwrap_or("unknown");
    let model_name = config
        .providers
        .fallback_provider()
        .and_then(|p| p.model.as_deref())
        .unwrap_or("default");
    println!("ClawSeed Chat (provider: {fallback_name}, model: {model_name})");
    println!("Type 'exit' or 'quit' to end the session.\n");

    let mut rl = rustyline::DefaultEditor::new()?;

    loop {
        let readline = rl.readline("You> ");
        let input = match readline {
            Ok(line) => line,
            Err(
                rustyline::error::ReadlineError::Interrupted | rustyline::error::ReadlineError::Eof,
            ) => break,
            Err(e) => return Err(e.into()),
        };

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }

        let _ = rl.add_history_entry(&input);

        let (tx, mut rx) = tokio::sync::mpsc::channel::<TurnEvent>(256);

        let printer = tokio::spawn(async move {
            let mut stdout = std::io::stdout();
            let mut in_thinking = false;
            while let Some(event) = rx.recv().await {
                match event {
                    TurnEvent::Chunk { delta } => {
                        if in_thinking {
                            print!("\x1b[0m");
                            in_thinking = false;
                        }
                        print!("{delta}");
                        let _ = stdout.flush();
                    }
                    TurnEvent::Thinking { delta } => {
                        if !in_thinking {
                            print!("\x1b[2m");
                            in_thinking = true;
                        }
                        print!("{delta}");
                        let _ = stdout.flush();
                    }
                    TurnEvent::ToolCall { name, args, .. } => {
                        if in_thinking {
                            print!("\x1b[0m");
                            in_thinking = false;
                        }
                        println!("\n\x1b[36m[tool: {name}]\x1b[0m {args}");
                    }
                    TurnEvent::ToolResult { name, output, .. } => {
                        let preview = if output.len() > 200 {
                            format!("{}...", &output[..200])
                        } else {
                            output
                        };
                        println!("\x1b[32m[result: {name}]\x1b[0m {preview}");
                    }
                    TurnEvent::DebugPrompt { .. } => {}
                }
            }
            if in_thinking {
                print!("\x1b[0m");
            }
        });

        match agent.turn_streamed(trimmed, tx, None, false).await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("\n\x1b[31mError: {e}\x1b[0m");
            }
        }

        printer.await?;
        println!("\n");
    }

    println!("Bye!");
    Ok(())
}
