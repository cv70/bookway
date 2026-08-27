mod coarse;
mod filter;
mod hydrator;
mod query_hydrator;
mod ranker;
mod scorer;
mod selector;
mod side_effect;
mod source;

use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use futures::future::join_all;
use thiserror::Error;
use uuid::Uuid;

use bookway_bbs_link_api::pb as bbs_link_pb;

use crate::api::pb;

pub(crate) use coarse::CoarseRanker;
pub(crate) use filter::{
    DuplicateFilter, FollowingOnlyFilter, FrequencyCapFilter, SafetyFilter, SeenFilter,
};
pub(crate) use hydrator::{
    FrequencyCapHydrator, ReactionContextHydrator, RouteContextHydrator, ServedHistoryHydrator,
    SocialContextHydrator, SocialProofHydrator,
};
pub(crate) use query_hydrator::DefaultQueryHydrator;
pub(crate) use ranker::RecommendRanker;
pub(crate) use scorer::{AuthorDiversityScorer, IntentScorer, QualityScorer};
pub(crate) use selector::DiversitySelector;
pub(crate) use side_effect::ExposureSideEffect;
pub(crate) use source::RecommendRecallSource;

use crate::datasource::{Exposure, ExposureError, ExposureItem};

#[derive(Clone, Debug)]
pub(crate) struct FeedQuery {
    pub(crate) interests: HashSet<bbs_link_pb::GrowthDomain>,
    pub(crate) seen: HashSet<String>,
    pub(crate) user_id: String,
    session_id: String,
    pub(crate) surface: String,
    pub(crate) cursor: Option<String>,
    pub(crate) limit: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) post: bbs_link_pb::PostSummary,
    pub(crate) author_id: String,
    pub(crate) status: i32,
    pub(crate) quality_score: f64,
    // Retain the recall-stage signal separately from heuristic and model
    // scores so every ranker receives the same retrieval evidence.
    pub(crate) recall_score: f64,
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
    // Today's hard-capped served count (frequency guard). Zero until the
    // FrequencyCapHydrator runs; the filter compares it against the allowance.
    pub(crate) daily_served_count: u32,
}

pub(crate) struct SourceResult {
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) next_cursor: Option<String>,
    pub(crate) degraded: bool,
    pub(crate) pipeline_version: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RankOutcome {
    pub(crate) model_version: Option<String>,
    pub(crate) experiment_bucket: Option<String>,
    pub(crate) degraded: bool,
}

#[derive(Debug, Error)]
pub(crate) enum PipelineError {
    #[error("recommend-recall request failed: {0}")]
    Recall(String),
    #[error("bbs request failed: {0}")]
    Bbs(String),
    #[error("interaction-status request failed: {0}")]
    InteractionStatus(String),
    #[error("frequency-cap store failed: {0}")]
    FrequencyCap(#[from] crate::datasource::FrequencyCapError),
    #[error("recommend-rank request failed: {0}")]
    Model(String),
}

pub(crate) trait QueryHydrator: Send + Sync {
    fn hydrate(&self, request: pb::FeedRequest) -> FeedQuery;
}

#[async_trait]
pub(crate) trait CandidateSource: Send + Sync {
    async fn get(&self, query: &FeedQuery) -> Result<SourceResult, PipelineError>;
}

#[async_trait]
pub(crate) trait CandidateHydrator: Send + Sync {
    // A failed visibility or reaction lookup leaves safety facts unknown. Those
    // hydrators must explicitly opt into a fail-closed Feed instead of letting
    // Rust's false defaults become an accidental allow decision.
    fn failure_policy(&self) -> HydratorFailurePolicy {
        HydratorFailurePolicy::BestEffort
    }

    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HydratorFailurePolicy {
    BestEffort,
    FailClosed,
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
    async fn run(&self, exposure: Exposure) -> Result<(), ExposureError>;
}

#[derive(Clone)]
pub(crate) struct FeedPipeline {
    query_hydrator: Arc<dyn QueryHydrator>,
    sources: Vec<Arc<dyn CandidateSource>>,
    hydrators: Vec<Arc<dyn CandidateHydrator>>,
    filters: Vec<Arc<dyn CandidateFilter>>,
    scorers: Vec<Arc<dyn CandidateScorer>>,
    coarse_ranker: Arc<dyn CandidateSelector>,
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
    pub(crate) coarse_ranker: Arc<dyn CandidateSelector>,
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
            coarse_ranker,
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
            coarse_ranker,
            ranker,
            selector,
            post_selection_filters,
            side_effects,
        }
    }

