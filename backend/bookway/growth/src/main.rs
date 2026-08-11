mod internal;

use internal::{conf::Config, registry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("growth");
    let config = Config::from_env()?;
    let app = registry::build().await?;
    bookway_runtime::serve("growth", config.listen_addr, app).await?;
    Ok(())
}
