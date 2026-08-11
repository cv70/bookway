use std::sync::Arc;

use axum::Router;

use super::{
    conf::Config,
    datasource::{
        ExposureDataSource, HttpBbsContextDataSource, HttpBbsLinkDataSource,
        HttpLikeStatusDataSource, HttpModelDataSource, MemoryExposureDataSource,
        PostgresExposureDataSource,
    },
    domain::{
        FeedService,
        pipeline::{
            AuthorDiversityScorer, ContentCandidateSource, DefaultQueryHydrator, DiversitySelector,
            ExposureSideEffect, FeedPipeline, FeedPipelineComponents, IntentScorer, QualityScorer,
            ReactionContextHydrator, RemoteModelRanker, SafetyFilter, SeenFilter,
            SocialContextHydrator, SocialProofHydrator, SourceStrategy,
        },
    },
    service::{self, AppState},
};

pub(crate) async fn build(config: Config) -> Result<Router, bookway_data::DataError> {
    let content = Arc::new(HttpBbsLinkDataSource::new(config.bbs_link_url));
    let bbs = Arc::new(HttpBbsContextDataSource::new(config.bbs_url));
    let like_status = Arc::new(HttpLikeStatusDataSource::new(config.like_status_url));
    let exposures: Arc<dyn ExposureDataSource> = match bookway_data::storage_mode()? {
        bookway_data::StorageMode::Memory => Arc::new(MemoryExposureDataSource::default()),
        bookway_data::StorageMode::Postgres => Arc::new(PostgresExposureDataSource::new(
            bookway_data::postgres_pool().await?,
        )),
    };
    let models = Arc::new(HttpModelDataSource::new(
        config.feature_main_url,
        config.rank_main_url,
    ));
    let pipeline = FeedPipeline::new(FeedPipelineComponents {
        query_hydrator: Arc::new(DefaultQueryHydrator),
        sources: vec![
            Arc::new(ContentCandidateSource::new(
                content.clone(),
                SourceStrategy::Quality,
            )),
            Arc::new(ContentCandidateSource::new(content, SourceStrategy::Fresh)),
        ],
        hydrators: vec![
            Arc::new(SocialContextHydrator::new(bbs)),
            Arc::new(ReactionContextHydrator::new(like_status)),
            Arc::new(SocialProofHydrator),
        ],
        filters: vec![Arc::new(SeenFilter), Arc::new(SafetyFilter)],
        scorers: vec![
            Arc::new(QualityScorer),
            Arc::new(IntentScorer),
            Arc::new(AuthorDiversityScorer),
        ],
        ranker: Some(Arc::new(RemoteModelRanker::new(models))),
        selector: Arc::new(DiversitySelector),
        post_selection_filters: vec![Arc::new(SafetyFilter)],
        side_effects: vec![Arc::new(ExposureSideEffect::new(exposures))],
    });
    Ok(service::router(AppState {
        feed: FeedService::new(pipeline),
    }))
}
