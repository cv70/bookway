use std::sync::Arc;

use async_trait::async_trait;

use super::PipelineSideEffect;
use crate::datasource::{Exposure, ExposureError, FrequencyCapDataSource, SharedExposureDataSource};

pub(crate) struct ExposureSideEffect {
    exposures: SharedExposureDataSource,
    frequency_caps: Arc<dyn FrequencyCapDataSource>,
}

impl ExposureSideEffect {
    pub(crate) fn new(
        exposures: SharedExposureDataSource,
        frequency_caps: Arc<dyn FrequencyCapDataSource>,
    ) -> Self {
        Self {
            exposures,
            frequency_caps,
        }
    }
}

#[async_trait]
impl PipelineSideEffect for ExposureSideEffect {
    async fn run(&self, exposure: Exposure) -> Result<(), ExposureError> {
        self.exposures.record(exposure.clone()).await?;
        // The guard ledger trails the durable record. A failed increment only
        // loosens capping for the rest of the day (deliberate fail-open); it
        // must never turn an already-served response into a reported failure.
        let content_ids = exposure
            .items
            .iter()
            .map(|item| item.content_id.clone())
            .collect::<Vec<_>>();
        if let Err(error) = self
            .frequency_caps
            .record_served(&exposure.user_id, &content_ids)
            .await
        {
            tracing::warn!(
                user_id = %exposure.user_id,
                request_id = %exposure.request_id,
                %error,
                "frequency-cap ledger update failed; guard continues fail-open"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::{ExposureSideEffect, PipelineSideEffect};
    use crate::datasource::{
        Exposure, ExposureDataSource, ExposureItem, FrequencyCapDataSource, FrequencyCapError,
        MemoryExposureDataSource,
    };

    #[derive(Default)]
    struct RecordingFrequencyStore {
        served: Mutex<Vec<(String, Vec<String>)>>,
    }

    #[async_trait]
    impl FrequencyCapDataSource for RecordingFrequencyStore {
        async fn served_counts(
            &self,
            _user_id: &str,
            content_ids: &[String],
        ) -> Result<Vec<u32>, FrequencyCapError> {
            Ok(content_ids.iter().map(|_| 0).collect())
        }

        async fn record_served(
            &self,
            user_id: &str,
            content_ids: &[String],
        ) -> Result<(), FrequencyCapError> {
            self.served
                .lock()
                .expect("recording store lock poisoned")
                .push((user_id.to_string(), content_ids.to_vec()));
            Ok(())
        }
    }

    fn exposure(user_id: &str) -> Exposure {
        Exposure {
            request_id: "request-1".to_string(),
            user_id: user_id.to_string(),
            session_id: "session-1".to_string(),
            surface: "home".to_string(),
            pipeline_id: "pipeline".to_string(),
            model_version: None,
            experiment_bucket: None,
            candidate_count: 2,
            degraded: false,
            items: vec![
                ExposureItem {
                    position: 0,
                    content_id: "post-a".to_string(),
                    source: "recall".to_string(),
                    score: 1.0,
                    reasons: Vec::new(),
                },
                ExposureItem {
                    position: 1,
                    content_id: "post-b".to_string(),
                    source: "recall".to_string(),
                    score: 0.5,
                    reasons: Vec::new(),
                },
            ],
        }
    }

    #[tokio::test]
    async fn served_response_increments_the_frequency_ledger() {
        let exposures = Arc::new(MemoryExposureDataSource::default());
        let caps = Arc::new(RecordingFrequencyStore::default());
        let side_effect = ExposureSideEffect::new(exposures.clone(), caps.clone());

        side_effect.run(exposure("user-1")).await.expect("side effect");

        assert_eq!(
            caps.served.lock().expect("lock").as_slice(),
            [("user-1".to_string(), vec!["post-a".to_string(), "post-b".to_string()])]
        );
        assert!(exposures.recent_content_ids("user-1", "home", 10).await.contains("post-a"));
    }

    #[tokio::test]
    async fn ledger_failure_never_fails_the_served_exposure() {
        struct FailingFrequencyStore;

        #[async_trait]
        impl FrequencyCapDataSource for FailingFrequencyStore {
            async fn served_counts(
                &self,
                _user_id: &str,
                _content_ids: &[String],
            ) -> Result<Vec<u32>, FrequencyCapError> {
                Ok(Vec::new())
            }

            async fn record_served(
                &self,
                _user_id: &str,
                _content_ids: &[String],
            ) -> Result<(), FrequencyCapError> {
                Err(FrequencyCapError::Redis(redis::RedisError::from((
                    redis::ErrorKind::IoError,
                    "simulated redis outage",
                ))))
            }
        }

        let exposures = Arc::new(MemoryExposureDataSource::default());
        let side_effect =
            ExposureSideEffect::new(exposures.clone(), Arc::new(FailingFrequencyStore));

        side_effect
            .run(exposure("user-1"))
            .await
            .expect("exposure stays recorded when the guard ledger is down");
        assert!(
            exposures
                .recent_content_ids("user-1", "home", 10)
                .await
                .contains("post-b")
        );
    }
}
