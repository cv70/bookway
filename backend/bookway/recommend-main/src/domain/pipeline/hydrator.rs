use async_trait::async_trait;

use super::{Candidate, CandidateHydrator, FeedQuery, PipelineError};
use crate::datasource::{
    SharedBbsContextDataSource, SharedExposureDataSource, SharedLikeStatusDataSource,
};

const SERVED_HISTORY_LIMIT: usize = 500;

pub(crate) struct ServedHistoryHydrator {
    exposures: SharedExposureDataSource,
}

impl ServedHistoryHydrator {
    pub(crate) fn new(exposures: SharedExposureDataSource) -> Self {
        Self { exposures }
    }
}

#[async_trait]
impl CandidateHydrator for ServedHistoryHydrator {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let served = self
            .exposures
            .recent_content_ids(&query.user_id, &query.surface, SERVED_HISTORY_LIMIT)
            .await;
        for candidate in candidates {
            candidate.previously_served = served.contains(&candidate.post.id);
        }
        Ok(())
    }
}

pub(crate) struct SocialContextHydrator {
    bbs: SharedBbsContextDataSource,
}

impl SocialContextHydrator {
    pub(crate) fn new(bbs: SharedBbsContextDataSource) -> Self {
        Self { bbs }
    }
}

#[async_trait]
impl CandidateHydrator for SocialContextHydrator {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let (context, visibility) = tokio::try_join!(
            self.bbs.context(&query.user_id),
            self.bbs.visibility_context(&query.user_id),
        )?;
        for candidate in candidates {
            candidate.followed_author = context.followed_author_ids.contains(&candidate.author_id);
            candidate.blocked_author = visibility
                .excluded_author_ids
                .contains(&candidate.author_id);
            candidate.muted_author = context.muted_author_ids.contains(&candidate.author_id);
        }
        Ok(())
    }
}

pub(crate) struct RouteContextHydrator {
    bbs: SharedBbsContextDataSource,
}

impl RouteContextHydrator {
    pub(crate) fn new(bbs: SharedBbsContextDataSource) -> Self {
        Self { bbs }
    }
}

#[async_trait]
impl CandidateHydrator for RouteContextHydrator {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let route_ids = candidates
            .iter()
            .filter(|candidate| !candidate.post.route_title.trim().is_empty())
            .map(|candidate| candidate.post.id.clone())
            .collect::<Vec<_>>();
        if route_ids.is_empty() {
            return Ok(());
        }
        let context = self.bbs.route_context(&query.user_id, route_ids).await?;
        for candidate in candidates {
            let live_count = context
                .participant_counts
                .get(&candidate.post.id)
                .copied()
                .unwrap_or_default();
            candidate.post.join_count = candidate
                .post
                .join_count
                .saturating_add(u32::try_from(live_count).unwrap_or(u32::MAX));
            if context.joined_route_ids.contains(&candidate.post.id) {
                candidate.reasons.insert(0, "你正在走这条路线".to_string());
            }
        }
        Ok(())
    }
}

pub(crate) struct ReactionContextHydrator {
    like_status: SharedLikeStatusDataSource,
}

impl ReactionContextHydrator {
    pub(crate) fn new(like_status: SharedLikeStatusDataSource) -> Self {
        Self { like_status }
    }
}

#[async_trait]
impl CandidateHydrator for ReactionContextHydrator {
    async fn hydrate(
        &self,
        query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        let post_ids = candidates
            .iter()
            .map(|candidate| candidate.post.id.clone())
            .collect();
        let context = self.like_status.context(&query.user_id, post_ids).await?;
        for candidate in candidates {
            candidate.liked = context.liked_post_ids.contains(&candidate.post.id);
            candidate.bookmarked = context.bookmarked_post_ids.contains(&candidate.post.id);
            candidate.hidden = context.hidden_post_ids.contains(&candidate.post.id);
        }
        Ok(())
    }
}

pub(crate) struct SocialProofHydrator;

