use serde::{Deserialize, Serialize};
use serde_json::json;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::time::{sleep, Duration};

#[derive(Serialize, Deserialize, Debug)]
struct TokenData {
    lp_signature: String,
    creator: String,
    timestamp: String,
    tokens: Vec<TokenInfo>, // Changed to handle an arbitrary number of tokens
}

#[derive(Serialize, Deserialize, Debug)]
struct TokenInfo {
    address: String,
    decimals: u8,
    lp_amount: f64,
}

#[derive(Error, Debug)]
enum StoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

use solana_transaction_status::{token_balances, UiTransactionEncoding, UiTransactionTokenBalance};

const LP_OWNER: &str = "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1";
const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

async fn monitor_new_tokens(client: Arc<RpcClient>, ray_fee: Pubkey) {
    println!("Monitoring new Solana tokens...");

    loop {
        let signatures = match client.get_signatures_for_address(&ray_fee) {
            Ok(signatures) => signatures,
            Err(e) => {
                eprintln!("Error getting signatures: {}", e);
                sleep(Duration::from_secs(10)).await; // Wait and retry
                continue;
            }
        };

        for signature_info in signatures {
            let signature = match Signature::from_str(&signature_info.signature) {
                Ok(sig) => sig,
                Err(e) => {
                    eprintln!("Error parsing signature: {}", e);
                    continue;
                }
            };

            let config = RpcTransactionConfig {
                max_supported_transaction_version: Some(0),
                commitment: Some(CommitmentConfig::confirmed()),
                encoding: Some(UiTransactionEncoding::Json),
            };

            match client.get_transaction(&signature, UiTransactionEncoding::Json) {
                Ok(transaction) => {
                    let slot = transaction.slot;
                    let signatures = transaction.transaction.transaction;
                    let post_token_balances = transaction
                        .transaction
                        .meta
                        .as_ref()
                        .and_then(|meta| Some(meta.post_token_balances.clone()));

                    let owner_address = "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1";

                    // let token_balances: Vec<UiTransactionTokenBalance> = match post_token_balances {
                    //     Some(balances) => {
                    //         // Extract the inner Vec from the OptionSerializer
                    //         let inner_vec: Vec<UiTransactionTokenBalance> = balances.into();

                    //         // Filter the balances based on the owner address
                    //         inner_vec
                    //             .into_iter()
                    //             .filter(|balance| {
                    //                 // Check if the owner matches the given address
                    //                 balance.owner.as_ref()
                    //                     == Some(&owner_address.to_string()).into()
                    //             })
                    //             .collect()
                    //     }
                    //     None => vec![], // Return an empty vector if no balances were found
                    // };

                    // Create JSON object
                    let transaction_json = json!({
                        "slot": slot,
                        "signatures": signatures,
                        "post_token_balances": post_token_balances,
                    });

                    // Print or handle the JSON object as needed
                    println!("Transaction JSON: {}", transaction_json.to_string());
                }
                Err(e) => eprintln!("Error getting transaction: {}", e),
            }
        }

        sleep(Duration::from_secs(2)).await;
    }
}

fn store_data<P: AsRef<Path>>(path: P, data: &TokenData) -> Result<(), StoreError> {
    let file = OpenOptions::new().write(true).create(true).open(path)?;

    let mut writer = BufWriter::new(file);
    let json_data = serde_json::to_string(data)?;

    // Manage file size: Limit to a certain number of entries
    if writer.get_ref().metadata()?.len() > 1_000_000 {
        // 1 MB limit
        // Optional: Handle file rotation or truncation logic here
        println!("File size exceeded limit, consider implementing a rotation strategy.");
    }

    writeln!(writer, "{}", json_data)?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <RPC_URL> <RAY_FEE_ADDRESS>", args[0]);
        return;
    }

    let rpc_url =
        env::var("RPC_URL").unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());

    let raydium_address = "7YttLkHDoNj9wyDur5pM1ejNaAvT9X4eqaYcHQqtj2G5";

    let ray_fee = Pubkey::from_str(&raydium_address).expect("Invalid Ray Fee address");
    let client = Arc::new(RpcClient::new(rpc_url)); // Use configurable RPC endpoint

    monitor_new_tokens(client, ray_fee).await;
}
