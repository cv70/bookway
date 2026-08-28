pub(crate) mod api;
pub(crate) mod conf;
pub(crate) mod datasource;
pub(crate) mod domain;

use bookway_ad_main_api::pb::ad_main_client::AdMainClient;
use bookway_bbs_api::pb::bbs_client::BbsClient;
use bookway_bbs_link_api::pb::bbs_link_client::BbsLinkClient;
use bookway_bbs_search_api::pb::bbs_search_client::BbsSearchClient;
use bookway_feature_main_api::pb::feature_main_client::FeatureMainClient;
use bookway_knowledge_catalog_api::pb::knowledge_catalog_client::KnowledgeCatalogClient;
use conf::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("search-main");
    let config = Config::from_env()?;
    let bbs_search =
        BbsSearchClient::new(bookway_runtime::grpc_channel(&config.bbs_search_url).await?);
    let bbs_link = BbsLinkClient::new(bookway_runtime::grpc_channel(&config.bbs_link_url).await?);
    let knowledge_catalog = KnowledgeCatalogClient::new(
        bookway_runtime::grpc_channel(&config.knowledge_catalog_url).await?,
    );
    let bbs = match bookway_runtime::grpc_channel(&config.bbs_url).await {
        Ok(channel) => Some(BbsClient::new(channel)),
        Err(error) => {
            tracing::warn!(%error, "bbs unavailable; search serves stored join counts");
            None
        }
    };
    let feature_main = match bookway_runtime::grpc_channel(&config.feature_main_url).await {
        Ok(channel) => Some(FeatureMainClient::new(channel)),
        Err(error) => {
            tracing::warn!(%error, "feature-main unavailable; search will use lexical ranking");
            None
        }
    };
    let ad_main = match bookway_runtime::grpc_channel(&config.ad_main_url).await {
        Ok(channel) => Some(AdMainClient::new(channel)),
        Err(error) => {
            tracing::warn!(%error, "ad-main unavailable; search will serve organic results");
            None
        }
    };
    api::serve(
        domain::Domain::new(
            config,
            bbs_search,
            bbs_link,
            bbs,
            knowledge_catalog,
            feature_main,
            ad_main,
        )
        .await?,
    )
    .await?;
    Ok(())
}
