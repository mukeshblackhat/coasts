use std::sync::Arc;

use axum::Router;
use clap::Parser;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

mod docker;
mod routes;
mod state;
mod sync;

use state::RemoteState;

#[derive(Parser, Debug)]
#[command(name = "coast-remote", about = "Coast remote Docker agent")]
struct Cli {
    /// Port to listen on.
    #[arg(long, default_value = "31416", env = "COAST_REMOTE_PORT")]
    port: u16,

    /// Directory where SSHFS mounts are created (one subdir per project).
    #[arg(long, default_value = "/mnt/coast", env = "COAST_REMOTE_MOUNT_DIR")]
    mount_dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "coast_remote=info".into()),
        )
        .init();

    let cli = Cli::parse();

    let state = Arc::new(RemoteState::new(&cli.mount_dir).await?);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .nest("/api/v1", routes::router())
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cli.port);
    info!("coast-remote listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
