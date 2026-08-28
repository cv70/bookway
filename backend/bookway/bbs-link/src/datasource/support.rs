use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum DaoError {
    #[error("content {0} was not found")]
    NotFound(String),
    #[error("idempotency key {0} is already bound to another operation")]
    IdempotencyConflict(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("stored content is invalid: {0}")]
    Serialization(#[source] serde_json::Error),
    #[error("content version conflict")]
    VersionConflict,
    #[error("stored content has an invalid timestamp: {0}")]
    InvalidTimestamp(String),
    #[error("stored content is invalid: {0}")]
    InvalidContent(String),
}

#[async_trait]
pub(crate) trait ContentDao: Send + Sync {
    async fn list(&self, query: &pb::ListRequest) -> Result<pb::ContentPage, DaoError>;
    async fn get(&self, id: &str) -> Result<pb::Content, DaoError>;
    async fn published_by_idempotency_key(
        &self,
        user_id: &str,
        idempotency_key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Content>, DaoError>;
    async fn create(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, DaoError>;
    async fn update(&self, content: pb::Content) -> Result<pb::Content, DaoError>;
    async fn publish(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, DaoError>;
}

struct State {
    contents: Vec<pb::Content>,
    idempotency: HashMap<(String, String, String), IdempotencyRecord>,
}

struct IdempotencyRecord {
    content_id: String,
    request_fingerprint: String,
    response: Option<pb::Content>,
}

struct SeedContent<'a> {
    id: &'a str,
    author_name: &'a str,
    author_id: &'a str,
    title: &'a str,
    summary: &'a str,
    domain: pb::GrowthDomain,
    route_title: &'a str,
    route_duration: &'a str,
    join_count: u32,
    like_count: u32,
    freshness: f64,
    tags: &'a str,
    created_at: &'a str,
    cover_url: &'a str,
    avatar_url: &'a str,
}

fn seed(input: SeedContent<'_>) -> pb::Content {
    pb::Content {
        id: input.id.to_string(),
        post: Some(pb::PostSummary {
            id: input.id.to_string(),
            author_name: input.author_name.to_string(),
            author_avatar_url: input.avatar_url.to_string(),
            title: input.title.to_string(),
            summary: input.summary.to_string(),
            domain: input.domain as i32,
            cover_url: input.cover_url.to_string(),
            route_title: input.route_title.to_string(),
            route_duration: input.route_duration.to_string(),
            join_count: input.join_count,
            like_count: input.like_count,
            freshness: input.freshness,
            tags: input.tags.split(',').map(str::to_string).collect(),
            is_route: true,
            is_milestone: false,
            is_question: false,
            fork_count: 0,
        }),
        author_id: input.author_id.to_string(),
        content_type: pb::ContentType::Route as i32,
        status: pb::ContentStatus::Published as i32,
        body: input.summary.to_string(),
        media: vec![pb::ContentMedia {
            id: format!("{}-cover", input.id),
            url: input.cover_url.to_string(),
            kind: "image".to_string(),
            width: 1200,
            height: 900,
            duration_ms: None,
        }],
        topics: input.tags.split(',').map(str::to_string).collect(),
        created_at: input.created_at.to_string(),
        published_at: Some(input.created_at.to_string()),
        version: 1,
        quality_score: input.freshness * 0.4 + f64::from(input.like_count).ln_1p() / 10.0,
        route_template: Some(seed_route_template(&input)),
        milestone: None,
        accepted_answer_id: None,
        question_context: None,
        route_fork: None,
    }
}

fn seed_route_template(input: &SeedContent<'_>) -> pb::RouteTemplate {
    pb::RouteTemplate {
        intent: input.summary.to_string(),
        completion_criteria: format!("完成{}中的核心练习", input.route_title),
        stages: vec![pb::RouteTemplateStage {
            title: "从第一步开始".to_string(),
            detail: "先在自己的节奏里完成一次练习。".to_string(),
            completion_criteria: "完成至少一次行动并留下简短记录".to_string(),
        }],
        actions: vec![pb::RouteTemplateAction {
            id: format!("{}-start", input.id),
            title: input.route_title.to_string(),
            detail: input.summary.to_string(),
            estimated_minutes: 20,
            scheduled_label: "开始时".to_string(),
            stage_index: Some(0),
            scene_equipment: vec!["行动记录工具".to_string()],
        }],
        journey_type: pb::RouteTemplateKind::Project as i32,
    }
}

fn published_idempotency_response(
    idempotency_key: &str,
    request_fingerprint: &str,
    stored_fingerprint: String,
    response: Option<serde_json::Value>,
) -> Result<pb::Content, DaoError> {
    if stored_fingerprint != request_fingerprint {
        return Err(DaoError::IdempotencyConflict(idempotency_key.to_string()));
    }
    let response = response.ok_or_else(|| {
        DaoError::InvalidContent(
            "publish idempotency record is missing its response snapshot".to_string(),
        )
    })?;
    serde_json::from_value(response).map_err(DaoError::Serialization)
}

async fn update_content_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    content: &pb::Content,
) -> Result<(), DaoError> {
    let post = content.post.as_ref().ok_or_else(|| {
        DaoError::InvalidContent("content is missing its post summary".to_string())
    })?;
    let payload = serde_json::to_value(content).map_err(DaoError::Serialization)?;
    let published_at = parse_timestamp(content.published_at.as_deref())?;
    let updated = sqlx::query(
        "UPDATE content_items SET status=$2,title=$3,summary=$4,body=$5,cover_url=$6,version=$7,quality_score=$8,published_at=$9,payload=$10,updated_at=now() WHERE id=$1 AND deleted_at IS NULL AND version < $7",
    )
    .bind(&content.id)
    .bind(content_status_name(content.status)?)
    .bind(&post.title)
    .bind(&post.summary)
    .bind(&content.body)
    .bind(&post.cover_url)
    .bind(i64::from(content.version))
    .bind(content.quality_score)
    .bind(published_at)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .map_err(DaoError::Database)?;
    if updated.rows_affected() == 0 {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM content_items WHERE id=$1 AND deleted_at IS NULL)",
        )
        .bind(&content.id)
        .fetch_one(&mut **tx)
        .await
        .map_err(DaoError::Database)?;
        return Err(if exists {
            DaoError::VersionConflict
        } else {
            DaoError::NotFound(content.id.clone())
        });
    }
    replace_content_media(tx, content).await?;
    queue_search_projection(tx, content).await?;
    Ok(())
}

