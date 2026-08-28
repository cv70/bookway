use std::{collections::BTreeSet, time::Duration};

use bookway_bbs_link_api::pb as bbs_link_pb;
use bookway_bbs_search_api::pb as bbs_search_pb;
use bookway_knowledge_catalog_api::pb as catalog_pb;

use crate::api::pb;

const SEMANTIC_EMBED_TIMEOUT: Duration = Duration::from_millis(100);
const SEMANTIC_SEARCH_TIMEOUT: Duration = Duration::from_millis(150);
const SEMANTIC_SUMMARY_TIMEOUT: Duration = Duration::from_millis(150);

/// Serving context for the embedding model, composed only from the hydrated
/// interest facts Recommend Main already trusted enough to put on the request
/// (the same labels Recommend Main's ranker uses for its LLM user context).
fn interest_labels(interests: &[i32]) -> Vec<&'static str> {
    // bbs-link's GrowthDomain discriminants: 0=learning .. 4=leisure.
    const DOMAIN_LABELS: &[(i32, &str)] = &[
        (0, "学习"),
        (1, "运动"),
        (2, "健康"),
        (3, "旅行"),
        (4, "休闲"),
    ];
    let mut labels = interests
        .iter()
        .filter_map(|interest| {
            DOMAIN_LABELS
                .iter()
                .find(|(value, _)| value == interest)
                .map(|(_, label)| *label)
        })
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    labels
}

/// The embedding query for the semantic lane. Empty means "no interest
/// signal", which skips the lane instead of issuing an anchor-less query.
fn interest_query(interests: &[i32]) -> String {
    interest_labels(interests).join("、")
}

/// Maps one `SearchSemantic` window into recall candidates, joining every hit
/// against BBS Link's authoritative public projection. The kNN similarity
/// order is the relevance order, so hit order is preserved; retrieval
/// strength itself is assigned per eligible batch by the caller.
fn candidates_from_semantic_page(
    response: bbs_search_pb::SearchResponse,
    summaries: bbs_link_pb::PublicContentSummaries,
) -> Vec<pb::Candidate> {
    let mut authoritative = summaries
        .items
        .into_iter()
        .filter_map(|summary| {
            summary
                .post
                .is_some()
                .then_some((summary.id.clone(), summary))
        })
        .collect::<std::collections::HashMap<_, _>>();
    response
        .items
        .into_iter()
        .filter(|hit| {
            matches!(
                bbs_search_pb::SearchResultType::try_from(hit.result_type),
                Ok(
                    bbs_search_pb::SearchResultType::Post
                        | bbs_search_pb::SearchResultType::Journey
                )
            )
        })
        .filter(|hit| !hit.id.trim().is_empty())
        .filter_map(|hit| {
            let summary = authoritative.remove(hit.id.trim())?;
            let post = summary.post?;
            let freshness = post.freshness;
            Some(pb::Candidate {
                content_id: summary.id,
                post: Some(post),
                author_id: summary.author_id,
                status: bbs_link_pb::ContentStatus::Published as i32,
                quality_score: summary.quality_score,
                freshness,
                // Rank-based retrieval strength; the caller assigns it over
                // the eligible batch once seen filtering has run.
                recall_score: 0.0,
                score: 0.0,
                source: "recall:semantic".to_string(),
                reasons: vec!["符合你的兴趣语义".to_string()],
                p_ctr: 0.0,
                p_cvr: 0.0,
                p_wegu: 0.0,
                // Recall sources carry no model features; recommend-rank
                // fills the snapshot during ranking.
                feature_snapshot: Default::default(),
            })
        })
        .collect()
}

