use std::collections::HashSet;

use bookway_api::{ContentDto, RecommendationCandidateDto};

use crate::api::pb;

pub(crate) fn candidate_from_content(
    content: ContentDto,
    source: &str,
) -> RecommendationCandidateDto {
    let freshness = content.post.freshness;
    let quality_score = content.quality_score;
    let (recall_score, reasons) = recall_details(source, quality_score, freshness);
    RecommendationCandidateDto {
        content_id: content.post.id.clone(),
        post: content.post,
        author_id: content.author_id,
        status: content.status,
        quality_score,
        freshness,
        recall_score,
        score: 0.0,
        source: format!("recall:{source}"),
        reasons,
    }
}

fn recall_details(source: &str, quality_score: f64, freshness: f64) -> (f64, Vec<String>) {
    match source {
        "fresh" => (freshness, vec!["来自新内容召回".to_string()]),
        source if source.starts_with("interest:") => (
            quality_score * 0.8 + freshness * 0.2 + 0.25,
            vec![format!("{}", interest_reason(source))],
        ),
        _ => (quality_score, vec!["来自优质内容召回".to_string()]),
    }
}

fn interest_reason(source: &str) -> &'static str {
    match source.strip_prefix("interest:") {
        Some("learning") => "符合你的学习兴趣",
        Some("movement") => "符合你的运动兴趣",
        Some("wellness") => "符合你的健康兴趣",
        Some("travel") => "符合你的旅行兴趣",
        Some("leisure") => "符合你的休闲兴趣",
        _ => "符合你的兴趣",
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

#[cfg(test)]
mod tests {
    use super::recall_details;

    #[test]
    fn interest_source_boosts_matching_content_and_keeps_user_reason() {
        let (score, reasons) = recall_details("interest:travel", 0.6, 0.8);

        assert!((score - 0.89).abs() < f64::EPSILON);
        assert_eq!(reasons, ["符合你的旅行兴趣"]);
    }
}
