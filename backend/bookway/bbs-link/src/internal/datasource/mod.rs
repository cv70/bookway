use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

use super::api::{ContentDto, ContentPageDto, ContentQueryRequest};
use bookway_api::{ApiResponse, AuditDecisionDto, ContentAuditRequest, ContentAuditResponse};

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
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
}

#[async_trait]
pub(crate) trait ContentRepository: Send + Sync {
    async fn list(&self, query: &ContentQueryRequest) -> Result<ContentPageDto, RepositoryError>;
    async fn get(&self, id: &str) -> Result<ContentDto, RepositoryError>;
    async fn create(
        &self,
        content: ContentDto,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<ContentDto, RepositoryError>;
    async fn update(&self, content: ContentDto) -> Result<ContentDto, RepositoryError>;
}

#[async_trait]
pub(crate) trait ContentAuditor: Send + Sync {
    async fn audit(
        &self,
        request: ContentAuditRequest,
    ) -> Result<ContentAuditResponse, reqwest::Error>;
}

pub(crate) struct LocalContentAuditor;
#[async_trait]
impl ContentAuditor for LocalContentAuditor {
    async fn audit(
        &self,
        _request: ContentAuditRequest,
    ) -> Result<ContentAuditResponse, reqwest::Error> {
        Ok(ContentAuditResponse {
            decision: AuditDecisionDto::Approved,
            risk_score: 0.0,
            reasons: Vec::new(),
            provider: "local-development".to_string(),
        })
    }
}

pub(crate) struct HttpContentAuditor {
    client: reqwest::Client,
    base_url: String,
}
impl HttpContentAuditor {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}
#[async_trait]
impl ContentAuditor for HttpContentAuditor {
    async fn audit(
        &self,
        request: ContentAuditRequest,
    ) -> Result<ContentAuditResponse, reqwest::Error> {
        self.client
            .post(format!("{}/internal/v1/audit", self.base_url))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<ContentAuditResponse>>()
            .await
            .map(|response| response.data)
    }
}

pub(crate) struct MemoryContentRepository {
    state: RwLock<State>,
}

struct State {
    contents: Vec<ContentDto>,
    idempotency: HashMap<String, IdempotencyRecord>,
}

struct IdempotencyRecord {
    content_id: String,
    request_fingerprint: String,
}

impl MemoryContentRepository {
    pub(crate) fn seeded() -> Self {
        Self {
            state: RwLock::new(State {
                contents: vec![
                    seed(SeedContent {
                        id: "post-city-walk",
                        author_name: "木川",
                        author_id: "author-muchuan",
                        title: "我用 7 次散步重新认识了杭州",
                        summary: "不赶景点，只沿着水系和旧城慢慢走。每次回来，我都画一张自己的城市地图。",
                        domain: bookway_api::GrowthDomainDto::Travel,
                        route_title: "7 次城市观察散步",
                        route_duration: "3 周",
                        join_count: 4862,
                        like_count: 9128,
                        freshness: 0.94,
                        tags: "城市漫游,观察",
                        created_at: "2026-08-10T08:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1537531383496-f4749b8032cf?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1500648767791-00dcc994a43e?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-reading",
                        author_name: "一册",
                        author_id: "author-yice",
                        title: "读完 12 本书后，我留下了这套主题阅读法",
                        summary: "从问题出发选择三本结构不同的书，每周只整理一个能用于生活的结论。",
                        domain: bookway_api::GrowthDomainDto::Learning,
                        route_title: "四周主题阅读实验",
                        route_duration: "4 周",
                        join_count: 7130,
                        like_count: 15420,
                        freshness: 0.88,
                        tags: "阅读,知识管理",
                        created_at: "2026-08-09T09:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1495446815901-a7297e633e8d?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1494790108377-be9c29b29330?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-running",
                        author_name: "长风",
                        author_id: "author-changfeng",
                        title: "从跑不动两公里，到享受清晨的五公里",
                        summary: "真正有用的不是逼自己更快，而是给身体足够的恢复时间，并记录每次感受。",
                        domain: bookway_api::GrowthDomainDto::Movement,
                        route_title: "零压力晨跑计划",
                        route_duration: "6 周",
                        join_count: 9854,
                        like_count: 22180,
                        freshness: 0.91,
                        tags: "跑步,晨间",
                        created_at: "2026-08-08T06:30:00Z",
                        cover_url: "https://images.unsplash.com/photo-1552674605-db6ffd4facb5?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1534528741775-53994a69daeb?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-sleep",
                        author_name: "林间钟",
                        author_id: "author-linjian",
                        title: "把睡前一小时还给自己之后",
                        summary: "我没有追求完美作息，只做了三个小调整，白天的注意力却明显回来了。",
                        domain: bookway_api::GrowthDomainDto::Wellness,
                        route_title: "温和睡眠修复",
                        route_duration: "14 天",
                        join_count: 6321,
                        like_count: 10438,
                        freshness: 0.84,
                        tags: "睡眠,精力",
                        created_at: "2026-08-07T21:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1455642305367-68834a9d9aab?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1506794778202-cad84cf45f1d?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-pottery",
                        author_name: "未名",
                        author_id: "author-weiming",
                        title: "周末做陶，让时间重新慢下来",
                        summary: "手上的泥总有自己的脾气。两个周末之后，我不再急着控制最后的样子。",
                        domain: bookway_api::GrowthDomainDto::Leisure,
                        route_title: "陶艺初体验",
                        route_duration: "2 周",
                        join_count: 2176,
                        like_count: 6890,
                        freshness: 0.96,
                        tags: "手作,放松",
                        created_at: "2026-08-06T10:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1610701596007-11502861dcfa?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1531123897727-8f129e1688ce?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-museum",
                        author_name: "知也",
                        author_id: "author-zhiye",
                        title: "不做功课，也能认真看完一场展",
                        summary: "从一件真正好奇的作品开始，先描述看到什么，再去读作品背后的故事。",
                        domain: bookway_api::GrowthDomainDto::Learning,
                        route_title: "三次博物馆观察练习",
                        route_duration: "3 次",
                        join_count: 3541,
                        like_count: 7842,
                        freshness: 0.79,
                        tags: "艺术,博物馆",
                        created_at: "2026-08-05T14:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1564399579883-451a5d44ec08?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1527980965255-d3b416303d12?w=160&h=160&fit=crop",
                    }),
                ],
                idempotency: HashMap::new(),
            }),
        }
    }
}

struct SeedContent<'a> {
    id: &'a str,
    author_name: &'a str,
    author_id: &'a str,
    title: &'a str,
    summary: &'a str,
    domain: bookway_api::GrowthDomainDto,
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

fn seed(input: SeedContent<'_>) -> ContentDto {
    let SeedContent {
        id,
        author_name,
        author_id,
        title,
        summary,
        domain,
        route_title,
        route_duration,
        join_count,
        like_count,
        freshness,
        tags,
        created_at,
        cover_url,
        avatar_url,
    } = input;
    ContentDto {
        id: id.to_string(),
        post: bookway_api::PostSummaryDto {
            id: id.to_string(),
            author_name: author_name.to_string(),
            author_avatar_url: avatar_url.to_string(),
            title: title.to_string(),
            summary: summary.to_string(),
            domain,
            cover_url: cover_url.to_string(),
            route_title: route_title.to_string(),
            route_duration: route_duration.to_string(),
            join_count,
            like_count,
            freshness,
            tags: tags.split(',').map(str::to_string).collect(),
        },
        author_id: author_id.to_string(),
        content_type: bookway_api::ContentTypeDto::Route,
        status: bookway_api::ContentStatusDto::Published,
        body: summary.to_string(),
        media: vec![bookway_api::ContentMediaDto {
            id: format!("{id}-cover"),
            url: cover_url.to_string(),
            kind: "image".to_string(),
            width: 1200,
            height: 900,
            duration_ms: None,
        }],
        topics: tags.split(',').map(str::to_string).collect(),
        created_at: created_at.to_string(),
        published_at: Some(created_at.to_string()),
        version: 1,
        quality_score: freshness * 0.4 + f64::from(like_count).ln_1p() / 10.0,
    }
}

#[async_trait]
impl ContentRepository for MemoryContentRepository {
    async fn list(&self, query: &ContentQueryRequest) -> Result<ContentPageDto, RepositoryError> {
        let state = self.state.read().await;
        let mut items: Vec<_> = state
            .contents
            .iter()
            .filter(|content| query.status.is_none_or(|status| content.status == status))
            .filter(|content| {
                query
                    .ids
                    .as_deref()
                    .is_none_or(|ids| ids.split(',').any(|id| id.trim() == content.id))
            })
            .cloned()
            .collect();
        match query.strategy.as_deref() {
            Some("fresh") => items.sort_by(|left, right| right.created_at.cmp(&left.created_at)),
            _ => items.sort_by(|left, right| {
                right
                    .quality_score
                    .total_cmp(&left.quality_score)
                    .then_with(|| right.created_at.cmp(&left.created_at))
            }),
        }

        let total = items.len();
        let offset = query
            .cursor
            .as_deref()
            .and_then(|cursor| cursor.parse::<usize>().ok())
            .unwrap_or(0)
            .min(total);
        let limit = query.limit.unwrap_or(20).clamp(1, 100);
        let page: Vec<_> = items.into_iter().skip(offset).take(limit).collect();
        let next = if offset + page.len() < total {
            Some((offset + page.len()).to_string())
        } else {
            None
        };
        Ok(ContentPageDto {
            items: page,
            next_cursor: next,
            total_estimate: total,
        })
    }

