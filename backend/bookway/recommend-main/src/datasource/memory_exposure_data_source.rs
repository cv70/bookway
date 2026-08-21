use super::*;

#[derive(Default)]
pub(crate) struct MemoryExposureDataSource {
    exposures: RwLock<Vec<Exposure>>,
}

#[async_trait]
impl ExposureDataSource for MemoryExposureDataSource {
    async fn record(&self, exposure: Exposure) -> Result<(), ExposureError> {
        tracing::debug!(request_id=%exposure.request_id, selected=exposure.items.len(), "recommendation exposure recorded");
        let mut exposures = self.exposures.write().await;
        exposures.push(exposure);
        const MAX_EXPOSURES: usize = 10_000;
        if exposures.len() > MAX_EXPOSURES {
            let overflow = exposures.len() - MAX_EXPOSURES;
            exposures.drain(..overflow);
        }
        Ok(())
    }

    async fn recent_content_ids(
        &self,
        user_id: &str,
        surface: &str,
        limit: usize,
    ) -> HashSet<String> {
        let exposures = self.exposures.read().await;
        let mut content_ids = HashSet::new();
        for exposure in exposures
            .iter()
            .rev()
            .filter(|exposure| exposure.user_id == user_id && exposure.surface == surface)
        {
            for item in &exposure.items {
                content_ids.insert(item.content_id.clone());
                if content_ids.len() >= limit {
                    return content_ids;
                }
            }
        }
        content_ids
    }

    async fn validate_attributions(
        &self,
        user_id: &str,
        attributions: &[ExposureAttribution],
    ) -> Result<Vec<bool>, ExposureError> {
        let exposures = self.exposures.read().await;
        Ok(attributions
            .iter()
            .map(|attribution| {
                exposures.iter().any(|exposure| {
                    exposure.request_id == attribution.request_id
                        && exposure.user_id == user_id
                        && exposure.session_id == attribution.session_id
                        && exposure.items.iter().any(|item| {
                            usize::try_from(attribution.position)
                                .is_ok_and(|position| position == item.position)
                                && item.content_id == attribution.content_id
                        })
                })
            })
            .collect())
    }
}
