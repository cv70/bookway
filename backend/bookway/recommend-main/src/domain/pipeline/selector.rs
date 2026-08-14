use std::collections::HashMap;

use super::{Candidate, CandidateSelector};

pub(crate) struct DiversitySelector;

impl CandidateSelector for DiversitySelector {
    fn select(&self, mut candidates: Vec<Candidate>, limit: usize) -> Vec<Candidate> {
        candidates.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.post.id.cmp(&right.post.id))
        });
        let mut selected: Vec<Candidate> = Vec::with_capacity(limit.min(candidates.len()));
        let mut author_counts = HashMap::<String, usize>::new();

        while !candidates.is_empty() && selected.len() < limit {
            let last_domain = selected.last().map(|candidate| candidate.post.domain);
            let index = candidates
                .iter()
                .position(|candidate| {
                    Some(candidate.post.domain) != last_domain
                        && author_counts
                            .get(&candidate.author_id)
                            .copied()
                            .unwrap_or_default()
                            < 2
                })
                .unwrap_or(0);
            let candidate = candidates.remove(index);
            *author_counts
                .entry(candidate.author_id.clone())
                .or_default() += 1;
            selected.push(candidate);
        }
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::DiversitySelector;
    use crate::{
        api::{ContentStatusDto, GrowthDomainDto, PostSummaryDto},
        domain::pipeline::{Candidate, CandidateSelector},
    };

    fn candidate(id: &str, author_id: &str, domain: GrowthDomainDto, score: f64) -> Candidate {
        Candidate {
            post: PostSummaryDto {
                id: id.to_string(),
                author_name: author_id.to_string(),
                author_avatar_url: String::new(),
                title: String::new(),
                summary: String::new(),
                domain,
                cover_url: String::new(),
                route_title: String::new(),
                route_duration: String::new(),
                join_count: 0,
                like_count: 0,
                freshness: 0.0,
                tags: Vec::new(),
            },
            author_id: author_id.to_string(),
            status: ContentStatusDto::Published,
            quality_score: 0.0,
            score,
            source: String::new(),
            reasons: Vec::new(),
            followed_author: false,
            blocked_author: false,
            muted_author: false,
            liked: false,
            bookmarked: false,
            previously_served: false,
        }
    }

    #[test]
    fn interleaves_domains_before_using_the_next_highest_score() {
        let selected = DiversitySelector.select(
            vec![
                candidate("travel-1", "author-a", GrowthDomainDto::Travel, 10.0),
                candidate("travel-2", "author-b", GrowthDomainDto::Travel, 9.0),
                candidate("learning-1", "author-c", GrowthDomainDto::Learning, 8.0),
            ],
            3,
        );

        assert_eq!(
            selected
                .iter()
                .map(|candidate| candidate.post.id.as_str())
                .collect::<Vec<_>>(),
            ["travel-1", "learning-1", "travel-2"]
        );
    }
}
