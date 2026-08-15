use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

use super::api::CommentDto;
use bookway_api::{
    AuditDecisionDto, ContentAuditRequest, ContentAuditResponse, ContentStatusDto,
    CreateCommentResult,
};
use bookway_content_audit::api::pb::{self, content_audit_client::ContentAuditClient};

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("parent comment {0} was not found on this post")]
    ParentNotFound(String),
    #[error("comment reply nesting exceeds the maximum depth")]
    ReplyDepthExceeded,
    #[error("idempotency key was already used for a different comment")]
    IdempotencyConflict,
    #[error("comment {0} was not found")]
    NotFound(String),
    #[error("stored comment has invalid moderation state {0}")]
    InvalidModerationState(String),
    #[error("stored comment has invalid reply hierarchy")]
    InvalidReplyHierarchy,
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

/// A bounded thread avoids unbounded recursive rendering and ancestor scans.
/// Root comments have depth zero, so a thread can contain three reply levels.
pub(crate) const MAX_REPLY_DEPTH: usize = 3;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CommentCursor {
    pub(crate) created_at: OffsetDateTime,
    pub(crate) id: String,
}

impl CommentCursor {
    pub(crate) fn decode(value: &str) -> Option<Self> {
        let (created_at, id) = value.split_once('|')?;
        let created_at = OffsetDateTime::parse(created_at, &Rfc3339).ok()?;
        (!id.is_empty()).then(|| Self {
            created_at,
            id: id.to_string(),
        })
    }

    pub(crate) fn from_comment(comment: &CommentDto) -> Option<Self> {
        Some(Self {
            created_at: OffsetDateTime::parse(&comment.created_at, &Rfc3339).ok()?,
            id: comment.id.clone(),
        })
    }

    pub(crate) fn encode(&self) -> String {
        format!("{}|{}", format_timestamp(self.created_at), self.id)
    }
}

#[async_trait]
pub(crate) trait CommentRepository: Send + Sync {
    async fn list(
        &self,
        post_id: &str,
        cursor: Option<&CommentCursor>,
        limit: usize,
        excluded_author_ids: &[String],
    ) -> Result<Vec<CommentDto>, RepositoryError>;
    async fn create(
        &self,
        input: CreateCommentInput<'_>,
    ) -> Result<CreateCommentResult, RepositoryError>;
    async fn delete(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), RepositoryError>;
    async fn set_moderation_status(
        &self,
        comment_id: &str,
        status: ContentStatusDto,
    ) -> Result<CommentDto, RepositoryError>;
}

pub(crate) struct CreateCommentInput<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) post_id: &'a str,
    pub(crate) author_name: &'a str,
    pub(crate) body: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) excluded_author_ids: &'a [String],
    pub(crate) idempotency_key: Option<String>,
}

#[async_trait]
pub(crate) trait CommentAuditor: Send + Sync {
    async fn audit(&self, request: ContentAuditRequest) -> Result<ContentAuditResponse, String>;
}

pub(crate) struct LocalCommentAuditor;

#[async_trait]
impl CommentAuditor for LocalCommentAuditor {
    async fn audit(&self, _request: ContentAuditRequest) -> Result<ContentAuditResponse, String> {
        Ok(ContentAuditResponse {
            decision: AuditDecisionDto::Approved,
            risk_score: 0.0,
            reasons: Vec::new(),
            provider: "local-development".to_string(),
        })
    }
}

pub(crate) struct UnavailableCommentAuditor;

#[async_trait]
impl CommentAuditor for UnavailableCommentAuditor {
    async fn audit(&self, _request: ContentAuditRequest) -> Result<ContentAuditResponse, String> {
        Err("CONTENT_AUDIT_GRPC_URL is required for persistent comment publishing".to_string())
    }
}

pub(crate) struct GrpcCommentAuditor {
    client: ContentAuditClient<tonic::transport::Channel>,
}

impl GrpcCommentAuditor {
    pub(crate) async fn connect(base_url: String) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            client: ContentAuditClient::connect(base_url).await?,
        })
    }
}

#[async_trait]
impl CommentAuditor for GrpcCommentAuditor {
    async fn audit(&self, request: ContentAuditRequest) -> Result<ContentAuditResponse, String> {
        let mut client = self.client.clone();
        let response = client
            .audit(pb::AuditRequest {
                request_json: serde_json::to_string(&request).map_err(|error| error.to_string())?,
            })
            .await
            .map_err(|error| error.to_string())?
            .into_inner();
        serde_json::from_str(&response.response_json).map_err(|error| error.to_string())
    }
}

#[derive(Default)]
pub(crate) struct MemoryCommentRepository {
    state: RwLock<MemoryCommentState>,
}

