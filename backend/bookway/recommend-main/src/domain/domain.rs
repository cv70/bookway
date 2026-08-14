use std::sync::Arc;
use tonic::transport::Channel;

use bookway_recommend_rank::api::pb as rank;
use bookway_recommend_recall::api::pb as recall;

use super::FeedService;
use crate::{
    conf::Config,
    datasource::{
        GrpcBbsContextDataSource, GrpcLikeStatusDataSource, MemoryExposureDataSource,
        PostgresExposureDataSource, SharedBbsContextDataSource, SharedExposureDataSource,
        SharedLikeStatusDataSource,
    },
    domain::pipeline::{
        AuthorDiversityScorer, DefaultQueryHydrator, DiversitySelector, ExposureSideEffect,
        FeedPipeline, FeedPipelineComponents, IntentScorer, QualityScorer, ReactionContextHydrator,
        RecommendRanker, RecommendRecallSource, SafetyFilter, SeenFilter, ServedHistoryFilter,
        ServedHistoryHydrator, SocialContextHydrator, SocialProofHydrator,
    },
};

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    pub(crate) bbs: SharedBbsContextDataSource,
    pub(crate) like_status: SharedLikeStatusDataSource,
    pub(crate) exposures: SharedExposureDataSource,
    pub(crate) recall_client:
        Arc<recall::recommend_recall_client::RecommendRecallClient<tonic::transport::Channel>>,
    pub(crate) rank_client:
        Arc<rank::recommend_rank_client::RecommendRankClient<tonic::transport::Channel>>,
    pub(crate) feature_client:
        bookway_feature_main::api::pb::feature_main_client::FeatureMainClient<Channel>,
    pub(crate) feed: FeedService,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, bookway_data::DataError> {
        let bbs: SharedBbsContextDataSource = Arc::new(
            GrpcBbsContextDataSource::connect(config.bbs_url.clone())
                .await
                .map_err(|error| setting_error("BBS_GRPC_URL", error))?,
        );
        let like_status: SharedLikeStatusDataSource = Arc::new(
            GrpcLikeStatusDataSource::connect(config.like_status_url.clone())
                .await
                .map_err(|error| setting_error("LIKE_STATUS_GRPC_URL", error))?,
        );
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
            bookway_feature_main::api::pb::feature_main_client::FeatureMainClient::connect(
                config.feature_main_url.clone(),
            )
            .await
            .map_err(|error| setting_error("FEATURE_MAIN_GRPC_URL", error))?;
        let pipeline = FeedPipeline::new(FeedPipelineComponents {
            query_hydrator: Arc::new(DefaultQueryHydrator),
            sources: vec![Arc::new(RecommendRecallSource::new(recall_client.clone()))],
            hydrators: vec![
                Arc::new(ServedHistoryHydrator::new(exposures.clone())),
                Arc::new(SocialContextHydrator::new(bbs.clone())),
                Arc::new(ReactionContextHydrator::new(like_status.clone())),
                Arc::new(SocialProofHydrator),
            ],
            filters: vec![
                Arc::new(SeenFilter),
                Arc::new(ServedHistoryFilter),
                Arc::new(SafetyFilter),
            ],
            scorers: vec![
                Arc::new(QualityScorer),
                Arc::new(IntentScorer),
                Arc::new(AuthorDiversityScorer),
            ],
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
            bbs,
            like_status,
            exposures,
            recall_client,
            rank_client,
            feature_client,
            feed: super::FeedService::new(pipeline),
        })
    }

    pub(crate) async fn recommend(
        &self,
        request: crate::api::FeedQueryRequest,
    ) -> crate::api::FeedDto {
        self.feed.recommend(request).await
    }
}

fn setting_error<E: std::fmt::Display>(key: &'static str, error: E) -> bookway_data::DataError {
    bookway_data::DataError::InvalidPoolSetting {
        key,
        value: error.to_string(),
    }
}
