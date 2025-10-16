mod take_transactions;
mod db;
mod api;

use std::sync::Arc;
use axum::serve;
use dotenv::dotenv;
use tokio::{task, net::TcpListener, time::{sleep, Duration}};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let pool = loop {
        match db::init_pool().await {
            Ok(pool) => break pool,
            Err(e) => {
                //eprintln!("⏳ DB not ready yet: {e}");
                sleep(Duration::from_secs(2)).await;
            }
        }
    };

    let pool = Arc::new(pool);

    let tracker_task = {
        let pool = Arc::clone(&pool);
        task::spawn(async move {
            if let Err(e) = take_transactions::take_transactions(pool.clone()).await {
                //eprintln!("❌ Tracker error: {:?}", e);
            }
        })
    };

    let app = api::create_router(pool.clone());

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await?;
    //println!("🚀 API server running at http://{}", addr);

    tokio::select! {
        _ = serve(listener, app.into_make_service()) => {},
        _ = tracker_task => {},
    }

    Ok(())
}
