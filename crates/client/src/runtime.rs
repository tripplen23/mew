//! TUI runtime entry point.

use anyhow::Result;

/// Run the client.
pub async fn run(_config: super::config::ClientConfig) -> Result<()> {
    println!("mewcode");
    Ok(())
}
