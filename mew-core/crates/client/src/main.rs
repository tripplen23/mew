use clap::{Args, Parser, Subcommand};

use mewcode_client::ClientConfig;

/// Name of the server binary that the `server` subcommand shells out to.
const SERVER_BINARY: &str = "mewcode-server";

#[derive(Debug, Parser)]
#[command(
    name = "mewcode",
    version,
    about = "A hyper-sick terminal coding agent"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Open the ratatui TUI (default).
    Tui,
    /// Start the backend server.
    Server,
    /// Run database migrations.
    Migrate,
    /// Print version info and exit.
    Version,
    /// Smoke test and exit.
    Hello,
    /// Read, write, and list persistent memory.
    Memory(MemoryArgs),
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
struct MemoryArgs {
    #[command(subcommand)]
    command: MemoryCommand,
}

#[derive(Debug, Subcommand)]
enum MemoryCommand {
    /// Print the current memory content.
    Read,
    /// Overwrite memory with new content.
    Write {
        /// The new memory content (markdown).
        content: String,
    },
    /// List available memory profiles.
    List,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        match cli.cmd {
            Cmd::Hello => {
                println!("mewcode");
                Ok(())
            }
            Cmd::Version => {
                println!("mewcode {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
            Cmd::Tui => {
                let config = ClientConfig::load()?;
                let log_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log))
                    .add_directive("tui_markdown=error".parse()?);
                tracing_subscriber::fmt()
                    .with_env_filter(log_filter)
                    .with_target(true)
                    .init();
                mewcode_client::run(config).await
            }
            Cmd::Server => {
                // Look for `mewcode-server` on PATH; fall back to a sibling
                // binary next to us (useful when running from `target/debug/`).
                let status = std::process::Command::new(SERVER_BINARY)
                    .status()
                    .or_else(|_| {
                        let exe = std::env::current_exe()?;
                        let sibling = exe.with_file_name(if cfg!(windows) {
                            "mewcode-server.exe"
                        } else {
                            SERVER_BINARY
                        });
                        std::process::Command::new(sibling).status()
                    })?;
                std::process::exit(status.code().unwrap_or(1));
            }
            Cmd::Migrate => {
                anyhow::bail!("migrate is not implemented yet")
            }
            Cmd::Memory(args) => match args.command {
                MemoryCommand::Read => {
                    let content = read_memory().await?;
                    println!("{}", content);
                    Ok(())
                }
                MemoryCommand::Write { content } => {
                    write_memory(&content).await?;
                    println!("memory written");
                    Ok(())
                }
                MemoryCommand::List => {
                    let profiles = list_profiles().await?;
                    for p in profiles {
                        println!("{p}");
                    }
                    Ok(())
                }
            },
        }
    })
}

/// Read memory from the server.
async fn read_memory() -> Result<String, anyhow::Error> {
    let config = ClientConfig::load()?;
    let url = format!("{}/memory", config.api_url);
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    Ok(resp["content"].as_str().unwrap_or_default().to_string())
}

/// Write memory via the server.
async fn write_memory(content: &str) -> Result<(), anyhow::Error> {
    let config = ClientConfig::load()?;
    let url = format!("{}/memory", config.api_url);
    let client = reqwest::Client::new();
    client
        .post(&url)
        .json(&serde_json::json!({ "content": content }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// List memory profiles from the server.
async fn list_profiles() -> Result<Vec<String>, anyhow::Error> {
    // For now, just call GET /memory and report the active profile.
    // A future RPC can return available profiles once the server supports it.
    let config = ClientConfig::load()?;
    let url = format!("{}/memory", config.api_url);
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    Ok(vec![
        resp["profile"].as_str().unwrap_or("default").to_string(),
    ])
}
