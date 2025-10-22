use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{postgres::PgPoolOptions, Row};
pub use sqlx::PgPool;

pub async fn init_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    Ok(pool)
}

pub async fn insert_transfer_if_not_exists(
    pool: &PgPool,
    tx_hash: &str,
    log_index: u64,
    block_number: u64,
    from: &str,
    to: &str,
    amount: &Decimal,
    block_time: &DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO usdc_transfers
        (tx_hash, log_index, block_number, from_address, to_address, amount, block_time)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (tx_hash, log_index) DO NOTHING
        "#
    )
        .bind(tx_hash)
        .bind(log_index as i64)
        .bind(block_number as i64)
        .bind(from)
        .bind(to)
        .bind(amount)
        .bind(block_time)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn update_sync_state(pool: &PgPool, last_block: u64) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO sync_state (id, last_block, updated_at)
        VALUES (1, $1, now())
        ON CONFLICT (id) DO UPDATE
        SET last_block = EXCLUDED.last_block, updated_at = now()
        "#
    )
        .bind(last_block as i64)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn get_last_block_or_default(pool: &PgPool) -> Result<u64> {
    let start_block_env: u64 = std::env::var("START_BLOCK")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap_or(0);

    let row = sqlx::query("SELECT last_block FROM sync_state WHERE id = 1")
        .fetch_optional(pool)
        .await?;

    if let Some(record) = row {
        let db_block: i64 = record.get("last_block");
        let db_block = db_block as u64;

        if start_block_env > db_block {
            update_sync_state(pool, start_block_env).await?;
            Ok(start_block_env)
        }
        else {
            Ok(db_block)
        }
    }
    else {
        update_sync_state(pool, start_block_env).await?;
        Ok(start_block_env)
    }
}
