mod filter;
mod hydrator;
mod query_hydrator;
mod ranker;
mod scorer;
mod selector;
mod side_effect;
mod source;

use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use futures::future::join_all;
use thiserror::Error;
use uuid::Uuid;

use crate::api::{
    ContentStatusDto, FeedDto, FeedItemDto, FeedMetaDto, FeedQueryRequest, GrowthDomainDto,
    PostSummaryDto,
};

pub(crate) use filter::{
    DuplicateFilter, FollowingOnlyFilter, SafetyFilter, SeenFilter, ServedHistoryFilter,
};
pub(crate) use hydrator::{
    ReactionContextHydrator, RouteContextHydrator, ServedHistoryHydrator, SocialContextHydrator,
    SocialProofHydrator,
};
pub(crate) use query_hydrator::DefaultQueryHydrator;
pub(crate) use ranker::RecommendRanker;
pub(crate) use scorer::{AuthorDiversityScorer, IntentScorer, QualityScorer};
pub(crate) use selector::DiversitySelector;
pub(crate) use side_effect::ExposureSideEffect;
pub(crate) use source::RecommendRecallSource;

use crate::datasource::{
    BbsClientError, Exposure, ExposureItem, LikeStatusClientError, ModelClientError,
    RecallClientError,
};

#[derive(Clone, Debug)]
pub(crate) struct FeedQuery {
    pub(crate) interests: HashSet<GrowthDomainDto>,
    pub(crate) seen: HashSet<String>,
    pub(crate) user_id: String,
    session_id: String,
    pub(crate) surface: String,
    pub(crate) cursor: Option<String>,
    pub(crate) limit: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) post: PostSummaryDto,
    pub(crate) author_id: String,
    pub(crate) status: ContentStatusDto,
    pub(crate) quality_score: f64,
    pub(crate) score: f64,
    pub(crate) source: String,
    pub(crate) reasons: Vec<String>,
    pub(crate) followed_author: bool,
    pub(crate) blocked_author: bool,
    pub(crate) muted_author: bool,
    pub(crate) liked: bool,
    pub(crate) bookmarked: bool,
    pub(crate) hidden: bool,
    pub(crate) previously_served: bool,
}

pub(crate) struct SourceResult {
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) next_cursor: Option<String>,
    pub(crate) degraded: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RankOutcome {
    pub(crate) model_version: Option<String>,
    pub(crate) experiment_bucket: Option<String>,
    pub(crate) degraded: bool,
}

