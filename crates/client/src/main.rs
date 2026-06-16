use clap::{Parser, Subcommand};

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
                tracing_subscriber::fmt()
                    .with_env_filter(
                        tracing_subscriber::EnvFilter::try_from_default_env()
                            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log)),
                    )
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
        }
    })
}
