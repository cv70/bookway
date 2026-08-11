use std::sync::Arc;

use axum::Router;

use super::{
    datasource::HttpBbsFeedDataSource,
    domain::BbsFeedService,
    service::{self, AppState},
};

pub(crate) fn build(recommend_main_url: String) -> Router {
    service::router(AppState {
        feed: BbsFeedService::new(Arc::new(HttpBbsFeedDataSource::new(recommend_main_url))),
    })
}
