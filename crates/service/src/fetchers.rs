use std::env;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use dotenv::dotenv;
use ethers::prelude::*;
use ethers::types::U256;
use ethers::utils::keccak256;
use rust_decimal::Decimal;
use tokio::time::{sleep, Duration};
use db::{insert_transfer_if_not_exists, update_sync_state, get_last_block, PgPool};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};


const TRANSFER_EVENT_SIG: &str = "Transfer(address,address,uint256)";
const LOGS_BATCH_SIZE: u64 = 100;
const HISTORICAL_SLEEP_MS: u64 = 200;
const RATE_LIMIT_WAIT_SECS: u64 = 10;
const RETRY_TIMES: u64 = 3;

enum RpcErrorKind {
    RateLimited,
    TooManyLogs,
    Temporary,
    Fatal,
}

struct BatchSizer {
    original: u64,
    current: u64,
}

impl BatchSizer {
    fn new(original: u64) -> Self {
        Self { original, current: original }
    }

    fn halve(&mut self) {
        self.current = std::cmp::max(1, self.current / 2);
    }

    fn reset(&mut self) {
        self.current = self.original;
    }

    fn is_min(&self) -> bool {
        self.current <= 1
    }
}


pub async fn take_and_push_transactions(pool: Arc<PgPool>) -> anyhow::Result<()> {
    dotenv().ok();

    let rpc_http = env::var("RPC_HTTP")?;
    let rpc_ws = env::var("RPC_WS")?;
    let usdc_address: Address = env::var("USDC_CONTRACT")?.parse()?;
    let start_block = get_last_block(&pool).await?;


    let provider_http = Provider::<Http>::try_from(rpc_http.clone())?;
    let provider_ws = Provider::<Ws>::connect(rpc_ws.clone()).await?;
    let provider_ws = Arc::new(provider_ws);

    let transfer_topic = H256::from_slice(&keccak256(TRANSFER_EVENT_SIG));

    let _latest_block = process_historical_transactions(&provider_http, usdc_address, start_block, transfer_topic, &pool).await?;

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
    

    let mut current = start_block;
    let mut batch = BatchSizer::new(LOGS_BATCH_SIZE);

    while current <= latest_block {

        let mut attempt = 0;
        let mut success = false;

        while attempt < RETRY_TIMES {
            let end = std::cmp::min(current + batch.current - 1, latest_block);
            let filter = Filter::new()
                .address(usdc_address)
                .from_block(current)
                .to_block(end)
                .topic0(transfer_topic);

            let response = provider_http.get_logs(&filter).await;

            match response {
                Ok(logs) => {
                    let mut last_block: Option<U64> = None;
                    let mut last_block_time: Option<DateTime<Utc>> = None;

                    for log in logs {
                        if let Some((from, to, amount)) = decode_transfer(&log) {
                            if log.block_number != last_block {
                                last_block = log.block_number;
                                last_block_time = get_block_time(&provider_http, log.block_number).await;
                            }

                            if let Some(datetime) = last_block_time {
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
                        }
                    }

                    update_sync_state(pool, end).await?;
                    current = end + 1;
                    batch.reset();
                    sleep(Duration::from_millis(HISTORICAL_SLEEP_MS)).await;
                    success = true;
                    break;
                }

                Err(err) => {
                    let msg = err.to_string();
                    let kind = classify_rpc_error(&msg);
                    if !handle_rpc_error(kind, &mut current, &mut batch, &mut attempt).await {
                        break;
                    }
                }
            }
        }

        if !success {
            current = current.saturating_add(1);
            batch.reset();
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

    let filter_live = Filter::new()
        .address(usdc_address)
        .topic0(transfer_topic);

    let can_update_sync_state = Arc::new(AtomicBool::new(false));

    let mut sub = provider_ws.subscribe_logs(&filter_live).await?;

    let pool_clone = pool.clone();
    let provider_http_clone = provider_http.clone();
    let usdc_address_clone = usdc_address;
    let transfer_topic_clone = transfer_topic;

    let can_update_clone = can_update_sync_state.clone();

    let _historical_handle = tokio::spawn(async move {
        if let Ok(last_stored_block) = get_last_block(&pool_clone).await {
            if let Ok(current_block) = provider_http_clone.get_block_number().await {
                let current_block = current_block.as_u64();
                if current_block > last_stored_block {
                    if let Err(_e) = process_historical_transactions(
                        &provider_http_clone,
                        usdc_address_clone,
                        last_stored_block,
                        transfer_topic_clone,
                        &pool_clone,
                    ).await {
                    }
                    else {
                        can_update_clone.store(true, Ordering::SeqCst);
                    }
                }
                else {
                    can_update_clone.store(false, Ordering::SeqCst);
                }
            }
        }
    });


    let mut last_block: Option<U64> = None;
    let mut last_block_time: Option<DateTime<Utc>> = None;

    while let Some(log) = sub.next().await {
        if let Some((from, to, amount)) = decode_transfer(&log) {
            if log.block_number != last_block {
                last_block = log.block_number;
                last_block_time = get_block_time(&provider_http, log.block_number).await;
            }
            if let (Some(_block_number), Some(datetime)) = (log.block_number, last_block_time) {
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

                    if can_update_sync_state.load(Ordering::SeqCst) {
                        update_sync_state(pool, log.block_number.unwrap().as_u64()).await?;
                    }
                }
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


fn classify_rpc_error(msg: &str) -> RpcErrorKind {
    if msg.contains("Too Many Requests") {
        RpcErrorKind::RateLimited
    } else if msg.contains("query returned more than 10000 results") {
        RpcErrorKind::TooManyLogs
    } else if msg.contains("timeout") || msg.contains("Temporary failure") {
        RpcErrorKind::Temporary
    } else {
        RpcErrorKind::Fatal
    }
}



async fn handle_rpc_error(
    kind: RpcErrorKind,
    current: &mut u64,
    batch: &mut BatchSizer,
    attempt: &mut u64,
) -> bool {
    use RpcErrorKind::*;

    static mut LAST_WAS_BATCH_REDUCTION: bool = false;

    match kind {
        RateLimited => {
            unsafe {
                if LAST_WAS_BATCH_REDUCTION {
                    sleep(Duration::from_secs(2)).await;
                    LAST_WAS_BATCH_REDUCTION = false;
                }
                else {
                    sleep(Duration::from_secs(RATE_LIMIT_WAIT_SECS)).await;
                }
            }
            *attempt += 1;
            true
        }

        TooManyLogs => {
            if !batch.is_min() {
                batch.halve();
                unsafe { LAST_WAS_BATCH_REDUCTION = true; }
                sleep(Duration::from_millis(300)).await;
                true
            } else {
                *current += 1;
                batch.reset();
                unsafe { LAST_WAS_BATCH_REDUCTION = false; }
                false
            }
        }

        Temporary => {
            sleep(Duration::from_millis(500)).await;
            *attempt += 1;
            unsafe { LAST_WAS_BATCH_REDUCTION = false; }
            true
        }

        Fatal => {
            unsafe { LAST_WAS_BATCH_REDUCTION = false; }
            false
        }
    }
}
