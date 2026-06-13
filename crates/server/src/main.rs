use mewcode_protocol::env::DATABASE_URL;
use mewcode_server::{config::ServerConfig, db, AppState};
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

    let pool = match &config.database_url {
        Some(url) => Some(db::connect(url).await?),
        None => {
            tracing::warn!("{DATABASE_URL} is not set; running in in-memory mode");
            None
        }
    };

    let state = AppState::new(config.clone(), pool);

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "mewcode server listening");
    let app = mewcode_server::build_app(state);
    axum::serve(listener, app).await?;
    Ok(())
}
