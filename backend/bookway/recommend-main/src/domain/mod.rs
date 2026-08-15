#![allow(clippy::module_inception)]

mod domain;
pub(crate) mod pipeline;

use self::pipeline::FeedPipeline;
use super::api::pb;

#[derive(Clone)]
pub(crate) struct FeedService {
    pipeline: FeedPipeline,
}

pub use domain::Domain;

impl FeedService {
    pub(crate) fn new(pipeline: FeedPipeline) -> Self {
        Self { pipeline }
    }

    pub(crate) async fn recommend(&self, request: pb::FeedRequest) -> pb::FeedResponse {
        self.pipeline.execute(request).await
    }
}
