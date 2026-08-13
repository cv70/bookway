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

pub(crate) use filter::{DuplicateFilter, SafetyFilter, SeenFilter};
pub(crate) use hydrator::{ReactionContextHydrator, SocialContextHydrator, SocialProofHydrator};
pub(crate) use query_hydrator::DefaultQueryHydrator;
pub(crate) use ranker::RecommendRanker;
pub(crate) use scorer::{AuthorDiversityScorer, IntentScorer, QualityScorer};
pub(crate) use selector::DiversitySelector;
pub(crate) use side_effect::ExposureSideEffect;
pub(crate) use source::RecommendRecallSource;

use crate::datasource::{
    BbsClientError, LikeStatusClientError, ModelClientError, RecallClientError,
};

#[derive(Clone, Debug)]
pub(crate) struct FeedQuery {
    pub(crate) interests: HashSet<GrowthDomainDto>,
    pub(crate) seen: HashSet<String>,
    pub(crate) user_id: String,
    session_id: String,
    surface: String,
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
}

pub(crate) struct SourceResult {
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) next_cursor: Option<String>,
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
    ) -> Result<(), PipelineError>;
}

pub(crate) trait CandidateSelector: Send + Sync {
    fn select(&self, candidates: Vec<Candidate>, limit: usize) -> Vec<Candidate>;
}

#[async_trait]
pub(crate) trait PipelineSideEffect: Send + Sync {
    async fn run(
        &self,
        request_id: String,
        user_id: String,
        session_id: String,
        surface: String,
        post_ids: Vec<String>,
    );
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
        if let Some(ranker) = &self.ranker
            && let Err(error) = ranker.rank(&query, &mut candidates).await
        {
            degraded = true;
            tracing::warn!(%error, "model ranking degraded; heuristic scores retained");
        }

        let mut selected = self.selector.select(candidates, query.limit);
        for filter in &self.post_selection_filters {
            selected.retain(|candidate| filter.retain(&query, candidate));
        }
        let request_id = Uuid::now_v7().to_string();
        let post_ids: Vec<_> = selected
            .iter()
            .map(|candidate| candidate.post.id.clone())
            .collect();
        for side_effect in &self.side_effects {
            let side_effect = Arc::clone(side_effect);
            let request_id = request_id.clone();
            let user_id = query.user_id.clone();
            let session_id = query.session_id.clone();
            let surface = query.surface.clone();
            let post_ids = post_ids.clone();
            tokio::spawn(async move {
                side_effect
                    .run(request_id, user_id, session_id, surface, post_ids)
                    .await
            });
        }

        let items = selected
            .into_iter()
            .map(|candidate| FeedItemDto {
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
                pipeline_id: format!("bookway-recommend-main-{}", query.surface),
                degraded,
            },
        }
    }
}
