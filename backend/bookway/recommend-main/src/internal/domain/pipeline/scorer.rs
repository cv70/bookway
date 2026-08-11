use std::collections::HashMap;

use super::{Candidate, CandidateScorer, FeedQuery};
use crate::internal::api::GrowthDomainDto;

pub(crate) struct QualityScorer;

impl CandidateScorer for QualityScorer {
    fn score(&self, _query: &FeedQuery, candidates: &mut [Candidate]) {
        for candidate in candidates {
            let popularity = f64::from(candidate.post.like_count).ln_1p() / 10.0;
            let freshness = candidate.post.freshness.clamp(0.0, 1.0);
            candidate.score += candidate.quality_score * 0.8 + popularity + freshness;
        }
    }
}

pub(crate) struct IntentScorer;

impl CandidateScorer for IntentScorer {
    fn score(&self, query: &FeedQuery, candidates: &mut [Candidate]) {
        for candidate in candidates {
            if query.interests.contains(&candidate.post.domain) {
                candidate.score += 2.5;
                candidate.reasons.insert(
                    0,
                    format!("符合你的{}兴趣", domain_label(candidate.post.domain)),
                );
            }
            if candidate.followed_author {
                candidate.score += 2.0;
            }
            candidate.score += candidate.post.freshness * 1.5;
        }
    }
}

pub(crate) struct AuthorDiversityScorer;

impl CandidateScorer for AuthorDiversityScorer {
    fn score(&self, _query: &FeedQuery, candidates: &mut [Candidate]) {
        let mut counts = HashMap::<String, usize>::new();
        for candidate in candidates {
            let count = counts.entry(candidate.author_id.clone()).or_default();
            candidate.score *= 1.0 / (1.0 + *count as f64 * 0.35);
            if *count > 0 {
                candidate.reasons.push("为你打散了作者重复内容".to_string());
            }
            *count += 1;
        }
    }
}

fn domain_label(domain: GrowthDomainDto) -> &'static str {
    match domain {
        GrowthDomainDto::Learning => "学习",
        GrowthDomainDto::Movement => "运动",
        GrowthDomainDto::Wellness => "健康",
        GrowthDomainDto::Travel => "旅行",
        GrowthDomainDto::Leisure => "休闲",
    }
}
