use std::collections::HashSet;

use super::{FeedQuery, QueryHydrator};
use crate::api::{FeedQueryRequest, GrowthDomainDto};

pub(crate) struct DefaultQueryHydrator;

impl QueryHydrator for DefaultQueryHydrator {
    fn hydrate(&self, request: FeedQueryRequest) -> FeedQuery {
        let interests = request
            .interests
            .as_deref()
            .map(parse_domains)
            .filter(|domains| !domains.is_empty())
            .unwrap_or_else(|| {
                [
                    GrowthDomainDto::Learning,
                    GrowthDomainDto::Movement,
                    GrowthDomainDto::Travel,
                ]
                .into_iter()
                .collect()
            });
        let seen = request
            .seen
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();

        FeedQuery {
            interests,
            seen,
            limit: request.limit.unwrap_or(10).clamp(1, 20),
            user_id: request.user_id.unwrap_or_else(|| "demo-user".to_string()),
            session_id: request
                .session_id
                .unwrap_or_else(|| "anonymous-session".to_string()),
            surface: request.surface.unwrap_or_else(|| "home".to_string()),
            cursor: request.cursor,
        }
    }
}

fn parse_domains(value: &str) -> HashSet<GrowthDomainDto> {
    value
        .split(',')
        .filter_map(|domain| match domain.trim() {
            "learning" => Some(GrowthDomainDto::Learning),
            "movement" => Some(GrowthDomainDto::Movement),
            "wellness" => Some(GrowthDomainDto::Wellness),
            "travel" => Some(GrowthDomainDto::Travel),
            "leisure" => Some(GrowthDomainDto::Leisure),
            _ => None,
        })
        .collect()
}
