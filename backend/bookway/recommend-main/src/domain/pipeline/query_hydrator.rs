use std::collections::HashSet;

use bookway_bbs_link_api::pb::GrowthDomain;

use super::{FeedQuery, QueryHydrator};
use crate::api::pb;

pub(crate) struct DefaultQueryHydrator;

impl QueryHydrator for DefaultQueryHydrator {
    fn hydrate(&self, request: pb::FeedRequest) -> FeedQuery {
        let interests = request
            .interests
            .into_iter()
            .filter_map(|domain| GrowthDomain::try_from(domain).ok())
            .collect::<HashSet<_>>();
        // An empty interest set stays empty. Substituting three domains here
        // meant every interest-free request was told it "符合你的学习兴趣",
        // opened recall lanes for domains the user never declared, and
        // systematically demoted the two domains not in the substitute list.
        // The downstream ranker has an honest "暂无明确兴趣" branch for this.

        FeedQuery {
            interests,
            seen: request
                .seen
                .into_iter()
                .filter(|value| !value.trim().is_empty())
                .collect(),
            // Same bounds as FeedService's clamp: one limit contract, so a
            // client asking for 50 is never silently cut to 20 while the ad
            // slot math planned for 50.
            limit: request.limit.unwrap_or(20).clamp(1, 100) as usize,
            // Anonymous requests stay anonymous. There is no "demo-user":
            // a fabricated id once shared one exposure ledger, one frequency
            // cap and one experiment bucket across every anonymous visitor.
            user_id: non_empty(request.user_id),
            session_id: non_empty(request.session_id),
            surface: normalize_surface(&request.surface),
            cursor: request.cursor,
        }
    }
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalize_surface(surface: &str) -> String {
    match surface.trim() {
        "following" => "following".to_string(),
        _ => "home".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{DefaultQueryHydrator, GrowthDomain, QueryHydrator};
    use crate::api::pb;

    #[test]
    fn an_empty_interest_set_is_never_substituted() {
        let query = DefaultQueryHydrator.hydrate(pb::FeedRequest {
            interests: Vec::new(),
            ..Default::default()
        });
        assert!(
            query.interests.is_empty(),
            "a user who declared no interest must not be given one"
        );
    }

    #[test]
    fn declared_interests_survive_and_unknown_domains_are_dropped() {
        let query = DefaultQueryHydrator.hydrate(pb::FeedRequest {
            interests: vec![GrowthDomain::Wellness as i32, 9_999],
            ..Default::default()
        });
        assert_eq!(
            query.interests,
            std::iter::once(GrowthDomain::Wellness).collect()
        );
    }
}
