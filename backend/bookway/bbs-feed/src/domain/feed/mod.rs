use crate::api::{FeedDto, FeedQueryRequest};
use crate::datasource::BbsFeedDataSource;
use crate::domain::{BbsFeedError, Domain};

impl Domain {
    pub(crate) async fn feed(
        &self,
        mut request: FeedQueryRequest,
    ) -> Result<FeedDto, BbsFeedError> {
        request.limit = Some(request.limit.unwrap_or(10).clamp(1, 20));
        request.surface.get_or_insert_with(|| "home".to_string());
        Ok(self.recommend_main.feed(request).await?)
    }
}
