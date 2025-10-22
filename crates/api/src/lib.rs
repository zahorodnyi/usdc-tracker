use axum::{
    routing::{get, post},
    extract::State,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use db::{self, PgPool};
use std::sync::Arc;


#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct LastBlockResponse {
    last_block: u64,
}

#[derive(Deserialize)]
struct UpdateSyncRequest {
    last_block: u64,
}


pub fn create_router(pool: Arc<PgPool>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/last_block", get(get_last_block))
        .route("/update_sync", post(update_sync))
        .with_state(pool)
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn get_last_block(State(pool): State<Arc<PgPool>>) -> Json<LastBlockResponse> {
    let last_block = db::get_last_block_or_default(&pool).await.unwrap_or(0);
    Json(LastBlockResponse { last_block })
}

async fn update_sync(
    State(pool): State<Arc<PgPool>>,
    Json(payload): Json<UpdateSyncRequest>,
) -> Json<HealthResponse> {
    if db::update_sync_state(&pool, payload.last_block).await.is_ok() {
        Json(HealthResponse { status: "updated" })
    }
    else {
        Json(HealthResponse { status: "error" })
    }
}
