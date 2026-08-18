pub(crate) mod api;
pub(crate) mod conf;
pub(crate) mod datasource;
pub(crate) mod domain;

use bookway_bbs_link_api::pb::bbs_link_client::BbsLinkClient;
use bookway_bbs_search_api::pb::bbs_search_client::BbsSearchClient;
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
    api::serve(domain::Domain::new(config, bbs_search, bbs_link, knowledge_catalog).await?).await?;
    Ok(())
}
