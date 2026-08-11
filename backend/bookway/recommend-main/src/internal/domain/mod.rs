pub(crate) mod pipeline;

use self::pipeline::FeedPipeline;
use super::api::{FeedDto, FeedQueryRequest};

#[derive(Clone)]
pub(crate) struct FeedService {
    pipeline: FeedPipeline,
}

impl FeedService {
    pub(crate) fn new(pipeline: FeedPipeline) -> Self {
        Self { pipeline }
    }

    pub(crate) async fn recommend(&self, request: FeedQueryRequest) -> FeedDto {
        self.pipeline.execute(request).await
    }
}
