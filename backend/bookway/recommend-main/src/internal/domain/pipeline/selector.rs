use super::{Candidate, CandidateSelector};

pub(crate) struct DiversitySelector;

impl CandidateSelector for DiversitySelector {
    fn select(&self, mut candidates: Vec<Candidate>, limit: usize) -> Vec<Candidate> {
        candidates.sort_by(|left, right| right.score.total_cmp(&left.score));
        let mut selected: Vec<Candidate> = Vec::with_capacity(limit.min(candidates.len()));

        while !candidates.is_empty() && selected.len() < limit {
            let last_domain = selected.last().map(|candidate| candidate.post.domain);
            let index = candidates
                .iter()
                .position(|candidate| Some(candidate.post.domain) != last_domain)
                .unwrap_or(0);
            selected.push(candidates.remove(index));
        }
        selected
    }
}
