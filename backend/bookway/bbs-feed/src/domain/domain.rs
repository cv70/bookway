use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{GrpcRecommendMainDataSource, RecommendMainClientError},
};

#[derive(Debug, Error)]
pub(crate) enum BbsFeedError {
    #[error(transparent)]
    Recommend(#[from] RecommendMainClientError),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) recommend_main: GrpcRecommendMainDataSource,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, tonic::transport::Error> {
        let recommend_main =
            GrpcRecommendMainDataSource::connect(config.recommend_main_url.clone()).await?;
        Ok(Self {
            recommend_main,
            config,
        })
    }
}
