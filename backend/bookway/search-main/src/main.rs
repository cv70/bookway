pub(crate) mod api;
pub(crate) mod conf;
pub(crate) mod datasource;
pub(crate) mod domain;

use bookway_ad_main_api::pb::ad_main_client::AdMainClient;
use bookway_bbs_link_api::pb::bbs_link_client::BbsLinkClient;
use bookway_bbs_search_api::pb::bbs_search_client::BbsSearchClient;
use bookway_feature_main_api::pb::feature_main_client::FeatureMainClient;
use bookway_knowledge_catalog_api::pb::knowledge_catalog_client::KnowledgeCatalogClient;
use conf::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("search-main");
    let config = Config::from_env()?;
    let bbs_search = BbsSearchClient::connect(config.bbs_search_url.clone()).await?;
    let bbs_link = BbsLinkClient::connect(config.bbs_link_url.clone()).await?;
    let knowledge_catalog =
        KnowledgeCatalogClient::connect(config.knowledge_catalog_url.clone()).await?;
    let feature_main = match FeatureMainClient::connect(config.feature_main_url.clone()).await {
        Ok(client) => Some(client),
        Err(error) => {
            tracing::warn!(%error, "feature-main unavailable; search will use lexical ranking");
            None
        }
    };
    let ad_main = match AdMainClient::connect(config.ad_main_url.clone()).await {
        Ok(client) => Some(client),
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
            knowledge_catalog,
            feature_main,
            ad_main,
        )
        .await?,
    )
    .await?;
    Ok(())
}
