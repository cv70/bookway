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

        while !candidates.is_empty() && selected.len() < limit {
            // Server-side exposure history is a preference, never a reason to
            // return an empty Feed. First take an unseen item that satisfies
            // local diversity; only then use an older impression to preserve
            // author/topic variety in a narrow candidate pool.
            let unseen = |candidate: &Candidate| !candidate.previously_served;
            let index = select_index(&candidates, &selected, unseen)
                .or_else(|| select_index(&candidates, &selected, |_| true))
                .or_else(|| candidates.iter().position(unseen))
                .unwrap_or_default();
            let mut candidate = candidates.remove(index);
            if index > 0 {
                candidate
                    .reasons
                    .push("已做作者与主题多样性打散".to_string());
            }
            if candidate.previously_served {
                candidate
                    .reasons
                    .push("已放宽历史曝光限制以避免信息流中断".to_string());
            }
            selected.push(candidate);
        }
        selected
    }
}

fn select_index(
    candidates: &[Candidate],
    selected: &[Candidate],
    predicate: impl Fn(&Candidate) -> bool,
) -> Option<usize> {
    [
        RuleLevel::Strict,
        RuleLevel::Balanced,
        RuleLevel::AuthorOnly,
    ]
    .into_iter()
    .find_map(|level| {
        candidates
            .iter()
            .position(|candidate| predicate(candidate) && passes(level, selected, candidate))
    })
}

#[derive(Clone, Copy)]
enum RuleLevel {
    Strict,
    Balanced,
    AuthorOnly,
}

fn passes(level: RuleLevel, selected: &[Candidate], next: &Candidate) -> bool {
    let author_window = match level {
        RuleLevel::Strict => 4,
        RuleLevel::Balanced => 3,
        RuleLevel::AuthorOnly => 2,
    };
    if selected
        .iter()
        .rev()
        .take(author_window)
        .any(|item| item.author_id == next.author_id)
    {
        return false;
    }
    if matches!(level, RuleLevel::AuthorOnly) {
        return true;
    }

    if matches!(level, RuleLevel::Strict)
        && selected
            .last()
            .is_some_and(|previous| previous.post.domain == next.post.domain)
    {
        return false;
    }

    let domain_limit = if matches!(level, RuleLevel::Strict) {
        2
    } else {
        3
    };
    if selected
        .iter()
        .rev()
        .take(4)
        .filter(|item| item.post.domain == next.post.domain)
        .count()
        >= domain_limit
    {
        return false;
    }
    if matches!(level, RuleLevel::Strict)
        && selected.last().is_some_and(|previous| {
            previous
                .post
                .tags
                .iter()
                .any(|tag| next.post.tags.contains(tag))
        })
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use bookway_bbs_link_api::pb::{ContentStatus, GrowthDomain, PostSummary};

    use super::DiversitySelector;
    use crate::domain::pipeline::{Candidate, CandidateSelector};

    fn candidate(
        id: &str,
        author_id: &str,
        domain: GrowthDomain,
        score: f64,
        previously_served: bool,
    ) -> Candidate {
        Candidate {
            post: PostSummary {
                id: id.to_string(),
                author_name: author_id.to_string(),
                author_avatar_url: String::new(),
                title: String::new(),
                summary: String::new(),
                domain: domain as i32,
                cover_url: String::new(),
                route_title: String::new(),
                route_duration: String::new(),
                join_count: 0,
                like_count: 0,
                freshness: 0.0,
                tags: Vec::new(),
                is_route: false,
                is_milestone: false,
                is_question: false,
            },
            author_id: author_id.to_string(),
            status: ContentStatus::Published as i32,
            quality_score: 0.0,
            score,
            source: String::new(),
            reasons: Vec::new(),
            followed_author: false,
            blocked_author: false,
            muted_author: false,
            liked: false,
            bookmarked: false,
            hidden: false,
            previously_served,
        }
    }

    #[test]
    fn interleaves_domains_before_using_the_next_highest_score() {
        let selected = DiversitySelector.select(
            vec![
                candidate("travel-1", "author-a", GrowthDomain::Travel, 10.0, false),
                candidate("travel-2", "author-b", GrowthDomain::Travel, 9.0, false),
                candidate("learning-1", "author-c", GrowthDomain::Learning, 8.0, false),
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

    #[test]
    fn uses_progressive_fallback_without_starving_narrow_pools() {
        let selected = DiversitySelector.select(
            vec![
                candidate("a-1", "author-a", GrowthDomain::Learning, 4.0, false),
                candidate("a-2", "author-a", GrowthDomain::Learning, 3.0, false),
                candidate("a-3", "author-a", GrowthDomain::Learning, 2.0, false),
            ],
            3,
        );

        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].post.id, "a-1");
    }

    #[test]
    fn prefers_an_unseen_candidate_over_a_higher_scored_repeat() {
        let selected = DiversitySelector.select(
            vec![
                candidate("seen", "author-a", GrowthDomain::Learning, 100.0, true),
                candidate("new", "author-b", GrowthDomain::Learning, 1.0, false),
            ],
            2,
        );

        assert_eq!(selected[0].post.id, "new");
        assert_eq!(selected[1].post.id, "seen");
        assert!(
            selected[1]
                .reasons
                .iter()
                .any(|reason| reason == "已放宽历史曝光限制以避免信息流中断")
        );
    }

    #[test]
    fn returns_a_repeat_when_history_is_the_only_available_pool() {
        let selected = DiversitySelector.select(
            vec![candidate(
                "seen-only",
                "author-a",
                GrowthDomain::Learning,
                1.0,
                true,
            )],
            1,
        );

        assert_eq!(selected.len(), 1);
        assert!(
            selected[0]
                .reasons
                .iter()
                .any(|reason| reason == "已放宽历史曝光限制以避免信息流中断")
        );
    }
}
