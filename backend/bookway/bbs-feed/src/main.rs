mod internal;

use internal::{conf::Config, registry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("bbs-feed");
    let config = Config::from_env()?;
    let app = registry::build(config.recommend_main_url.clone());
    bookway_runtime::serve("bbs-feed", config.listen_addr, app).await?;
    Ok(())
}
