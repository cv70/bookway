use bookway_recommend_main_api::pb::recommend_main_client::RecommendMainClient;

use crate::conf::Config;

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) recommend_main: RecommendMainClient<tonic::transport::Channel>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, bookway_runtime::ConnectFailure> {
        let recommend_main = RecommendMainClient::new(
            bookway_runtime::grpc_channel(&config.recommend_main_url).await?,
        );
        Ok(Self {
            recommend_main,
            config,
        })
    }
}
