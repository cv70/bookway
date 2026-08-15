use bookway_bbs_link_api::pb::bbs_link_client::BbsLinkClient;

use crate::conf::Config;

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    pub(crate) content_client: BbsLinkClient<tonic::transport::Channel>,
    pub(crate) max_candidates: usize,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, tonic::transport::Error> {
        let content_client = BbsLinkClient::connect(config.bbs_link_url.clone()).await?;
        Ok(Self {
            content_client,
            max_candidates: config.max_candidates,
            config,
        })
    }
}
