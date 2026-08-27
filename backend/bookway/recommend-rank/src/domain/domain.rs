use std::sync::Arc;

use crate::conf::Config;
use crate::datasource::RankModelDataSource;
use crate::domain::rank::predictor::{MultiObjectivePredictor, choose_predictor};

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    pub(crate) model: Arc<RankModelDataSource>,
    pub(crate) predictor: Arc<dyn MultiObjectivePredictor>,
}

impl Domain {
    pub fn new(config: Config) -> Self {
        Self {
            model: Arc::new(RankModelDataSource::new(config.model_version.clone())),
            predictor: Arc::from(choose_predictor(config.model_endpoint.as_deref())),
            config,
        }
    }
}
