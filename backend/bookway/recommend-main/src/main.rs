mod internal;

use internal::{conf::Config, registry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("recommend-main");
    let config = Config::from_env()?;
    let listen_addr = config.listen_addr;
    let app = registry::build(config).await?;
    bookway_runtime::serve("recommend-main", listen_addr, app).await?;
    Ok(())
}
