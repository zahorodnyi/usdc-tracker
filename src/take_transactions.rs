use std::env;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dotenv::dotenv;
use ethers::prelude::*;
use ethers::types::U256;
use ethers::utils::keccak256;
use rust_decimal::Decimal;
use tokio::time::{sleep, Duration};
use crate::db::{init_pool, insert_transfer_if_not_exists, update_sync_state, get_last_block_or_default};

const TRANSFER_EVENT_SIG: &str = "Transfer(address,address,uint256)";
const LOGS_BATCH_SIZE: u64 = 100;
const HISTORICAL_SLEEP_MS: u64 = 200;
const RATE_LIMIT_WAIT_SECS: u64 = 10;

pub async fn take_transactions() -> anyhow::Result<()> {
    dotenv().ok();
    //dotenv::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/.env")).ok();



    let rpc_http = env::var("RPC_HTTP")?;
    let rpc_ws = env::var("RPC_WS")?;
    let usdc_address: Address = env::var("USDC_CONTRACT")?.parse()?;
    let pool = init_pool().await?;
    let start_block = get_last_block_or_default(&pool).await?;


    let provider_http = Provider::<Http>::try_from(rpc_http.clone())?;
    let provider_ws = Provider::<Ws>::connect(rpc_ws.clone()).await?;
    let provider_ws = Arc::new(provider_ws);

    let transfer_topic = H256::from_slice(&keccak256(TRANSFER_EVENT_SIG));

    let latest_block = process_historical_transactions(&provider_http, usdc_address, start_block, transfer_topic, &pool).await?;

    process_live_transactions(&provider_http, provider_ws, usdc_address, transfer_topic, &pool).await?;

    Ok(())
}

async fn process_historical_transactions(
    provider_http: &Provider<Http>,
    usdc_address: Address,
    start_block: u64,
    transfer_topic: H256,
    pool: &sqlx::PgPool,
) -> anyhow::Result<u64> {
    let latest_block = provider_http.get_block_number().await?.as_u64();

    println!("🔍 Зчитуємо USDC Transfer з блоків {start_block}..{latest_block}");

    let mut current = start_block;

    while current <= latest_block {
        let end = std::cmp::min(current + LOGS_BATCH_SIZE, latest_block);

        let filter = Filter::new()
            .address(usdc_address)
            .from_block(current)
            .to_block(end)
            .topic0(transfer_topic);

        let response = provider_http.get_logs(&filter).await;

        match response {
            Ok(logs) => {
                println!("📦 Отримано {} подій із блоків {current}..{end}", logs.len());

                let mut last_block: Option<U64> = None;
                let mut last_block_time: Option<DateTime<Utc>> = None;
                
                for log in logs {
                    if let Some((from, to, amount)) = decode_transfer(&log) {
                        if log.block_number != last_block {
                            last_block = log.block_number;
                            last_block_time = get_block_time(&provider_http, log.block_number).await;
                        }
                        
                        if let Some(datetime) = last_block_time {
                            //println!("📜 {from:?} → {to:?} : {amount} USDC 🕒 {datetime}");

                            if let (Some(tx_hash), Some(li)) = (log.transaction_hash, log.log_index) {
                                insert_transfer_if_not_exists(
                                    pool,
                                    &format!("{:?}", tx_hash),
                                    li.as_u64(),
                                    log.block_number.unwrap().as_u64(),
                                    &format!("{:?}", from),
                                    &format!("{:?}", to),
                                    &amount,
                                    &datetime,
                                ).await?;
                            }
                        }
                        else {
                            //println!("📜 {from:?} → {to:?} : {amount} USDC");
                        }
                    }
                }

                update_sync_state(pool, end).await?;
                current = end + 1;
                sleep(Duration::from_millis(HISTORICAL_SLEEP_MS)).await;
            }

            Err(err) => {
                let msg = err.to_string();

                if msg.contains("Too Many Requests") {
                    println!("⏳ Rate limit Infura — чекаємо 10 секунд...");
                    sleep(Duration::from_secs(RATE_LIMIT_WAIT_SECS)).await;
                    continue;
                }

                if msg.contains("query returned more than 10000 results") {
                    println!("⚠️ Забагато подій у блоках {current}..{end} — пропускаємо без затримки.");
                    current = end + 1;
                    continue;
                }

                println!("⚠️ Помилка при читанні {current}..{end}: {msg}");
            }
        }
    }

    Ok(latest_block)
}

