mod internal;

use internal::{conf::Config, registry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("content-audit");
    let config = Config::from_env()?;
    let addr = config.listen_addr;
    let app = registry::build(config).await?;
    bookway_runtime::serve("content-audit", addr, app).await?;
    Ok(())
}
