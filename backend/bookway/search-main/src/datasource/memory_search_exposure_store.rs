use super::*;

#[derive(Default)]
pub(crate) struct MemorySearchExposureStore {
    exposures: RwLock<Vec<(SearchExposure, Instant)>>,
}

#[cfg(test)]
impl MemorySearchExposureStore {
    pub(crate) async fn len(&self) -> usize {
        self.exposures.read().await.len()
    }
}

#[async_trait]
impl SearchExposureStore for MemorySearchExposureStore {
    async fn record(&self, exposure: SearchExposure) -> Result<(), SearchExposureError> {
        let mut exposures = self.exposures.write().await;
        let now = Instant::now();
        exposures.retain(|(_, expires_at)| *expires_at > now);
        exposures.push((exposure, now + SEARCH_EXPOSURE_TTL));
        if exposures.len() > MAX_MEMORY_SEARCH_EXPOSURES {
            let overflow = exposures.len() - MAX_MEMORY_SEARCH_EXPOSURES;
            exposures.drain(..overflow);
        }
        Ok(())
    }

    async fn validate(
        &self,
        user_id: &str,
        attributions: &[SearchAttribution],
    ) -> Result<Vec<bool>, SearchExposureError> {
        let mut exposures = self.exposures.write().await;
        let now = Instant::now();
        exposures.retain(|(_, expires_at)| *expires_at > now);
        Ok(attributions
            .iter()
            .map(|attribution| {
                exposures.iter().any(|(exposure, _)| {
                    exposure.request_id == attribution.request_id
                        && exposure.user_id == user_id
                        && exposure.session_id == attribution.session_id
                        && exposure.items.iter().any(|item| {
                            usize::try_from(attribution.position)
                                .is_ok_and(|position| position == item.position)
                                && item.result_id == attribution.result_id
                        })
                })
            })
            .collect())
    }
}
