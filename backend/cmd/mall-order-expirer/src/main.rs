use std::{env, time::Duration};

use bookway_mall_order_api::pb::{BatchRequest, mall_order_client::MallOrderClient};
use thiserror::Error;

#[derive(Debug, Error)]
enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
}

#[derive(Debug, Error)]
enum WorkerError {
    #[error("mall-order request failed: {0}")]
    Grpc(#[from] tonic::Status),
    #[error("service authentication failed: {0}")]
    ServiceAuth(#[from] bookway_runtime::GrpcServiceAuthError),
}

struct Config {
    mall_order_url: String,
    batch_size: u32,
    idle_interval: Duration,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            mall_order_url: env::var("MALL_ORDER_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8103".to_string()),
            batch_size: env_number("MALL_ORDER_EXPIRER_BATCH_SIZE", 100_u32)?.clamp(1, 1_000),
            idle_interval: Duration::from_millis(
                env_number("MALL_ORDER_EXPIRER_IDLE_MS", 1_000_u64)?.clamp(100, 60_000),
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

struct RoundOutcome {
    promoted: u64,
    scanned: u32,
    expired: u32,
    failed: u32,
}

async fn run_once(
    client: &mut MallOrderClient<tonic::transport::Channel>,
    batch_size: u32,
) -> Result<RoundOutcome, WorkerError> {
    // Promotion runs before expiry: creator shares whose settlement hold
    // (MALL_AFFILIATE_HOLD_DAYS) elapsed become payable the moment they
    // qualify — this job is the promoter per migrations/README.md. Without
    // it every share stays `pending` forever and creators can never be paid.
    let promoted = client
        .promote_affiliate_settlements(bookway_runtime::grpc_service_request(BatchRequest {
            limit: batch_size,
        })?)
        .await?
        .into_inner()
        .promoted;
    let expiry = client
        .expire_pending(bookway_runtime::grpc_service_request(BatchRequest {
            limit: batch_size,
        })?)
        .await?
        .into_inner();
    Ok(RoundOutcome {
        promoted,
        scanned: expiry.scanned,
        expired: expiry.expired,
        failed: expiry.failed,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("mall-order-expirer");
    let config = Config::from_env()?;
    let channel = bookway_runtime::grpc_channel(&config.mall_order_url).await?;
    let mut client = MallOrderClient::new(channel);
    loop {
        match run_once(&mut client, config.batch_size).await {
            Ok(result) if result.scanned == 0 && result.promoted == 0 => {
                tokio::time::sleep(config.idle_interval).await
            }
            Ok(result) => tracing::info!(
                promoted = result.promoted,
                scanned = result.scanned,
                expired = result.expired,
                failed = result.failed,
                "mall order settlement/expiry round processed"
            ),
            Err(error) => {
                tracing::error!(%error, "mall order expiry batch failed");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
