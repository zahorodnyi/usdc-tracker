mod take_transactions;
mod db;
mod api;



#[tokio::main]
async fn main() -> anyhow::Result<()> {
    //take_transactions::take_transactions().await
    let _pool = db::get_db_pool().await?;
    Ok(())
}