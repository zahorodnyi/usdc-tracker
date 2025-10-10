use sqlx::{PgPool, postgres::PgPoolOptions};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use anyhow::Result;

pub async fn init_pool() -> Result<PgPool> {
    let db_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    Ok(pool)
}

pub async fn insert_transfer_if_not_exists(
    pool: &PgPool,
    tx_hash: &str,
    block_number: u64,
    from: &str,
    to: &str,
    amount: &Decimal,
    block_time: &DateTime<Utc>,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO usdc_transfers (tx_hash, block_number, from_address, to_address, amount, block_time)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (tx_hash) DO NOTHING
        "#,
        tx_hash,
        block_number as i64,
        from,
        to,
        amount,
        block_time
    )
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn update_sync_state(pool: &PgPool, last_block: u64) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO sync_state (id, last_block, updated_at)
        VALUES (1, $1, now())
        ON CONFLICT (id) DO UPDATE
        SET last_block = EXCLUDED.last_block, updated_at = now()
        "#,
        last_block as i64
    )
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_last_block_or_default(pool: &PgPool) -> Result<u64> {
    let record = sqlx::query!(
        "SELECT last_block FROM sync_state WHERE id = 1"
    )
        .fetch_optional(pool)
        .await?;

    if let Some(row) = record {
        Ok(row.last_block as u64)
    } else {
        let start_block: u64 = std::env::var("START_BLOCK")?.parse()?;
        Ok(start_block)
    }
}
