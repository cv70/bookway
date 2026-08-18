mod candidate;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use bookway_bbs_link_api::pb as bbs_link_pb;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::Domain;
use crate::{api::pb, conf::SourceBlend};

const CURSOR_V1_PREFIX: &str = "v1:";
const CURSOR_V2_PREFIX: &str = "v2:";
const MAX_CURSOR_BYTES: usize = 1_024;
const MAX_FOLLOWING_AUTHORS: usize = 5_000;
const QUALITY_SOURCE_WEIGHT: usize = 4;
const FRESH_SOURCE_WEIGHT: usize = 2;

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
                let mut client = self.content_client.clone();
                let source_cursor = cursor_states.get(&source.name).cloned().flatten();
                async move {
                    let request = bbs_link_pb::ListRequest {
                        cursor: source_cursor.clone(),
                        limit: Some((limit * 2).min(self.max_candidates) as u32),
                        status: Some(bbs_link_pb::ContentStatus::Published as i32),
                        strategy: Some(source.content_strategy.to_string()),
                        ids: None,
                        author_id: None,
                        content_type: None,
                        domain: source.domain.map(|domain| domain as i32),
                        author_ids: Vec::new(),
                    };
                    let result = async {
                        let request = bookway_runtime::grpc_service_request(request)
                            .map_err(|error| error.to_string())?;
                        client
                            .list(request)
                            .await
                            .map(|response| response.into_inner())
                            .map_err(|error| error.to_string())
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
                    let fetched = page.items.len();
                    let mut batch = Vec::with_capacity(fetched);
                    for content in page.items {
                        if let Some(candidate) =
                            candidate::candidate_from_content(content, &source.name)
                        {
                            if !seen.contains(&candidate.content_id) {
                                batch.push(SourcedCandidate {
                                    content_id: candidate.content_id.clone(),
                                    recall_score: candidate.recall_score,
                                });
                            }
                            merge_candidate(&mut candidates, candidate);
                        }
                    }
                    sort_and_deduplicate_batch(&mut batch);
                    tracing::debug!(
                        source = %source.name,
                        fetched,
                        eligible = batch.len(),
                        exhausted = source_is_exhausted(&next_cursor_states, &source),
                        "recall source completed"
                    );
                    source_batches.insert(source.name.clone(), batch);
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
        let request = match bookway_runtime::grpc_service_request(bbs_link_pb::ListRequest {
            cursor: source_cursor.clone(),
            // Never advance past candidates that this chronological surface
            // has not returned to its caller yet.
            limit: Some(limit as u32),
            status: Some(bbs_link_pb::ContentStatus::Published as i32),
            strategy: Some(source.content_strategy.to_string()),
            ids: None,
            author_id: None,
            content_type: None,
            domain: None,
            author_ids: following_author_ids,
        }) {
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

#[derive(Clone)]
struct RecallSource {
    name: String,
    content_strategy: &'static str,
    domain: Option<bbs_link_pb::GrowthDomain>,
}

#[derive(Clone, Debug, PartialEq)]
struct SourcedCandidate {
    content_id: String,
    recall_score: f64,
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
    if let Some(value) = cursor
        .strip_prefix(CURSOR_V2_PREFIX)
        .or_else(|| cursor.strip_prefix(CURSOR_V1_PREFIX))
    {
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
        // A legacy, general-feed, or malformed cursor has no author-set
        // binding, so it cannot safely continue a Following timeline.
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

fn recall_sources(interests: &[i32]) -> Vec<RecallSource> {
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
    let domains = interests
        .iter()
        .filter_map(|interest| bbs_link_pb::GrowthDomain::try_from(*interest).ok())
        .collect::<BTreeSet<_>>();
    for interest in domains {
        sources.push(RecallSource {
            name: format!("interest:{}", domain_name(interest)),
            content_strategy: "quality",
            domain: Some(interest),
        });
    }
    sources
}

fn following_recall_source() -> RecallSource {
    RecallSource {
        name: "following-fresh".to_string(),
        content_strategy: "fresh",
        domain: None,
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

    // Reserve the first available placement for fresh and interest recall so
    // a high-scoring quality index cannot eliminate all exploration.
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
    source.name == "fresh" || source.name.starts_with("interest:")
}

fn source_mix_weight(source: &RecallSource) -> usize {
    match source.name.as_str() {
        "quality" => QUALITY_SOURCE_WEIGHT,
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
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use bookway_bbs_link_api::pb::{
        self as bbs_link_pb, ContentStatus, GrowthDomain, bbs_link_client::BbsLinkClient,
        bbs_link_server::BbsLink,
    };
    use tokio::net::TcpListener;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::{
        Request, Response, Status,
        transport::{Endpoint, Server},
    };

    use super::{
        Domain, SourcedCandidate, decode_cursor, decode_following_cursor, encode_cursor,
        encode_following_cursor, following_author_set_fingerprint, following_recall_source,
        merge_candidate, normalize_following_author_ids, recall_sources,
        select_source_mixed_candidates, sort_and_deduplicate_batch, source_is_exhausted,
    };
    use crate::{api::pb, conf::Config};

    #[derive(Clone, Default)]
    struct RecordingBbsLink {
        list_requests: Arc<Mutex<Vec<bbs_link_pb::ListRequest>>>,
    }

    #[tonic::async_trait]
    impl BbsLink for RecordingBbsLink {
        async fn list(
            &self,
            request: Request<bbs_link_pb::ListRequest>,
        ) -> Result<Response<bbs_link_pb::ContentPage>, Status> {
            let request = request.into_inner();
            self.list_requests
                .lock()
                .expect("recording BBS Link lock")
                .push(request.clone());
            let author_id = request
                .author_ids
                .first()
                .cloned()
                .unwrap_or_else(|| "missing-author".to_string());
            Ok(Response::new(bbs_link_pb::ContentPage {
                items: vec![bbs_link_pb::Content {
                    id: format!("post-{author_id}"),
                    author_id: author_id.clone(),
                    status: ContentStatus::Published as i32,
                    post: Some(bbs_link_pb::PostSummary {
                        id: format!("post-{author_id}"),
                        freshness: 1.0,
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                next_cursor: Some("page-2".to_string()),
                total_estimate: 2,
            }))
        }

        async fn get_public_summaries(
            &self,
            _request: Request<bbs_link_pb::PublicContentSummariesRequest>,
        ) -> Result<Response<bbs_link_pb::PublicContentSummaries>, Status> {
            Err(Status::unimplemented("not used by following recall"))
        }

        async fn get(
            &self,
            _request: Request<bbs_link_pb::IdRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by following recall"))
        }

        async fn get_public(
            &self,
            _request: Request<bbs_link_pb::IdRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by following recall"))
        }

        async fn create(
            &self,
            _request: Request<bbs_link_pb::CreateRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by following recall"))
        }

        async fn update(
            &self,
            _request: Request<bbs_link_pb::UpdateRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by following recall"))
        }

        async fn publish(
            &self,
            _request: Request<bbs_link_pb::PublishRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by following recall"))
        }

        async fn restrict(
            &self,
            _request: Request<bbs_link_pb::RestrictRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by following recall"))
        }

        async fn restore(
            &self,
            _request: Request<bbs_link_pb::RestoreRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by following recall"))
        }

        async fn accept_answer(
            &self,
            _request: Request<bbs_link_pb::AcceptAnswerRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by following recall"))
        }
    }

    async fn recording_bbs_link() -> (RecordingBbsLink, String, tokio::task::JoinHandle<()>) {
        let service = RecordingBbsLink::default();
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test BBS Link server");
        let address = listener.local_addr().expect("test BBS Link address");
        let server_service = service.clone();
        let server = tokio::spawn(async move {
            Server::builder()
                .add_service(bbs_link_pb::bbs_link_server::BbsLinkServer::new(
                    server_service,
                ))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .expect("serve test BBS Link");
        });
        (service, format!("http://{address}"), server)
    }

    fn recall_domain(bbs_link_url: &str) -> Domain {
        Domain {
            config: Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid address"),
                bbs_link_url: bbs_link_url.to_string(),
                max_candidates: 20,
                source_blend: super::SourceBlend::BalancedV1,
            },
            content_client: BbsLinkClient::new(
                Endpoint::from_shared(bbs_link_url.to_string())
                    .expect("valid BBS Link URL")
                    .connect_lazy(),
            ),
            max_candidates: 20,
        }
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
            recall_sources(&[GrowthDomain::Travel as i32, GrowthDomain::Travel as i32, 99]);

        assert_eq!(sources.len(), 3);
        assert_eq!(sources[2].domain, Some(GrowthDomain::Travel));
    }

    #[test]
    fn source_mix_reserves_fresh_and_interest_candidates_before_quality_fill() {
        let sources = recall_sources(&[GrowthDomain::Travel as i32]);
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
    fn source_mix_deduplicates_candidates_and_preserves_all_source_reasons() {
        let sources = recall_sources(&[GrowthDomain::Travel as i32]);
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
        let sources = recall_sources(&[GrowthDomain::Travel as i32]);
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
        let sources = recall_sources(&[GrowthDomain::Learning as i32, GrowthDomain::Travel as i32]);
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
        let sources = recall_sources(&[GrowthDomain::Learning as i32]);
        let states = decode_cursor("80", &sources);

        assert!(
            states
                .values()
                .all(|cursor| cursor.as_deref() == Some("80"))
        );
    }

    #[test]
    fn following_recall_uses_one_fresh_source_and_normalizes_author_ids() {
        let source = following_recall_source();

        assert_eq!(source.name, "following-fresh");
        assert_eq!(source.content_strategy, "fresh");
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

        // A legacy offset has no author-set binding and must not be reused.
        let (states, reset) = decode_following_cursor("20", &current_fingerprint);
        assert!(reset);
        assert!(states.is_empty());
    }

    #[tokio::test]
    async fn following_cursor_restart_uses_a_new_bbs_link_window_after_relationship_mutation() {
        let (bbs_link, bbs_link_url, server) = recording_bbs_link().await;
        let domain = recall_domain(&bbs_link_url);
        let first_page = domain
            .recall(pb::RecallRequest {
                user_id: "reader-1".to_string(),
                following_only: true,
                following_author_ids: vec!["author-b".to_string(), "author-a".to_string()],
                limit: 1,
                ..Default::default()
            })
            .await;
        assert!(!first_page.next_cursor.is_empty());

        let second_page = domain
            .recall(pb::RecallRequest {
                user_id: "reader-1".to_string(),
                following_only: true,
                following_author_ids: vec!["author-c".to_string(), "author-a".to_string()],
                cursor: first_page.next_cursor,
                limit: 1,
                ..Default::default()
            })
            .await;

        assert!(!second_page.degraded);
        assert_eq!(second_page.candidates[0].author_id, "author-a");
        let requests = bbs_link
            .list_requests
            .lock()
            .expect("recording BBS Link lock")
            .clone();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].author_ids, ["author-a", "author-b"]);
        assert_eq!(requests[0].cursor, None);
        assert_eq!(requests[1].author_ids, ["author-a", "author-c"]);
        assert_eq!(requests[1].cursor, None);

        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn empty_following_set_is_an_empty_timeline_not_a_global_fallback() {
        let response = recall_domain("http://127.0.0.1:18004")
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
}
