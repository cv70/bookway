pub(crate) mod api;
pub(crate) mod conf;
pub(crate) mod datasource;
pub(crate) mod domain;

use conf::Config;
use domain::Domain;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("media");
    let config = Config::from_env()?;
    let domain = Domain::new(config).await?;
    tokio::try_join!(api::serve_http(domain.clone()), async {
        api::serve_grpc(domain)
            .await
            .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
    },)?;
    Ok(())
}
