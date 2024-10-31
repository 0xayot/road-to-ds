use solana_client::rpc_response::RpcTransaction; // For transaction type
use solana_client::rpc_response::UiTransactionEncoding;
use {
    anyhow::{anyhow, Result},
    chrono::Utc,
    colored::*,
    log::{error, info},
    serde::{Deserialize, Serialize},
    serde_json::Value,
    solana_client::{
        nonblocking::pubsub_client::PubsubClient,
        nonblocking::rpc_client::RpcClient,
        rpc_config::{RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter},
    },
    solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature},
    std::{path::PathBuf, str::FromStr},
    tokio::{fs::OpenOptions, io::AsyncWriteExt},
};

const RAY_FEE: &str = "YOUR_RAY_FEE_ADDRESS_HERE";
const LP_OWNER: &str = "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1";
const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
const ERROR_LOG_PATH: &str = "error_new_lps_logs.txt";

#[derive(Debug, Serialize, Deserialize)]
struct TokenAmount {
    decimals: u8,
    amount: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenInfo {
    address: String,
    decimals: u8,
    lp_amount: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenData {
    lp_signature: String,
    // creator: String,
    timestamp: String,
    // base_info: TokenInfo,
    // quote_info: TokenInfo,
}

struct TokenMonitor {
    rpc_client: RpcClient,
    pubsub_client: PubsubClient,
    data_path: PathBuf,
}

impl TokenMonitor {
    pub async fn new(rpc_url: &str, ws_url: &str, data_path: PathBuf) -> Result<Self> {
        let pubsub_client = PubsubClient::new(ws_url).await?;
        Ok(Self {
            rpc_client: RpcClient::new_with_commitment(
                rpc_url.to_string(),
                CommitmentConfig::confirmed(),
            ),
            pubsub_client,
            data_path,
        })
    }

    async fn parse_transaction(&self, signature: &Signature) -> Result<Option<TokenData>> {
        let config = RpcTransactionConfig {
            max_supported_transaction_version: Some(0),
            commitment: Some(CommitmentConfig::confirmed()),
            encoding: Some(UiTransactionEncoding::Json), // Specify the encoding here
        };

        let transaction = self.rpc_client.get_transaction(signature, config).await?;

        if transaction
            .transaction
            .meta
            .as_ref()
            .map_or(true, |m| m.err.is_some())
        {
            return Ok(None);
        }

        info!("Successfully parsed transaction {:?}", transaction);

        let signer = transaction
            .transaction
            .message
            .account_keys
            .get(0)
            .ok_or_else(|| anyhow!("No signer found"))?
            .to_string();

        info!("Creator: {}", signer);

        let post_token_balances = transaction
            .transaction
            .meta
            .ok_or_else(|| anyhow!("No transaction metadata"))?
            .post_token_balances;

        let base_info = Self::extract_token_info(&post_token_balances, false)?;
        let quote_info = Self::extract_token_info(&post_token_balances, true)?;

        Ok(Some(TokenData {
            lp_signature: signature.to_string(),
            creator: signer,
            timestamp: Utc::now().to_rfc3339(),
            base_info,
            quote_info,
        }))
    }

    fn extract_token_info(balances: &[Value], is_quote: bool) -> Result<TokenInfo> {
        let balance = balances
            .iter()
            .find(|balance| {
                let owner = balance["owner"].as_str().unwrap_or_default();
                let mint = balance["mint"].as_str().unwrap_or_default();
                owner == LP_OWNER
                    && if is_quote {
                        mint == WSOL_MINT
                    } else {
                        mint != WSOL_MINT
                    }
            })
            .ok_or_else(|| anyhow!("Token info not found"))?;

        Ok(TokenInfo {
            address: balance["mint"].as_str().unwrap_or_default().to_string(),
            decimals: balance["uiTokenAmount"]["decimals"]
                .as_u64()
                .unwrap_or_default() as u8,
            lp_amount: balance["uiTokenAmount"]["uiAmount"]
                .as_f64()
                .unwrap_or_default(),
        })
    }

    async fn store_data(&self, data: &TokenData) -> Result<()> {
        let json = serde_json::to_string_pretty(data)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.data_path)
            .await?;

        file.write_all(json.as_bytes()).await?;
        file.write_all(b"\n").await?;
        Ok(())
    }

    async fn log_error(&self, error: &anyhow::Error) -> Result<()> {
        let error_message = format!(
            "Error occurred: {}\nTimestamp: {}\n",
            error,
            Utc::now().to_rfc3339()
        );

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(ERROR_LOG_PATH)
            .await?;

        file.write_all(error_message.as_bytes()).await?;
        Ok(())
    }

    pub async fn monitor_new_tokens(&self) -> Result<()> {
        println!("{}", "Monitoring new solana tokens...".green());

        let ray_fee_pubkey = Pubkey::from_str(RAY_FEE)?;

        let (mut notification_receiver, _subscription) = self
            .pubsub_client
            .logs_subscribe(
                RpcTransactionLogsFilter::Mentions(vec![ray_fee_pubkey.to_string()]),
                RpcTransactionLogsConfig {
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await?;

        while let Some(logs) = notification_receiver.recv().await {
            if let Err(err) = self.handle_log_notification(logs).await {
                error!("Error processing log: {}", err);
                self.log_error(&err).await?;
            }
        }

        Ok(())
    }

    async fn handle_log_notification(&self, logs: Value) -> Result<()> {
        let signature = Signature::from_str(
            logs["signature"]
                .as_str()
                .ok_or_else(|| anyhow!("No signature in logs"))?,
        )?;

        println!(
            "{}",
            format!("Found new token signature: {}", signature).on_green()
        );

        if let Some(token_data) = self.parse_transaction(&signature).await? {
            self.store_data(&token_data).await?;
        }

        Ok(())
    }
}

pub async fn run_token_monitor(rpc_url: &str, ws_url: &str, data_path: PathBuf) -> Result<()> {
    let monitor = TokenMonitor::new(rpc_url, ws_url, data_path).await?;
    monitor.monitor_new_tokens().await
}

#[tokio::main]
async fn main() -> Result<()> {
    let rpc_url = "https://api.mainnet-beta.solana.com";
    let ws_url = "wss://api.mainnet-beta.solana.com";
    let data_path = PathBuf::from("data/new_solana_tokens.json");

    run_token_monitor(rpc_url, ws_url, data_path).await
}