    async fn get(&self, id: &str) -> Result<ContentDto, RepositoryError> {
        self.state
            .read()
            .await
            .contents
            .iter()
            .find(|content| content.id == id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))
    }

    async fn create(
        &self,
        content: ContentDto,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<ContentDto, RepositoryError> {
        let mut state = self.state.write().await;
        if let Some(key) = idempotency_key {
            let scoped_key = format!("{}:{key}", content.author_id);
            if let Some(existing) = state.idempotency.get(&scoped_key) {
                if existing.request_fingerprint != request_fingerprint {
                    return Err(RepositoryError::IdempotencyConflict(key));
                }
                return state
                    .contents
                    .iter()
                    .find(|item| item.id == existing.content_id)
                    .cloned()
                    .ok_or_else(|| RepositoryError::NotFound(existing.content_id.clone()));
            }
            state.idempotency.insert(
                scoped_key,
                IdempotencyRecord {
                    content_id: content.id.clone(),
                    request_fingerprint,
                },
            );
        }
        state.contents.push(content.clone());
        Ok(content)
    }

    async fn update(&self, content: ContentDto) -> Result<ContentDto, RepositoryError> {
        let mut state = self.state.write().await;
        let existing = state
            .contents
            .iter_mut()
            .find(|item| item.id == content.id)
            .ok_or_else(|| RepositoryError::NotFound(content.id.clone()))?;
        *existing = content.clone();
        Ok(content)
    }
}

pub(crate) struct PostgresContentRepository {
    pool: sqlx::PgPool,
}

impl PostgresContentRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ContentRepository for PostgresContentRepository {
    async fn list(&self, query: &ContentQueryRequest) -> Result<ContentPageDto, RepositoryError> {
        let limit = query.limit.unwrap_or(20).clamp(1, 100) as i64;
        let offset = query
            .cursor
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0)
            .max(0);
        let status = query.status.map(status_name);
        let strategy = query.strategy.as_deref().unwrap_or("quality");
        let order = if strategy == "fresh" {
            "created_at DESC, id DESC"
        } else {
            "quality_score DESC, created_at DESC, id DESC"
        };
        let sql = format!(
            "SELECT payload FROM content_items WHERE deleted_at IS NULL AND ($1::text IS NULL OR status = $1) AND ($2::text IS NULL OR id = ANY(string_to_array($2, ','))) ORDER BY {order} LIMIT $3 OFFSET $4"
        );
        let rows = sqlx::query_scalar::<_, serde_json::Value>(&sql)
            .bind(status)
            .bind(query.ids.as_deref())
            .bind(limit + 1)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::Database)?;
        let has_more = rows.len() > limit as usize;
        let items = rows
            .into_iter()
            .take(limit as usize)
            .map(|value| serde_json::from_value(value).map_err(RepositoryError::Serialization))
            .collect::<Result<Vec<ContentDto>, _>>()?;
        Ok(ContentPageDto {
            total_estimate: items.len() + if has_more { 1 } else { 0 },
            next_cursor: has_more.then(|| (offset + limit).to_string()),
            items,
        })
    }

    async fn get(&self, id: &str) -> Result<ContentDto, RepositoryError> {
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM content_items WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        serde_json::from_value(payload).map_err(RepositoryError::Serialization)
    }

    async fn create(
        &self,
        content: ContentDto,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<ContentDto, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if let Some(key) = idempotency_key.as_deref()
            && let Some((resource_id, fingerprint)) = sqlx::query_as::<_, (String, String)>(
                "SELECT resource_id, request_hash FROM content_idempotency_keys WHERE user_id = $1 AND idempotency_key = $2 AND operation = 'create' FOR UPDATE",
            )
            .bind(&content.author_id)
            .bind(key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(RepositoryError::Database)?
            {
                if fingerprint != request_fingerprint {
                    return Err(RepositoryError::IdempotencyConflict(key.to_string()));
                }
                let payload = sqlx::query_scalar::<_, serde_json::Value>(
                    "SELECT payload FROM content_items WHERE id = $1 AND deleted_at IS NULL",
                )
                .bind(resource_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(RepositoryError::Database)?;
                return serde_json::from_value(payload).map_err(RepositoryError::Serialization);
            }
        let payload = serde_json::to_value(&content).map_err(RepositoryError::Serialization)?;
        let published_at = parse_timestamp(content.published_at.as_deref())?;
        sqlx::query(
            "INSERT INTO content_items (id, author_id, content_type, status, title, summary, body, domain, cover_url, route_title, route_duration, version, quality_score, published_at, payload) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
        .bind(&content.id)
        .bind(&content.author_id)
        .bind(content_type_name(content.content_type))
        .bind(status_name(content.status))
        .bind(&content.post.title)
        .bind(&content.post.summary)
        .bind(&content.body)
        .bind(domain_name(content.post.domain))
        .bind(&content.post.cover_url)
        .bind(&content.post.route_title)
        .bind(&content.post.route_duration)
        .bind(i64::from(content.version))
        .bind(content.quality_score)
        .bind(published_at)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .map_err(RepositoryError::Database)?;
        if let Some(key) = idempotency_key {
            sqlx::query("INSERT INTO content_idempotency_keys (user_id,idempotency_key,operation,resource_id,request_hash) VALUES ($1,$2,'create',$3,$4)")
                .bind(&content.author_id).bind(key).bind(&content.id).bind(request_fingerprint)
                .execute(&mut *tx).await.map_err(RepositoryError::Database)?;
        }
        tx.commit().await.map_err(RepositoryError::Database)?;
        Ok(content)
    }

    async fn update(&self, content: ContentDto) -> Result<ContentDto, RepositoryError> {
        let payload = serde_json::to_value(&content).map_err(RepositoryError::Serialization)?;
        let published_at = parse_timestamp(content.published_at.as_deref())?;
        let updated = sqlx::query(
            "UPDATE content_items SET status=$2,title=$3,summary=$4,body=$5,cover_url=$6,version=$7,quality_score=$8,published_at=$9,payload=$10,updated_at=now() WHERE id=$1 AND deleted_at IS NULL AND version < $7",
        )
        .bind(&content.id)
        .bind(status_name(content.status))
        .bind(&content.post.title)
        .bind(&content.post.summary)
        .bind(&content.body)
        .bind(&content.post.cover_url)
        .bind(i64::from(content.version))
        .bind(content.quality_score)
        .bind(published_at)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        if updated.rows_affected() == 0 {
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM content_items WHERE id=$1 AND deleted_at IS NULL)",
            )
            .bind(&content.id)
            .fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::Database)?;
            return Err(if exists {
                RepositoryError::VersionConflict
            } else {
                RepositoryError::NotFound(content.id)
            });
        }
        Ok(content)
    }
}