    pub(crate) async fn execute(&self, request: pb::FeedRequest) -> pb::FeedResponse {
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
        let mut pipeline_versions = BTreeSet::new();
        for result in source_results {
            match result {
                Ok(result) => {
                    degraded |= result.degraded;
                    candidates.extend(result.candidates);
                    if let Some(version) = result.pipeline_version {
                        pipeline_versions.insert(version);
                    }
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

        let mut safety_context_unavailable = false;
        for hydrator in &self.hydrators {
            if let Err(error) = hydrator.hydrate(&query, &mut candidates).await {
                degraded = true;
                if hydrator.failure_policy() == HydratorFailurePolicy::FailClosed {
                    safety_context_unavailable = true;
                    candidates.clear();
                    tracing::warn!(%error, "feed safety hydrator unavailable; suppressing candidates");
                    break;
                }
                tracing::warn!(%error, "feed hydrator degraded");
            }
        }
        if safety_context_unavailable {
            // A cursor could otherwise make the client page through a response
            // whose safety context was never verified. The next fresh request
            // retries hydration from a known-safe boundary.
            next_cursor = None;
        }
        for filter in &self.filters {
            candidates.retain(|candidate| filter.retain(&query, candidate));
        }
        let filtered = sourced.saturating_sub(candidates.len());
        let mut rank_outcome = RankOutcome::default();
        let mut selected = if query.surface == "following" {
            // Following is a social timeline: source order is BBS Link's
            // stable newest-first order, not an input to personalized ranking
            // or diversity mixing.
            candidates.truncate(query.limit);
            candidates
        } else {
            if !candidates.is_empty() {
                for scorer in &self.scorers {
                    scorer.score(&query, &mut candidates);
                }
                // The coarse stage bounds expensive feature/model work while
                // retaining a broad enough pool for the final diversity pass.
                candidates = self
                    .coarse_ranker
                    .select(candidates, CoarseRanker::candidate_limit(query.limit));
            }
            if !candidates.is_empty()
                && let Some(ranker) = &self.ranker
            {
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
            self.selector.select(candidates, query.limit)
        };
        for filter in &self.post_selection_filters {
            selected.retain(|candidate| filter.retain(&query, candidate));
        }
        let request_id = Uuid::now_v7().to_string();
        let pipeline_id = pipeline_id(&query.surface, &pipeline_versions);
        let exposure = Exposure {
            request_id: request_id.clone(),
            user_id: query.user_id.clone(),
            session_id: query.session_id.clone(),
            surface: query.surface.clone(),
            pipeline_id: pipeline_id.clone(),
            model_version: rank_outcome.model_version.clone(),
            experiment_bucket: rank_outcome.experiment_bucket.clone(),
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
        // The response's request ID is the key User Event uses for attribution.
        // Persist it before returning so a legitimate immediate interaction can
        // always be verified against the exact ranked candidate.
        for side_effect in &self.side_effects {
            if let Err(error) = side_effect.run(exposure.clone()).await {
                degraded = true;
                tracing::warn!(%error, request_id = %request_id, "exposure persistence degraded");
            }
        }

        let items = selected
            .into_iter()
            .map(|candidate| pb::FeedItem {
                author_id: candidate.author_id,
                post: Some(candidate.post),
                score: candidate.score,
                source: candidate.source,
                reasons: candidate.reasons,
                ad: None,
            })
            .collect::<Vec<_>>();
        let selected_count = items.len();
        pb::FeedResponse {
            request_id,
            items,
            meta: Some(pb::FeedMeta {
                sourced: u32::try_from(sourced).unwrap_or(u32::MAX),
                filtered: u32::try_from(filtered).unwrap_or(u32::MAX),
                selected: u32::try_from(selected_count).unwrap_or(u32::MAX),
                next_cursor,
                pipeline_id,
                degraded,
                model_version: rank_outcome.model_version,
                experiment_bucket: rank_outcome.experiment_bucket,
            }),
        }
    }
}

fn pipeline_id(surface: &str, versions: &BTreeSet<String>) -> String {
    let base = format!("bookway-recommend-main-{surface}");
    if versions.is_empty() {
        return base;
    }
    format!(
        "{base}-{}",
        versions.iter().cloned().collect::<Vec<_>>().join("+")
    )
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc};

    use async_trait::async_trait;
    use bookway_bbs_link_api::pb::{ContentStatus, GrowthDomain, PostSummary};

    use super::{
        Candidate, CandidateHydrator, CandidateSource, DiversitySelector, FeedPipeline,
        FeedPipelineComponents, FeedQuery, HydratorFailurePolicy, PipelineError, SourceResult,
        pipeline_id,
    };
    use crate::api::pb;

    struct StaticSource;

    struct OrderedFollowingSource;

    #[async_trait]
    impl CandidateSource for StaticSource {
        async fn get(&self, _query: &FeedQuery) -> Result<SourceResult, PipelineError> {
            Ok(SourceResult {
                candidates: vec![Candidate {
                    post: PostSummary {
                        id: "content-1".to_string(),
                        author_name: "作者".to_string(),
                        author_avatar_url: String::new(),
                        title: "安全边界".to_string(),
                        summary: String::new(),
                        domain: GrowthDomain::Learning as i32,
                        cover_url: String::new(),
                        route_title: String::new(),
                        route_duration: String::new(),
                        join_count: 0,
                        like_count: 0,
                        freshness: 0.0,
                        tags: Vec::new(),
                        is_route: false,
                        is_milestone: false,
                        is_question: false,
                    },
                    author_id: "author-1".to_string(),
                    status: ContentStatus::Published as i32,
                    quality_score: 0.0,
                    recall_score: 1.0,
                    score: 1.0,
                    source: "test".to_string(),
                    reasons: Vec::new(),
                    followed_author: false,
                    blocked_author: false,
                    muted_author: false,
                    liked: false,
                    bookmarked: false,
                    hidden: false,
                    previously_served: false,
            daily_served_count: 0,
                }],
                next_cursor: Some("page-2".to_string()),
                degraded: false,
                pipeline_version: None,
            })
        }
    }

    #[async_trait]
    impl CandidateSource for OrderedFollowingSource {
        async fn get(&self, _query: &FeedQuery) -> Result<SourceResult, PipelineError> {
            Ok(SourceResult {
                // This is newest-first source order. Scores are deliberately
                // inverted so the test catches accidental ranking or mixing.
                candidates: vec![
                    test_candidate("newest", "author-a", 0.1),
                    test_candidate("older", "author-b", 0.9),
                ],
                next_cursor: Some("following-page-2".to_string()),
                degraded: false,
                pipeline_version: None,
            })
        }
    }

    struct FailingHydrator(HydratorFailurePolicy);

    #[test]
    fn pipeline_id_persists_the_recall_strategy_version() {
        assert_eq!(
            pipeline_id("home", &BTreeSet::from(["balanced-v1".to_string()])),
            "bookway-recommend-main-home-balanced-v1"
        );
        assert_eq!(
            pipeline_id(
                "home",
                &BTreeSet::from(["balanced-v1".to_string(), "score-v1".to_string()]),
            ),
            "bookway-recommend-main-home-balanced-v1+score-v1"
        );
    }

    #[async_trait]
    impl CandidateHydrator for FailingHydrator {
        fn failure_policy(&self) -> HydratorFailurePolicy {
            self.0
        }

        async fn hydrate(
            &self,
            _query: &FeedQuery,
            _candidates: &mut [Candidate],
        ) -> Result<(), PipelineError> {
            Err(PipelineError::Bbs("context unavailable".to_string()))
        }
    }

    fn pipeline(policy: HydratorFailurePolicy) -> FeedPipeline {
        FeedPipeline::new(FeedPipelineComponents {
            query_hydrator: Arc::new(super::DefaultQueryHydrator),
            sources: vec![Arc::new(StaticSource)],
            hydrators: vec![Arc::new(FailingHydrator(policy))],
            filters: Vec::new(),
            scorers: Vec::new(),
            coarse_ranker: Arc::new(super::CoarseRanker),
            ranker: None,
            selector: Arc::new(DiversitySelector),
            post_selection_filters: Vec::new(),
            side_effects: Vec::new(),
        })
    }

    fn chronological_pipeline() -> FeedPipeline {
        FeedPipeline::new(FeedPipelineComponents {
            query_hydrator: Arc::new(super::DefaultQueryHydrator),
            sources: vec![Arc::new(OrderedFollowingSource)],
            hydrators: Vec::new(),
            filters: Vec::new(),
            scorers: Vec::new(),
            coarse_ranker: Arc::new(super::CoarseRanker),
            ranker: None,
            selector: Arc::new(DiversitySelector),
            post_selection_filters: Vec::new(),
            side_effects: Vec::new(),
        })
    }

    fn request() -> pb::FeedRequest {
        pb::FeedRequest {
            user_id: "user-1".to_string(),
            interests: Vec::new(),
            seen: Vec::new(),
            limit: Some(10),
            session_id: "session-1".to_string(),
            surface: "home".to_string(),
            cursor: None,
            action_context: None,
        }
    }

    #[tokio::test]
    async fn hides_all_candidates_when_a_safety_hydrator_is_unavailable() {
        let response = pipeline(HydratorFailurePolicy::FailClosed)
            .execute(request())
            .await;

        let meta = response.meta.expect("feed metadata");
        assert!(response.items.is_empty());
        assert_eq!(meta.sourced, 1);
        assert_eq!(meta.filtered, 1);
        assert_eq!(meta.selected, 0);
        assert!(meta.degraded);
        assert!(meta.next_cursor.is_none());
    }

    #[tokio::test]
    async fn retains_candidates_for_an_optional_hydrator_outage() {
        let response = pipeline(HydratorFailurePolicy::BestEffort)
            .execute(request())
            .await;

        let meta = response.meta.expect("feed metadata");
        assert_eq!(response.items.len(), 1);
        assert!(meta.degraded);
        assert_eq!(meta.next_cursor.as_deref(), Some("page-2"));
    }

    #[tokio::test]
    async fn following_surface_keeps_newest_first_source_order() {
        let response = chronological_pipeline()
            .execute(pb::FeedRequest {
                surface: "following".to_string(),
                limit: Some(2),
                ..request()
            })
            .await;

        assert_eq!(
            response
                .items
                .iter()
                .filter_map(|item| item.post.as_ref().map(|post| post.id.as_str()))
                .collect::<Vec<_>>(),
            vec!["newest", "older"]
        );
        assert!(
            response
                .meta
                .as_ref()
                .is_some_and(|meta| meta.model_version.is_none())
        );
    }

    fn test_candidate(id: &str, author_id: &str, score: f64) -> Candidate {
        Candidate {
            post: PostSummary {
                id: id.to_string(),
                author_name: author_id.to_string(),
                author_avatar_url: String::new(),
                title: id.to_string(),
                summary: String::new(),
                domain: GrowthDomain::Learning as i32,
                cover_url: String::new(),
                route_title: String::new(),
                route_duration: String::new(),
                join_count: 0,
                like_count: 0,
                freshness: 0.0,
                tags: Vec::new(),
                is_route: false,
                is_milestone: false,
                is_question: false,
            },
            author_id: author_id.to_string(),
            status: ContentStatus::Published as i32,
            quality_score: 0.0,
            recall_score: score,
            score,
            source: "test".to_string(),
            reasons: Vec::new(),
            followed_author: true,
            blocked_author: false,
            muted_author: false,
            liked: false,
            bookmarked: false,
            hidden: false,
            previously_served: false,
            daily_served_count: 0,
        }
    }
}
