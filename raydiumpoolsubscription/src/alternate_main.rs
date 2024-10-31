use anyhow::Result;
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use std::str::FromStr;
use tokio::time::{sleep, Duration};

pub struct RaydiumPoolListener {
    rpc_client: RpcClient,
    amm_program_id: Pubkey,
}

impl RaydiumPoolListener {
    pub fn new(rpc_url: &str) -> Self {
        let rpc_client =
            RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

        // Raydium AMM Program ID
        let amm_program_id = Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8")
            .expect("Failed to parse Raydium AMM program ID");

        Self {
            rpc_client,
            amm_program_id,
        }
    }

    pub async fn start_listening(&self) -> Result<()> {
        println!("Starting to listen for new Raydium pool creation...");

        // Keep track of pools we've already seen
        let mut known_pools = self.get_existing_pools()?;
        println!("Found {} existing pools", known_pools.len());

        loop {
            // Get current pools
            let current_pools = self.get_existing_pools()?;

            // Find new pools
            for pool in current_pools.iter() {
                if !known_pools.contains(pool) {
                    println!("New pool detected: {}", pool);
                    // Here you can add custom logic to handle new pools
                    self.process_new_pool(pool)?;
                }
            }

            // Update known pools
            known_pools = current_pools;

            // Wait before next check
            sleep(Duration::from_secs(1)).await;
        }
    }

    fn get_existing_pools(&self) -> Result<Vec<Pubkey>> {
        let config = RpcProgramAccountsConfig {
            filters: Some(vec![
                RpcFilterType::DataSize(592), // Raydium pool account size
            ]),
            account_config: RpcAccountInfoConfig {
                encoding: None,
                data_slice: None,
                commitment: Some(CommitmentConfig::confirmed()),
                min_context_slot: None,
            },
            with_context: None,
        };

        let accounts = self
            .rpc_client
            .get_program_accounts_with_config(&self.amm_program_id, config)?;

        Ok(accounts.into_iter().map(|(pubkey, _)| pubkey).collect())
    }

    fn process_new_pool(&self, pool_address: &Pubkey) -> Result<()> {
        // Get pool account data
        let account = self.rpc_client.get_account(pool_address)?;

        // Here you would add your custom logic to:
        // 1. Decode the pool data
        // 2. Extract token pairs
        // 3. Get pool parameters
        // 4. Trigger any notifications or actions

        println!("Processing new pool: {}", pool_address);
        println!("Data length: {} bytes", account.data.len());

        Ok(())
    }
}

// Usage example
#[tokio::main]
async fn main() -> Result<()> {
    let rpc_url = "https://raydium-raydium-5ad5.mainnet.rpcpool.com";
    let listener = RaydiumPoolListener::new(rpc_url);
    listener.start_listening().await?;
    Ok(())
}