#[derive(Default)]
struct MemoryCommentState {
    comments: HashMap<String, Vec<CommentDto>>,
    requests: HashMap<(String, String), String>,
}

fn public_comment_items(items: &[CommentDto], excluded_author_ids: &[String]) -> Vec<CommentDto> {
    let comments_by_id = items
        .iter()
        .map(|comment| (comment.id.as_str(), comment))
        .collect::<HashMap<_, _>>();
    let mut tombstone_ids = HashSet::new();

    for comment in items.iter().filter(|comment| {
        comment.status == ContentStatusDto::Published
            && !excluded_author_ids.contains(&comment.author_id)
    }) {
        let mut parent_id = comment.parent_id.as_deref();
        let mut visited = HashSet::new();
        while let Some(parent_comment_id) = parent_id {
            if !visited.insert(parent_comment_id) {
                break;
            }
            let Some(parent) = comments_by_id.get(parent_comment_id) else {
                break;
            };
            if parent.status != ContentStatusDto::Deleted
                || excluded_author_ids.contains(&parent.author_id)
            {
                break;
            }
            tombstone_ids.insert(parent.id.as_str());
            parent_id = parent.parent_id.as_deref();
        }
    }

    items
        .iter()
        .filter_map(|comment| {
            if comment.status == ContentStatusDto::Published
                && !excluded_author_ids.contains(&comment.author_id)
            {
                Some(comment.clone())
            } else if comment.status == ContentStatusDto::Deleted
                && !excluded_author_ids.contains(&comment.author_id)
                && tombstone_ids.contains(comment.id.as_str())
            {
                Some(deleted_comment(
                    comment.id.clone(),
                    comment.post_id.clone(),
                    comment.parent_id.clone(),
                    comment.created_at.clone(),
                ))
            } else {
                None
            }
        })
        .collect()
}

#[async_trait]
impl CommentRepository for MemoryCommentRepository {
    async fn list(
        &self,
        post_id: &str,
        cursor: Option<&CommentCursor>,
        limit: usize,
        excluded_author_ids: &[String],
    ) -> Result<Vec<CommentDto>, RepositoryError> {
        let mut items = self
            .state
            .read()
            .await
            .comments
            .get(post_id)
            .cloned()
            .unwrap_or_default();
        items = public_comment_items(&items, excluded_author_ids);
        items.sort_by_key(CommentCursor::from_comment);
        if let Some(cursor) = cursor {
            items.retain(|comment| {
                CommentCursor::from_comment(comment).is_some_and(|value| value > cursor.clone())
            });
        }
        items.truncate(limit);
        Ok(items)
    }

