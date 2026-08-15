mod candidate;

use std::collections::{BTreeMap, HashMap, HashSet};

use bookway_api::{GrowthDomainDto, RecommendationCandidateDto};
use futures::future::join_all;
use serde::{Deserialize, Serialize};

use crate::api::pb;
use crate::domain::Domain;

const CURSOR_PREFIX: &str = "v1:";
const MAX_CURSOR_BYTES: usize = 1_024;

impl Domain {
    pub(crate) async fn recall(&self, request: pb::RecallRequest) -> pb::RecallResponse {
        let limit = (request.limit as usize).clamp(1, self.max_candidates);
        let seen = candidate::seen(&request.seen);
        let sources = recall_sources(&request.interests);
        let cursor_states = decode_cursor(&request.cursor, &sources);
        let source_names = sources
            .iter()
            .map(|source| format!("recall:{}", source.name))
            .collect();
        let mut next_cursor_states = sources
            .iter()
            .filter(|source| source_is_exhausted(&cursor_states, source))
            .map(|source| (source.name.clone(), None))
            .collect::<BTreeMap<_, _>>();
        let jobs = sources
            .iter()
            .filter(|source| !source_is_exhausted(&cursor_states, source))
            .cloned()
            .map(|source| {
                let content = self.content.clone();
                let source_cursor = cursor_states.get(&source.name).cloned().flatten();
                async move {
                    let result = content
                        .list(
                            source.content_strategy,
                            source_cursor.clone(),
                            (limit * 2).min(self.max_candidates),
                            source.domain,
                        )
                        .await;
                    (source, source_cursor, result)
                }
            });
        let mut candidates = HashMap::new();
        let mut degraded = false;
        for (source, source_cursor, result) in join_all(jobs).await {
            match result {
                Ok(page) => {
                    next_cursor_states.insert(source.name.clone(), page.next_cursor);
                    for content in page.items {
                        merge_candidate(
                            &mut candidates,
                            candidate::candidate_from_content(content, &source.name),
                        );
                    }
                }
                Err(error) => {
                    degraded = true;
                    // Retain the source position when a continuation fails.
                    // Retrying the feed page can then resume this source instead
                    // of silently skipping its candidate window.
                    if let Some(cursor) = source_cursor {
                        next_cursor_states.insert(source.name.clone(), Some(cursor));
                    }
                    tracing::warn!(%error, source = source.name, "recall source degraded");
                }
            }
        }
        let mut candidates: Vec<_> = candidates
            .into_values()
            .filter(|candidate| !seen.contains(&candidate.content_id))
            .collect();
        candidates.sort_by(|left, right| {
            right
                .recall_score
                .total_cmp(&left.recall_score)
                .then_with(|| left.content_id.cmp(&right.content_id))
        });
        candidates.truncate(limit);
        let has_more = sources
            .iter()
            .any(|source| !source_is_exhausted(&next_cursor_states, source));
        pb::RecallResponse {
            candidates: candidates.into_iter().map(candidate::to_proto).collect(),
            next_cursor: if has_more {
                encode_cursor(&next_cursor_states)
            } else {
                String::new()
            },
            sources: source_names,
            degraded,
        }
    }
}

#[derive(Clone)]
struct RecallSource {
    name: String,
    content_strategy: &'static str,
    domain: Option<GrowthDomainDto>,
}

/// A source has reached the end only when the cursor explicitly records a
/// completed `null` value. A missing source is new or previously unavailable
/// and must be retried rather than treated as exhausted.
fn source_is_exhausted(states: &BTreeMap<String, Option<String>>, source: &RecallSource) -> bool {
    matches!(states.get(&source.name), Some(None))
}

#[derive(Default, Serialize, Deserialize)]
struct RecallCursor {
    sources: BTreeMap<String, Option<String>>,
}

fn decode_cursor(cursor: &str, sources: &[RecallSource]) -> BTreeMap<String, Option<String>> {
    if cursor.trim().is_empty() {
        return BTreeMap::new();
    }
    if cursor.len() > MAX_CURSOR_BYTES {
        tracing::warn!(
            cursor_len = cursor.len(),
            "recall cursor exceeds the size limit"
        );
        return BTreeMap::new();
    }
    if let Some(value) = cursor.strip_prefix(CURSOR_PREFIX) {
        return serde_json::from_str::<RecallCursor>(value)
            .map(|cursor| cursor.sources)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "recall cursor is invalid; restarting recall");
                BTreeMap::new()
            });
    }
    // Pre-v1 clients receive the same source offset for every source. This
    // keeps an in-flight pagination chain usable during a rolling deployment.
    sources
        .iter()
        .map(|source| (source.name.clone(), Some(cursor.to_string())))
        .collect()
}

