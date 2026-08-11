use std::sync::Arc;

use axum::Router;

use super::{
    conf::Config,
    datasource::HttpSearchDataSource,
    domain::SearchMainService,
    service::{self, AppState},
};

pub(crate) fn build(config: Config) -> Router {
    service::router(AppState {
        search: SearchMainService::new(Arc::new(HttpSearchDataSource::new(config.bbs_search_url))),
    })
}