    async fn create(
        &self,
        input: CreateCommentInput<'_>,
    ) -> Result<CreateCommentResult, RepositoryError> {
        let CreateCommentInput {
            user_id,
            post_id,
            author_name,
            body,
            parent_id,
            excluded_author_ids,
            idempotency_key,
        } = input;
        let mut state = self.state.write().await;
        if let Some(key) = idempotency_key.as_deref()
            && let Some(comment_id) = state.requests.get(&(user_id.to_string(), key.to_string()))
        {
            let existing = state
                .comments
                .values()
                .flatten()
                .find(|comment| comment.id == *comment_id)
                .cloned()
                .ok_or(RepositoryError::IdempotencyConflict)?;
            if existing.post_id == post_id
                && existing.body == body
                && existing.parent_id == parent_id
            {
                return Ok(CreateCommentResult {
                    parent_author_id: memory_parent_author_id(
                        &state,
                        existing.parent_id.as_deref(),
                    ),
                    comment: existing,
                });
            }
            return Err(RepositoryError::IdempotencyConflict);
        }
        let parent_author_id = if let Some(parent_id) = parent_id.as_deref() {
            let items = state
                .comments
                .get(post_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let parent = items
                .iter()
                .find(|comment| {
                    comment.id == parent_id
                        && comment.status == ContentStatusDto::Published
                        && !excluded_author_ids.contains(&comment.author_id)
                })
                .ok_or_else(|| RepositoryError::ParentNotFound(parent_id.to_string()))?;
            if memory_comment_depth(items, parent)? >= MAX_REPLY_DEPTH {
                return Err(RepositoryError::ReplyDepthExceeded);
            }
            Some(parent.author_id.clone())
        } else {
            None
        };
        let comment = CommentDto {
            id: uuid::Uuid::now_v7().to_string(),
            post_id: post_id.to_string(),
            author_id: user_id.to_string(),
            author_name: author_name.to_string(),
            body,
            parent_id,
            like_count: 0,
            created_at: now_timestamp(),
            status: ContentStatusDto::Reviewing,
        };
        state
            .comments
            .entry(post_id.to_string())
            .or_default()
            .push(comment.clone());
        if let Some(key) = idempotency_key {
            state
                .requests
                .insert((user_id.to_string(), key), comment.id.clone());
        }
        Ok(CreateCommentResult {
            comment,
            parent_author_id,
        })
    }

    async fn delete(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state.write().await;
        let comment = state
            .comments
            .get_mut(post_id)
            .into_iter()
            .flatten()
            .find(|comment| comment.id == comment_id && comment.author_id == user_id)
            .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
        comment.status = ContentStatusDto::Deleted;
        Ok(())
    }

    async fn set_moderation_status(
        &self,
        comment_id: &str,
        status: ContentStatusDto,
    ) -> Result<CommentDto, RepositoryError> {
        if moderation_state_name(status).is_none() {
            return Err(RepositoryError::InvalidModerationState(format!(
                "{status:?}"
            )));
        }
        let mut state = self.state.write().await;
        let comment = state
            .comments
            .values_mut()
            .flatten()
            .find(|comment| comment.id == comment_id)
            .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
        comment.status = status;
        Ok(comment.clone())
    }
}

pub(crate) struct PostgresCommentRepository {
    pool: sqlx::PgPool,
}

impl PostgresCommentRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CommentRepository for PostgresCommentRepository {
    async fn list(
        &self,
        post_id: &str,
        cursor: Option<&CommentCursor>,
        limit: usize,
        excluded_author_ids: &[String],
    ) -> Result<Vec<CommentDto>, RepositoryError> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                String,
                i64,
                time::OffsetDateTime,
                String,
                bool,
            ),
        >(
            "WITH RECURSIVE visible_published AS (SELECT id, parent_id FROM comments WHERE post_id = $1 AND deleted_at IS NULL AND moderation_state = 'published' AND (cardinality($4::TEXT[]) = 0 OR author_id <> ALL($4::TEXT[]))), visible_deleted_ancestors (id, parent_id) AS (SELECT parent.id, parent.parent_id FROM comments AS parent JOIN visible_published AS child ON child.parent_id = parent.id WHERE parent.post_id = $1 AND parent.deleted_at IS NOT NULL AND (cardinality($4::TEXT[]) = 0 OR parent.author_id <> ALL($4::TEXT[])) UNION SELECT parent.id, parent.parent_id FROM comments AS parent JOIN visible_deleted_ancestors AS descendant ON descendant.parent_id = parent.id WHERE parent.post_id = $1 AND parent.deleted_at IS NOT NULL AND (cardinality($4::TEXT[]) = 0 OR parent.author_id <> ALL($4::TEXT[]))) SELECT id, author_id, parent_id, body, like_count, created_at, moderation_state, deleted_at IS NOT NULL FROM comments WHERE post_id = $1 AND ((deleted_at IS NULL AND moderation_state = 'published' AND (cardinality($4::TEXT[]) = 0 OR author_id <> ALL($4::TEXT[]))) OR (deleted_at IS NOT NULL AND id IN (SELECT id FROM visible_deleted_ancestors))) AND ($2::TIMESTAMPTZ IS NULL OR (created_at, id) > ($2::TIMESTAMPTZ, $3)) ORDER BY created_at ASC, id ASC LIMIT $5",
        )
        .bind(post_id)
        .bind(cursor.map(|value| format_timestamp(value.created_at)))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(excluded_author_ids)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(
                |(
                    id,
                    author_id,
                    parent_id,
                    body,
                    like_count,
                    created_at,
                    moderation_state,
                    deleted,
                )| {
                    if deleted {
                        Ok(deleted_comment(
                            id,
                            post_id.to_string(),
                            parent_id,
                            format_timestamp(created_at),
                        ))
                    } else {
                        Ok(CommentDto {
                            id,
                            post_id: post_id.to_string(),
                            author_id: author_id.clone(),
                            author_name: author_id,
                            body,
                            parent_id,
                            like_count: like_count.max(0) as u64,
                            created_at: format_timestamp(created_at),
                            status: moderation_status(&moderation_state)?,
                        })
                    }
                },
            )
            .collect::<Result<Vec<_>, RepositoryError>>()
    }

    async fn create(
        &self,
        input: CreateCommentInput<'_>,
    ) -> Result<CreateCommentResult, RepositoryError> {
        let CreateCommentInput {
            user_id,
            post_id,
            author_name,
            body,
            parent_id,
            excluded_author_ids,
            idempotency_key,
        } = input;
        let mut tx = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if let Some(key) = idempotency_key.as_deref()
            && let Some(existing) = find_idempotent_comment(&mut tx, user_id, key).await?
        {
            if existing.comment.post_id == post_id
                && existing.request_body == body
                && existing.comment.parent_id == parent_id
            {
                let parent_author_id =
                    find_parent_author_id(&mut tx, existing.comment.parent_id.as_deref()).await?;
                tx.commit().await.map_err(RepositoryError::Database)?;
                return Ok(CreateCommentResult {
                    comment: existing.comment,
                    parent_author_id,
                });
            }
            return Err(RepositoryError::IdempotencyConflict);
        }
        let (parent_author_id, depth) = if let Some(parent) = parent_id.as_deref() {
            let (parent_author_id, parent_depth) = sqlx::query_as::<_, (String, i16)>(
                "SELECT author_id, depth FROM comments WHERE id = $1 AND post_id = $2 AND deleted_at IS NULL AND moderation_state = 'published' AND (cardinality($3::TEXT[]) = 0 OR author_id <> ALL($3::TEXT[]))",
            )
            .bind(parent)
            .bind(post_id)
            .bind(excluded_author_ids)
            .fetch_optional(&mut *tx)
            .await
            .map_err(RepositoryError::Database)?
            .ok_or_else(|| RepositoryError::ParentNotFound(parent.to_string()))?
            ;
            if parent_depth < 0 {
                return Err(RepositoryError::InvalidReplyHierarchy);
            }
            if parent_depth as usize >= MAX_REPLY_DEPTH {
                return Err(RepositoryError::ReplyDepthExceeded);
            }
            (Some(parent_author_id), parent_depth + 1)
        } else {
            (None, 0)
        };
        let id = uuid::Uuid::now_v7().to_string();
        let inserted = sqlx::query_as::<_, (time::OffsetDateTime, String)>(
            "INSERT INTO comments (id, post_id, author_id, parent_id, body, depth, moderation_state, client_request_id) VALUES ($1,$2,$3,$4,$5,$6,'reviewing',$7) ON CONFLICT (author_id, client_request_id) WHERE client_request_id IS NOT NULL DO NOTHING RETURNING created_at, moderation_state",
        )
        .bind(&id).bind(post_id).bind(user_id).bind(&parent_id).bind(&body)
        .bind(depth).bind(&idempotency_key)
        .fetch_optional(&mut *tx).await.map_err(RepositoryError::Database)?;
        let Some((created_at, moderation_state)) = inserted else {
            let existing = find_idempotent_comment(
                &mut tx,
                user_id,
                idempotency_key.as_deref().unwrap_or_default(),
            )
            .await?
            .ok_or(RepositoryError::IdempotencyConflict)?;
            if existing.comment.post_id != post_id
                || existing.request_body != body
                || existing.comment.parent_id != parent_id
            {
                return Err(RepositoryError::IdempotencyConflict);
            }
            let parent_author_id =
                find_parent_author_id(&mut tx, existing.comment.parent_id.as_deref()).await?;
            tx.commit().await.map_err(RepositoryError::Database)?;
            return Ok(CreateCommentResult {
                comment: existing.comment,
                parent_author_id,
            });
        };
        tx.commit().await.map_err(RepositoryError::Database)?;
        Ok(CreateCommentResult {
            comment: CommentDto {
                id,
                post_id: post_id.to_string(),
                author_id: user_id.to_string(),
                author_name: author_name.to_string(),
                body,
                parent_id,
                like_count: 0,
                created_at: format_timestamp(created_at),
                status: moderation_status(&moderation_state)?,
            },
            parent_author_id,
        })
    }

    async fn delete(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query_scalar::<_, String>(
            "UPDATE comments SET deleted_at = COALESCE(deleted_at, now()), updated_at = CASE WHEN deleted_at IS NULL THEN now() ELSE updated_at END WHERE id = $1 AND post_id = $2 AND author_id = $3 RETURNING id",
        )
        .bind(comment_id)
        .bind(post_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
        Ok(())
    }

    async fn set_moderation_status(
        &self,
        comment_id: &str,
        status: ContentStatusDto,
    ) -> Result<CommentDto, RepositoryError> {
        let state = moderation_state_name(status)
            .ok_or_else(|| RepositoryError::InvalidModerationState(format!("{status:?}")))?;
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                String,
                i64,
                time::OffsetDateTime,
                String,
            ),
        >(
            "UPDATE comments SET moderation_state = $2, updated_at = now() WHERE id = $1 AND deleted_at IS NULL RETURNING id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state",
        )
        .bind(comment_id)
        .bind(state)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::NotFound(comment_id.to_string()))?;
        comment_from_row(row)
    }
}

