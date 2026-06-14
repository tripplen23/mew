use std::sync::Arc;

use anyhow::Context;
use mewcode_server::store::fs::{resolve_data_dir, FsStore};
use mewcode_server::{config::ServerConfig, AppState};
use tokio::net::TcpListener;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ServerConfig::load()?;
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log)))
        .with(fmt::layer().with_target(true))
        .init();

    let addr: std::net::SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("MEWCODE_HOST/MEWCODE_PORT must form a valid SocketAddr");

    // Resolve the per-user data dir and open the filesystem-backed store.
    // A create/write failure here aborts startup (no in-memory fallback).
    let data_dir = resolve_data_dir().context("failed to resolve mewcode data directory")?;
    let store = Arc::new(
        FsStore::new(data_dir.clone())
            .with_context(|| format!("failed to open session store at {}", data_dir.display()))?,
    );
    tracing::info!(data_dir = %data_dir.display(), "session store ready");

    let state = AppState::new(config.clone(), store);

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "mewcode server listening");
    let app = mewcode_server::build_app(state);
    axum::serve(listener, app).await?;
    Ok(())
}
