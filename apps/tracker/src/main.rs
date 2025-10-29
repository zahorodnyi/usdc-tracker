use std::sync::Arc;
use anyhow::Result;
use axum::serve;
use tokio::{net::TcpListener, task};

use config;
use db::{init_pool, PostgresRepo, WriteData};
use api::create_router;
use service::fetchers::take_and_push_transactions;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = config::init().await?;

    let pool = init_pool_with_retry(&cfg.db_url, cfg.start_block).await?;
    let pool = Arc::new(pool);

    let tracker_task = {
        let pool = Arc::clone(&pool);
        task::spawn(async move {
            let _ = take_and_push_transactions(pool.clone()).await;
        })
    };

    let app = create_router(pool.clone());
    let addr = format!("0.0.0.0:{}", cfg.server_port);
    let listener = TcpListener::bind(&addr).await?;

    tokio::select! {
        _ = serve(listener, app.into_make_service()) => {},
        _ = tracker_task => {},
        _ = tokio::signal::ctrl_c() => {},
    }

    Ok(())
}

async fn init_pool_with_retry(db_url: &str, start_block: u64) -> Result<sqlx::PgPool> {
    use tokio::time::{sleep, Duration};

    const MAX_RETRIES: usize = 10;
    let mut attempts = 0;

    while attempts < MAX_RETRIES {
        if let Ok(pool) = init_pool(db_url).await {
            let repo = PostgresRepo::new(pool.clone());
            repo.update_sync_state_if_needs(start_block).await?;
            return Ok(pool);
        }
        attempts += 1;
        sleep(Duration::from_secs(2)).await;
    }
    Err(anyhow::Error::msg("database connection failed"))
}