fn memory_parent_author_id(state: &MemoryCommentState, parent_id: Option<&str>) -> Option<String> {
    let parent_id = parent_id?;
    state
        .comments
        .values()
        .flatten()
        .find(|comment| comment.id == parent_id)
        .map(|comment| comment.author_id.clone())
}

fn memory_comment_depth(
    items: &[CommentDto],
    start: &CommentDto,
) -> Result<usize, RepositoryError> {
    let mut current = start;
    let mut depth = 0;
    while let Some(parent_id) = current.parent_id.as_deref() {
        if depth >= MAX_REPLY_DEPTH {
            return Ok(depth);
        }
        depth += 1;
        current = items
            .iter()
            .find(|comment| comment.id == parent_id)
            .ok_or_else(|| RepositoryError::ParentNotFound(parent_id.to_string()))?;
    }
    Ok(depth)
}

async fn find_parent_author_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    parent_id: Option<&str>,
) -> Result<Option<String>, RepositoryError> {
    let Some(parent_id) = parent_id else {
        return Ok(None);
    };
    sqlx::query_scalar::<_, String>("SELECT author_id FROM comments WHERE id = $1")
        .bind(parent_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(RepositoryError::Database)
}

async fn find_idempotent_comment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
    key: &str,
) -> Result<Option<IdempotentComment>, RepositoryError> {
    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            String,
            i64,
            time::OffsetDateTime,
            String,
            bool,
        ),
    >(
        "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state, deleted_at IS NOT NULL FROM comments WHERE author_id = $1 AND client_request_id = $2",
    )
    .bind(user_id)
    .bind(key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(RepositoryError::Database)?;
    row.map(
        |(
            id,
            post_id,
            author_id,
            parent_id,
            body,
            like_count,
            created_at,
            moderation_state,
            deleted,
        )| {
            let request_body = body.clone();
            let comment = comment_from_stored_row((
                id,
                post_id,
                author_id,
                parent_id,
                body,
                like_count,
                created_at,
                moderation_state,
                deleted,
            ))?;
            Ok(IdempotentComment {
                comment,
                request_body,
            })
        },
    )
    .transpose()
}

