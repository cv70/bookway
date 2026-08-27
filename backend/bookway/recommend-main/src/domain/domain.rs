use std::sync::Arc;

use crate::datasource::FrequencyCapDataSource;

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
        DisabledFrequencyCapDataSource, ExposureAttribution, ExposureError,
        MemoryExposureDataSource, MemoryFrequencyCapDataSource, PostgresExposureDataSource,
        RedisFrequencyCapDataSource, SharedExposureDataSource,
    },
    domain::pipeline::{
        AuthorDiversityScorer, CandidateFilter, CandidateHydrator, CoarseRanker,
        DefaultQueryHydrator, DiversitySelector, ExposureSideEffect, FeedPipeline,
        FeedPipelineComponents, FollowingOnlyFilter, FrequencyCapFilter, FrequencyCapHydrator,
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
        let bbs = BbsClient::new(
            bookway_runtime::grpc_channel(&config.bbs_url)
                .await
                .map_err(|error| setting_error("BBS_GRPC_URL", error))?,
        );
        let interaction_status = InteractionStatusClient::new(
            bookway_runtime::grpc_channel(&config.interaction_status_url)
                .await
                .map_err(|error| setting_error("INTERACTION_STATUS_GRPC_URL", error))?,
        );
        let exposures: SharedExposureDataSource = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryExposureDataSource::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresExposureDataSource::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let rank_client = Arc::new(rank::recommend_rank_client::RecommendRankClient::new(
            bookway_runtime::grpc_channel(&config.recommend_rank_url)
                .await
                .map_err(|error| setting_error("RECOMMEND_RANK_GRPC_URL", error))?,
        ));
        let recall_client = Arc::new(recall::recommend_recall_client::RecommendRecallClient::new(
            bookway_runtime::grpc_channel(&config.recommend_recall_url)
                .await
                .map_err(|error| setting_error("RECOMMEND_RECALL_GRPC_URL", error))?,
        ));
        let feature_client =
            bookway_feature_main_api::pb::feature_main_client::FeatureMainClient::new(
                bookway_runtime::grpc_channel(&config.feature_main_url)
                    .await
                    .map_err(|error| setting_error("FEATURE_MAIN_GRPC_URL", error))?,
            );
        let ad_main = AdMainClient::new(
            bookway_runtime::grpc_channel(&config.ad_main_url)
                .await
                .map_err(|error| setting_error("AD_MAIN_GRPC_URL", error))?,
        );
        // Frequency-cap guard wiring. Memory storage (local dev) keeps counts
        // in-process; Postgres-mode production reads/writes Redis. A missing
        // REDIS_URL or an explicit zero cap disables the guard entirely and is
        // announced once here rather than silently tolerated per request.
        let (frequency_caps, cap_store_functional): (Arc<dyn FrequencyCapDataSource>, bool) =
            match bookway_data::storage_mode()? {
                bookway_data::StorageMode::Memory => {
                    (Arc::new(MemoryFrequencyCapDataSource::default()), true)
                }
                bookway_data::StorageMode::Postgres => {
                    match bookway_data::redis_connection().await? {
                        Some(redis) => (Arc::new(RedisFrequencyCapDataSource::new(redis)), true),
                        None => {
                            tracing::warn!(
                                "REDIS_URL not configured; daily exposure frequency cap is DISABLED"
                            );
                            (Arc::new(DisabledFrequencyCapDataSource), false)
                        }
                    }
                }
            };
        let cap_enabled = config.frequency_cap_daily > 0 && cap_store_functional;
        let mut hydrators: Vec<Arc<dyn CandidateHydrator>> =
            vec![Arc::new(ServedHistoryHydrator::new(exposures.clone()))];
        let mut filters: Vec<Arc<dyn CandidateFilter>> =
            vec![Arc::new(SeenFilter), Arc::new(SafetyFilter)];
        if cap_enabled {
            hydrators.push(Arc::new(FrequencyCapHydrator::new(frequency_caps.clone())));
            filters.push(Arc::new(FrequencyCapFilter {
                daily_cap: config.frequency_cap_daily,
            }));
        } else {
            tracing::warn!(
                "feed exposure frequency cap inactive (FEED_FREQUENCY_CAP_DAILY={})",
                config.frequency_cap_daily
            );
        }
        let pipeline = FeedPipeline::new(FeedPipelineComponents {
            query_hydrator: Arc::new(DefaultQueryHydrator),
            sources: vec![Arc::new(RecommendRecallSource::new(
                recall_client.clone(),
                feature_client.clone(),
                bbs.clone(),
            ))],
            hydrators: {
                hydrators.push(Arc::new(SocialContextHydrator::new(bbs.clone())));
                hydrators.push(Arc::new(RouteContextHydrator::new(bbs)));
                hydrators.push(Arc::new(ReactionContextHydrator::new(interaction_status)));
                hydrators.push(Arc::new(SocialProofHydrator));
                hydrators
            },
            filters: {
                filters.push(Arc::new(FollowingOnlyFilter));
                filters
            },
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
            side_effects: vec![Arc::new(ExposureSideEffect::new(
                exposures.clone(),
                frequency_caps.clone(),
            ))],
        });
        // Cold-start page cache wiring. Only anonymous first pages are shared;
        // personalization always bypasses it. Memory mode (local dev) and a
        // missing REDIS_URL both disable the cache — announced once here, not
        // per request.
        let page_cache: Option<Arc<bookway_cache::SingleFlightCache<super::CachedFeedPage>>> =
            match bookway_data::storage_mode()? {
                bookway_data::StorageMode::Memory => None,
                bookway_data::StorageMode::Postgres if config.anon_page_ttl_secs > 0 => {
                    match bookway_data::redis_connection().await? {
                        Some(redis) => Some(Arc::new(bookway_cache::SingleFlightCache::new(
                            Some(redis),
                            "feed:anon-page",
                            config.anon_page_ttl_secs,
                        ))),
                        None => {
                            tracing::warn!(
                                "REDIS_URL not configured; cold-start feed page cache is DISABLED"
                            );
                            None
                        }
                    }
                }
                bookway_data::StorageMode::Postgres => {
                    tracing::warn!(
                        "FEED_ANON_PAGE_TTL_SECS=0; cold-start feed page cache is DISABLED"
                    );
                    None
                }
            };
        Ok(Self {
            config,
            feed: super::FeedService::new(pipeline, ad_main, page_cache),
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