async fn replace_content_media(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    content: &pb::Content,
) -> Result<(), DaoError> {
    sqlx::query("DELETE FROM content_media WHERE content_id=$1")
        .bind(&content.id)
        .execute(&mut **tx)
        .await
        .map_err(DaoError::Database)?;
    for (sort_order, media) in content.media.iter().enumerate() {
        let mapping_id = format!("{}:{}", content.id, media.id);
        let inserted = sqlx::query(
            r#"
            INSERT INTO content_media (
                id, content_id, object_key, mime_type, width, height,
                duration_ms, sort_order, media_asset_id
            )
            SELECT
                $1, $2, object_key, mime_type, COALESCE(width, 0),
                COALESCE(height, 0), duration_ms, $4, id
            FROM media_assets
            WHERE id=$3 AND status='ready'
            "#,
        )
        .bind(mapping_id)
        .bind(&content.id)
        .bind(&media.id)
        .bind(i32::try_from(sort_order).unwrap_or(i32::MAX))
        .execute(&mut **tx)
        .await
        .map_err(DaoError::Database)?;
        if inserted.rows_affected() != 1 {
            return Err(DaoError::InvalidContent(format!(
                "media asset {} was no longer ready while persisting content",
                media.id
            )));
        }
    }
    Ok(())
}

async fn queue_search_projection(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    content: &pb::Content,
) -> Result<(), DaoError> {
    // A worker may be indexing an older version. Preserve its lease and let
    // completion requeue the newer version instead of allowing a stale worker
    // to acknowledge work it did not perform.
    sqlx::query(
        r#"
        INSERT INTO content_index_outbox (content_id, content_version)
        VALUES ($1, $2)
        ON CONFLICT (content_id) DO UPDATE
        SET content_version = EXCLUDED.content_version,
            status = CASE
                WHEN content_index_outbox.status = 'processing' THEN 'processing'
                ELSE 'pending'
            END,
            attempts = CASE
                WHEN content_index_outbox.status = 'processing' THEN content_index_outbox.attempts
                ELSE 0
            END,
            available_at = now(),
            locked_at = CASE
                WHEN content_index_outbox.status = 'processing' THEN content_index_outbox.locked_at
                ELSE NULL
            END,
            lease_id = CASE
                WHEN content_index_outbox.status = 'processing' THEN content_index_outbox.lease_id
                ELSE NULL
            END,
            last_error = CASE
                WHEN content_index_outbox.status = 'processing' THEN content_index_outbox.last_error
                ELSE NULL
            END,
            updated_at = now()
        "#,
    )
    .bind(&content.id)
    .bind(i64::from(content.version))
    .execute(&mut **tx)
    .await
    .map_err(DaoError::Database)?;
    Ok(())
}

fn parse_timestamp(value: Option<&str>) -> Result<Option<time::OffsetDateTime>, DaoError> {
    value
        .map(|value| {
            time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                .map_err(|_| DaoError::InvalidTimestamp(value.to_string()))
        })
        .transpose()
}

fn content_status_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::ContentStatus::try_from(value) {
        Ok(pb::ContentStatus::Draft) => Ok("draft"),
        Ok(pb::ContentStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::ContentStatus::Published) => Ok("published"),
        Ok(pb::ContentStatus::Restricted) => Ok("restricted"),
        Ok(pb::ContentStatus::Deleted) => Ok("deleted"),
        Err(_) => Err(DaoError::InvalidContent(
            "invalid content status".to_string(),
        )),
    }
}

fn content_type_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::ContentType::try_from(value) {
        Ok(pb::ContentType::Note) => Ok("note"),
        Ok(pb::ContentType::Article) => Ok("article"),
        Ok(pb::ContentType::Video) => Ok("video"),
        Ok(pb::ContentType::Route) => Ok("route"),
        Ok(pb::ContentType::Milestone) => Ok("milestone"),
        Ok(pb::ContentType::Question) => Ok("question"),
        Err(_) => Err(DaoError::InvalidContent("invalid content type".to_string())),
    }
}

fn growth_domain_name(value: i32) -> Result<&'static str, DaoError> {
    match pb::GrowthDomain::try_from(value) {
        Ok(pb::GrowthDomain::Learning) => Ok("learning"),
        Ok(pb::GrowthDomain::Movement) => Ok("movement"),
        Ok(pb::GrowthDomain::Wellness) => Ok("wellness"),
        Ok(pb::GrowthDomain::Travel) => Ok("travel"),
        Ok(pb::GrowthDomain::Leisure) => Ok("leisure"),
        Err(_) => Err(DaoError::InvalidContent(
            "invalid growth domain".to_string(),
        )),
    }
}

#[path = "memory_content_dao.rs"]
mod memory_content_dao;
pub(crate) use memory_content_dao::MemoryContentDao;
#[path = "postgres_content_dao.rs"]
mod postgres_content_dao;
pub(crate) use postgres_content_dao::PostgresContentDao;
