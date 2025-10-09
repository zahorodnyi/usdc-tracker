use ethers::prelude::*;
use ethers::utils::keccak256;
use dotenv::dotenv;
use std::env;
use std::sync::Arc;
use rust_decimal::Decimal;
use ethers::types::U256;
use std::str::FromStr;
use tokio::time::{sleep, Duration};
use chrono::{DateTime, Utc};

pub async fn take_transactions() -> anyhow::Result<()> {
    dotenv().ok();

    let rpc_http = env::var("RPC_HTTP")?;
    let rpc_ws = env::var("RPC_WS")?;
    let usdc_address: Address = env::var("USDC_CONTRACT")?.parse()?;
    let start_block: u64 = env::var("START_BLOCK")?.parse()?;

    let provider_http = Provider::<Http>::try_from(rpc_http.clone())?;
    let provider_ws = Provider::<Ws>::connect(rpc_ws.clone()).await?;
    let provider_ws = Arc::new(provider_ws);

    let transfer_topic = H256::from_slice(&keccak256("Transfer(address,address,uint256)"));

    let latest_block = provider_http.get_block_number().await?.as_u64();

    println!("🔍 Зчитуємо USDC Transfer з блоків {start_block}..{latest_block}");

    let batch_size: u64 = 100;
    let mut current = start_block;

    while current <= latest_block {
        let end = std::cmp::min(current + batch_size, latest_block);

        let filter = Filter::new()
            .address(usdc_address)
            .from_block(current)
            .to_block(end)
            .topic0(transfer_topic);

        let response = provider_http.get_logs(&filter).await;

        match response {
            Ok(logs) => {
                println!("📦 Отримано {} подій із блоків {current}..{end}", logs.len());
                for log in logs {
                    if let Some((from, to, amount)) = decode_transfer(&log) {
                        let time = get_block_time(&provider_http, log.block_number).await;
                        if let Some(datetime) = time {
                            println!("📜 {from:?} → {to:?} : {amount} USDC 🕒 {datetime}");
                        }
                        else {
                            println!("📜 {from:?} → {to:?} : {amount} USDC");
                        }
                    }
                }

                current = end + 1;
                sleep(Duration::from_millis(200)).await;
            }

            Err(err) => {
                let msg = err.to_string();

                if msg.contains("Too Many Requests") {
                    println!("⏳ Rate limit Infura — чекаємо 10 секунд...");
                    sleep(Duration::from_secs(10)).await;
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

    println!("🚀 Підключення до WebSocket для нових подій...");

    let filter_live = Filter::new()
        .address(usdc_address)
        .topic0(transfer_topic);

    let mut sub = provider_ws.subscribe_logs(&filter_live).await?;

    while let Some(log) = sub.next().await {
        if let Some((from, to, amount)) = decode_transfer(&log) {
            let time = get_block_time(&provider_http, log.block_number).await;
            if let Some(datetime) = time {
                println!("⚡ Live: {from:?} → {to:?} : {amount} USDC 🕒 {datetime}");
            }
            else {
                println!("⚡ Live: {from:?} → {to:?} : {amount} USDC");
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