#[async_trait]
impl CandidateHydrator for SocialProofHydrator {
    async fn hydrate(
        &self,
        _query: &FeedQuery,
        candidates: &mut [Candidate],
    ) -> Result<(), PipelineError> {
        for candidate in candidates {
            if !candidate.post.route_title.trim().is_empty() && candidate.post.join_count > 0 {
                candidate
                    .reasons
                    .push(format!("{} 人正在同行", candidate.post.join_count));
            }
            if candidate.followed_author {
                candidate.reasons.insert(0, "来自你关注的作者".to_string());
            }
            if candidate.liked {
                candidate.reasons.push("你已经赞过这篇内容".to_string());
            }
            if candidate.bookmarked {
                candidate.reasons.push("你已经收藏过这篇内容".to_string());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use bookway_api::{
        ContentStatusDto, GrowthDomainDto, PostSummaryDto, RouteParticipationContextDto,
        SocialContextDto, SocialVisibilityDto,
    };

    use super::*;
    use crate::datasource::{BbsClientError, BbsContextDataSource};

    struct RouteContextStub;

    struct VisibilityContextStub;

    #[async_trait]
    impl BbsContextDataSource for RouteContextStub {
        async fn context(&self, _user_id: &str) -> Result<SocialContextDto, BbsClientError> {
            Ok(SocialContextDto {
                followed_author_ids: Vec::new(),
                blocked_author_ids: Vec::new(),
                muted_author_ids: Vec::new(),
                liked_post_ids: Vec::new(),
                bookmarked_post_ids: Vec::new(),
            })
        }

        async fn visibility_context(
            &self,
            _user_id: &str,
        ) -> Result<SocialVisibilityDto, BbsClientError> {
            Ok(SocialVisibilityDto::default())
        }

        async fn route_context(
            &self,
            _user_id: &str,
            route_ids: Vec<String>,
        ) -> Result<RouteParticipationContextDto, BbsClientError> {
            assert_eq!(route_ids, vec!["route-a"]);
            Ok(RouteParticipationContextDto {
                joined_route_ids: vec!["route-a".to_string()],
                participant_counts: HashMap::from([("route-a".to_string(), 12)]),
            })
        }
    }

    #[async_trait]
    impl BbsContextDataSource for VisibilityContextStub {
        async fn context(&self, _user_id: &str) -> Result<SocialContextDto, BbsClientError> {
            Ok(SocialContextDto {
                followed_author_ids: Vec::new(),
                blocked_author_ids: Vec::new(),
                muted_author_ids: Vec::new(),
                liked_post_ids: Vec::new(),
                bookmarked_post_ids: Vec::new(),
            })
        }

        async fn visibility_context(
            &self,
            _user_id: &str,
        ) -> Result<SocialVisibilityDto, BbsClientError> {
            Ok(SocialVisibilityDto {
                excluded_author_ids: vec!["author-inbound-block".to_string()],
            })
        }

        async fn route_context(
            &self,
            _user_id: &str,
            _route_ids: Vec<String>,
        ) -> Result<RouteParticipationContextDto, BbsClientError> {
            Ok(RouteParticipationContextDto::default())
        }
    }

    fn query() -> FeedQuery {
        FeedQuery {
            interests: Default::default(),
            seen: Default::default(),
            user_id: "viewer".to_string(),
            session_id: "session-a".to_string(),
            surface: "home".to_string(),
            cursor: None,
            limit: 10,
        }
    }

    fn candidate(author_id: &str) -> Candidate {
        Candidate {
            post: PostSummaryDto {
                id: "post-a".to_string(),
                author_name: "author".to_string(),
                author_avatar_url: String::new(),
                title: "title".to_string(),
                summary: String::new(),
                domain: GrowthDomainDto::Learning,
                cover_url: String::new(),
                route_title: String::new(),
                route_duration: String::new(),
                join_count: 0,
                like_count: 0,
                freshness: 1.0,
                tags: Vec::new(),
            },
            author_id: author_id.to_string(),
            status: ContentStatusDto::Published,
            quality_score: 1.0,
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
        }
    }

    #[tokio::test]
    async fn social_context_hides_an_author_who_blocked_the_viewer() {
        let hydrator = SocialContextHydrator::new(Arc::new(VisibilityContextStub));
        let mut candidates = vec![
            candidate("author-inbound-block"),
            candidate("author-visible"),
        ];

        hydrator
            .hydrate(&query(), &mut candidates)
            .await
            .expect("social visibility");

        assert!(candidates[0].blocked_author);
        assert!(!candidates[1].blocked_author);
    }

    #[tokio::test]
    async fn route_context_adds_live_counts_and_companionship_reason() {
        let hydrator = RouteContextHydrator::new(Arc::new(RouteContextStub));
        let query = query();
        let mut candidates = vec![Candidate {
            post: PostSummaryDto {
                id: "route-a".to_string(),
                author_name: "author".to_string(),
                author_avatar_url: String::new(),
                title: "title".to_string(),
                summary: String::new(),
                domain: GrowthDomainDto::Learning,
                cover_url: String::new(),
                route_title: "route".to_string(),
                route_duration: "4 周".to_string(),
                join_count: 100,
                like_count: 0,
                freshness: 1.0,
                tags: Vec::new(),
            },
            author_id: "author-a".to_string(),
            status: ContentStatusDto::Published,
            quality_score: 1.0,
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
        }];

        hydrator
            .hydrate(&query, &mut candidates)
            .await
            .expect("route context");

        assert_eq!(candidates[0].post.join_count, 112);
        assert_eq!(candidates[0].reasons, vec!["你正在走这条路线"]);
    }
}
