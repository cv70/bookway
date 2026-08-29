mod candidate;
pub(crate) mod semantic;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use bookway_bbs_link_api::pb as bbs_link_pb;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::Domain;
use crate::{api::pb, conf::SourceBlend};

const CURSOR_V2_PREFIX: &str = "v2:";
const MAX_CURSOR_BYTES: usize = 1_024;
const MAX_FOLLOWING_AUTHORS: usize = 5_000;
const QUALITY_SOURCE_WEIGHT: usize = 4;
const FRESH_SOURCE_WEIGHT: usize = 2;
const SEMANTIC_SOURCE_WEIGHT: usize = 2;

impl Domain {
    pub(crate) async fn recall(&self, request: pb::RecallRequest) -> pb::RecallResponse {
        if request.following_only {
            return self.recall_following(request).await;
        }
        self.recall_general(request).await
    }

    async fn recall_general(&self, request: pb::RecallRequest) -> pb::RecallResponse {
        let limit = (request.limit as usize).clamp(1, self.max_candidates);
        let seen = candidate::seen(&request.seen);
        let sources = recall_sources(&request.interests, self.semantic.is_some());
        let cursor_states = decode_cursor(&request.cursor);
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
                let mut content_client = self.content_client.clone();
                let semantic_clients = self.semantic.clone();
                let interests = request.interests.clone();
                let user_id = request.user_id.clone();
                let source_cursor = cursor_states.get(&source.name).cloned().flatten();
                let window = (limit * 2).min(self.max_candidates);
                async move {
                    let result: Result<SourcePage, String> = async {
                        match source.kind {
                            SourceKind::List { strategy, domain } => {
                                let list_request = bbs_link_pb::ListRequest {
                                    cursor: source_cursor.clone(),
                                    limit: Some(window as u32),
                                    status: Some(bbs_link_pb::ContentStatus::Published as i32),
                                    strategy: Some(strategy.to_string()),
                                    ids: None,
                                    author_id: None,
                                    content_type: None,
                                    domain: domain.map(|domain| domain as i32),
                                    author_ids: Vec::new(),
                                };
                                let page = async {
                                    let request =
                                        bookway_runtime::grpc_service_request(list_request)
                                            .map_err(|error| error.to_string())?;
                                    content_client
                                        .list(request)
                                        .await
                                        .map(|response| response.into_inner())
                                        .map_err(|error| error.to_string())
                                }
                                .await?;
                                Ok(SourcePage {
                                    next_cursor: page.next_cursor,
                                    candidates: page
                                        .items
                                        .into_iter()
                                        .filter_map(|content| {
                                            candidate::candidate_from_content(
                                                content,
                                                &source.name,
                                            )
                                        })
                                        .collect(),
                                })
                            }
                            SourceKind::Semantic => {
                                // The cursor key is enough to prove the source
                                // is registered; the callers above only build
                                // this branch when the semantic clients exist.
                                let clients = semantic_clients
                                    .expect("registered semantic source has clients");
                                let mut catalog = clients.catalog;
                                let mut search_client = clients.search;
                                let candidates = semantic::recall_semantic_page(
                                    &mut catalog,
                                    &mut search_client,
                                    &mut content_client,
                                    &user_id,
                                    &interests,
                                    window,
                                )
                                .await?;
                                // SearchSemantic has no continuation token: the
                                // lane is one window per feed, exhausted until a
                                // later feed restarts recall.
                                Ok(SourcePage {
                                    next_cursor: None,
                                    candidates,
                                })
                            }
                        }
                    }
                    .await;
                    (source, source_cursor, result)
                }
            });
        let mut candidates = HashMap::new();
        let mut source_batches = BTreeMap::new();
        let mut degraded = false;
        for (source, source_cursor, result) in join_all(jobs).await {
            match result {
                Ok(page) => {
                    next_cursor_states.insert(source.name.clone(), page.next_cursor);
                    let fetched = page.candidates.len();
                    let mut batch: Vec<pb::Candidate> = page
                        .candidates
                        .into_iter()
                        .filter(|candidate| !seen.contains(&candidate.content_id))
                        .collect();
                    if source.name == "quality" || source.name == "semantic" {
                        candidate::assign_rank_retrieval_strength(&mut batch);
                    }
                    let mut sourced_batch = batch
                        .iter()
                        .map(|candidate| SourcedCandidate {
                            content_id: candidate.content_id.clone(),
                            recall_score: candidate.recall_score,
                        })
                        .collect::<Vec<_>>();
                    sort_and_deduplicate_batch(&mut sourced_batch);
                    for candidate in batch {
                        merge_candidate(&mut candidates, candidate);
                    }
                    tracing::debug!(
                        source = %source.name,
                        fetched,
                        eligible = sourced_batch.len(),
                        exhausted = source_is_exhausted(&next_cursor_states, &source),
                        "recall source completed"
                    );
                    source_batches.insert(source.name.clone(), sourced_batch);
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
        candidates.retain(|content_id, _| !seen.contains(content_id));
        let candidates = match self.config.source_blend {
            SourceBlend::BalancedV1 => {
                select_source_mixed_candidates(&sources, candidates, &source_batches, limit)
            }
            SourceBlend::ScoreV1 => select_score_sorted_candidates(candidates, limit),
        };
        let has_more = sources
            .iter()
            .any(|source| !source_is_exhausted(&next_cursor_states, source));
        pb::RecallResponse {
            candidates,
            next_cursor: if has_more {
                encode_cursor(&next_cursor_states)
            } else {
                String::new()
            },
            sources: source_names,
            degraded,
            blend_version: self.config.source_blend.version().to_string(),
        }
    }

    async fn recall_following(&self, request: pb::RecallRequest) -> pb::RecallResponse {
        let source = following_recall_source();
        let source_names = vec![format!("recall:{}", source.name)];
        let following_author_ids =
            match normalize_following_author_ids(request.following_author_ids) {
                Ok(ids) => ids,
                Err(error) => {
                    tracing::warn!(%error, "following recall author filter is invalid");
                    return pb::RecallResponse {
                        candidates: Vec::new(),
                        next_cursor: String::new(),
                        sources: source_names,
                        degraded: true,
                        blend_version: "following-chronological-v1".to_string(),
                    };
                }
            };
        if following_author_ids.is_empty() {
            return pb::RecallResponse {
                candidates: Vec::new(),
                next_cursor: String::new(),
                sources: source_names,
                degraded: false,
                blend_version: "following-chronological-v1".to_string(),
            };
        }

        let limit = (request.limit as usize).clamp(1, self.max_candidates);
        let seen = candidate::seen(&request.seen);
        let author_set_fingerprint = following_author_set_fingerprint(&following_author_ids);
        let (cursor_states, cursor_reset) =
            decode_following_cursor(&request.cursor, &author_set_fingerprint);
        if cursor_reset {
            tracing::info!(
                user_id = %request.user_id,
                "following cursor author set changed; restarting the timeline from its current snapshot"
            );
        }
        if source_is_exhausted(&cursor_states, &source) {
            return pb::RecallResponse {
                candidates: Vec::new(),
                next_cursor: String::new(),
                sources: source_names,
                degraded: false,
                blend_version: "following-chronological-v1".to_string(),
            };
        }
        let source_cursor = cursor_states.get(&source.name).cloned().flatten();
        let mut client = self.content_client.clone();
        let request = match bookway_runtime::grpc_service_request(following_list_request(
            source_cursor.clone(),
            limit,
            following_author_ids,
        )) {
            Ok(request) => request,
            Err(error) => {
                tracing::warn!(%error, "following recall request could not be authenticated");
                return pb::RecallResponse {
                    candidates: Vec::new(),
                    next_cursor: String::new(),
                    sources: source_names,
                    degraded: true,
                    blend_version: "following-chronological-v1".to_string(),
                };
            }
        };
        let page = client.list(request).await;
        match page {
            Ok(page) => {
                let page = page.into_inner();
                let mut next_cursor_states = BTreeMap::new();
                next_cursor_states.insert(source.name.clone(), page.next_cursor);
                let candidates = page
                    .items
                    .into_iter()
                    .filter_map(|content| candidate::candidate_from_content(content, &source.name))
                    .filter(|candidate| !seen.contains(&candidate.content_id))
                    .collect::<Vec<_>>();
                let has_more = !source_is_exhausted(&next_cursor_states, &source);
                pb::RecallResponse {
                    candidates,
                    next_cursor: if has_more {
                        encode_following_cursor(&next_cursor_states, &author_set_fingerprint)
                    } else {
                        String::new()
                    },
                    sources: source_names,
                    degraded: false,
                    blend_version: "following-chronological-v1".to_string(),
                }
            }
            Err(error) => {
                tracing::warn!(%error, source = source.name, "following recall source degraded");
                let mut next_cursor_states = BTreeMap::new();
                if let Some(cursor) = source_cursor {
                    next_cursor_states.insert(source.name.clone(), Some(cursor));
                }
                pb::RecallResponse {
                    candidates: Vec::new(),
                    next_cursor: if next_cursor_states.is_empty() {
                        String::new()
                    } else {
                        encode_following_cursor(&next_cursor_states, &author_set_fingerprint)
                    },
                    sources: source_names,
                    degraded: true,
                    blend_version: "following-chronological-v1".to_string(),
                }
            }
        }
    }
}

fn following_list_request(
    cursor: Option<String>,
    limit: usize,
    following_author_ids: Vec<String>,
) -> bbs_link_pb::ListRequest {
    bbs_link_pb::ListRequest {
        cursor,
        // Never advance past candidates that this chronological surface
        // has not returned to its caller yet.
        limit: Some(limit as u32),
        status: Some(bbs_link_pb::ContentStatus::Published as i32),
        strategy: following_recall_source().kind.list_strategy().map(str::to_string),
        ids: None,
        author_id: None,
        content_type: None,
        domain: None,
        author_ids: following_author_ids,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceKind {
    /// A paged BBS Link listing: a curated strategy index, optionally scoped
    /// to one growth domain.
    List {
        strategy: &'static str,
        domain: Option<bbs_link_pb::GrowthDomain>,
    },
    /// One embedding-backed nearest-document window from BBS Search's
    /// `SearchSemantic`, hydrated against BBS Link's public projection.
    Semantic,
}

impl SourceKind {
    fn list_strategy(self) -> Option<&'static str> {
        match self {
            Self::List { strategy, .. } => Some(strategy),
            Self::Semantic => None,
        }
    }
}

#[derive(Clone)]
struct RecallSource {
    name: String,
    kind: SourceKind,
}

#[derive(Clone, Debug, PartialEq)]
struct SourcedCandidate {
    content_id: String,
    recall_score: f64,
}

/// One recall window as produced by a source: its mapped candidates plus the
/// continuation token for the next window (`None` once the source is done).
struct SourcePage {
    candidates: Vec<pb::Candidate>,
    next_cursor: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    following_author_set_fingerprint: Option<String>,
}

fn decode_cursor(cursor: &str) -> BTreeMap<String, Option<String>> {
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
    if let Some(value) = cursor.strip_prefix(CURSOR_V2_PREFIX) {
        return serde_json::from_str::<RecallCursor>(value)
            .map(|cursor| cursor.sources)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "recall cursor is invalid; restarting recall");
                BTreeMap::new()
            });
    }
    tracing::warn!("recall cursor uses an unsupported version; restarting recall");
    BTreeMap::new()
}

