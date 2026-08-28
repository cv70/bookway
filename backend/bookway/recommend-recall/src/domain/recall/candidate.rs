use std::collections::HashSet;

use crate::api::pb;
use bookway_bbs_link_api::pb as bbs_link_pb;

pub(crate) fn candidate_from_content(
    content: bbs_link_pb::Content,
    source: &str,
) -> Option<pb::Candidate> {
    let post = content.post?;
    let freshness = post.freshness;
    let quality_score = content.quality_score;
    let (recall_score, reasons) = recall_details(source, quality_score, freshness);
    let content_id = if content.id.is_empty() {
        post.id.clone()
    } else {
        content.id
    };
    Some(pb::Candidate {
        content_id,
        post: Some(post),
        author_id: content.author_id,
        status: content.status,
        quality_score,
        freshness,
        recall_score,
        score: 0.0,
        source: format!("recall:{source}"),
        reasons,
        p_ctr: 0.0,
        p_cvr: 0.0,
        p_wegu: 0.0,
        // Recall sources carry no model features; recommend-rank fills the
        // snapshot during ranking.
        feature_snapshot: Default::default(),
    })
}

fn recall_details(source: &str, quality_score: f64, freshness: f64) -> (f64, Vec<String>) {
    match source {
        "following-fresh" => (freshness, vec!["来自关注时间流".to_string()]),
        "fresh" => (freshness, vec!["来自新内容召回".to_string()]),
        source if source.starts_with("interest:") => (
            quality_score * 0.8 + freshness * 0.2 + 0.25,
            vec![format!("{}", interest_reason(source))],
        ),
        _ => (quality_score, vec!["来自优质内容召回".to_string()]),
    }
}

/// The quality index returns items already ordered by static quality. If its
/// retrieval strength were that quality value, the coarse ranker would count
/// the same signal twice (quality 0.60 + recall 0.30). Instead the channel's
/// retrieval strength is the item's rank inside its own eligible batch,
/// normalized to (0, 1] with position 0 strongest. The semantic lane reuses
/// the same rule: its kNN similarity order is the relevance order, and the
/// raw score exposed by search is a lexical artifact, not the similarity.
pub(crate) fn assign_rank_retrieval_strength(batch: &mut [pb::Candidate]) {
    let denominator = batch.len().max(1) as f64;
    for (position, candidate) in batch.iter_mut().enumerate() {
        candidate.recall_score = 1.0 - position as f64 / denominator;
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

pub(crate) fn seen(values: &[String]) -> HashSet<String> {
    values.iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use bookway_bbs_link_api::pb::PostSummary;

    use super::recall_details;

    #[test]
    fn interest_source_boosts_matching_content_and_keeps_user_reason() {
        let (score, reasons) = recall_details("interest:travel", 0.6, 0.8);

        assert!((score - 0.89).abs() < f64::EPSILON);
        assert_eq!(reasons, ["符合你的旅行兴趣"]);
    }

    #[test]
    fn following_source_identifies_the_chronological_social_timeline() {
        let (score, reasons) = recall_details("following-fresh", 0.2, 0.8);

        assert!((score - 0.8).abs() < f64::EPSILON);
        assert_eq!(reasons, ["来自关注时间流"]);
    }

    #[test]
    fn quality_channel_retrieval_strength_is_batch_rank_not_duplicated_quality() {
        let mut batch = Vec::new();
        for id in ["first", "second", "third"] {
            batch.push(
                super::candidate_from_content(
                    bookway_bbs_link_api::pb::Content {
                        post: Some(PostSummary {
                            id: id.to_string(),
                            ..Default::default()
                        }),
                        quality_score: 0.9,
                        ..Default::default()
                    },
                    "quality",
                )
                .expect("candidate"),
            );
        }

        super::assign_rank_retrieval_strength(&mut batch);

        let strength = |id: &str| {
            batch
                .iter()
                .find(|candidate| candidate.content_id == id)
                .map(|candidate| candidate.recall_score)
                .expect("candidate stays in its batch")
        };
        assert!(strength("first") > strength("second"));
        assert!(strength("second") > strength("third"));
        assert!(
            strength("third") > 0.0,
            "retrieval strength stays positive so coarse ranking keeps the tail"
        );
    }
}
