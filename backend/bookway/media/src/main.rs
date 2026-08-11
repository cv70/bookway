mod internal;

use internal::{conf::Config, registry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("media");
    let config = Config::from_env()?;
    let addr = config.listen_addr;
    let app = registry::build(config).await?;
    bookway_runtime::serve("media", addr, app).await?;
    Ok(())
}