fn encode_cursor(states: &BTreeMap<String, Option<String>>) -> String {
    let cursor = RecallCursor {
        sources: states.clone(),
        following_author_set_fingerprint: None,
    };
    encode_cursor_value(cursor)
}

fn encode_following_cursor(
    states: &BTreeMap<String, Option<String>>,
    author_set_fingerprint: &str,
) -> String {
    let cursor = RecallCursor {
        sources: states.clone(),
        following_author_set_fingerprint: Some(author_set_fingerprint.to_string()),
    };
    encode_cursor_value(cursor)
}

fn encode_cursor_value(cursor: RecallCursor) -> String {
    serde_json::to_string(&cursor)
        .map(|value| format!("{CURSOR_V2_PREFIX}{value}"))
        .unwrap_or_else(|error| {
            tracing::error!(%error, "failed to encode recall cursor");
            String::new()
        })
}

/// A following cursor is only valid for the exact normalized author set that
/// generated it. If that set has changed, restarting at offset zero avoids
/// applying an old offset to a different timeline window.
fn decode_following_cursor(
    cursor: &str,
    author_set_fingerprint: &str,
) -> (BTreeMap<String, Option<String>>, bool) {
    if cursor.trim().is_empty() {
        return (BTreeMap::new(), false);
    }
    if cursor.len() > MAX_CURSOR_BYTES {
        tracing::warn!(
            cursor_len = cursor.len(),
            "following recall cursor exceeds the size limit; restarting timeline"
        );
        return (BTreeMap::new(), true);
    }
    let Some(value) = cursor.strip_prefix(CURSOR_V2_PREFIX) else {
        // Any cursor without the current version and author-set binding cannot
        // safely continue a Following timeline.
        return (BTreeMap::new(), true);
    };
    match serde_json::from_str::<RecallCursor>(value) {
        Ok(cursor)
            if cursor.following_author_set_fingerprint.as_deref()
                == Some(author_set_fingerprint) =>
        {
            (cursor.sources, false)
        }
        Ok(_) => (BTreeMap::new(), true),
        Err(error) => {
            tracing::warn!(%error, "following recall cursor is invalid; restarting timeline");
            (BTreeMap::new(), true)
        }
    }
}

