mod candidate;

use std::collections::{HashMap, HashSet};

use bookway_api::{GrowthDomainDto, RecommendationCandidateDto};
use futures::future::join_all;

use crate::api::pb;
use crate::domain::Domain;

impl Domain {
    pub(crate) async fn recall(&self, request: pb::RecallRequest) -> pb::RecallResponse {
        let limit = (request.limit as usize).clamp(1, self.max_candidates);
        let cursor = (!request.cursor.is_empty()).then_some(request.cursor);
        let seen = candidate::seen(&request.seen);
        let sources = recall_sources(&request.interests);
        let source_names = sources
            .iter()
            .map(|source| format!("recall:{}", source.name))
            .collect();
        let jobs = sources.iter().cloned().map(|source| {
            let content = self.content.clone();
            let cursor = cursor.clone();
            async move {
                (
                    source.clone(),
                    content
                        .list(
                            source.content_strategy,
                            cursor,
                            (limit * 2).min(self.max_candidates),
                            source.domain,
                        )
                        .await,
                )
            }
        });
        let mut candidates = HashMap::new();
        let mut next_cursor = None;
        let mut degraded = false;
        for (source, result) in join_all(jobs).await {
            match result {
                Ok(page) => {
                    if next_cursor.is_none() {
                        next_cursor = page.next_cursor;
                    }
                    for content in page.items {
                        merge_candidate(
                            &mut candidates,
                            candidate::candidate_from_content(content, &source.name),
                        );
                    }
                }
                Err(error) => {
                    degraded = true;
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
        pb::RecallResponse {
            candidates: candidates.into_iter().map(candidate::to_proto).collect(),
            next_cursor: next_cursor.unwrap_or_default(),
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
    use super::{parse_domain, recall_sources};
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
}
