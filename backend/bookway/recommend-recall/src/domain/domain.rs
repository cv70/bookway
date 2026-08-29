use bookway_bbs_link_api::pb::bbs_link_client::BbsLinkClient;

use crate::conf::Config;
use crate::domain::recall::semantic::SemanticRecallClients;

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    pub(crate) content_client: BbsLinkClient<tonic::transport::Channel>,
    pub(crate) semantic: Option<SemanticRecallClients>,
    pub(crate) max_candidates: usize,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, bookway_runtime::ConnectFailure> {
        let content_client =
            BbsLinkClient::new(bookway_runtime::grpc_channel(&config.bbs_link_url).await?);
        // An unconfigured semantic lane is absent, never a hidden failure: the
        // listing sources alone serve the feed. Explicitly configured lanes
        // must connect at startup so operator mistakes fail fast.
        let semantic = match config.semantic.as_ref() {
            Some(semantic_config) => Some(SemanticRecallClients::connect(semantic_config).await?),
            None => None,
        };
        Ok(Self {
            content_client,
            semantic,
            max_candidates: config.max_candidates,
            config,
        })
    }
}
