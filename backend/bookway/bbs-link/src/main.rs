pub(crate) mod api;
pub(crate) mod conf;
pub(crate) mod datasource;
pub(crate) mod domain;

use bookway_media_api::pb::media_client::MediaClient;
use conf::Config;
use domain::Domain;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("bbs-link");
    let config = Config::from_env()?;
    let media = MediaClient::new(bookway_runtime::grpc_channel(&config.media_grpc_url).await?);
    let domain = Domain::new(config, Some(media)).await?;
    tokio::try_join!(api::serve_http(domain.clone()), async {
        api::serve_grpc(domain)
            .await
            .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
    },)?;
    Ok(())
}
