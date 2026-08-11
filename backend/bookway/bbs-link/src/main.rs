mod internal;

use internal::{conf::Config, registry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("bbs-link");
    let config = Config::from_env()?;
    let listen_addr = config.listen_addr;
    let app = registry::build().await?;
    bookway_runtime::serve("bbs-link", listen_addr, app).await?;
    Ok(())
}
