use std::{env, time::Duration};

use bookway_mall_inventory_api::pb::{
    BatchRequest, ExpireReservationsResponse, mall_inventory_client::MallInventoryClient,
};
use thiserror::Error;

#[derive(Debug, Error)]
enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
}

#[derive(Debug, Error)]
enum WorkerError {
    #[error("mall-inventory request failed: {0}")]
    Grpc(#[from] tonic::Status),
    #[error("service authentication failed: {0}")]
    ServiceAuth(#[from] bookway_runtime::GrpcServiceAuthError),
}

struct Config {
    inventory_url: String,
    batch_size: u32,
    idle_interval: Duration,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            inventory_url: env::var("MALL_INVENTORY_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8102".to_string()),
            batch_size: env_number("MALL_INVENTORY_SWEEPER_BATCH_SIZE", 100_u32)?.clamp(1, 1_000),
            idle_interval: Duration::from_millis(
                env_number("MALL_INVENTORY_SWEEPER_IDLE_MS", 1_000_u64)?.clamp(100, 60_000),
            ),
        })
    }
}

fn env_number<T>(key: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    match env::var(key) {
        Ok(value) => value
            .parse()
            .map_err(|_| ConfigError::Invalid { key, value }),
        Err(_) => Ok(default),
    }
}

async fn run_once(
    client: &mut MallInventoryClient<tonic::transport::Channel>,
    batch_size: u32,
) -> Result<ExpireReservationsResponse, WorkerError> {
    Ok(client
        .expire_reservations(bookway_runtime::grpc_service_request(BatchRequest {
            limit: batch_size,
        })?)
        .await?
        .into_inner())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("mall-inventory-sweeper");
    let config = Config::from_env()?;
    let mut client = MallInventoryClient::connect(config.inventory_url.clone()).await?;
    loop {
        match run_once(&mut client, config.batch_size).await {
            Ok(result) if result.expired == 0 => tokio::time::sleep(config.idle_interval).await,
            Ok(result) => {
                tracing::info!(expired = result.expired, "inventory expiry batch processed")
            }
            Err(error) => {
                tracing::error!(%error, "inventory expiry batch failed");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
