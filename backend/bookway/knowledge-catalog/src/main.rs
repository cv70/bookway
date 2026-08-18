mod api;
mod conf;
mod datasource;
mod domain;

use conf::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("knowledge-catalog");
    api::serve(domain::Domain::new(Config::from_env()?).await?).await?;
    Ok(())
}
