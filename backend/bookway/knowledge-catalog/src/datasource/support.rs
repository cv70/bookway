use std::collections::HashMap;

use async_trait::async_trait;
use bookway_knowledge_catalog_api::pb;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DaoError {
    #[error("resource {0} was not found")]
    NotFound(String),
    #[error("resource attachment conflict: {0}")]
    Conflict(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("stored resource is invalid: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug)]
pub(crate) struct NewNodeResourceAttachment {
    pub(crate) route_id: String,
    pub(crate) action_node_id: String,
    pub(crate) scene_equipment: String,
    pub(crate) resource_id: String,
    pub(crate) kind: pb::AttachmentKind,
    pub(crate) title_override: String,
    pub(crate) note: String,
    pub(crate) sort_rank: i32,
    pub(crate) rag_enabled: bool,
    pub(crate) embedding_collection: String,
    pub(crate) retrieval_scope: String,
    pub(crate) idempotency_key: String,
    pub(crate) created_by: String,
}

#[derive(Clone, Debug)]
pub(crate) struct RagVectorHit {
    pub(crate) attachment_id: String,
    pub(crate) relevance: f64,
}

/// A validated catalog write. `resource_id` is `None` on create; the DAO
/// generates the id (or adopts the entry that already owns the URL).
#[derive(Clone, Debug)]
pub(crate) struct NewPublicResource {
    pub(crate) resource_id: Option<String>,
    pub(crate) title: String,
    pub(crate) kind: pb::ResourceKind,
    pub(crate) provider: String,
    pub(crate) summary: String,
    pub(crate) url: String,
    pub(crate) license: String,
    pub(crate) version: String,
    pub(crate) citation: String,
    pub(crate) topics: Vec<String>,
    pub(crate) status: pb::ResourceStatus,
}

#[async_trait]
pub(crate) trait ResourceDao: Send + Sync {
    async fn search(&self, request: &pb::SearchRequest) -> Result<pb::SearchResponse, DaoError>;
    async fn get(&self, resource_id: &str) -> Result<pb::Resource, DaoError>;
    async fn upsert_public_resource(
        &self,
        request: NewPublicResource,
    ) -> Result<pb::Resource, DaoError>;
    async fn list_node_resources(
        &self,
        route_id: &str,
        action_node_id: &str,
        scene_equipment: Option<&str>,
        include_archived: bool,
    ) -> Result<pb::ListNodeResourcesResponse, DaoError>;
    async fn attach_node_resource(
        &self,
        request: NewNodeResourceAttachment,
    ) -> Result<pb::RouteNodeResourceAttachment, DaoError>;
    async fn detach_node_resource(
        &self,
        route_id: &str,
        action_node_id: &str,
        attachment_id: &str,
    ) -> Result<bool, DaoError>;
    async fn upsert_rag_embedding(
        &self,
        attachment: &pb::RouteNodeResourceAttachment,
        embedding_model: &str,
        embedding: Vec<f32>,
    ) -> Result<(), DaoError>;
    async fn search_rag_embeddings(
        &self,
        route_id: &str,
        action_node_id: &str,
        embedding_collection: &str,
        embedding_model: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<RagVectorHit>, DaoError>;
}

#[derive(Clone)]
struct RagEmbedding {
    route_id: String,
    action_node_id: String,
    embedding_collection: String,
    embedding_model: String,
    embedding: Vec<f32>,
}

fn attachment_matches_request(
    attachment: &pb::RouteNodeResourceAttachment,
    request: &NewNodeResourceAttachment,
) -> bool {
    attachment.route_id == request.route_id
        && attachment.action_node_id == request.action_node_id
        && attachment.scene_equipment == request.scene_equipment
        && attachment.resource_id == request.resource_id
        && attachment.kind == request.kind as i32
        && attachment.title_override == request.title_override
        && attachment.note == request.note
        && attachment.sort_rank == request.sort_rank
        && attachment.rag_enabled == request.rag_enabled
        && attachment.embedding_collection == request.embedding_collection
        && attachment.retrieval_scope == request.retrieval_scope
        && attachment.created_by == request.created_by
}

#[allow(clippy::too_many_arguments)]
fn resource(
    id: &str,
    title: &str,
    kind: pb::ResourceKind,
    provider: &str,
    summary: &str,
    url: &str,
    license: &str,
    version: &str,
    citation: &str,
    topics: &[&str],
) -> pb::Resource {
    pb::Resource {
        id: id.to_string(),
        title: title.to_string(),
        kind: kind as i32,
        provider: provider.to_string(),
        summary: summary.to_string(),
        url: url.to_string(),
        license: license.to_string(),
        version: version.to_string(),
        citation: citation.to_string(),
        topics: topics.iter().map(|topic| (*topic).to_string()).collect(),
        status: pb::ResourceStatus::Published as i32,
        published_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-08-01T00:00:00Z".to_string(),
    }
}

fn parse_cursor(cursor: &str) -> Result<usize, DaoError> {
    if cursor.trim().is_empty() {
        return Ok(0);
    }
    cursor
        .trim()
        .parse()
        .map_err(|_| DaoError::Invalid("cursor must be a non-negative offset".to_string()))
}

fn validate_embedding(model: &str, embedding: &[f32]) -> Result<(), DaoError> {
    if model.trim().is_empty() || model.chars().count() > 80 {
        return Err(DaoError::Invalid(
            "RAG embedding model is invalid".to_string(),
        ));
    }
    if !(8..=4096).contains(&embedding.len())
        || embedding.iter().any(|value| !value.is_finite())
        || embedding.iter().all(|value| *value == 0.0)
    {
        return Err(DaoError::Invalid(
            "RAG embedding vector is invalid".to_string(),
        ));
    }
    Ok(())
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> Option<f64> {
    if left.len() != right.len() {
        return None;
    }
    let (dot, left_norm, right_norm) = left.iter().zip(right).fold(
        (0.0_f64, 0.0_f64, 0.0_f64),
        |(dot, left_norm, right_norm), (left, right)| {
            let left = f64::from(*left);
            let right = f64::from(*right);
            (
                dot + left * right,
                left_norm + left * left,
                right_norm + right * right,
            )
        },
    );
    let denominator = left_norm.sqrt() * right_norm.sqrt();
    (denominator > f64::EPSILON).then(|| (dot / denominator).clamp(-1.0, 1.0))
}

fn sort_rag_hits(hits: &mut [RagVectorHit]) {
    hits.sort_by(|left, right| {
        right
            .relevance
            .total_cmp(&left.relevance)
            .then_with(|| left.attachment_id.cmp(&right.attachment_id))
    });
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
fn kind_name(kind: pb::ResourceKind) -> &'static str {
    match kind {
        pb::ResourceKind::Book => "book",
        pb::ResourceKind::Course => "course",
        pb::ResourceKind::Tool => "tool",
        pb::ResourceKind::Article => "article",
        pb::ResourceKind::Podcast => "podcast",
        pb::ResourceKind::Unspecified => "",
    }
}
fn status_name(status: pb::ResourceStatus) -> &'static str {
    match status {
        pb::ResourceStatus::Published => "published",
        pb::ResourceStatus::Archived => "archived",
        pb::ResourceStatus::Unspecified => "",
    }
}
fn attachment_kind_name(kind: pb::AttachmentKind) -> &'static str {
    match kind {
        pb::AttachmentKind::Document => "document",
        pb::AttachmentKind::Pdf => "pdf",
        pb::AttachmentKind::ExternalLink => "external_link",
        pb::AttachmentKind::ToolChecklist => "tool_checklist",
        pb::AttachmentKind::AiActionGuide => "ai_action_guide",
        pb::AttachmentKind::RagCorpus => "rag_corpus",
        pb::AttachmentKind::ResourcePackage => "resource_package",
        pb::AttachmentKind::Unspecified => "",
    }
}
fn parse_kind(value: &str) -> Result<i32, DaoError> {
    Ok(match value {
        "book" => pb::ResourceKind::Book,
        "course" => pb::ResourceKind::Course,
        "tool" => pb::ResourceKind::Tool,
        "article" => pb::ResourceKind::Article,
        "podcast" => pb::ResourceKind::Podcast,
        _ => {
            return Err(DaoError::Invalid(format!("invalid resource kind {value}")));
        }
    } as i32)
}
fn parse_status(value: &str) -> Result<i32, DaoError> {
    Ok(match value {
        "published" => pb::ResourceStatus::Published,
        "archived" => pb::ResourceStatus::Archived,
        _ => {
            return Err(DaoError::Invalid(format!(
                "invalid resource status {value}"
            )));
        }
    } as i32)
}
fn parse_attachment_kind(value: &str) -> Result<i32, DaoError> {
    Ok(match value {
        "document" => pb::AttachmentKind::Document,
        "pdf" => pb::AttachmentKind::Pdf,
        "external_link" => pb::AttachmentKind::ExternalLink,
        "tool_checklist" => pb::AttachmentKind::ToolChecklist,
        "ai_action_guide" => pb::AttachmentKind::AiActionGuide,
        "rag_corpus" => pb::AttachmentKind::RagCorpus,
        "resource_package" => pb::AttachmentKind::ResourcePackage,
        _ => {
            return Err(DaoError::Invalid(format!(
                "invalid attachment kind {value}"
            )));
        }
    } as i32)
}
fn format_time(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::{MemoryResourceDao, NewPublicResource, ResourceDao};
    use bookway_knowledge_catalog_api::pb;

    fn upsert_request(resource_id: Option<String>, url: &str) -> NewPublicResource {
        NewPublicResource {
            resource_id,
            title: "开放工具".to_string(),
            kind: pb::ResourceKind::Tool,
            provider: "Open Tools".to_string(),
            summary: "工具简介".to_string(),
            url: url.to_string(),
            license: "MIT".to_string(),
            version: "2.0".to_string(),
            citation: "Open Tools. 2026.".to_string(),
            topics: vec!["工具".to_string()],
            status: pb::ResourceStatus::Published,
        }
    }

    #[tokio::test]
    async fn memory_upsert_creates_then_updates_in_place_for_duplicate_url() {
        let dao = MemoryResourceDao::seeded();
        let created = dao
            .upsert_public_resource(upsert_request(None, "https://test.example/tool"))
            .await
            .expect("create should succeed");
        assert_eq!(created.status, pb::ResourceStatus::Published as i32);

        // A duplicate canonical URL updates the owning entry; no second row.
        let updated = dao
            .upsert_public_resource(upsert_request(
                Some("caller-chosen-id".to_string()),
                "https://test.example/tool",
            ))
            .await
            .expect("duplicate url should update");
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.title, "开放工具");
        let page = dao
            .search(&pb::SearchRequest {
                query: "开放工具".to_string(),
                ..Default::default()
            })
            .await
            .expect("search should succeed");
        assert_eq!(page.items.len(), 1);

        // An unknown id with a fresh URL creates a new entry with that id.
        let caller_id = dao
            .upsert_public_resource(upsert_request(
                Some("resource-caller-id".to_string()),
                "https://test.example/other",
            ))
            .await
            .expect("create with caller id");
        assert_eq!(caller_id.id, "resource-caller-id");
    }

    #[tokio::test]
    async fn memory_catalog_filters_published_resources_by_kind_and_topic() {
        let dao = MemoryResourceDao::seeded();
        let response = dao
            .search(&pb::SearchRequest {
                kind: Some(pb::ResourceKind::Book as i32),
                topic: "阅读".to_string(),
                limit: Some(10),
                ..Default::default()
            })
            .await
            .expect("catalog search should succeed");

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].id, "resource-gutenberg");
        assert_eq!(response.items[0].license, "Public Domain");
    }

    #[tokio::test]
    async fn memory_catalog_cursor_is_bounded_and_get_hides_unknown_resources() {
        let dao = MemoryResourceDao::seeded();
        let first = dao
            .search(&pb::SearchRequest {
                limit: Some(1),
                ..Default::default()
            })
            .await
            .expect("first page should succeed");
        assert_eq!(first.items.len(), 1);
        assert_eq!(first.next_cursor.as_deref(), Some("1"));

        let second = dao
            .search(&pb::SearchRequest {
                cursor: first.next_cursor.unwrap_or_default(),
                limit: Some(10),
                ..Default::default()
            })
            .await
            .expect("second page should succeed");
        assert_eq!(second.items.len(), 2);
        assert!(dao.get("missing-resource").await.is_err());
    }
}

#[path = "memory_resource_dao.rs"]
mod memory_resource_dao;
pub(crate) use memory_resource_dao::MemoryResourceDao;
#[path = "postgres_resource_dao.rs"]
mod postgres_resource_dao;
pub(crate) use postgres_resource_dao::PostgresResourceDao;
