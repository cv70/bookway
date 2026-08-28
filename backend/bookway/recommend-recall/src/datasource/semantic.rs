use bookway_bbs_search_api::pb::bbs_search_client::BbsSearchClient;
use bookway_knowledge_catalog_api::pb::knowledge_catalog_client::KnowledgeCatalogClient;
use tonic::transport::Channel;

use crate::conf::SemanticConfig;

/// Outbound clients for the semantic recall lane: query embedding through
/// knowledge-catalog's embedding provider and nearest-document retrieval
/// through BBS Search's `SearchSemantic` lane. Present only when both
/// endpoints are configured; otherwise the semantic recall source is not
/// registered and the listing sources serve the feed unchanged.
#[derive(Clone)]
pub(crate) struct SemanticRecallDataSource {
    pub(crate) catalog: KnowledgeCatalogClient<Channel>,
    pub(crate) search: BbsSearchClient<Channel>,
}

impl SemanticRecallDataSource {
    pub(crate) async fn connect(
        config: &SemanticConfig,
    ) -> Result<Self, bookway_runtime::ConnectFailure> {
        let catalog =
            KnowledgeCatalogClient::new(bookway_runtime::grpc_channel(&config.knowledge_catalog_url).await?);
        let search = BbsSearchClient::new(bookway_runtime::grpc_channel(&config.bbs_search_url).await?);
        Ok(Self { catalog, search })
    }
}
