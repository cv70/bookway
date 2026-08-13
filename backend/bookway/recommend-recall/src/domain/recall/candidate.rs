use std::collections::HashSet;

use bookway_api::{ContentDto, RecommendationCandidateDto};

use crate::api::pb;

pub(crate) fn candidate_from_content(
    content: ContentDto,
    strategy: &str,
) -> RecommendationCandidateDto {
    let freshness = content.post.freshness;
    let quality_score = content.quality_score;
    RecommendationCandidateDto {
        content_id: content.post.id.clone(),
        post: content.post,
        author_id: content.author_id,
        status: content.status,
        quality_score,
        freshness,
        recall_score: if strategy == "fresh" {
            freshness
        } else {
            quality_score
        },
        score: 0.0,
        source: format!("recall:{strategy}"),
        reasons: Vec::new(),
    }
}

pub(crate) fn to_proto(candidate: RecommendationCandidateDto) -> pb::Candidate {
    pb::Candidate {
        content_id: candidate.content_id,
        post_json: serde_json::to_string(&candidate.post).unwrap_or_default(),
        author_id: candidate.author_id,
        status: serde_json::to_string(&candidate.status).unwrap_or_default(),
        quality_score: candidate.quality_score,
        freshness: candidate.freshness,
        recall_score: candidate.recall_score,
        score: candidate.score,
        source: candidate.source,
        reasons: candidate.reasons,
    }
}

pub(crate) fn seen(values: &[String]) -> HashSet<String> {
    values.iter().cloned().collect()
}
