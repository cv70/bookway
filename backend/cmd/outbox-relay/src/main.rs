mod purchase;

use std::time::Duration;
use sqlx::PgPool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("outbox-relay");
    let pool: PgPool = bookway_data::postgres_pool().await?;
    let relay = bookway_event::OutboxRelay::from_env(pool.clone())?;
    let purchase_relay = purchase::PurchaseEventRelay::from_env(pool.clone())?;
    // Two independent delivery lanes share the process but not fate: a
    // Kafka outage must not stall purchase attribution, and vice versa.
    tokio::spawn(async move { purchase_relay.run_forever().await });
    loop {
        match relay.run_once().await {
            Ok(0) => tokio::time::sleep(Duration::from_millis(500)).await,
            Ok(count) => tracing::debug!(count, "outbox events published"),
            Err(error) => {
                tracing::error!(%error, "outbox relay iteration failed");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