/// Runs the full semantic lane for one recall window: embed the user's
/// interest query, fetch the nearest indexed documents, and rehydrate their
/// authoritative public summaries. Any upstream failure degrades the lane;
/// an empty interest signal or an unpopulated vector index is an ordinary
/// empty window, not a failure.
pub(crate) async fn recall_semantic_page(
    catalog: &mut catalog_pb::knowledge_catalog_client::KnowledgeCatalogClient<tonic::transport::Channel>,
    search: &mut bbs_search_pb::bbs_search_client::BbsSearchClient<tonic::transport::Channel>,
    content: &mut bbs_link_pb::bbs_link_client::BbsLinkClient<tonic::transport::Channel>,
    user_id: &str,
    interests: &[i32],
    limit: usize,
) -> Result<Vec<pb::Candidate>, String> {
    let query = interest_query(interests);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let embed_request =
        bookway_runtime::grpc_service_request(catalog_pb::EmbedTextsRequest {
            texts: vec![query.clone()],
        })
        .map_err(|error| error.to_string())?;
    let embeddings =
        tokio::time::timeout(SEMANTIC_EMBED_TIMEOUT, catalog.embed_texts(embed_request))
            .await
            .map_err(|_| "query embedding timed out".to_string())?
            .map_err(|error| error.to_string())?
            .into_inner();
    let Some(query_vector) = embeddings
        .embeddings
        .into_iter()
        .next()
        .map(|embedding| embedding.values)
        .filter(|values| !values.is_empty())
    else {
        tracing::debug!("embedding provider returned no vector; semantic lane skipped");
        return Ok(Vec::new());
    };
    let search_request = bookway_runtime::grpc_service_request(bbs_search_pb::SearchSemanticRequest {
        q: query,
        query_vector,
        limit: Some(limit as u32),
        // An absent user ID must stay absent upstream instead of traveling as
        // an empty-string identity.
        user_id: (!user_id.is_empty()).then(|| user_id.to_string()),
        excluded_author_ids: Vec::new(),
        // Unset recalls the mixed public content surface; the mapping below
        // keeps only Post/Journey hits.
        search_type: None,
    })
    .map_err(|error| error.to_string())?;
    let response =
        tokio::time::timeout(SEMANTIC_SEARCH_TIMEOUT, search.search_semantic(search_request))
            .await
            .map_err(|_| "semantic search timed out".to_string())?
            .map_err(|error| error.to_string())?
            .into_inner();
    let ids = response
        .items
        .iter()
        .filter(|hit| {
            matches!(
                bbs_search_pb::SearchResultType::try_from(hit.result_type),
                Ok(
                    bbs_search_pb::SearchResultType::Post
                        | bbs_search_pb::SearchResultType::Journey
                )
            )
        })
        .map(|hit| hit.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let summaries_request = bookway_runtime::grpc_service_request(
        bbs_link_pb::PublicContentSummariesRequest {
            ids: ids.into_iter().collect(),
        },
    )
    .map_err(|error| error.to_string())?;
    let summaries = tokio::time::timeout(
        SEMANTIC_SUMMARY_TIMEOUT,
        content.get_public_summaries(summaries_request),
    )
    .await
    .map_err(|_| "semantic summary hydration timed out".to_string())?
    .map_err(|error| error.to_string())?
    .into_inner();
    Ok(candidates_from_semantic_page(response, summaries))
}

#[cfg(test)]
mod tests {
    use bookway_bbs_link_api::pb as bbs_link_pb;
    use bookway_bbs_search_api::pb as bbs_search_pb;

    use bookway_bbs_link_api::pb::{GrowthDomain, PostSummary, PublicContentSummary};
    use bookway_bbs_search_api::pb::{SearchResult, SearchResultType};

    use super::{candidates_from_semantic_page, interest_query};

    fn hit(id: &str, result_type: SearchResultType) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            result_type: result_type as i32,
            ..Default::default()
        }
    }

    fn summary(id: &str, title: &str) -> PublicContentSummary {
        PublicContentSummary {
            id: id.to_string(),
            post: Some(PostSummary {
                id: id.to_string(),
                title: title.to_string(),
                freshness: 0.4,
                ..Default::default()
            }),
            author_id: format!("author-{id}"),
            quality_score: 0.7,
            ..Default::default()
        }
    }

    #[test]
    fn interest_query_builds_deterministic_labels_from_hydrated_domains() {
        assert_eq!(
            interest_query(&[
                GrowthDomain::Travel as i32,
                GrowthDomain::Learning as i32,
                GrowthDomain::Travel as i32,
            ]),
            "学习、旅行"
        );
        assert!(interest_query(&[99]).is_empty());
        assert!(interest_query(&[]).is_empty());
    }

    #[test]
    fn semantic_page_keeps_knn_order_and_attaches_authoritative_summaries() {
        let response = bbs_search_pb::SearchResponse {
            items: vec![
                hit("content-2", SearchResultType::Journey),
                hit("content-1", SearchResultType::Post),
                // Non-content entities and unknown hits never become candidates.
                hit("user-1", SearchResultType::User),
                hit("content-missing", SearchResultType::Post),
                hit(" ", SearchResultType::Post),
            ],
            ..Default::default()
        };
        let summaries = bbs_link_pb::PublicContentSummaries {
            items: vec![summary("content-1", "早睡指南"), summary("content-2", "徒步路线")],
        };

        let candidates = candidates_from_semantic_page(response, summaries);

        let ids = candidates
            .iter()
            .map(|candidate| candidate.content_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["content-2", "content-1"]);
        assert!(candidates.iter().all(|c| c.source == "recall:semantic"));
        assert!(
            candidates
                .iter()
                .all(|c| c.reasons == ["符合你的兴趣语义"])
        );
        let first = &candidates[0];
        assert_eq!(
            first.post.as_ref().expect("public post").title,
            "徒步路线"
        );
        assert_eq!(first.author_id, "author-content-2");
        assert_eq!(first.quality_score, 0.7);
        assert_eq!(first.status, bookway_bbs_link_api::pb::ContentStatus::Published as i32);
    }
}
