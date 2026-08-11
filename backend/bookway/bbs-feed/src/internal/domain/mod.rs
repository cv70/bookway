use std::sync::Arc;

use thiserror::Error;

use super::{
    api::{FeedDto, FeedQueryRequest},
    datasource::{BbsFeedDataSource, RecommendMainClientError},
};

#[derive(Debug, Error)]
pub(crate) enum BbsFeedError {
    #[error(transparent)]
    Recommend(#[from] RecommendMainClientError),
}

#[derive(Clone)]
pub(crate) struct BbsFeedService {
    recommend_main: Arc<dyn BbsFeedDataSource>,
}

impl BbsFeedService {
    pub(crate) fn new(recommend_main: Arc<dyn BbsFeedDataSource>) -> Self {
        Self { recommend_main }
    }

    pub(crate) async fn feed(
        &self,
        mut request: FeedQueryRequest,
    ) -> Result<FeedDto, BbsFeedError> {
        request.limit = Some(request.limit.unwrap_or(10).clamp(1, 20));
        request.surface.get_or_insert_with(|| "home".to_string());
        Ok(self.recommend_main.feed(request).await?)
    }
}
