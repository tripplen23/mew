//! Tracing setup.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Default `tracing` filter when `RUST_LOG` is unset.
pub const DEFAULT_LOG: &str = "info";

/// Initialise a global `tracing` subscriber for the engine.
///
/// Honours `RUST_LOG`. Safe to call multiple times — only the first call
/// has any effect.
pub fn init() {
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG)))
        .with(fmt::layer().with_target(true).with_level(true))
        .try_init();
}

/// Initialise a JSON-formatted file appender at the given path, suitable
/// for the TUI's trace pane to tail.
pub fn init_json_file(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let (file, guard) = tracing_appender::non_blocking(file);
    let _ = guard; // leak on purpose: tracing is process-global
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG)))
        .with(fmt::layer().json().with_writer(file))
        .try_init();
    Ok(())
}
