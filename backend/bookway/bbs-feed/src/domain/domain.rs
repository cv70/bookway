use bookway_recommend_main_api::pb::recommend_main_client::RecommendMainClient;

use crate::conf::Config;

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) recommend_main: RecommendMainClient<tonic::transport::Channel>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, tonic::transport::Error> {
        let recommend_main =
            RecommendMainClient::connect(config.recommend_main_url.clone()).await?;
        Ok(Self {
            recommend_main,
            config,
        })
    }
}
