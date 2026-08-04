use clap::{Args, Parser, Subcommand};

use mewcode_client::ClientConfig;
use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, StreamEvent};

use futures::StreamExt;
use std::io::Write as _;

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
    /// Review the current branch diff against a base branch.
    Review(ReviewArgs),
}

#[derive(Debug, Args)]
struct ReviewArgs {
    /// Base branch (or ref) to compare against. Defaults to `main`.
    #[arg(default_value = "main")]
    base: String,
    /// Extra focus instruction appended to the review prompt.
    #[arg(long)]
    extra: Option<String>,
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
            Cmd::Review(args) => run_review(&args).await,
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

/// Review the current branch's diff against `base`, streamed via the server.
///
/// Runs in `Mode::Plan` so the reviewing model can read files and run
/// read-only git inspection, but cannot modify the working tree.
async fn run_review(args: &ReviewArgs) -> Result<(), anyhow::Error> {
    let diff = git_diff(&args.base)?;
    if diff.trim().is_empty() {
        anyhow::bail!(
            "no diff between '{base}' and HEAD — nothing to review",
            base = args.base
        );
    }

    let mut prompt = format!(
        "Review the diff below — it is already fetched, do not re-fetch it. \
         Load the `review-pr` skill from the catalog and follow its procedure.\n\
         Review in two passes: first scan the diff for issues, then re-check \
         each candidate issue against the surrounding code and drop anything \
         you cannot confirm. Report findings one per line in the skill's \
         format, with a Verdict line at the end.\n\
         \n```diff\n{diff}\n```"
    );
    if let Some(extra) = &args.extra {
        prompt.push_str(&format!("\n\nExtra focus: {extra}"));
    }

    let config = ClientConfig::load()?;
    let client = mewcode_client::net::ApiClient::new(&config.api_url);
    let session = client
        .create_session(&mewcode_client::net::CreateSessionRequest {
            title: "mew review".into(),
            model: Some(ModelId::DEFAULT),
            mode: Some(Mode::Plan),
        })
        .await?;
    let req = ChatRequest {
        session_id: session.id,
        model: ModelId::DEFAULT,
        provider: None,
        mode: Mode::Plan,
        messages: vec![Message::user(vec![MessagePart::Text { text: prompt }])],
    };

    let mut stream = client.chat_stream(&req).await?;
    while let Some(event) = stream.next().await {
        if let StreamEvent::TextDelta { delta } = event? {
            print!("{delta}");
            std::io::stdout().flush()?;
        }
    }
    println!();
    Ok(())
}

/// `git diff <base>...HEAD` (three-dot: merge-base semantics).
fn git_diff(base: &str) -> Result<String, anyhow::Error> {
    let out = std::process::Command::new("git")
        .args(["diff", &format!("{base}...HEAD")])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run git: {e} (is git installed?)"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git diff {base}...HEAD failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