fn recall_sources(interests: &[i32], semantic_enabled: bool) -> Vec<RecallSource> {
    let mut sources = vec![
        RecallSource {
            name: "quality".to_string(),
            kind: SourceKind::List {
                strategy: "quality",
                domain: None,
            },
        },
        RecallSource {
            name: "fresh".to_string(),
            kind: SourceKind::List {
                strategy: "fresh",
                domain: None,
            },
        },
    ];
    let domains = interests
        .iter()
        .filter_map(|interest| bbs_link_pb::GrowthDomain::try_from(*interest).ok())
        .collect::<BTreeSet<_>>();
    for interest in domains {
        sources.push(RecallSource {
            name: format!("interest:{}", domain_name(interest)),
            kind: SourceKind::List {
                strategy: "quality",
                domain: Some(interest),
            },
        });
    }
    if semantic_enabled {
        sources.push(RecallSource {
            name: "semantic".to_string(),
            kind: SourceKind::Semantic,
        });
    }
    sources
}

fn following_recall_source() -> RecallSource {
    RecallSource {
        name: "following-fresh".to_string(),
        kind: SourceKind::List {
            strategy: "fresh",
            domain: None,
        },
    }
}

fn normalize_following_author_ids(values: Vec<String>) -> Result<Vec<String>, String> {
    if values.len() > MAX_FOLLOWING_AUTHORS {
        return Err(format!(
            "following author count exceeds the {MAX_FOLLOWING_AUTHORS} recall limit"
        ));
    }
    let mut ids = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    if ids
        .iter()
        .any(|value| value.is_empty() || value.chars().count() > 160)
    {
        return Err("following author ID is invalid".to_string());
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn following_author_set_fingerprint(author_ids: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"bookway.following-author-set.v1\\0");
    for author_id in author_ids {
        let length = u32::try_from(author_id.len()).unwrap_or(u32::MAX);
        hasher.update(length.to_be_bytes());
        hasher.update(author_id.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn merge_candidate(candidates: &mut HashMap<String, pb::Candidate>, mut incoming: pb::Candidate) {
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

/// Select a bounded, deterministic blend before handing candidates to
/// Recommend Main's ranker. Every available exploration source gets one
/// placement first; weighted rounds then keep quality as the dominant source.
/// Any remaining capacity is filled by the global recall score so sparse or
/// failed sources can never reduce a healthy page to an underfilled response.
fn select_source_mixed_candidates(
    sources: &[RecallSource],
    mut candidates: HashMap<String, pb::Candidate>,
    source_batches: &BTreeMap<String, Vec<SourcedCandidate>>,
    limit: usize,
) -> Vec<pb::Candidate> {
    let mut selected = Vec::with_capacity(limit);
    let mut source_offsets = BTreeMap::new();

    // Reserve the first available placement for fresh, semantic and interest
    // recall so a high-scoring quality index cannot eliminate all exploration.
    for source in sources
        .iter()
        .filter(|source| is_exploration_source(source))
    {
        if selected.len() == limit {
            return selected;
        }
        take_next_source_candidate(
            source,
            source_batches,
            &mut source_offsets,
            &mut candidates,
            &mut selected,
        );
    }

    while selected.len() < limit {
        let mut selected_in_round = false;
        for source in sources {
            for _ in 0..source_mix_weight(source) {
                if selected.len() == limit {
                    return selected;
                }
                selected_in_round |= take_next_source_candidate(
                    source,
                    source_batches,
                    &mut source_offsets,
                    &mut candidates,
                    &mut selected,
                );
            }
        }
        if !selected_in_round {
            break;
        }
    }

    let mut globally_scored = candidates.into_values().collect::<Vec<_>>();
    sort_candidates_by_recall_score(&mut globally_scored);
    selected.extend(
        globally_scored
            .into_iter()
            .take(limit.saturating_sub(selected.len())),
    );
    selected
}

fn select_score_sorted_candidates(
    candidates: HashMap<String, pb::Candidate>,
    limit: usize,
) -> Vec<pb::Candidate> {
    let mut candidates = candidates.into_values().collect::<Vec<_>>();
    sort_candidates_by_recall_score(&mut candidates);
    candidates.truncate(limit);
    candidates
}

fn take_next_source_candidate(
    source: &RecallSource,
    source_batches: &BTreeMap<String, Vec<SourcedCandidate>>,
    source_offsets: &mut BTreeMap<String, usize>,
    candidates: &mut HashMap<String, pb::Candidate>,
    selected: &mut Vec<pb::Candidate>,
) -> bool {
    let Some(batch) = source_batches.get(&source.name) else {
        return false;
    };
    let offset = source_offsets.entry(source.name.clone()).or_default();
    while let Some(sourced) = batch.get(*offset) {
        *offset += 1;
        if let Some(candidate) = candidates.remove(&sourced.content_id) {
            selected.push(candidate);
            return true;
        }
    }
    false
}

fn is_exploration_source(source: &RecallSource) -> bool {
    source.name == "fresh"
        || source.name == "semantic"
        || source.name.starts_with("interest:")
}

fn source_mix_weight(source: &RecallSource) -> usize {
    match source.name.as_str() {
        "quality" => QUALITY_SOURCE_WEIGHT,
        "semantic" => SEMANTIC_SOURCE_WEIGHT,
        "fresh" => FRESH_SOURCE_WEIGHT,
        _ if source.name.starts_with("interest:") => 1,
        _ => 1,
    }
}

fn sort_and_deduplicate_batch(batch: &mut Vec<SourcedCandidate>) {
    batch.sort_by(|left, right| {
        right
            .recall_score
            .total_cmp(&left.recall_score)
            .then_with(|| left.content_id.cmp(&right.content_id))
    });
    let mut seen = HashSet::new();
    batch.retain(|candidate| seen.insert(candidate.content_id.clone()));
}

fn sort_candidates_by_recall_score(candidates: &mut [pb::Candidate]) {
    candidates.sort_by(|left, right| {
        right
            .recall_score
            .total_cmp(&left.recall_score)
            .then_with(|| left.content_id.cmp(&right.content_id))
    });
}

fn append_reasons(target: &mut Vec<String>, incoming: Vec<String>) {
    for reason in incoming {
        if !target.contains(&reason) {
            target.push(reason);
        }
    }
}

fn domain_name(domain: bbs_link_pb::GrowthDomain) -> &'static str {
    match domain {
        bbs_link_pb::GrowthDomain::Learning => "learning",
        bbs_link_pb::GrowthDomain::Movement => "movement",
        bbs_link_pb::GrowthDomain::Wellness => "wellness",
        bbs_link_pb::GrowthDomain::Travel => "travel",
        bbs_link_pb::GrowthDomain::Leisure => "leisure",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use bookway_bbs_link_api::pb::{
        self as bbs_link_list,
        Content, ContentPage, GrowthDomain, PostSummary, PublicContentSummaries,
        PublicContentSummary, PublicContentSummariesRequest,
        bbs_link_client::BbsLinkClient,
        bbs_link_server::{BbsLink, BbsLinkServer},
    };
    use bookway_bbs_search_api::pb::{
        self as search_stub,
        SearchResponse, SearchResult, SearchResultType, SearchSemanticRequest,
        bbs_search_client::BbsSearchClient,
        bbs_search_server::{BbsSearch, BbsSearchServer},
    };
    use bookway_knowledge_catalog_api::pb::{
        self as catalog_search,
        EmbedTextsRequest, EmbedTextsResponse, TextEmbedding,
        knowledge_catalog_client::KnowledgeCatalogClient,
        knowledge_catalog_server::{KnowledgeCatalog, KnowledgeCatalogServer},
    };
    use tonic::transport::Endpoint;
    use tonic::{Request, Response, Status};

    use super::{
        Domain, SourcedCandidate, SourceKind, decode_cursor, decode_following_cursor,
        encode_cursor, encode_following_cursor, following_author_set_fingerprint,
        following_list_request, following_recall_source, merge_candidate,
        normalize_following_author_ids, recall_sources, select_source_mixed_candidates,
        sort_and_deduplicate_batch, source_is_exhausted,
    };
    use crate::{
        api::pb,
        conf::{Config, SourceBlend},
        domain::recall::semantic::SemanticRecallClients,
    };

    fn recall_domain(bbs_link_url: &str, semantic: Option<SemanticRecallClients>) -> Domain {
        Domain {
            config: Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid address"),
                bbs_link_url: bbs_link_url.to_string(),
                max_candidates: 20,
                source_blend: SourceBlend::BalancedV1,
                semantic: None,
            },
            content_client: BbsLinkClient::new(
                Endpoint::from_shared(bbs_link_url.to_string())
                    .expect("valid BBS Link URL")
                    .connect_lazy(),
            ),
            semantic,
            max_candidates: 20,
        }
    }

    fn lazy_client<C>(url: &str, new_client: fn(tonic::transport::Channel) -> C) -> C {
        new_client(
            Endpoint::from_shared(url.to_string())
                .expect("valid test endpoint")
                .connect_lazy(),
        )
    }

    fn recall_candidate(
        content_id: &str,
        source: &str,
        recall_score: f64,
        reasons: &[&str],
    ) -> pb::Candidate {
        pb::Candidate {
            content_id: content_id.to_string(),
            recall_score,
            source: source.to_string(),
            reasons: reasons.iter().map(|reason| (*reason).to_string()).collect(),
            ..Default::default()
        }
    }

    fn source_batch(entries: &[(&str, f64)]) -> Vec<SourcedCandidate> {
        let mut batch = entries
            .iter()
            .map(|(content_id, recall_score)| SourcedCandidate {
                content_id: (*content_id).to_string(),
                recall_score: *recall_score,
            })
            .collect::<Vec<_>>();
        sort_and_deduplicate_batch(&mut batch);
        batch
    }

    #[test]
    fn adds_each_valid_interest_source_once() {
        let sources =
            recall_sources(&[GrowthDomain::Travel as i32, GrowthDomain::Travel as i32, 99], false);

        assert_eq!(sources.len(), 3);
        assert_eq!(sources[2].name, "interest:travel");
        assert_eq!(
            sources[2].kind,
            SourceKind::List {
                strategy: "quality",
                domain: Some(GrowthDomain::Travel),
            }
        );
    }

    #[test]
    fn semantic_source_is_registered_only_when_configured() {
        let without = recall_sources(&[GrowthDomain::Travel as i32], false);
        assert!(without.iter().all(|source| source.name != "semantic"));

        let with = recall_sources(&[], true);
        let semantic = with
            .iter()
            .find(|source| source.name == "semantic")
            .expect("semantic source registered");
        assert_eq!(semantic.kind, SourceKind::Semantic);
    }

    #[test]
    fn source_mix_reserves_fresh_and_interest_candidates_before_quality_fill() {
        let sources = recall_sources(&[GrowthDomain::Travel as i32], false);
        let candidates = HashMap::from([
            (
                "quality-1".to_string(),
                recall_candidate("quality-1", "recall:quality", 1.0, &["优质"]),
            ),
            (
                "quality-2".to_string(),
                recall_candidate("quality-2", "recall:quality", 0.99, &["优质"]),
            ),
            (
                "quality-3".to_string(),
                recall_candidate("quality-3", "recall:quality", 0.98, &["优质"]),
            ),
            (
                "quality-4".to_string(),
                recall_candidate("quality-4", "recall:quality", 0.97, &["优质"]),
            ),
            (
                "fresh-1".to_string(),
                recall_candidate("fresh-1", "recall:fresh", 0.1, &["新鲜"]),
            ),
            (
                "interest-1".to_string(),
                recall_candidate("interest-1", "recall:interest:travel", 0.1, &["兴趣"]),
            ),
        ]);
        let source_batches = BTreeMap::from([
            (
                "quality".to_string(),
                source_batch(&[
                    ("quality-1", 1.0),
                    ("quality-2", 0.99),
                    ("quality-3", 0.98),
                    ("quality-4", 0.97),
                ]),
            ),
            ("fresh".to_string(), source_batch(&[("fresh-1", 0.1)])),
            (
                "interest:travel".to_string(),
                source_batch(&[("interest-1", 0.1)]),
            ),
        ]);

        let selected = select_source_mixed_candidates(&sources, candidates, &source_batches, 6);
        let selected_ids = selected
            .into_iter()
            .map(|candidate| candidate.content_id)
            .collect::<Vec<_>>();

        assert_eq!(
            selected_ids,
            [
                "fresh-1",
                "interest-1",
                "quality-1",
                "quality-2",
                "quality-3",
                "quality-4",
            ]
        );
    }

    #[test]
    fn source_mix_reserves_a_semantic_exploration_placement() {
        let sources = recall_sources(&[], true);
        let candidates = HashMap::from([
            (
                "quality-1".to_string(),
                recall_candidate("quality-1", "recall:quality", 1.0, &["优质"]),
            ),
            (
                "quality-2".to_string(),
                recall_candidate("quality-2", "recall:quality", 0.9, &["优质"]),
            ),
            (
                "semantic-1".to_string(),
                recall_candidate("semantic-1", "recall:semantic", 0.5, &["符合你的兴趣语义"]),
            ),
        ]);
        let source_batches = BTreeMap::from([
            (
                "quality".to_string(),
                source_batch(&[("quality-1", 1.0), ("quality-2", 0.9)]),
            ),
            (
                "semantic".to_string(),
                source_batch(&[("semantic-1", 0.5)]),
            ),
        ]);

        let selected = select_source_mixed_candidates(&sources, candidates, &source_batches, 2);
        let selected_ids = selected
            .into_iter()
            .map(|candidate| candidate.content_id)
            .collect::<Vec<_>>();

        // The semantic lane's reserved placement survives a dominant quality
        // index, then quality fills the remaining capacity.
        assert_eq!(selected_ids, ["semantic-1", "quality-1"]);
    }

    #[test]
    fn source_mix_deduplicates_candidates_and_preserves_all_source_reasons() {
        let sources = recall_sources(&[GrowthDomain::Travel as i32], false);
        let mut candidates = HashMap::new();
        merge_candidate(
            &mut candidates,
            recall_candidate("shared", "recall:fresh", 0.3, &["来自新内容召回"]),
        );
        merge_candidate(
            &mut candidates,
            recall_candidate(
                "shared",
                "recall:interest:travel",
                0.9,
                &["符合你的旅行兴趣"],
            ),
        );
        merge_candidate(
            &mut candidates,
            recall_candidate(
                "travel-only",
                "recall:interest:travel",
                0.8,
                &["符合你的旅行兴趣"],
            ),
        );
        let source_batches = BTreeMap::from([
            ("fresh".to_string(), source_batch(&[("shared", 0.3)])),
            (
                "interest:travel".to_string(),
                source_batch(&[("shared", 0.9), ("travel-only", 0.8)]),
            ),
        ]);

        let selected = select_source_mixed_candidates(&sources, candidates, &source_batches, 2);

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].content_id, "shared");
        assert_eq!(selected[1].content_id, "travel-only");
        assert_eq!(selected[0].reasons, ["符合你的旅行兴趣", "来自新内容召回"]);
    }

    #[test]
    fn source_batch_keeps_only_the_highest_scored_copy_of_a_content_id() {
        let batch = source_batch(&[("shared", 0.9), ("other", 0.8), ("shared", 0.1)]);

        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].content_id, "shared");
        assert_eq!(batch[0].recall_score, 0.9);
        assert_eq!(batch[1].content_id, "other");
    }

    #[test]
    fn source_mix_fills_from_healthy_quality_when_exploration_sources_are_sparse() {
        let sources = recall_sources(&[GrowthDomain::Travel as i32], false);
        let candidates = HashMap::from([
            (
                "quality-1".to_string(),
                recall_candidate("quality-1", "recall:quality", 0.9, &["优质"]),
            ),
            (
                "quality-2".to_string(),
                recall_candidate("quality-2", "recall:quality", 0.8, &["优质"]),
            ),
            (
                "quality-3".to_string(),
                recall_candidate("quality-3", "recall:quality", 0.7, &["优质"]),
            ),
        ]);
        let source_batches = BTreeMap::from([(
            "quality".to_string(),
            source_batch(&[("quality-1", 0.9), ("quality-2", 0.8), ("quality-3", 0.7)]),
        )]);

        let selected = select_source_mixed_candidates(&sources, candidates, &source_batches, 3);

        assert_eq!(selected.len(), 3);
        assert!(
            selected
                .iter()
                .all(|candidate| candidate.source == "recall:quality")
        );
    }

    #[test]
    fn score_v1_is_a_deterministic_safe_rollback_to_global_recall_order() {
        let candidates = HashMap::from([
            (
                "lower".to_string(),
                recall_candidate("lower", "recall:fresh", 0.2, &["新鲜"]),
            ),
            (
                "higher".to_string(),
                recall_candidate("higher", "recall:quality", 0.9, &["优质"]),
            ),
        ]);

        let selected = super::select_score_sorted_candidates(candidates, 1);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].content_id, "higher");
    }

    #[test]
    fn keeps_an_independent_position_for_each_recall_source() {
        let sources = recall_sources(
            &[GrowthDomain::Learning as i32, GrowthDomain::Travel as i32],
            true,
        );
        let by_name = |name: &str| {
            sources
                .iter()
                .find(|source| source.name == name)
                .cloned()
                .expect("registered source")
        };
        let cursor = encode_cursor(&BTreeMap::from([
            ("quality".to_string(), Some("60".to_string())),
            ("semantic".to_string(), None),
            ("fresh".to_string(), None),
            ("interest:learning".to_string(), Some("20".to_string())),
        ]));

        let states = decode_cursor(&cursor);
        assert_eq!(states["quality"].as_deref(), Some("60"));
        assert_eq!(states["interest:learning"].as_deref(), Some("20"));
        assert!(source_is_exhausted(&states, &by_name("fresh")));
        assert!(source_is_exhausted(&states, &by_name("semantic")));
        assert!(!source_is_exhausted(&states, &by_name("interest:travel")));
    }

    #[test]
    fn unsupported_cursor_versions_restart_from_the_current_recall_contract() {
        assert!(decode_cursor("80").is_empty());
        assert!(decode_cursor("v1:{\"sources\":{}}").is_empty());
    }

    #[test]
    fn following_recall_uses_one_fresh_source_and_normalizes_author_ids() {
        let source = following_recall_source();

        assert_eq!(source.name, "following-fresh");
        assert_eq!(
            source.kind,
            SourceKind::List {
                strategy: "fresh",
                domain: None,
            }
        );
        assert_eq!(
            normalize_following_author_ids(vec![
                " author-b ".to_string(),
                "author-a".to_string(),
                "author-b".to_string(),
            ])
            .expect("valid following authors"),
            vec!["author-a", "author-b"]
        );
        assert!(normalize_following_author_ids(vec![" ".to_string()]).is_err());
        assert!(
            normalize_following_author_ids(
                (0..=super::MAX_FOLLOWING_AUTHORS)
                    .map(|index| format!("author-{index}"))
                    .collect(),
            )
            .is_err()
        );
    }

    #[test]
    fn following_cursor_restarts_when_the_author_set_changes() {
        let current_authors = normalize_following_author_ids(vec![
            "author-b".to_string(),
            "author-a".to_string(),
            "author-b".to_string(),
        ])
        .expect("valid following authors");
        let current_fingerprint = following_author_set_fingerprint(&current_authors);
        let cursor = encode_following_cursor(
            &BTreeMap::from([("following-fresh".to_string(), Some("20".to_string()))]),
            &current_fingerprint,
        );

        let (states, reset) = decode_following_cursor(&cursor, &current_fingerprint);
        assert!(!reset);
        assert_eq!(states["following-fresh"].as_deref(), Some("20"));

        // Reordered IDs resolve to the same normalized social-graph snapshot.
        let reordered =
            normalize_following_author_ids(vec![" author-a ".to_string(), "author-b".to_string()])
                .expect("valid following authors");
        let (states, reset) =
            decode_following_cursor(&cursor, &following_author_set_fingerprint(&reordered));
        assert!(!reset);
        assert_eq!(states["following-fresh"].as_deref(), Some("20"));

        let changed = normalize_following_author_ids(vec!["author-a".to_string()])
            .expect("valid following authors");
        let (states, reset) =
            decode_following_cursor(&cursor, &following_author_set_fingerprint(&changed));
        assert!(reset);
        assert!(states.is_empty());

        // An unversioned offset has no author-set binding and must not be reused.
        let (states, reset) = decode_following_cursor("20", &current_fingerprint);
        assert!(reset);
        assert!(states.is_empty());
    }

    #[test]
    fn following_cursor_restart_builds_a_new_bbs_link_window_after_relationship_mutation() {
        let previous_authors =
            normalize_following_author_ids(vec!["author-b".to_string(), "author-a".to_string()])
                .expect("valid following authors");
        let cursor = encode_following_cursor(
            &BTreeMap::from([("following-fresh".to_string(), Some("page-2".to_string()))]),
            &following_author_set_fingerprint(&previous_authors),
        );
        let current_authors =
            normalize_following_author_ids(vec!["author-c".to_string(), "author-a".to_string()])
                .expect("valid following authors");
        let (states, reset) =
            decode_following_cursor(&cursor, &following_author_set_fingerprint(&current_authors));
        assert!(reset);
        let request = following_list_request(
            states.get("following-fresh").cloned().flatten(),
            1,
            current_authors,
        );

        assert_eq!(request.author_ids, ["author-a", "author-c"]);
        assert_eq!(request.cursor, None);
    }

    #[tokio::test]
    async fn empty_following_set_is_an_empty_timeline_not_a_global_fallback() {
        let response = recall_domain("http://127.0.0.1:18004", None)
            .recall(pb::RecallRequest {
                following_only: true,
                ..Default::default()
            })
            .await;

        assert!(response.candidates.is_empty());
        assert!(response.next_cursor.is_empty());
        assert_eq!(response.sources, vec!["recall:following-fresh"]);
        assert!(!response.degraded);
    }

    // --- Mock upstreams for the semantic lane integration tests. ---

    #[derive(Clone, Default)]
    struct MockBbsLink {
        listings: HashMap<String, Vec<Content>>,
        summaries: Vec<PublicContentSummary>,
    }

    impl MockBbsLink {
        fn with_listing(strategy: &str, items: Vec<Content>) -> Self {
            Self {
                listings: HashMap::from([(strategy.to_string(), items)]),
                summaries: Vec::new(),
            }
        }

        fn with_summaries(mut self, summaries: Vec<PublicContentSummary>) -> Self {
            self.summaries = summaries;
            self
        }
    }

    #[tonic::async_trait]
    impl BbsLink for MockBbsLink {
        async fn list(
            &self,
            request: Request<bbs_link_list::ListRequest>,
        ) -> Result<Response<ContentPage>, Status> {
            let request = request.into_inner();
            // Only the global strategy listings serve items in these tests;
            // domain-scoped interest listings stay empty on purpose.
            let items = match (request.strategy.as_deref(), request.domain) {
                (Some(strategy), None) => self.listings.get(strategy).cloned().unwrap_or_default(),
                _ => Vec::new(),
            };
            Ok(Response::new(ContentPage {
                items,
                next_cursor: None,
                total_estimate: 0,
            }))
        }

        async fn get_public_summaries(
            &self,
            request: Request<PublicContentSummariesRequest>,
        ) -> Result<Response<PublicContentSummaries>, Status> {
            let ids = request.into_inner().ids;
            Ok(Response::new(PublicContentSummaries {
                items: self
                    .summaries
                    .iter()
                    .filter(|summary| ids.contains(&summary.id))
                    .cloned()
                    .collect(),
            }))
        }

        async fn get(
            &self,
            _request: Request<bbs_link_list::IdRequest>,
        ) -> Result<Response<Content>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn get_public(
            &self,
            _request: Request<bbs_link_list::IdRequest>,
        ) -> Result<Response<Content>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn create(
            &self,
            _request: Request<bbs_link_list::CreateRequest>,
        ) -> Result<Response<Content>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn update(
            &self,
            _request: Request<bbs_link_list::UpdateRequest>,
        ) -> Result<Response<Content>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn publish(
            &self,
            _request: Request<bbs_link_list::PublishRequest>,
        ) -> Result<Response<Content>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn restrict(
            &self,
            _request: Request<bbs_link_list::RestrictRequest>,
        ) -> Result<Response<Content>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn restore(
            &self,
            _request: Request<bbs_link_list::RestoreRequest>,
        ) -> Result<Response<Content>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn accept_answer(
            &self,
            _request: Request<bbs_link_list::AcceptAnswerRequest>,
        ) -> Result<Response<Content>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn fork_route(
            &self,
            _request: Request<bbs_link_list::ForkRouteRequest>,
        ) -> Result<Response<Content>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }
    }

    #[derive(Clone, Default)]
    struct MockCatalog {
        fail_embedding: bool,
    }

    #[tonic::async_trait]
    impl KnowledgeCatalog for MockCatalog {
        async fn embed_texts(
            &self,
            request: Request<EmbedTextsRequest>,
        ) -> Result<Response<EmbedTextsResponse>, Status> {
            if self.fail_embedding {
                return Err(Status::unavailable("embedding provider down"));
            }
            let request = request.into_inner();
            assert!(!request.texts.is_empty(), "embedding needs a query text");
            Ok(Response::new(EmbedTextsResponse {
                model: "test-embeddings".to_string(),
                embeddings: request
                    .texts
                    .into_iter()
                    .map(|_| TextEmbedding {
                        values: vec![0.1, 0.2, 0.3, 0.4],
                    })
                    .collect(),
            }))
        }

        async fn upsert_public_resource(
            &self,
            _request: Request<catalog_search::UpsertPublicResourceRequest>,
        ) -> Result<Response<catalog_search::Resource>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn search(
            &self,
            _request: Request<catalog_search::SearchRequest>,
        ) -> Result<Response<catalog_search::SearchResponse>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn get(
            &self,
            _request: Request<catalog_search::GetRequest>,
        ) -> Result<Response<catalog_search::Resource>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn list_node_resources(
            &self,
            _request: Request<catalog_search::ListNodeResourcesRequest>,
        ) -> Result<Response<catalog_search::ListNodeResourcesResponse>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn attach_node_resource(
            &self,
            _request: Request<catalog_search::AttachNodeResourceRequest>,
        ) -> Result<Response<catalog_search::RouteNodeResourceAttachment>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn detach_node_resource(
            &self,
            _request: Request<catalog_search::DetachNodeResourceRequest>,
        ) -> Result<Response<catalog_search::DetachNodeResourceResponse>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn retrieve_rag_context(
            &self,
            _request: Request<catalog_search::RetrieveRagContextRequest>,
        ) -> Result<Response<catalog_search::RetrieveRagContextResponse>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn upsert_rag_embedding(
            &self,
            _request: Request<catalog_search::UpsertRagEmbeddingRequest>,
        ) -> Result<Response<catalog_search::UpsertRagEmbeddingResponse>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn search_rag_embeddings(
            &self,
            _request: Request<catalog_search::SearchRagEmbeddingsRequest>,
        ) -> Result<Response<catalog_search::SearchRagEmbeddingsResponse>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }
    }

    #[derive(Clone, Default)]
    struct MockBbsSearch {
        hits: Vec<SearchResult>,
    }

    #[tonic::async_trait]
    impl BbsSearch for MockBbsSearch {
        async fn search_semantic(
            &self,
            request: Request<SearchSemanticRequest>,
        ) -> Result<Response<SearchResponse>, Status> {
            let request = request.into_inner();
            assert!(!request.query_vector.is_empty(), "vector is required");
            Ok(Response::new(SearchResponse {
                query: request.q,
                items: self.hits.clone(),
                ..Default::default()
            }))
        }

        async fn search(
            &self,
            _request: Request<search_stub::SearchRequest>,
        ) -> Result<Response<SearchResponse>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }

        async fn suggestions(
            &self,
            _request: Request<search_stub::SuggestionsRequest>,
        ) -> Result<Response<search_stub::SuggestionsResponse>, Status> {
            Err(Status::unimplemented("not used by recall tests"))
        }
    }

    async fn spawn_bbs_link(mock: MockBbsLink) -> String {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind test port");
        let address = listener.local_addr().expect("read test address");
        drop(listener);
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(BbsLinkServer::new(mock))
                .serve(address)
                .await
                .expect("run bbs-link test server");
        });
        format!("http://{address}")
    }

    async fn spawn_catalog(mock: MockCatalog) -> String {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind test port");
        let address = listener.local_addr().expect("read test address");
        drop(listener);
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(KnowledgeCatalogServer::new(mock))
                .serve(address)
                .await
                .expect("run knowledge-catalog test server");
        });
        format!("http://{address}")
    }

    async fn spawn_bbs_search(mock: MockBbsSearch) -> String {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind test port");
        let address = listener.local_addr().expect("read test address");
        drop(listener);
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(BbsSearchServer::new(mock))
                .serve(address)
                .await
                .expect("run bbs-search test server");
        });
        format!("http://{address}")
    }

    fn semantic_clients(catalog_url: &str, search_url: &str) -> SemanticRecallClients {
        SemanticRecallClients {
            catalog: lazy_client(catalog_url, KnowledgeCatalogClient::new),
            search: lazy_client(search_url, BbsSearchClient::new),
        }
    }

    fn content(id: &str) -> Content {
        Content {
            id: id.to_string(),
            post: Some(PostSummary {
                id: id.to_string(),
                ..Default::default()
            }),
            quality_score: 0.9,
            ..Default::default()
        }
    }

    fn summary(id: &str) -> PublicContentSummary {
        PublicContentSummary {
            id: id.to_string(),
            post: Some(PostSummary {
                id: id.to_string(),
                ..Default::default()
            }),
            author_id: format!("author-{id}"),
            quality_score: 0.6,
            ..Default::default()
        }
    }

    fn hit(id: &str, result_type: SearchResultType) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            result_type: result_type as i32,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn semantic_lane_joins_knn_hits_with_authoritative_summaries() {
        let bbs_link = spawn_bbs_link(
            MockBbsLink::with_listing("quality", vec![content("quality-1")])
                .with_summaries(vec![summary("semantic-1"), summary("semantic-2")]),
        )
        .await;
        let catalog = spawn_catalog(MockCatalog::default()).await;
        let bbs_search = spawn_bbs_search(MockBbsSearch {
            hits: vec![
                hit("semantic-2", SearchResultType::Journey),
                hit("semantic-1", SearchResultType::Post),
            ],
        })
        .await;
        let domain = recall_domain(
            &bbs_link,
            Some(semantic_clients(&catalog, &bbs_search)),
        );

        let response = domain
            .recall(pb::RecallRequest {
                user_id: "user-1".to_string(),
                interests: vec![GrowthDomain::Travel as i32],
                limit: 10,
                ..Default::default()
            })
            .await;

        assert_eq!(
            response.sources,
            [
                "recall:quality",
                "recall:fresh",
                "recall:interest:travel",
                "recall:semantic",
            ]
        );
        assert!(!response.degraded);
        let semantic = response
            .candidates
            .iter()
            .filter(|candidate| candidate.source == "recall:semantic")
            .collect::<Vec<_>>();
        assert_eq!(
            semantic
                .iter()
                .map(|candidate| candidate.content_id.as_str())
                .collect::<Vec<_>>(),
            ["semantic-2", "semantic-1"],
        );
        // kNN order becomes rank-based retrieval strength inside the lane.
        assert!(semantic[0].recall_score > semantic[1].recall_score);
        assert!(
            semantic
                .iter()
                .all(|candidate| candidate.reasons == ["符合你的兴趣语义"])
        );
        assert!(
            response
                .candidates
                .iter()
                .any(|candidate| candidate.content_id == "quality-1")
        );
    }

    #[tokio::test]
    async fn semantic_lane_failure_degrades_the_page_but_keeps_listing_sources() {
        let bbs_link = spawn_bbs_link(
            MockBbsLink::with_listing("quality", vec![content("quality-1")]),
        )
        .await;
        let catalog = spawn_catalog(MockCatalog {
            fail_embedding: true,
        })
        .await;
        let bbs_search = spawn_bbs_search(MockBbsSearch::default()).await;
        let domain = recall_domain(
            &bbs_link,
            Some(semantic_clients(&catalog, &bbs_search)),
        );

        let response = domain
            .recall(pb::RecallRequest {
                user_id: "user-1".to_string(),
                interests: vec![GrowthDomain::Travel as i32],
                limit: 10,
                ..Default::default()
            })
            .await;

        assert!(response.degraded);
        assert_eq!(
            response
                .candidates
                .iter()
                .map(|candidate| candidate.content_id.as_str())
                .collect::<Vec<_>>(),
            ["quality-1"],
        );
        assert!(
            response
                .candidates
                .iter()
                .all(|candidate| candidate.source != "recall:semantic")
        );
    }
}
