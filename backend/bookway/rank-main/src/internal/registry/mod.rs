use std::sync::Arc;

use axum::Router;

use super::{
    conf::Config,
    datasource::HeuristicRanker,
    domain::RankService,
    service::{self, AppState},
};

pub(crate) fn build(config: Config) -> Router {
    service::router(AppState {
        rank: RankService::new(Arc::new(HeuristicRanker::new(config.model_version))),
    })
}
