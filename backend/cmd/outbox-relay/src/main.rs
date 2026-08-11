use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("outbox-relay");
    let relay = bookway_event::OutboxRelay::from_env(bookway_data::postgres_pool().await?)?;
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
