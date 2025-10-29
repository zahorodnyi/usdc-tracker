use once_cell::sync::OnceCell;
use serde::Deserialize;
use anyhow::Result;

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub rpc_http: String,
    pub rpc_ws: String,
    pub usdc_contract: String,
    pub start_block: u64,
    pub db_url: String,
    pub server_port: u16,
}

impl AppConfig {
    pub fn from_env() -> Self {
        dotenv::dotenv().ok();
        Self {
            rpc_http: std::env::var("RPC_HTTP").expect("RPC_HTTP must be set"),
            rpc_ws: std::env::var("RPC_WS").expect("RPC_WS must be set"),
            usdc_contract: std::env::var("USDC_CONTRACT").expect("USDC_CONTRACT must be set"),
            start_block: std::env::var("START_BLOCK")
                .expect("START_BLOCK must be set")
                .parse()
                .expect("START_BLOCK must be a number"),
            db_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap(),
        }
    }
}

static CONFIG: OnceCell<AppConfig> = OnceCell::new();

pub async fn init() -> Result<&'static AppConfig> {
    dotenv::dotenv().ok();
    Ok(CONFIG.get_or_init(AppConfig::from_env))
}

pub fn get() -> &'static AppConfig {
    CONFIG.get().expect("config::init() must be called first")
}
