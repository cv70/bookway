pub(crate) mod api;
pub(crate) mod conf;
pub(crate) mod datasource;
pub(crate) mod domain;

use conf::Config;
use domain::Domain;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("bbs-feed");
    api::serve(Domain::new(Config::from_env()?).await?).await?;
    Ok(())
}
