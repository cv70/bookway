pub(crate) mod api;
pub(crate) mod conf;
pub(crate) mod datasource;
pub(crate) mod domain;

use bookway_recommend_main_api::pb::recommend_main_client::RecommendMainClient;
use bookway_search_main_api::pb::search_main_client::SearchMainClient;
use conf::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("user-event");
    let config = Config::from_env()?;
    // Attribution validation is best-effort: keep ingest available when the
    // recommender is temporarily down and strip unverifiable join fields later.
    let endpoint = tonic::transport::Endpoint::from_shared(config.recommend_main_url.clone())?;
    let recommend_main = RecommendMainClient::new(endpoint.connect_lazy());
    let endpoint = tonic::transport::Endpoint::from_shared(config.search_main_url.clone())?;
    let search_main = SearchMainClient::new(endpoint.connect_lazy());
    let domain = domain::Domain::new(config, recommend_main, search_main).await?;
    tokio::try_join!(api::serve_http(domain.clone()), async {
        api::serve_grpc(domain)
            .await
            .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
    },)?;
    Ok(())
}
