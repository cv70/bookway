use std::sync::Arc;

use axum::Router;

use super::{
    conf::Config,
    datasource::{HttpContentSearchSource, OpenSearchSource, SearchSource},
    domain::SearchService,
    service::{self, AppState},
};

pub(crate) fn build(config: Config) -> Router {
    let source: Arc<dyn SearchSource> = match config.opensearch_url {
        Some(url) => Arc::new(OpenSearchSource::new(
            url,
            config.opensearch_index,
            config.bbs_link_url,
        )),
        None => Arc::new(HttpContentSearchSource::new(config.bbs_link_url)),
    };
    service::router(AppState {
        search: SearchService::new(source),
    })
}