struct IdempotentComment {
    comment: CommentDto,
    request_body: String,
}

fn comment_from_stored_row(
    (id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state, deleted): (
        String,
        String,
        String,
        Option<String>,
        String,
        i64,
        time::OffsetDateTime,
        String,
        bool,
    ),
) -> Result<CommentDto, RepositoryError> {
    if deleted {
        return Ok(deleted_comment(
            id,
            post_id,
            parent_id,
            format_timestamp(created_at),
        ));
    }
    comment_from_row((
        id,
        post_id,
        author_id,
        parent_id,
        body,
        like_count,
        created_at,
        moderation_state,
    ))
}

fn deleted_comment(
    id: String,
    post_id: String,
    parent_id: Option<String>,
    created_at: String,
) -> CommentDto {
    CommentDto {
        id,
        post_id,
        author_id: String::new(),
        author_name: "已删除用户".to_string(),
        body: "该评论已删除".to_string(),
        parent_id,
        like_count: 0,
        created_at,
        status: ContentStatusDto::Deleted,
    }
}

fn comment_from_row(
    (id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state): (
        String,
        String,
        String,
        Option<String>,
        String,
        i64,
        time::OffsetDateTime,
        String,
    ),
) -> Result<CommentDto, RepositoryError> {
    Ok(CommentDto {
        id,
        post_id,
        author_name: author_id.clone(),
        author_id,
        body,
        parent_id,
        like_count: like_count.max(0) as u64,
        created_at: format_timestamp(created_at),
        status: moderation_status(&moderation_state)?,
    })
}

fn moderation_status(value: &str) -> Result<ContentStatusDto, RepositoryError> {
    match value {
        "reviewing" => Ok(ContentStatusDto::Reviewing),
        "published" => Ok(ContentStatusDto::Published),
        "restricted" => Ok(ContentStatusDto::Restricted),
        value => Err(RepositoryError::InvalidModerationState(value.to_string())),
    }
}

fn moderation_state_name(value: ContentStatusDto) -> Option<&'static str> {
    match value {
        ContentStatusDto::Reviewing => Some("reviewing"),
        ContentStatusDto::Published => Some("published"),
        ContentStatusDto::Restricted => Some("restricted"),
        ContentStatusDto::Draft | ContentStatusDto::Deleted => None,
    }
}

fn now_timestamp() -> String {
    format_timestamp(OffsetDateTime::now_utc())
}

fn format_timestamp(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
