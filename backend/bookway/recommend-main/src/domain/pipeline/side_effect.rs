use async_trait::async_trait;

use super::PipelineSideEffect;
use crate::datasource::{Exposure, ExposureError, SharedExposureDataSource};

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
    async fn run(&self, exposure: Exposure) -> Result<(), ExposureError> {
        self.exposures.record(exposure).await
    }
}
