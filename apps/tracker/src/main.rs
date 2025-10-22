use std::sync::Arc;
use anyhow::Result;
use axum::serve;
use tokio::{net::TcpListener, task};

use config::AppConfig;
use db::init_pool;
use api::create_router;
use service::fetchers::take_and_push_transactions;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = AppConfig::from_env();

    let pool = init_pool_with_retry(&cfg.db_url).await?;
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

async fn init_pool_with_retry(db_url: &str) -> Result<sqlx::PgPool> {
    use tokio::time::{sleep, Duration};
    loop {
        match init_pool(db_url).await {
            Ok(pool) => return Ok(pool),
            Err(_) => {
                sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