fn parse_timestamp(value: Option<&str>) -> Result<Option<time::OffsetDateTime>, RepositoryError> {
    value
        .map(|value| {
            time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                .map_err(|_| RepositoryError::InvalidTimestamp(value.to_string()))
        })
        .transpose()
}

fn status_name(status: bookway_api::ContentStatusDto) -> &'static str {
    match status {
        bookway_api::ContentStatusDto::Draft => "draft",
        bookway_api::ContentStatusDto::Reviewing => "reviewing",
        bookway_api::ContentStatusDto::Published => "published",
        bookway_api::ContentStatusDto::Restricted => "restricted",
        bookway_api::ContentStatusDto::Deleted => "deleted",
    }
}

fn content_type_name(value: bookway_api::ContentTypeDto) -> &'static str {
    match value {
        bookway_api::ContentTypeDto::Note => "note",
        bookway_api::ContentTypeDto::Article => "article",
        bookway_api::ContentTypeDto::Video => "video",
        bookway_api::ContentTypeDto::Route => "route",
    }
}

fn domain_name(value: bookway_api::GrowthDomainDto) -> &'static str {
    match value {
        bookway_api::GrowthDomainDto::Learning => "learning",
        bookway_api::GrowthDomainDto::Movement => "movement",
        bookway_api::GrowthDomainDto::Wellness => "wellness",
        bookway_api::GrowthDomainDto::Travel => "travel",
        bookway_api::GrowthDomainDto::Leisure => "leisure",
    }
}
