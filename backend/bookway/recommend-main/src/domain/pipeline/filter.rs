use std::collections::HashSet;

use super::{Candidate, CandidateFilter, FeedQuery};
use crate::api::ContentStatusDto;

pub(crate) struct SeenFilter;

impl CandidateFilter for SeenFilter {
    fn retain(&self, query: &FeedQuery, candidate: &Candidate) -> bool {
        !query.seen.contains(&candidate.post.id)
    }
}

pub(crate) struct ServedHistoryFilter;

impl CandidateFilter for ServedHistoryFilter {
    fn retain(&self, _query: &FeedQuery, candidate: &Candidate) -> bool {
        !candidate.previously_served
    }
}

pub(crate) struct SafetyFilter;

impl CandidateFilter for SafetyFilter {
    fn retain(&self, _query: &FeedQuery, candidate: &Candidate) -> bool {
        candidate.status == ContentStatusDto::Published
            && !candidate.blocked_author
            && !candidate.muted_author
            && !candidate.hidden
    }
}

pub(crate) struct FollowingOnlyFilter;

impl CandidateFilter for FollowingOnlyFilter {
    fn retain(&self, query: &FeedQuery, candidate: &Candidate) -> bool {
        query.surface != "following" || candidate.followed_author
    }
}

pub(crate) struct DuplicateFilter;

impl DuplicateFilter {
    pub(crate) fn deduplicate(candidates: &mut Vec<Candidate>) {
        let mut ids = HashSet::new();
        candidates.retain(|candidate| ids.insert(candidate.post.id.clone()));
    }
}

impl CandidateFilter for DuplicateFilter {
    fn retain(&self, _query: &FeedQuery, _candidate: &Candidate) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{CandidateFilter, FollowingOnlyFilter};
    use crate::{
        api::{ContentStatusDto, GrowthDomainDto, PostSummaryDto},
        domain::pipeline::{Candidate, FeedQuery},
    };

    fn query(surface: &str) -> FeedQuery {
        FeedQuery {
            interests: HashSet::new(),
            seen: HashSet::new(),
            user_id: "user-1".to_string(),
            session_id: "session-1".to_string(),
            surface: surface.to_string(),
            cursor: None,
            limit: 10,
        }
    }

    fn candidate(followed_author: bool) -> Candidate {
        Candidate {
            post: PostSummaryDto {
                id: "post-1".to_string(),
                author_name: "作者".to_string(),
                author_avatar_url: String::new(),
                title: String::new(),
                summary: String::new(),
                domain: GrowthDomainDto::Learning,
                cover_url: String::new(),
                route_title: String::new(),
                route_duration: String::new(),
                join_count: 0,
                like_count: 0,
                freshness: 0.0,
                tags: Vec::new(),
            },
            author_id: "author-1".to_string(),
            status: ContentStatusDto::Published,
            quality_score: 1.0,
            score: 1.0,
            source: "test".to_string(),
            reasons: Vec::new(),
            followed_author,
            blocked_author: false,
            muted_author: false,
            liked: false,
            bookmarked: false,
            hidden: false,
            previously_served: false,
        }
    }

    #[test]
    fn following_surface_only_keeps_followed_authors() {
        assert!(FollowingOnlyFilter.retain(&query("following"), &candidate(true)));
        assert!(!FollowingOnlyFilter.retain(&query("following"), &candidate(false)));
    }

    #[test]
    fn home_surface_is_not_restricted_to_followed_authors() {
        assert!(FollowingOnlyFilter.retain(&query("home"), &candidate(false)));
    }
}
