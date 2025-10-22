
pub mod fetchers;

pub use common::errors::*;

use std::sync::Arc;
use anyhow::Result;
use db::PgPool;

pub async fn start_tracker(pool: Arc<PgPool>) -> Result<()> {
    fetchers::take_and_push_transactions(pool).await
}
