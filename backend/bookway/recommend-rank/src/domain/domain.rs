use std::sync::Arc;

use crate::conf::Config;
use crate::datasource::RankModelDataSource;
use crate::domain::rank::predictor::{MultiObjectivePredictor, choose_predictor};
use crate::domain::rank::remote::RemoteScorer;

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    pub(crate) model: Arc<RankModelDataSource>,
    pub(crate) predictor: Arc<dyn MultiObjectivePredictor>,
    /// Present when the model-serving endpoint is configured; the LLM scorer
    /// stage degrades silently whenever the service is down or untrained.
    pub(crate) scorer: Option<RemoteScorer>,
}

impl Domain {
    pub fn new(config: Config) -> Result<Self, String> {
        let predictor = choose_predictor(config.model_artifact.as_deref())?;
        let scorer = config
            .model_endpoint
            .as_deref()
            .filter(|endpoint| !endpoint.trim().is_empty())
            .map(|endpoint| RemoteScorer::new(endpoint.to_string(), bookway_runtime::http_client()));
        Ok(Self {
            model: Arc::new(RankModelDataSource::new(config.model_version.clone())),
            predictor: Arc::from(predictor),
            scorer,
            config,
        })
    }
}
