use std::sync::Arc;

use crate::{conf::Config, datasource::RankModelDataSource};

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    pub(crate) model: Arc<RankModelDataSource>,
}

impl Domain {
    pub fn new(config: Config) -> Self {
        Self {
            model: Arc::new(RankModelDataSource::new(config.model_version.clone())),
            config,
        }
    }
}
