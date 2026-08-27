use std::collections::HashSet;

use super::{Candidate, CandidateSelector};

/// The model only sees a bounded candidate set. Keep enough candidates for
/// final reranking while reserving one slot per domain and recall source so a
/// high-volume recall channel cannot collapse exploration before reranking.
pub(crate) struct CoarseRanker;

impl CoarseRanker {
    const CANDIDATES_PER_RESULT: usize = 2;
    const MINIMUM_CANDIDATES: usize = 24;
    /// Fixed-strength fatigue discount applied inside the coarse pass so an
    /// item served earlier this window must buy back relevance elsewhere.
    const PREVIOUSLY_SERVED_PENALTY: f64 = 0.15;
    const FOLLOWED_AUTHOR_BONUS: f64 = 0.08;
    const ALREADY_ENGAGED_PENALTY: f64 = 0.05;

    pub(crate) fn candidate_limit(feed_limit: usize) -> usize {
        feed_limit
            .saturating_mul(Self::CANDIDATES_PER_RESULT)
            .max(Self::MINIMUM_CANDIDATES)
    }

    /// Independent lightweight scoring for THIS stage. Deliberately distinct
    /// from both the heuristic pre-scores (`candidate.score`) and the remote
    /// multi-objective model: coarse ranking only needs enough resolution to
    /// decide who survives truncation, so it relies on cheap hydrated facts
    /// (content quality, retrieval strength, follow state, exposure history).
    fn coarse_v1(candidate: &Candidate) -> f64 {
        let mut v1 = 0.60 * finite(candidate.quality_score)
            + 0.30 * finite(candidate.recall_score);
        if candidate.followed_author {
            v1 += Self::FOLLOWED_AUTHOR_BONUS;
        }
        if candidate.previously_served {
            v1 -= Self::PREVIOUSLY_SERVED_PENALTY;
        }
        if candidate.liked || candidate.bookmarked {
            // Already-engaged items are still relevant, just less urgent.
            v1 -= Self::ALREADY_ENGAGED_PENALTY;
        }
        v1.max(0.0)
    }
}

impl CandidateSelector for CoarseRanker {
    fn select(&self, mut candidates: Vec<Candidate>, limit: usize) -> Vec<Candidate> {
        candidates.sort_by(|left, right| {
            Self::coarse_v1(right)
                .total_cmp(&Self::coarse_v1(left))
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

fn finite(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
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
            daily_served_count: 0,
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

    #[test]
    fn lightweight_order_ignores_inherited_heuristic_score() {
        // Equal heuristic scores: coarse pass must still prefer the higher
        // static content quality instead of borrowing `candidate.score`.
        let mut strong = candidate("strong", "quality", GrowthDomain::Learning, 5.0);
        strong.quality_score = 0.9;
        let mut weak = candidate("weak", "quality", GrowthDomain::Learning, 5.0);
        weak.quality_score = 0.0;

        let selected = CoarseRanker.select(vec![weak.clone(), strong], 2);
        assert_eq!(selected[0].post.id, "strong");
    }

    #[test]
    fn served_history_and_engagement_dampen_coarse_priority() {
        let mut fresh = candidate("fresh-item", "quality", GrowthDomain::Learning, 5.0);
        fresh.quality_score = 0.8;
        let mut repeated = candidate("repeated", "quality", GrowthDomain::Learning, 5.0);
        repeated.quality_score = 0.8;
        repeated.previously_served = true;
        repeated.liked = true;

        let selected = CoarseRanker.select(vec![repeated, fresh], 2);
        assert_eq!(selected[0].post.id, "fresh-item");
    }
}