fn encode_cursor(states: &BTreeMap<String, Option<String>>) -> String {
    let cursor = RecallCursor {
        sources: states.clone(),
    };
    serde_json::to_string(&cursor)
        .map(|value| format!("{CURSOR_PREFIX}{value}"))
        .unwrap_or_else(|error| {
            tracing::error!(%error, "failed to encode recall cursor");
            String::new()
        })
}

fn recall_sources(interests: &[String]) -> Vec<RecallSource> {
    let mut sources = vec![
        RecallSource {
            name: "quality".to_string(),
            content_strategy: "quality",
            domain: None,
        },
        RecallSource {
            name: "fresh".to_string(),
            content_strategy: "fresh",
            domain: None,
        },
    ];
    let mut domains = HashSet::new();
    for interest in interests
        .iter()
        .filter_map(|interest| parse_domain(interest))
    {
        if domains.insert(interest) {
            sources.push(RecallSource {
                name: format!("interest:{}", domain_name(interest)),
                content_strategy: "quality",
                domain: Some(interest),
            });
        }
    }
    sources
}

fn merge_candidate(
    candidates: &mut HashMap<String, RecommendationCandidateDto>,
    mut incoming: RecommendationCandidateDto,
) {
    let Some(existing) = candidates.get_mut(&incoming.content_id) else {
        candidates.insert(incoming.content_id.clone(), incoming);
        return;
    };

    if incoming.recall_score > existing.recall_score {
        append_reasons(&mut incoming.reasons, std::mem::take(&mut existing.reasons));
        *existing = incoming;
    } else {
        append_reasons(&mut existing.reasons, incoming.reasons);
    }
}

fn append_reasons(target: &mut Vec<String>, incoming: Vec<String>) {
    for reason in incoming {
        if !target.contains(&reason) {
            target.push(reason);
        }
    }
}

fn parse_domain(value: &str) -> Option<GrowthDomainDto> {
    match value.trim().to_ascii_lowercase().as_str() {
        "learning" => Some(GrowthDomainDto::Learning),
        "movement" => Some(GrowthDomainDto::Movement),
        "wellness" => Some(GrowthDomainDto::Wellness),
        "travel" => Some(GrowthDomainDto::Travel),
        "leisure" => Some(GrowthDomainDto::Leisure),
        _ => None,
    }
}

fn domain_name(domain: GrowthDomainDto) -> &'static str {
    match domain {
        GrowthDomainDto::Learning => "learning",
        GrowthDomainDto::Movement => "movement",
        GrowthDomainDto::Wellness => "wellness",
        GrowthDomainDto::Travel => "travel",
        GrowthDomainDto::Leisure => "leisure",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{decode_cursor, encode_cursor, parse_domain, recall_sources, source_is_exhausted};
    use bookway_api::GrowthDomainDto;

    #[test]
    fn adds_each_valid_interest_source_once() {
        let sources = recall_sources(&[
            "travel".to_string(),
            "Travel".to_string(),
            "unknown".to_string(),
        ]);

        assert_eq!(sources.len(), 3);
        assert_eq!(sources[2].domain, Some(GrowthDomainDto::Travel));
    }

    #[test]
    fn accepts_the_domain_format_used_by_recommend_main() {
        assert_eq!(parse_domain("learning"), Some(GrowthDomainDto::Learning));
        assert_eq!(parse_domain("Learning"), Some(GrowthDomainDto::Learning));
    }

    #[test]
    fn keeps_an_independent_position_for_each_recall_source() {
        let sources = recall_sources(&["learning".to_string(), "travel".to_string()]);
        let cursor = encode_cursor(&BTreeMap::from([
            ("quality".to_string(), Some("60".to_string())),
            ("fresh".to_string(), None),
            ("interest:learning".to_string(), Some("20".to_string())),
        ]));

        let states = decode_cursor(&cursor, &sources);
        assert_eq!(states["quality"].as_deref(), Some("60"));
        assert_eq!(states["interest:learning"].as_deref(), Some("20"));
        assert!(source_is_exhausted(&states, &sources[1]));
        assert!(!source_is_exhausted(&states, &sources[3]));
    }

    #[test]
    fn accepts_legacy_shared_cursors_during_rollout() {
        let sources = recall_sources(&["learning".to_string()]);
        let states = decode_cursor("80", &sources);

        assert!(
            states
                .values()
                .all(|cursor| cursor.as_deref() == Some("80"))
        );
    }
}
