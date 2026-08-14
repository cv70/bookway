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
