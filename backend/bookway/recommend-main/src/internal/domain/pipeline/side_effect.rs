use async_trait::async_trait;

use super::PipelineSideEffect;
use crate::internal::datasource::{Exposure, SharedExposureDataSource};

pub(crate) struct ExposureSideEffect {
    exposures: SharedExposureDataSource,
}

impl ExposureSideEffect {
    pub(crate) fn new(exposures: SharedExposureDataSource) -> Self {
        Self { exposures }
    }
}

#[async_trait]
impl PipelineSideEffect for ExposureSideEffect {
    async fn run(
        &self,
        request_id: String,
        user_id: String,
        session_id: String,
        surface: String,
        post_ids: Vec<String>,
    ) {
        self.exposures
            .record(Exposure {
                request_id,
                user_id,
                session_id,
                surface,
                post_ids,
            })
            .await;
    }
}
