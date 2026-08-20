use std::collections::HashMap;

use async_trait::async_trait;
use bookway_knowledge_catalog_api::pb;
use sqlx::FromRow;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub(crate) enum RepositoryError {
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

#[async_trait]
pub(crate) trait ResourceRepository: Send + Sync {
    async fn search(
        &self,
        request: &pb::SearchRequest,
    ) -> Result<pb::SearchResponse, RepositoryError>;
    async fn get(&self, resource_id: &str) -> Result<pb::Resource, RepositoryError>;
    async fn list_node_resources(
        &self,
        route_id: &str,
        action_node_id: &str,
        include_archived: bool,
    ) -> Result<pb::ListNodeResourcesResponse, RepositoryError>;
    async fn attach_node_resource(
        &self,
        request: NewNodeResourceAttachment,
    ) -> Result<pb::RouteNodeResourceAttachment, RepositoryError>;
    async fn detach_node_resource(
        &self,
        route_id: &str,
        action_node_id: &str,
        attachment_id: &str,
    ) -> Result<bool, RepositoryError>;
    async fn upsert_rag_embedding(
        &self,
        attachment: &pb::RouteNodeResourceAttachment,
        embedding_model: &str,
        embedding: Vec<f32>,
    ) -> Result<(), RepositoryError>;
    async fn search_rag_embeddings(
        &self,
        route_id: &str,
        action_node_id: &str,
        embedding_collection: &str,
        embedding_model: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<RagVectorHit>, RepositoryError>;
}

pub(crate) struct MemoryResourceRepository {
    resources: RwLock<Vec<pb::Resource>>,
    attachments: RwLock<Vec<pb::RouteNodeResourceAttachment>>,
    attachment_idempotency: RwLock<HashMap<String, String>>,
    rag_embeddings: RwLock<HashMap<String, RagEmbedding>>,
}

#[derive(Clone)]
struct RagEmbedding {
    route_id: String,
    action_node_id: String,
    embedding_collection: String,
    embedding_model: String,
    embedding: Vec<f32>,
}

impl MemoryResourceRepository {
    pub(crate) fn seeded() -> Self {
        Self {
            resources: RwLock::new(vec![
                resource(
                    "resource-mdn-web",
                    "MDN Web Docs",
                    pb::ResourceKind::Article,
                    "Mozilla",
                    "面向开发者的开放 Web 平台文档与实践参考。",
                    "https://developer.mozilla.org/",
                    "CC BY-SA 2.5",
                    "2026.1",
                    "Mozilla Developer Network. MDN Web Docs. 2026.",
                    &["学习", "工具", "编程"],
                ),
                resource(
                    "resource-ocw-learning",
                    "MIT OpenCourseWare",
                    pb::ResourceKind::Course,
                    "MIT",
                    "公开课程资料，适合按主题建立长期学习路径。",
                    "https://ocw.mit.edu/",
                    "CC BY-NC-SA 4.0",
                    "2026",
                    "MIT OpenCourseWare. 2026.",
                    &["学习", "课程"],
                ),
                resource(
                    "resource-gutenberg",
                    "Project Gutenberg",
                    pb::ResourceKind::Book,
                    "Project Gutenberg",
                    "可合法阅读和下载的公共领域电子书目录。",
                    "https://www.gutenberg.org/",
                    "Public Domain",
                    "2026",
                    "Project Gutenberg. 2026.",
                    &["阅读", "书籍", "知识管理"],
                ),
            ]),
            attachments: RwLock::new(Vec::new()),
            attachment_idempotency: RwLock::new(HashMap::new()),
            rag_embeddings: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ResourceRepository for MemoryResourceRepository {
    async fn search(
        &self,
        request: &pb::SearchRequest,
    ) -> Result<pb::SearchResponse, RepositoryError> {
        let query = request.query.trim().to_lowercase();
        let topic = request.topic.trim().to_lowercase();
        let kind = request
            .kind
            .map(pb::ResourceKind::try_from)
            .transpose()
            .map_err(|_| RepositoryError::Invalid("invalid resource kind".to_string()))?;
        let offset = parse_cursor(&request.cursor)?;
        let limit = usize::try_from(request.limit.unwrap_or(20).clamp(1, 50)).unwrap_or(20);
        let resources = self.resources.read().await;
        let mut items = resources
            .iter()
            .filter(|item| item.status == pb::ResourceStatus::Published as i32)
            .filter(|item| kind.is_none() || kind == pb::ResourceKind::try_from(item.kind).ok())
            .filter(|item| {
                topic.is_empty()
                    || item
                        .topics
                        .iter()
                        .any(|value| value.to_lowercase() == topic)
            })
            .filter(|item| {
                query.is_empty()
                    || [
                        item.title.as_str(),
                        item.summary.as_str(),
                        item.provider.as_str(),
                        item.citation.as_str(),
                    ]
                    .into_iter()
                    .any(|value| value.to_lowercase().contains(&query))
                    || item
                        .topics
                        .iter()
                        .any(|value| value.to_lowercase().contains(&query))
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        let has_more = offset.saturating_add(limit) < items.len();
        let page = items
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        Ok(pb::SearchResponse {
            items: page,
            next_cursor: has_more.then(|| offset.saturating_add(limit).to_string()),
        })
    }

    async fn get(&self, resource_id: &str) -> Result<pb::Resource, RepositoryError> {
        self.resources
            .read()
            .await
            .iter()
            .find(|resource| {
                resource.id == resource_id
                    && resource.status == pb::ResourceStatus::Published as i32
            })
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound(resource_id.to_string()))
    }

    async fn list_node_resources(
        &self,
        route_id: &str,
        action_node_id: &str,
        include_archived: bool,
    ) -> Result<pb::ListNodeResourcesResponse, RepositoryError> {
        let resources = self.resources.read().await;
        let resource_by_id = resources
            .iter()
            .map(|resource| (resource.id.as_str(), resource.clone()))
            .collect::<HashMap<_, _>>();
        let mut items = self
            .attachments
            .read()
            .await
            .iter()
            .filter(|attachment| {
                attachment.route_id == route_id
                    && attachment.action_node_id == action_node_id
                    && (include_archived || !attachment.updated_at.starts_with("archived:"))
            })
            .cloned()
            .map(|mut attachment| {
                attachment.resource = resource_by_id.get(attachment.resource_id.as_str()).cloned();
                attachment
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            left.sort_rank
                .cmp(&right.sort_rank)
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(pb::ListNodeResourcesResponse { items })
    }

    async fn attach_node_resource(
        &self,
        request: NewNodeResourceAttachment,
    ) -> Result<pb::RouteNodeResourceAttachment, RepositoryError> {
        let resource = self.get(&request.resource_id).await?;
        if let Some(existing_id) = self
            .attachment_idempotency
            .read()
            .await
            .get(&request.idempotency_key)
            .cloned()
        {
            let attachments = self.attachments.read().await;
            let existing = attachments
                .iter()
                .find(|item| item.id == existing_id)
                .ok_or_else(|| {
                    RepositoryError::Invalid("missing node resource idempotency target".to_string())
                })?;
            if !attachment_matches_request(existing, &request) {
                return Err(RepositoryError::Conflict(
                    "idempotency key is already bound to a different resource attachment"
                        .to_string(),
                ));
            }
            let mut existing = existing.clone();
            existing.resource = Some(resource);
            return Ok(existing);
        }

        let mut attachments = self.attachments.write().await;
        if let Some(existing) = attachments.iter_mut().find(|item| {
            item.route_id == request.route_id
                && item.action_node_id == request.action_node_id
                && item.resource_id == request.resource_id
                && !item.updated_at.starts_with("archived:")
        }) {
            existing.kind = request.kind as i32;
            existing.title_override = request.title_override;
            existing.note = request.note;
            existing.sort_rank = request.sort_rank;
            existing.rag_enabled = request.rag_enabled;
            existing.embedding_collection = request.embedding_collection;
            existing.retrieval_scope = request.retrieval_scope;
            existing.created_by = request.created_by;
            existing.updated_at = "2026-08-18T00:00:00Z".to_string();
            existing.resource = Some(resource);
            self.attachment_idempotency
                .write()
                .await
                .insert(request.idempotency_key, existing.id.clone());
            return Ok(existing.clone());
        }

        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "bookway:route-node-resource:{}:{}:{}:{}",
                request.route_id,
                request.action_node_id,
                request.resource_id,
                request.idempotency_key
            )
            .as_bytes(),
        )
        .to_string();
        let now = "2026-08-18T00:00:00Z".to_string();
        let attachment = pb::RouteNodeResourceAttachment {
            id: id.clone(),
            route_id: request.route_id,
            action_node_id: request.action_node_id,
            resource_id: request.resource_id,
            kind: request.kind as i32,
            title_override: request.title_override,
            note: request.note,
            sort_rank: request.sort_rank,
            rag_enabled: request.rag_enabled,
            embedding_collection: request.embedding_collection,
            retrieval_scope: request.retrieval_scope,
            created_by: request.created_by,
            created_at: now.clone(),
            updated_at: now,
            resource: Some(resource),
        };
        attachments.push(attachment.clone());
        self.attachment_idempotency
            .write()
            .await
            .insert(request.idempotency_key, id);
        Ok(attachment)
    }

    async fn detach_node_resource(
        &self,
        route_id: &str,
        action_node_id: &str,
        attachment_id: &str,
    ) -> Result<bool, RepositoryError> {
        let mut attachments = self.attachments.write().await;
        let Some(attachment) = attachments.iter_mut().find(|attachment| {
            attachment.id == attachment_id
                && attachment.route_id == route_id
                && attachment.action_node_id == action_node_id
        }) else {
            return Ok(false);
        };
        if attachment.updated_at.starts_with("archived:") {
            return Ok(false);
        }
        attachment.updated_at = "archived:2026-08-18T00:00:00Z".to_string();
        attachment.resource = None;
        self.rag_embeddings.write().await.remove(attachment_id);
        Ok(true)
    }

    async fn upsert_rag_embedding(
        &self,
        attachment: &pb::RouteNodeResourceAttachment,
        embedding_model: &str,
        embedding: Vec<f32>,
    ) -> Result<(), RepositoryError> {
        validate_embedding(embedding_model, &embedding)?;
        if attachment.id.trim().is_empty()
            || attachment.route_id.trim().is_empty()
            || attachment.action_node_id.trim().is_empty()
            || attachment.embedding_collection.trim().is_empty()
        {
            return Err(RepositoryError::Invalid(
                "RAG embedding attachment scope is incomplete".to_string(),
            ));
        }
        self.rag_embeddings.write().await.insert(
            attachment.id.clone(),
            RagEmbedding {
                route_id: attachment.route_id.clone(),
                action_node_id: attachment.action_node_id.clone(),
                embedding_collection: attachment.embedding_collection.clone(),
                embedding_model: embedding_model.to_string(),
                embedding,
            },
        );
        Ok(())
    }

    async fn search_rag_embeddings(
        &self,
        route_id: &str,
        action_node_id: &str,
        embedding_collection: &str,
        embedding_model: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<RagVectorHit>, RepositoryError> {
        validate_embedding(embedding_model, query)?;
        let mut hits = self
            .rag_embeddings
            .read()
            .await
            .iter()
            .filter(|(_, embedding)| {
                embedding.route_id == route_id
                    && embedding.action_node_id == action_node_id
                    && embedding.embedding_collection == embedding_collection
                    && embedding.embedding_model == embedding_model
            })
            .filter_map(|(attachment_id, embedding)| {
                cosine_similarity(query, &embedding.embedding).map(|relevance| RagVectorHit {
                    attachment_id: attachment_id.clone(),
                    relevance,
                })
            })
            .collect::<Vec<_>>();
        sort_rag_hits(&mut hits);
        hits.truncate(limit);
        Ok(hits)
    }
}

#[derive(FromRow)]
struct ResourceRow {
    id: String,
    title: String,
    kind: String,
    provider: String,
    summary: String,
    url: String,
    license: String,
    version: String,
    citation: String,
    topics: Vec<String>,
    status: String,
    published_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(FromRow)]
struct AttachmentRow {
    id: String,
    route_id: String,
    action_node_id: String,
    resource_id: String,
    kind: String,
    title_override: String,
    note: String,
    sort_rank: i32,
    rag_enabled: bool,
    embedding_collection: String,
    retrieval_scope: String,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(FromRow)]
struct RagEmbeddingRow {
    attachment_id: String,
    embedding: Vec<f32>,
}

pub(crate) struct PostgresResourceRepository {
    pool: sqlx::PgPool,
}

impl PostgresResourceRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResourceRepository for PostgresResourceRepository {
    async fn search(
        &self,
        request: &pb::SearchRequest,
    ) -> Result<pb::SearchResponse, RepositoryError> {
        let offset = parse_cursor(&request.cursor)?;
        let limit = i64::from(request.limit.unwrap_or(20).clamp(1, 50));
        let kind = request
            .kind
            .and_then(|kind| pb::ResourceKind::try_from(kind).ok())
            .filter(|kind| *kind != pb::ResourceKind::Unspecified)
            .map(kind_name);
        let pattern = format!("%{}%", escape_like(&request.query.trim().to_lowercase()));
        let topic = request.topic.trim().to_string();
        let rows = sqlx::query_as::<_, ResourceRow>("SELECT id,title,kind,provider,summary,url,license,version,citation,topics,status,published_at,updated_at FROM public_resources WHERE status='published' AND ($1='' OR search_text ILIKE $2 ESCAPE '\\') AND ($3::TEXT IS NULL OR kind=$3) AND ($4='' OR $4 = ANY(topics)) ORDER BY updated_at DESC,id DESC LIMIT $5 OFFSET $6")
            .bind(request.query.trim())
            .bind(pattern)
            .bind(kind)
            .bind(topic)
            .bind(limit + 1)
            .bind(i64::try_from(offset).unwrap_or(i64::MAX))
            .fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::Database)?;
        let has_more = rows.len() > usize::try_from(limit).unwrap_or(50);
        let items = rows
            .into_iter()
            .take(usize::try_from(limit).unwrap_or(50))
            .map(row_to_resource)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(pb::SearchResponse {
            items,
            next_cursor: has_more.then(|| {
                offset
                    .saturating_add(usize::try_from(limit).unwrap_or(50))
                    .to_string()
            }),
        })
    }

    async fn get(&self, resource_id: &str) -> Result<pb::Resource, RepositoryError> {
        sqlx::query_as::<_, ResourceRow>("SELECT id,title,kind,provider,summary,url,license,version,citation,topics,status,published_at,updated_at FROM public_resources WHERE id=$1 AND status='published'")
            .bind(resource_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(RepositoryError::Database)?
            .map(row_to_resource)
            .transpose()?
            .ok_or_else(|| RepositoryError::NotFound(resource_id.to_string()))
    }

    async fn list_node_resources(
        &self,
        route_id: &str,
        action_node_id: &str,
        include_archived: bool,
    ) -> Result<pb::ListNodeResourcesResponse, RepositoryError> {
        let rows = sqlx::query_as::<_, AttachmentRow>("SELECT id,route_id,action_node_id,resource_id,kind,title_override,note,sort_rank,rag_enabled,embedding_collection,retrieval_scope,created_by,created_at,updated_at FROM route_node_resource_attachments WHERE route_id=$1 AND action_node_id=$2 AND ($3 OR archived_at IS NULL) ORDER BY sort_rank ASC, created_at ASC, id ASC")
            .bind(route_id)
            .bind(action_node_id)
            .bind(include_archived)
            .fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::Database)?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let resource = self.get(&row.resource_id).await.ok();
            items.push(row_to_attachment(row, resource)?);
        }
        Ok(pb::ListNodeResourcesResponse { items })
    }

    async fn attach_node_resource(
        &self,
        request: NewNodeResourceAttachment,
    ) -> Result<pb::RouteNodeResourceAttachment, RepositoryError> {
        let resource = self.get(&request.resource_id).await?;
        if let Some(row) = sqlx::query_as::<_, AttachmentRow>("SELECT id,route_id,action_node_id,resource_id,kind,title_override,note,sort_rank,rag_enabled,embedding_collection,retrieval_scope,created_by,created_at,updated_at FROM route_node_resource_attachments WHERE idempotency_key=$1")
            .bind(&request.idempotency_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(RepositoryError::Database)?
        {
            if !attachment_row_matches_request(&row, &request) {
                return Err(RepositoryError::Conflict(
                    "idempotency key is already bound to a different resource attachment"
                        .to_string(),
                ));
            }
            return row_to_attachment(row, Some(resource));
        }

        let id = Uuid::new_v4().to_string();
        let kind = attachment_kind_name(request.kind);
        let row = sqlx::query_as::<_, AttachmentRow>("INSERT INTO route_node_resource_attachments (id,route_id,action_node_id,resource_id,kind,title_override,note,sort_rank,rag_enabled,embedding_collection,retrieval_scope,created_by,idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (route_id, action_node_id, resource_id) WHERE archived_at IS NULL DO UPDATE SET kind=EXCLUDED.kind,title_override=EXCLUDED.title_override,note=EXCLUDED.note,sort_rank=EXCLUDED.sort_rank,rag_enabled=EXCLUDED.rag_enabled,embedding_collection=EXCLUDED.embedding_collection,retrieval_scope=EXCLUDED.retrieval_scope,created_by=EXCLUDED.created_by,updated_at=now() RETURNING id,route_id,action_node_id,resource_id,kind,title_override,note,sort_rank,rag_enabled,embedding_collection,retrieval_scope,created_by,created_at,updated_at")
            .bind(id)
            .bind(&request.route_id)
            .bind(&request.action_node_id)
            .bind(&request.resource_id)
            .bind(kind)
            .bind(&request.title_override)
            .bind(&request.note)
            .bind(request.sort_rank)
            .bind(request.rag_enabled)
            .bind(&request.embedding_collection)
            .bind(&request.retrieval_scope)
            .bind(&request.created_by)
            .bind(&request.idempotency_key)
            .fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::Database)?;
        row_to_attachment(row, Some(resource))
    }

    async fn detach_node_resource(
        &self,
        route_id: &str,
        action_node_id: &str,
        attachment_id: &str,
    ) -> Result<bool, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let result = sqlx::query("UPDATE route_node_resource_attachments SET archived_at=now(), updated_at=now() WHERE id=$1 AND route_id=$2 AND action_node_id=$3 AND archived_at IS NULL")
            .bind(attachment_id)
            .bind(route_id)
            .bind(action_node_id)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        let detached = result.rows_affected() > 0;
        if detached {
            sqlx::query("DELETE FROM route_node_resource_embeddings WHERE attachment_id=$1")
                .bind(attachment_id)
                .execute(&mut *transaction)
                .await
                .map_err(RepositoryError::Database)?;
        }
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(detached)
    }

    async fn upsert_rag_embedding(
        &self,
        attachment: &pb::RouteNodeResourceAttachment,
        embedding_model: &str,
        embedding: Vec<f32>,
    ) -> Result<(), RepositoryError> {
        validate_embedding(embedding_model, &embedding)?;
        if attachment.id.trim().is_empty()
            || attachment.route_id.trim().is_empty()
            || attachment.action_node_id.trim().is_empty()
            || attachment.embedding_collection.trim().is_empty()
        {
            return Err(RepositoryError::Invalid(
                "RAG embedding attachment scope is incomplete".to_string(),
            ));
        }
        sqlx::query("INSERT INTO route_node_resource_embeddings (attachment_id,route_id,action_node_id,embedding_collection,embedding_model,embedding) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (attachment_id) DO UPDATE SET route_id=EXCLUDED.route_id,action_node_id=EXCLUDED.action_node_id,embedding_collection=EXCLUDED.embedding_collection,embedding_model=EXCLUDED.embedding_model,embedding=EXCLUDED.embedding,updated_at=now()")
            .bind(&attachment.id)
            .bind(&attachment.route_id)
            .bind(&attachment.action_node_id)
            .bind(&attachment.embedding_collection)
            .bind(embedding_model)
            .bind(embedding)
            .execute(&self.pool)
            .await
            .map_err(RepositoryError::Database)?;
        Ok(())
    }

    async fn search_rag_embeddings(
        &self,
        route_id: &str,
        action_node_id: &str,
        embedding_collection: &str,
        embedding_model: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<RagVectorHit>, RepositoryError> {
        validate_embedding(embedding_model, query)?;
        let rows = sqlx::query_as::<_, RagEmbeddingRow>("SELECT attachment_id,embedding FROM route_node_resource_embeddings WHERE route_id=$1 AND action_node_id=$2 AND embedding_collection=$3 AND embedding_model=$4")
            .bind(route_id)
            .bind(action_node_id)
            .bind(embedding_collection)
            .bind(embedding_model)
            .fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::Database)?;
        let mut hits = rows
            .into_iter()
            .filter_map(|row| {
                cosine_similarity(query, &row.embedding).map(|relevance| RagVectorHit {
                    attachment_id: row.attachment_id,
                    relevance,
                })
            })
            .collect::<Vec<_>>();
        sort_rag_hits(&mut hits);
        hits.truncate(limit);
        Ok(hits)
    }
}

fn row_to_resource(row: ResourceRow) -> Result<pb::Resource, RepositoryError> {
    Ok(pb::Resource {
        id: row.id,
        title: row.title,
        kind: parse_kind(&row.kind)?,
        provider: row.provider,
        summary: row.summary,
        url: row.url,
        license: row.license,
        version: row.version,
        citation: row.citation,
        topics: row.topics,
        status: parse_status(&row.status)?,
        published_at: format_time(row.published_at),
        updated_at: format_time(row.updated_at),
    })
}

fn row_to_attachment(
    row: AttachmentRow,
    resource: Option<pb::Resource>,
) -> Result<pb::RouteNodeResourceAttachment, RepositoryError> {
    Ok(pb::RouteNodeResourceAttachment {
        id: row.id,
        route_id: row.route_id,
        action_node_id: row.action_node_id,
        resource_id: row.resource_id,
        kind: parse_attachment_kind(&row.kind)?,
        title_override: row.title_override,
        note: row.note,
        sort_rank: row.sort_rank,
        rag_enabled: row.rag_enabled,
        embedding_collection: row.embedding_collection,
        retrieval_scope: row.retrieval_scope,
        created_by: row.created_by,
        created_at: format_time(row.created_at),
        updated_at: format_time(row.updated_at),
        resource,
    })
}

fn attachment_matches_request(
    attachment: &pb::RouteNodeResourceAttachment,
    request: &NewNodeResourceAttachment,
) -> bool {
    attachment.route_id == request.route_id
        && attachment.action_node_id == request.action_node_id
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

fn attachment_row_matches_request(
    row: &AttachmentRow,
    request: &NewNodeResourceAttachment,
) -> bool {
    row.route_id == request.route_id
        && row.action_node_id == request.action_node_id
        && row.resource_id == request.resource_id
        && row.kind == attachment_kind_name(request.kind)
        && row.title_override == request.title_override
        && row.note == request.note
        && row.sort_rank == request.sort_rank
        && row.rag_enabled == request.rag_enabled
        && row.embedding_collection == request.embedding_collection
        && row.retrieval_scope == request.retrieval_scope
        && row.created_by == request.created_by
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

fn parse_cursor(cursor: &str) -> Result<usize, RepositoryError> {
    if cursor.trim().is_empty() {
        return Ok(0);
    }
    cursor
        .trim()
        .parse()
        .map_err(|_| RepositoryError::Invalid("cursor must be a non-negative offset".to_string()))
}

fn validate_embedding(model: &str, embedding: &[f32]) -> Result<(), RepositoryError> {
    if model.trim().is_empty() || model.chars().count() > 80 {
        return Err(RepositoryError::Invalid(
            "RAG embedding model is invalid".to_string(),
        ));
    }
    if !(8..=4096).contains(&embedding.len())
        || embedding.iter().any(|value| !value.is_finite())
        || embedding.iter().all(|value| *value == 0.0)
    {
        return Err(RepositoryError::Invalid(
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
fn attachment_kind_name(kind: pb::AttachmentKind) -> &'static str {
    match kind {
        pb::AttachmentKind::Document => "document",
        pb::AttachmentKind::Pdf => "pdf",
        pb::AttachmentKind::ExternalLink => "external_link",
        pb::AttachmentKind::ToolChecklist => "tool_checklist",
        pb::AttachmentKind::AiActionGuide => "ai_action_guide",
        pb::AttachmentKind::RagCorpus => "rag_corpus",
        pb::AttachmentKind::Unspecified => "",
    }
}
fn parse_kind(value: &str) -> Result<i32, RepositoryError> {
    Ok(match value {
        "book" => pb::ResourceKind::Book,
        "course" => pb::ResourceKind::Course,
        "tool" => pb::ResourceKind::Tool,
        "article" => pb::ResourceKind::Article,
        "podcast" => pb::ResourceKind::Podcast,
        _ => {
            return Err(RepositoryError::Invalid(format!(
                "invalid resource kind {value}"
            )));
        }
    } as i32)
}
fn parse_status(value: &str) -> Result<i32, RepositoryError> {
    Ok(match value {
        "published" => pb::ResourceStatus::Published,
        "archived" => pb::ResourceStatus::Archived,
        _ => {
            return Err(RepositoryError::Invalid(format!(
                "invalid resource status {value}"
            )));
        }
    } as i32)
}
fn parse_attachment_kind(value: &str) -> Result<i32, RepositoryError> {
    Ok(match value {
        "document" => pb::AttachmentKind::Document,
        "pdf" => pb::AttachmentKind::Pdf,
        "external_link" => pb::AttachmentKind::ExternalLink,
        "tool_checklist" => pb::AttachmentKind::ToolChecklist,
        "ai_action_guide" => pb::AttachmentKind::AiActionGuide,
        "rag_corpus" => pb::AttachmentKind::RagCorpus,
        _ => {
            return Err(RepositoryError::Invalid(format!(
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
    use super::{MemoryResourceRepository, ResourceRepository};
    use bookway_knowledge_catalog_api::pb;

    #[tokio::test]
    async fn memory_catalog_filters_published_resources_by_kind_and_topic() {
        let repository = MemoryResourceRepository::seeded();
        let response = repository
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
        let repository = MemoryResourceRepository::seeded();
        let first = repository
            .search(&pb::SearchRequest {
                limit: Some(1),
                ..Default::default()
            })
            .await
            .expect("first page should succeed");
        assert_eq!(first.items.len(), 1);
        assert_eq!(first.next_cursor.as_deref(), Some("1"));

        let second = repository
            .search(&pb::SearchRequest {
                cursor: first.next_cursor.unwrap_or_default(),
                limit: Some(10),
                ..Default::default()
            })
            .await
            .expect("second page should succeed");
        assert_eq!(second.items.len(), 2);
        assert!(repository.get("missing-resource").await.is_err());
    }
}
