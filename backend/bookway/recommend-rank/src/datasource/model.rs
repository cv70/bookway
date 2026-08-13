use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct RankModelDataSource {
    model_version: Arc<str>,
}
impl RankModelDataSource {
    pub(crate) fn new(model_version: String) -> Self {
        Self {
            model_version: Arc::from(model_version),
        }
    }
    pub(crate) fn model_version(&self) -> &str {
        &self.model_version
    }
}
