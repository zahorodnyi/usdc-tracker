use sqlx::postgres::PgPoolOptions;
use dotenv::dotenv;
use std::env;

pub async fn get_db_pool() -> anyhow::Result<sqlx::PgPool> {
    dotenv().ok();

    let db_url = env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    println!("✅ Connected to PostgreSQL!");
    Ok(pool)
}