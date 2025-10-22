use std::sync::Arc;
use anyhow::Result;
use axum::serve;
use tokio::{net::TcpListener, task};
use tracing::{info, warn, error};

use config::AppConfig;
use db::init_pool;
use api::create_router;
use service::fetchers::take_and_push_transactions;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let cfg = AppConfig::from_env();
    info!("Loaded config, starting USDC Tracker...");

    let pool = init_pool_with_retry(&cfg.db_url).await?;
    let pool = Arc::new(pool);
    info!("Connected to database successfully");

    let tracker_task = {
        let pool = Arc::clone(&pool);
        task::spawn(async move {
            info!("Starting USDC tracker background task...");
            if let Err(e) = take_and_push_transactions(pool.clone()).await {
                error!("Fetcher task failed: {:?}", e);
            }
        })
    };

    let app = create_router(pool.clone());
    let addr = format!("0.0.0.0:{}", cfg.server_port);
    let listener = TcpListener::bind(&addr).await?;
    info!("HTTP server listening on {}", addr);

    tokio::select! {
        _ = serve(listener, app.into_make_service()) => {
            warn!("HTTP server stopped unexpectedly");
        },
        _ = tracker_task => {
            warn!("Tracker task ended unexpectedly");
        },
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down gracefully...");
        },
    }

    Ok(())
}

async fn init_pool_with_retry(db_url: &str) -> Result<sqlx::PgPool> {
    use tokio::time::{sleep, Duration};
    loop {
        match init_pool(db_url).await {
            Ok(pool) => return Ok(pool),
            Err(e) => {
                warn!("Database not ready yet: {:?}, retrying in 2s...", e);
                sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
