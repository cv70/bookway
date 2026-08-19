use std::sync::Arc;

use bookway_ad_main_api::pb::ad_main_client::AdMainClient;
use bookway_bbs_api::pb::bbs_client::BbsClient;
use bookway_interaction_status_api::pb::interaction_status_client::InteractionStatusClient;
use bookway_recommend_rank_api::pb as rank;
use bookway_recommend_recall_api::pb as recall;

use super::FeedService;
use crate::{
    api::pb,
    conf::Config,
    datasource::{
        ExposureAttribution, ExposureError, MemoryExposureDataSource, PostgresExposureDataSource,
        SharedExposureDataSource,
    },
    domain::pipeline::{
        AuthorDiversityScorer, CoarseRanker, DefaultQueryHydrator, DiversitySelector,
        ExposureSideEffect, FeedPipeline, FeedPipelineComponents, FollowingOnlyFilter,
        IntentScorer, QualityScorer, ReactionContextHydrator, RecommendRanker,
        RecommendRecallSource, RouteContextHydrator, SafetyFilter, SeenFilter,
        ServedHistoryHydrator, SocialContextHydrator, SocialProofHydrator,
    },
};

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    pub(crate) feed: FeedService,
    exposures: SharedExposureDataSource,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let bbs = BbsClient::connect(config.bbs_url.clone())
            .await
            .map_err(|error| setting_error("BBS_GRPC_URL", error))?;
        let interaction_status =
            InteractionStatusClient::connect(config.interaction_status_url.clone())
                .await
                .map_err(|error| setting_error("INTERACTION_STATUS_GRPC_URL", error))?;
        let exposures: SharedExposureDataSource = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryExposureDataSource::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresExposureDataSource::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let rank_client = Arc::new(
            rank::recommend_rank_client::RecommendRankClient::connect(
                config.recommend_rank_url.clone(),
            )
            .await
            .map_err(|error| setting_error("RECOMMEND_RANK_GRPC_URL", error))?,
        );
        let recall_client = Arc::new(
            recall::recommend_recall_client::RecommendRecallClient::connect(
                config.recommend_recall_url.clone(),
            )
            .await
            .map_err(|error| setting_error("RECOMMEND_RECALL_GRPC_URL", error))?,
        );
        let feature_client =
            bookway_feature_main_api::pb::feature_main_client::FeatureMainClient::connect(
                config.feature_main_url.clone(),
            )
            .await
            .map_err(|error| setting_error("FEATURE_MAIN_GRPC_URL", error))?;
        let ad_main = AdMainClient::connect(config.ad_main_url.clone())
            .await
            .map_err(|error| setting_error("AD_MAIN_GRPC_URL", error))?;
        let pipeline = FeedPipeline::new(FeedPipelineComponents {
            query_hydrator: Arc::new(DefaultQueryHydrator),
            sources: vec![Arc::new(RecommendRecallSource::new(
                recall_client.clone(),
                feature_client.clone(),
                bbs.clone(),
            ))],
            hydrators: vec![
                Arc::new(ServedHistoryHydrator::new(exposures.clone())),
                Arc::new(SocialContextHydrator::new(bbs.clone())),
                Arc::new(RouteContextHydrator::new(bbs)),
                Arc::new(ReactionContextHydrator::new(interaction_status)),
                Arc::new(SocialProofHydrator),
            ],
            filters: vec![
                Arc::new(SeenFilter),
                Arc::new(SafetyFilter),
                Arc::new(FollowingOnlyFilter),
            ],
            scorers: vec![
                Arc::new(QualityScorer),
                Arc::new(IntentScorer),
                Arc::new(AuthorDiversityScorer),
            ],
            coarse_ranker: Arc::new(CoarseRanker),
            ranker: Some(Arc::new(RecommendRanker::new(
                rank_client.clone(),
                feature_client.clone(),
            ))),
            selector: Arc::new(DiversitySelector),
            post_selection_filters: vec![Arc::new(SafetyFilter)],
            side_effects: vec![Arc::new(ExposureSideEffect::new(exposures.clone()))],
        });
        Ok(Self {
            config,
            feed: super::FeedService::new(pipeline, ad_main),
            exposures,
        })
    }

    pub(crate) async fn recommend(&self, request: pb::FeedRequest) -> pb::FeedResponse {
        self.feed.recommend(request).await
    }

    pub(crate) async fn validate_attributions(
        &self,
        request: pb::ValidateAttributionsRequest,
    ) -> Result<pb::ValidateAttributionsResponse, ExposureError> {
        let attributions = request
            .attributions
            .into_iter()
            .map(|attribution| ExposureAttribution {
                request_id: attribution.request_id,
                session_id: attribution.session_id,
                content_id: attribution.content_id,
                position: attribution.position,
            })
            .collect::<Vec<_>>();
        let valid = self
            .exposures
            .validate_attributions(&request.user_id, &attributions)
            .await?;
        Ok(pb::ValidateAttributionsResponse { valid })
    }
}

fn setting_error<E: std::fmt::Display>(key: &'static str, error: E) -> bookway_data::DataError {
    bookway_data::DataError::InvalidPoolSetting {
        key,
        value: error.to_string(),
    }
}
