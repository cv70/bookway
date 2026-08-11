use std::sync::Arc;

use axum::Router;

use super::{
    conf::Config,
    datasource::{
        HttpBbsDataSource, HttpBbsFeedDataSource, HttpBbsLinkDataSource, HttpCommentDataSource,
        HttpGrowthDataSource, HttpLikeStatusDataSource, HttpMediaDataSource,
        HttpSearchMainDataSource, HttpUserEventDataSource,
    },
    domain::{GatewayDependencies, GatewayService},
    service::{self, AppState},
};

pub(crate) fn build(config: Config) -> Router {
    let growth = Arc::new(HttpGrowthDataSource::new(config.growth_url));
    let bbs_feed = Arc::new(HttpBbsFeedDataSource::new(config.bbs_feed_url));
    let bbs_link = Arc::new(HttpBbsLinkDataSource::new(config.bbs_link_url));
    let search_main = Arc::new(HttpSearchMainDataSource::new(config.search_main_url));
    let bbs = Arc::new(HttpBbsDataSource::new(config.bbs_url));
    let comment = Arc::new(HttpCommentDataSource::new(config.comment_url));
    let like_status = Arc::new(HttpLikeStatusDataSource::new(config.like_status_url));
    let user_event = Arc::new(HttpUserEventDataSource::new(config.user_event_url));
    let media = Arc::new(HttpMediaDataSource::new(config.media_url));
    service::router(AppState {
        gateway: GatewayService::new(GatewayDependencies {
            growth,
            bbs_feed,
            bbs_link,
            search_main,
            bbs,
            comment,
            like_status,
            user_event,
            media,
        }),
    })
}
