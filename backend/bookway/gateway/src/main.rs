pub(crate) mod api;
pub(crate) mod conf;
pub(crate) mod datasource;
pub(crate) mod domain;

use conf::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("gateway");
    let domain = domain::Domain::new(Config::from_env()?).await?;
    api::serve(domain).await?;
    Ok(())
}
