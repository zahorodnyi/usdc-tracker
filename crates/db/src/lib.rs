use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
pub use sqlx::{postgres::PgPoolOptions, PgPool, FromRow};
use serde::Serialize;
use sqlx::Row;


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

#[derive(Serialize, FromRow, Debug)]
pub struct UsdcTransfer {
    pub id: i64,
    pub tx_hash: String,
    pub log_index: i64,
    pub block_number: i64,
    pub from_address: String,
    pub to_address: String,
    pub amount: Decimal,
    pub block_time: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

pub async fn get_transfer_by_id(pool: &PgPool, id: i64) -> Result<Option<UsdcTransfer>> {
    let record = sqlx::query_as::<_, UsdcTransfer>(
        r#"
        SELECT id, tx_hash, log_index, block_number, from_address, to_address, amount, block_time, created_at
        FROM usdc_transfers
        WHERE id = $1
        "#,
    )
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(record)
}

pub async fn list_transfers(
    pool: &PgPool,
    from: Option<String>,
    to: Option<String>,
    created_before: Option<DateTime<Utc>>,
    created_after: Option<DateTime<Utc>>,
    page: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<UsdcTransfer>> {
    let limit = limit.unwrap_or(20).min(100); // безпека
    let offset = (page.unwrap_or(1).saturating_sub(1) * limit) as i64;

    let mut conditions = Vec::new();
    let mut binds: Vec<(usize, String)> = Vec::new();
    let mut bind_index = 1;

    if let Some(ref addr) = from {
        conditions.push(format!("from_address = ${}", bind_index));
        binds.push((bind_index, addr.clone()));
        bind_index += 1;
    }
    if let Some(ref addr) = to {
        conditions.push(format!("to_address = ${}", bind_index));
        binds.push((bind_index, addr.clone()));
        bind_index += 1;
    }
    if created_before.is_some() {
        conditions.push(format!("block_time < ${}", bind_index));
        bind_index += 1;
    }
    if created_after.is_some() {
        conditions.push(format!("block_time > ${}", bind_index));
        bind_index += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::from("")
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let query = format!(
        r#"
        SELECT id, tx_hash, log_index, block_number, from_address, to_address, amount, block_time, created_at
        FROM usdc_transfers
        {}
        ORDER BY block_time DESC
        LIMIT ${} OFFSET ${}
        "#,
        where_clause,
        bind_index,
        bind_index + 1
    );

    let mut q = sqlx::query_as::<_, UsdcTransfer>(&query);

    for (_, val) in binds {
        q = q.bind(val);
    }

    if let Some(v) = created_before {
        q = q.bind(v);
    }
    if let Some(v) = created_after {
        q = q.bind(v);
    }

    q = q.bind(limit as i64).bind(offset);

    let transfers = q.fetch_all(pool).await?;
    Ok(transfers)
}
