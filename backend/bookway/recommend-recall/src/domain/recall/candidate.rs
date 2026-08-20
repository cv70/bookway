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

/// A small, deterministic two-tower contract for the online recall stage.
/// The user tower is an interest vector and the item tower is a domain vector;
/// production model artifacts can replace the encoders without changing the
/// candidate protocol or cursor semantics.
pub(crate) fn apply_two_tower_score(candidate: &mut pb::Candidate, interests: &[i32]) {
    let user_tower = user_tower(interests);
    let item_tower = item_tower(candidate.post.as_ref());
    let similarity = user_tower
        .iter()
        .zip(item_tower)
        .map(|(user, item)| user * item)
        .sum::<f64>();
    candidate.recall_score = (0.75 * similarity
        + 0.15 * candidate.quality_score.clamp(0.0, 1.0)
        + 0.10 * candidate.freshness.clamp(0.0, 1.0))
    .clamp(0.0, 1.0);
    candidate.score = candidate.recall_score;
    candidate.reasons.insert(0, "双塔语义召回".to_string());
}

fn user_tower(interests: &[i32]) -> [f64; 5] {
    let mut vector = [0.2; 5];
    for interest in interests {
        if let Ok(index) = usize::try_from(*interest)
            && let Some(value) = vector.get_mut(index)
        {
            *value += 0.8;
        }
    }
    normalize(vector)
}

fn item_tower(post: Option<&bookway_bbs_link_api::pb::PostSummary>) -> [f64; 5] {
    let mut vector = [0.0; 5];
    if let Some(post) = post
        && let Ok(index) = usize::try_from(post.domain)
        && let Some(value) = vector.get_mut(index)
    {
        *value = 1.0;
    }
    normalize(vector)
}

fn normalize(mut vector: [f64; 5]) -> [f64; 5] {
    let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm > f64::EPSILON {
        vector.iter_mut().for_each(|value| *value /= norm);
    }
    vector
}

#[cfg(test)]
mod tests {
    use bookway_bbs_link_api::pb::{GrowthDomain, PostSummary};

    use super::{apply_two_tower_score, recall_details};

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
    fn two_tower_recall_prefers_the_user_interest_domain() {
        let mut matching = super::candidate_from_content(
            bookway_bbs_link_api::pb::Content {
                post: Some(PostSummary {
                    domain: GrowthDomain::Movement as i32,
                    ..Default::default()
                }),
                quality_score: 0.4,
                ..Default::default()
            },
            "two-tower",
        )
        .expect("candidate");
        let mut unrelated = matching.clone();
        unrelated
            .post
            .as_mut()
            .expect("candidate should retain its public post")
            .domain = GrowthDomain::Learning as i32;
        apply_two_tower_score(&mut matching, &[GrowthDomain::Movement as i32]);
        apply_two_tower_score(&mut unrelated, &[GrowthDomain::Movement as i32]);
        assert!(matching.recall_score > unrelated.recall_score);
        assert!(
            matching
                .reasons
                .iter()
                .any(|reason| reason == "双塔语义召回")
        );
    }
}
