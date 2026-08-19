use std::collections::HashSet;

use super::{Candidate, CandidateSelector};

/// The model only sees a bounded candidate set. Keep enough candidates for
/// final reranking while reserving one slot per domain and recall source so a
/// high-volume recall channel cannot collapse exploration before reranking.
pub(crate) struct CoarseRanker;

impl CoarseRanker {
    const CANDIDATES_PER_RESULT: usize = 2;
    const MINIMUM_CANDIDATES: usize = 24;

    pub(crate) fn candidate_limit(feed_limit: usize) -> usize {
        feed_limit
            .saturating_mul(Self::CANDIDATES_PER_RESULT)
            .max(Self::MINIMUM_CANDIDATES)
    }
}

impl CandidateSelector for CoarseRanker {
    fn select(&self, mut candidates: Vec<Candidate>, limit: usize) -> Vec<Candidate> {
        candidates.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.post.id.cmp(&right.post.id))
        });
        if candidates.len() <= limit {
            return candidates;
        }

        let mut selected = Vec::with_capacity(limit);
        let mut represented_domains = HashSet::new();
        let mut represented_sources = HashSet::new();
        let mut retained = vec![false; candidates.len()];

        for (index, candidate) in candidates.iter().enumerate() {
            if represented_domains.insert(candidate.post.domain) && selected.len() < limit {
                let source = candidate.source.clone();
                let mut candidate = candidate.clone();
                candidate.reasons.push("粗排保留多样召回来源".to_string());
                selected.push(candidate);
                retained[index] = true;
                represented_sources.insert(source);
            }
        }
        for (index, candidate) in candidates.iter().enumerate() {
            if !retained[index]
                && represented_sources.insert(candidate.source.clone())
                && selected.len() < limit
            {
                let mut candidate = candidate.clone();
                candidate.reasons.push("粗排保留多样召回来源".to_string());
                selected.push(candidate);
                retained[index] = true;
            }
        }
        for (index, candidate) in candidates.into_iter().enumerate() {
            if selected.len() >= limit {
                break;
            }
            if !retained[index] {
                selected.push(candidate);
            }
        }
        selected
    }
}

#[cfg(test)]
mod tests {
    use bookway_bbs_link_api::pb::{ContentStatus, GrowthDomain, PostSummary};

    use super::CoarseRanker;
    use crate::domain::pipeline::{Candidate, CandidateSelector};

    fn candidate(id: &str, source: &str, domain: GrowthDomain, score: f64) -> Candidate {
        Candidate {
            post: PostSummary {
                id: id.to_string(),
                domain: domain as i32,
                ..Default::default()
            },
            author_id: id.to_string(),
            status: ContentStatus::Published as i32,
            quality_score: 0.0,
            recall_score: score,
            score,
            source: source.to_string(),
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

    #[test]
    fn reserves_diverse_recall_context_before_filling_by_score() {
        let selected = CoarseRanker.select(
            vec![
                candidate("learning-quality", "quality", GrowthDomain::Learning, 10.0),
                candidate("learning-fresh", "fresh", GrowthDomain::Learning, 9.0),
                candidate("travel-quality", "quality", GrowthDomain::Travel, 1.0),
            ],
            2,
        );

        assert_eq!(selected.len(), 2);
        assert!(
            selected
                .iter()
                .any(|candidate| candidate.post.domain == GrowthDomain::Travel as i32)
        );
        assert!(selected.iter().all(|candidate| {
            candidate
                .reasons
                .iter()
                .any(|reason| reason == "粗排保留多样召回来源")
        }));
    }
}