async fn process_live_transactions(
    provider_http: &Provider<Http>,
    provider_ws: Arc<Provider<Ws>>,
    usdc_address: Address,
    transfer_topic: H256,
    pool: &sqlx::PgPool,
) -> anyhow::Result<()> {
    println!("🚀 Підключення до WebSocket для нових подій...");

    let filter_live = Filter::new()
        .address(usdc_address)
        .topic0(transfer_topic);

    let mut sub = provider_ws.subscribe_logs(&filter_live).await?;

    let pool_clone = pool.clone();
    let provider_http_clone = provider_http.clone();
    let usdc_address_clone = usdc_address;
    let transfer_topic_clone = transfer_topic;

    let historical_handle = tokio::spawn(async move {
        if let Ok(last_stored_block) = get_last_block_or_default(&pool_clone).await {
            if let Ok(current_block) = provider_http_clone.get_block_number().await {
                let current_block = current_block.as_u64();
                if current_block > last_stored_block {
                    println!("📜 Дочитуємо пропущені блоки: {}..{}", last_stored_block + 1, current_block);
                    if let Err(e) = process_historical_transactions(
                        &provider_http_clone,
                        usdc_address_clone,
                        last_stored_block + 1,
                        transfer_topic_clone,
                        &pool_clone,
                    ).await {
                        eprintln!("⚠️ Помилка при дочитуванні пропущених блоків: {e}");
                    } else {
                        println!("✅ Пропущені блоки заповнено");
                    }
                } else {
                    println!("✅ Пропущених блоків немає");
                }
            }
        }
    });


    let mut last_block: Option<U64> = None;
    let mut last_block_time: Option<DateTime<Utc>> = None;
    let mut can_update_sync_state = false;

    while let Some(log) = sub.next().await {
        if let Some((from, to, amount)) = decode_transfer(&log) {
            if log.block_number != last_block {
                last_block = log.block_number;
                last_block_time = get_block_time(&provider_http, log.block_number).await;
            }
            if let (Some(block_number), Some(datetime)) = (log.block_number, last_block_time) {
            //if let Some(datetime) = last_block_time {
                println!("⚡ Live: block #{block_number} | {from:?} → {to:?} : {amount} USDC 🕒 {datetime}");
                if let (Some(tx_hash), Some(li)) = (log.transaction_hash, log.log_index) {
                    insert_transfer_if_not_exists(
                        pool,
                        &format!("{:?}", tx_hash),
                        li.as_u64(),
                        log.block_number.unwrap().as_u64(),
                        &format!("{:?}", from),
                        &format!("{:?}", to),
                        &amount,
                        &datetime,
                    ).await?;

                    if can_update_sync_state {
                        update_sync_state(pool, log.block_number.unwrap().as_u64()).await?;
                    }
                }
            }
            else {
                //println!("⚡ Live: {from:?} → {to:?} : {amount} USDC");
            }
        }
    }

    Ok(())
}

fn decode_transfer(log: &Log) -> Option<(Address, Address, Decimal)> {
    if log.topics.len() != 3 {
        return None;
    }

    let from = Address::from_slice(&log.topics[1].as_bytes()[12..]);
    let to = Address::from_slice(&log.topics[2].as_bytes()[12..]);
    let amount = U256::from_big_endian(&log.data.0);

    let raw_str = amount.to_string();
    let raw_dec = Decimal::from_str(&raw_str).ok()?;
    let divisor = Decimal::new(1, 6);
    let value = raw_dec * divisor;

    Some((from, to, value))
}

async fn get_block_time(provider: &Provider<Http>, block_number: Option<U64>) -> Option<DateTime<Utc>> {
    if let Some(block_number) = block_number {
        if let Ok(Some(block)) = provider.get_block(block_number).await {
            let ts = block.timestamp.as_u64() as i64;
            return DateTime::from_timestamp(ts, 0);
        }
    }
    None
}