use std::collections::HashSet;

use bookway_bbs_link_api::pb::ContentStatus;

use super::{Candidate, CandidateFilter, FeedQuery};

pub(crate) struct SeenFilter;

impl CandidateFilter for SeenFilter {
    fn retain(&self, query: &FeedQuery, candidate: &Candidate) -> bool {
        !query.seen.contains(&candidate.post.id)
    }
}

pub(crate) struct SafetyFilter;

impl CandidateFilter for SafetyFilter {
    fn retain(&self, _query: &FeedQuery, candidate: &Candidate) -> bool {
        candidate.status == ContentStatus::Published as i32
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

/// Hard daily exposure guard. Counters come from `FrequencyCapHydrator`; on a
/// failed lookup the hydrator errors before this filter runs, so the feed
/// fails open instead of silently over-serving.
/// A cap of zero disables the guard (never filters everything).
pub(crate) struct FrequencyCapFilter {
    pub(crate) daily_cap: u32,
}

impl CandidateFilter for FrequencyCapFilter {
    fn retain(&self, _query: &FeedQuery, candidate: &Candidate) -> bool {
        self.daily_cap == 0 || candidate.daily_served_count < self.daily_cap
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use bookway_bbs_link_api::pb::{ContentStatus, GrowthDomain, PostSummary};

    use super::{CandidateFilter, FollowingOnlyFilter};
    #[cfg(test)]
    use super::FrequencyCapFilter;
    use crate::domain::pipeline::{Candidate, FeedQuery};

    fn query(surface: &str) -> FeedQuery {
        FeedQuery {
            interests: HashSet::new(),
            seen: HashSet::new(),
            user_id: Some("user-1".to_string()),
            session_id: Some("session-1".to_string()),
            surface: surface.to_string(),
            cursor: None,
            limit: 10,
        }
    }

    fn candidate(followed_author: bool) -> Candidate {
        Candidate {
            post: PostSummary {
                id: "post-1".to_string(),
                author_name: "作者".to_string(),
                author_avatar_url: String::new(),
                title: String::new(),
                summary: String::new(),
                domain: GrowthDomain::Learning as i32,
                cover_url: String::new(),
                route_title: String::new(),
                route_duration: String::new(),
                join_count: None,
                like_count: 0,
                freshness: 0.0,
                tags: Vec::new(),
                is_route: false,
                is_milestone: false,
                is_question: false,
                fork_count: 0,
            },
            author_id: "author-1".to_string(),
            status: ContentStatus::Published as i32,
            quality_score: 1.0,
            recall_score: 1.0,
            score: 1.0,
            p_ctr: 0.0,
            p_cvr: 0.0,
            p_wegu: 0.0,
            feature_snapshot: Default::default(),
            source: "test".to_string(),
            reasons: Vec::new(),
            followed_author,
            blocked_author: false,
            muted_author: false,
            liked: false,
            bookmarked: false,
            hidden: false,
            previously_served: false,
            daily_served_count: 0,
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

    #[test]
    fn frequency_cap_drops_items_at_or_over_the_daily_allowance() {
        let filter = FrequencyCapFilter { daily_cap: 3 };
        let mut fresh = candidate(true);
        fresh.daily_served_count = 2;
        let mut at_cap = candidate(true);
        at_cap.daily_served_count = 3;
        let mut over_cap = candidate(true);
        over_cap.daily_served_count = 9;

        assert!(filter.retain(&query("home"), &fresh));
        assert!(!filter.retain(&query("home"), &at_cap));
        assert!(!filter.retain(&query("home"), &over_cap));
    }

    #[test]
    fn zero_frequency_cap_disables_the_guard() {
        let filter = FrequencyCapFilter { daily_cap: 0 };
        let mut exhausted = candidate(true);
        exhausted.daily_served_count = u32::MAX;
        assert!(filter.retain(&query("home"), &exhausted));
    }

    #[test]
    fn frequency_counts_start_at_zero_when_unhydrated() {
        let filter = FrequencyCapFilter { daily_cap: 1 };
        assert!(filter.retain(&query("home"), &candidate(true)));
    }
}
