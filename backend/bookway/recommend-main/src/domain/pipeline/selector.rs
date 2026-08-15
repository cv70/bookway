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
            // Apply strict local-window rules first, then progressively relax
            // them so a narrow candidate pool can still fill the response.
            let index = [
                RuleLevel::Strict,
                RuleLevel::Balanced,
                RuleLevel::AuthorOnly,
            ]
            .into_iter()
            .find_map(|level| {
                candidates
                    .iter()
                    .position(|candidate| passes(level, &selected, candidate))
            })
            .unwrap_or_default();
            let mut candidate = candidates.remove(index);
            if index > 0 {
                candidate
                    .reasons
                    .push("已做作者与主题多样性打散".to_string());
            }
            selected.push(candidate);
        }
        selected
    }
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
            hidden: false,
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

    #[test]
    fn uses_progressive_fallback_without_starving_narrow_pools() {
        let selected = DiversitySelector.select(
            vec![
                candidate("a-1", "author-a", GrowthDomainDto::Learning, 4.0),
                candidate("a-2", "author-a", GrowthDomainDto::Learning, 3.0),
                candidate("a-3", "author-a", GrowthDomainDto::Learning, 2.0),
            ],
            3,
        );

        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].post.id, "a-1");
    }
}
