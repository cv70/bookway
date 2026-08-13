use crate::api::pb;

pub(crate) fn stable_bucket(value: &str) -> u8 {
    value
        .bytes()
        .fold(0_u8, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte))
        % 10
}
pub(crate) fn rank(
    mut candidates: Vec<pb::Candidate>,
    history_signal: f64,
    bucket: u8,
) -> Vec<pb::Candidate> {
    for candidate in &mut candidates {
        candidate.score = 0.82 * candidate.quality_score
            + 0.10 * candidate.recall_score
            + 0.05 * candidate.freshness
            + 0.03 * history_signal;
        candidate
            .reasons
            .push(format!("recommend-rank bucket {bucket}"));
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.content_id.cmp(&right.content_id))
    });
    candidates
}
