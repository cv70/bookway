use std::collections::HashMap;

use super::{Candidate, CandidateScorer, FeedQuery};

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
            let domain = bookway_bbs_link_api::pb::GrowthDomain::try_from(candidate.post.domain)
                .unwrap_or(bookway_bbs_link_api::pb::GrowthDomain::Learning);
            if query.interests.contains(&domain) {
                candidate.score += 2.5;
                candidate
                    .reasons
                    .insert(0, format!("符合你的{}兴趣", domain_label(domain)));
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
        let mut ordered: Vec<_> = (0..candidates.len()).collect();
        ordered.sort_by(|left, right| {
            candidates[*right]
                .score
                .total_cmp(&candidates[*left].score)
                .then_with(|| candidates[*left].post.id.cmp(&candidates[*right].post.id))
        });
        let mut counts = HashMap::<String, usize>::new();
        for index in ordered {
            let candidate = &mut candidates[index];
            let count = counts.entry(candidate.author_id.clone()).or_default();
            candidate.score *= 0.65_f64.powi(*count as i32).max(0.35);
            if *count > 0 {
                candidate.reasons.push("为你打散了作者重复内容".to_string());
            }
            *count += 1;
        }
    }
}

fn domain_label(domain: bookway_bbs_link_api::pb::GrowthDomain) -> &'static str {
    match domain {
        bookway_bbs_link_api::pb::GrowthDomain::Learning => "学习",
        bookway_bbs_link_api::pb::GrowthDomain::Movement => "运动",
        bookway_bbs_link_api::pb::GrowthDomain::Wellness => "健康",
        bookway_bbs_link_api::pb::GrowthDomain::Travel => "旅行",
        bookway_bbs_link_api::pb::GrowthDomain::Leisure => "休闲",
    }
}
