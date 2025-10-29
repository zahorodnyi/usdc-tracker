use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use db::{PgPool, UsdcTransfer, PostgresRepo, ReadData};

#[derive(Deserialize)]
struct TransferFilter {
    from: Option<String>,
    to: Option<String>,
    created_before: Option<DateTime<Utc>>,
    created_after: Option<DateTime<Utc>>,
    page: Option<u32>,
    limit: Option<u32>,
}

pub fn create_router(pool: Arc<PgPool>) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/last_block", get(get_last_block))
        .route("/tx/{id}", get(get_transfer_by_id))
        .route("/tx", get(list_transfers))
        .with_state(pool)
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn get_last_block(State(pool): State<Arc<PgPool>>) -> Json<serde_json::Value> {
    let repo = PostgresRepo::new(pool.as_ref().clone());
    let last_block = repo.get_last_block().await.unwrap_or(0);
    Json(serde_json::json!({ "last_block": last_block }))
}

async fn get_transfer_by_id(
    State(pool): State<Arc<PgPool>>,
    Path(id): Path<i64>,
) -> Json<Option<UsdcTransfer>> {
    let repo = PostgresRepo::new(pool.as_ref().clone());
    let tx = repo.get_transfer_by_id(id).await.unwrap_or(None);
    Json(tx)
}

async fn list_transfers(
    State(pool): State<Arc<PgPool>>,
    Query(filter): Query<TransferFilter>,
) -> Json<Vec<UsdcTransfer>> {
    let repo = PostgresRepo::new(pool.as_ref().clone());
    let txs = repo
        .list_transfers(
            filter.from,
            filter.to,
            filter.created_before,
            filter.created_after,
            filter.page,
            filter.limit,
        )
        .await
        .unwrap_or_default();
    Json(txs)
}