#[derive(Debug, Error)]
pub(crate) enum PipelineError {
    #[error(transparent)]
    Recall(#[from] RecallClientError),
    #[error(transparent)]
    Bbs(#[from] BbsClientError),
    #[error(transparent)]
    LikeStatus(#[from] LikeStatusClientError),
    #[error(transparent)]
    Model(#[from] ModelClientError),
}

pub(crate) trait QueryHydrator: Send + Sync {
    fn hydrate(&self, request: FeedQueryRequest) -> FeedQuery;
}

#[async_trait]
pub(crate) trait CandidateSource: Send + Sync {
    async fn get(&self, query: &FeedQuery) -> Result<SourceResult, PipelineError>;
}

#[async_trait]
pub(crate) trait CandidateHydrator: Send + Sync {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError>;
}

pub(crate) trait CandidateFilter: Send + Sync {
    fn retain(&self, query: &FeedQuery, candidate: &Candidate) -> bool;
}

pub(crate) trait CandidateScorer: Send + Sync {
    fn score(&self, query: &FeedQuery, candidates: &mut [Candidate]);
}

#[async_trait]
pub(crate) trait CandidateRanker: Send + Sync {
    async fn rank(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<RankOutcome, PipelineError>;
}

pub(crate) trait CandidateSelector: Send + Sync {
    fn select(&self, candidates: Vec<Candidate>, limit: usize) -> Vec<Candidate>;
}

#[async_trait]
pub(crate) trait PipelineSideEffect: Send + Sync {
    async fn run(&self, exposure: Exposure);
}

#[derive(Clone)]
pub(crate) struct FeedPipeline {
    query_hydrator: Arc<dyn QueryHydrator>,
    sources: Vec<Arc<dyn CandidateSource>>,
    hydrators: Vec<Arc<dyn CandidateHydrator>>,
    filters: Vec<Arc<dyn CandidateFilter>>,
    scorers: Vec<Arc<dyn CandidateScorer>>,
    ranker: Option<Arc<dyn CandidateRanker>>,
    selector: Arc<dyn CandidateSelector>,
    post_selection_filters: Vec<Arc<dyn CandidateFilter>>,
    side_effects: Vec<Arc<dyn PipelineSideEffect>>,
}

pub(crate) struct FeedPipelineComponents {
    pub(crate) query_hydrator: Arc<dyn QueryHydrator>,
    pub(crate) sources: Vec<Arc<dyn CandidateSource>>,
    pub(crate) hydrators: Vec<Arc<dyn CandidateHydrator>>,
    pub(crate) filters: Vec<Arc<dyn CandidateFilter>>,
    pub(crate) scorers: Vec<Arc<dyn CandidateScorer>>,
    pub(crate) ranker: Option<Arc<dyn CandidateRanker>>,
    pub(crate) selector: Arc<dyn CandidateSelector>,
    pub(crate) post_selection_filters: Vec<Arc<dyn CandidateFilter>>,
    pub(crate) side_effects: Vec<Arc<dyn PipelineSideEffect>>,
}

impl FeedPipeline {
    pub(crate) fn new(components: FeedPipelineComponents) -> Self {
        let FeedPipelineComponents {
            query_hydrator,
            sources,
            hydrators,
            filters,
            scorers,
            ranker,
            selector,
            post_selection_filters,
            side_effects,
        } = components;
        Self {
            query_hydrator,
            sources,
            hydrators,
            filters,
            scorers,
            ranker,
            selector,
            post_selection_filters,
            side_effects,
        }
    }

    pub(crate) async fn execute(&self, request: FeedQueryRequest) -> FeedDto {
        let query = self.query_hydrator.hydrate(request);
        tracing::debug!(
            user_id = %query.user_id,
            session_id = %query.session_id,
            surface = %query.surface,
            "feed request hydrated"
        );
        let source_results = join_all(self.sources.iter().map(|source| source.get(&query))).await;
        let mut candidates = Vec::new();
        let mut next_cursor = None;
        let mut degraded = false;
        for result in source_results {
            match result {
                Ok(result) => {
                    degraded |= result.degraded;
                    candidates.extend(result.candidates);
                    if next_cursor.is_none() {
                        next_cursor = result.next_cursor;
                    }
                }
                Err(error) => {
                    degraded = true;
                    tracing::warn!(%error, "feed source degraded");
                }
            }
        }
        let sourced = candidates.len();
        DuplicateFilter::deduplicate(&mut candidates);

        for hydrator in &self.hydrators {
            if let Err(error) = hydrator.hydrate(&query, &mut candidates).await {
                degraded = true;
                tracing::warn!(%error, "feed hydrator degraded");
            }
        }
        for filter in &self.filters {
            candidates.retain(|candidate| filter.retain(&query, candidate));
        }
        let filtered = sourced.saturating_sub(candidates.len());
        for scorer in &self.scorers {
            scorer.score(&query, &mut candidates);
        }
        let mut rank_outcome = RankOutcome::default();
        if let Some(ranker) = &self.ranker {
            match ranker.rank(&query, &mut candidates).await {
                Ok(outcome) => {
                    degraded |= outcome.degraded;
                    rank_outcome = outcome;
                }
                Err(error) => {
                    degraded = true;
                    tracing::warn!(%error, "model ranking degraded; heuristic scores retained");
                }
            }
        }

        let mut selected = self.selector.select(candidates, query.limit);
        for filter in &self.post_selection_filters {
            selected.retain(|candidate| filter.retain(&query, candidate));
        }
        let request_id = Uuid::now_v7().to_string();
        let pipeline_id = format!("bookway-recommend-main-{}", query.surface);
        let exposure = Exposure {
            request_id: request_id.clone(),
            user_id: query.user_id.clone(),
            session_id: query.session_id.clone(),
            surface: query.surface.clone(),
            pipeline_id: pipeline_id.clone(),
            candidate_count: sourced,
            degraded,
            items: selected
                .iter()
                .enumerate()
                .map(|(position, candidate)| ExposureItem {
                    position,
                    content_id: candidate.post.id.clone(),
                    source: candidate.source.clone(),
                    score: candidate.score,
                    reasons: candidate.reasons.clone(),
                })
                .collect(),
        };
        for side_effect in &self.side_effects {
            let side_effect = Arc::clone(side_effect);
            let exposure = exposure.clone();
            tokio::spawn(async move { side_effect.run(exposure).await });
        }

        let items = selected
            .into_iter()
            .map(|candidate| FeedItemDto {
                author_id: candidate.author_id,
                post: candidate.post,
                score: candidate.score,
                source: candidate.source,
                reasons: candidate.reasons,
            })
            .collect::<Vec<_>>();
        let selected_count = items.len();
        FeedDto {
            request_id,
            items,
            meta: FeedMetaDto {
                sourced,
                filtered,
                selected: selected_count,
                next_cursor,
                pipeline_id,
                degraded,
                model_version: rank_outcome.model_version,
                experiment_bucket: rank_outcome.experiment_bucket,
            },
        }
    }
}
