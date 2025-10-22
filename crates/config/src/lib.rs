use serde::Deserialize;

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

    pub fn clone_for_task(&self) -> Self {
        Self {
            rpc_http: self.rpc_http.clone(),
            rpc_ws: self.rpc_ws.clone(),
            usdc_contract: self.usdc_contract.clone(),
            start_block: self.start_block,
            db_url: self.db_url.clone(),
            server_port: self.server_port,
        }
    }
}
