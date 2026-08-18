use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::api::pb;

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
    #[error("stored content is invalid: {0}")]
    InvalidContent(String),
}

#[async_trait]
pub(crate) trait ContentRepository: Send + Sync {
    async fn list(&self, query: &pb::ListRequest) -> Result<pb::ContentPage, RepositoryError>;
    async fn get(&self, id: &str) -> Result<pb::Content, RepositoryError>;
    async fn published_by_idempotency_key(
        &self,
        user_id: &str,
        idempotency_key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Content>, RepositoryError>;
    async fn create(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, RepositoryError>;
    async fn update(&self, content: pb::Content) -> Result<pb::Content, RepositoryError>;
    async fn publish(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, RepositoryError>;
}

pub(crate) struct MemoryContentRepository {
    state: RwLock<State>,
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
                        domain: pb::GrowthDomain::Travel,
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
                        domain: pb::GrowthDomain::Learning,
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
                        domain: pb::GrowthDomain::Movement,
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
                        domain: pb::GrowthDomain::Wellness,
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
                        domain: pb::GrowthDomain::Leisure,
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
                        author_id: "author-zhiy",
                        title: "不做功课，也能认真看完一场展",
                        summary: "从一件真正好奇的作品开始，先描述看到什么，再去读作品背后的故事。",
                        domain: pb::GrowthDomain::Learning,
                        route_title: "三次博物馆观察练习",
                        route_duration: "3 周",
                        join_count: 3952,
                        like_count: 10582,
                        freshness: 0.82,
                        tags: "博物馆,观察",
                        created_at: "2026-08-05T14:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1564399579883-451a5d44ec08?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1534528741775-53994a69daeb?w=160&h=160&fit=crop",
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
        }],
        journey_type: pb::RouteTemplateKind::Project as i32,
    }
}

#[async_trait]
impl ContentRepository for MemoryContentRepository {
    async fn list(&self, query: &pb::ListRequest) -> Result<pb::ContentPage, RepositoryError> {
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
            .filter(|content| {
                query
                    .author_id
                    .as_deref()
                    .is_none_or(|author_id| content.author_id == author_id)
            })
            .filter(|content| {
                query.author_ids.is_empty() || query.author_ids.contains(&content.author_id)
            })
            .filter(|content| {
                query
                    .content_type
                    .is_none_or(|content_type| content.content_type == content_type)
            })
            .filter(|content| {
                query.domain.is_none_or(|domain| {
                    content
                        .post
                        .as_ref()
                        .is_some_and(|post| post.domain == domain)
                })
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
        let limit = query.limit.unwrap_or(20).clamp(1, 100) as usize;
        let page: Vec<_> = items.into_iter().skip(offset).take(limit).collect();
        Ok(pb::ContentPage {
            next_cursor: (offset + page.len() < total).then(|| (offset + page.len()).to_string()),
            items: page,
            total_estimate: total as u64,
        })
    }

    async fn get(&self, id: &str) -> Result<pb::Content, RepositoryError> {
        self.state
            .read()
            .await
            .contents
            .iter()
            .find(|content| content.id == id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))
    }

    async fn published_by_idempotency_key(
        &self,
        user_id: &str,
        idempotency_key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Content>, RepositoryError> {
        let state = self.state.read().await;
        let key = (
            user_id.to_string(),
            "publish".to_string(),
            idempotency_key.to_string(),
        );
        let Some(existing) = state.idempotency.get(&key) else {
            return Ok(None);
        };
        if existing.request_fingerprint != request_fingerprint {
            return Err(RepositoryError::IdempotencyConflict(
                idempotency_key.to_string(),
            ));
        }
        existing.response.clone().map(Some).ok_or_else(|| {
            RepositoryError::InvalidContent(
                "publish idempotency record is missing its response snapshot".to_string(),
            )
        })
    }

    async fn create(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, RepositoryError> {
        let mut state = self.state.write().await;
        if let Some(key) = idempotency_key {
            let scoped_key = (content.author_id.clone(), "create".to_string(), key.clone());
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
                    response: None,
                },
            );
        }
        state.contents.push(content.clone());
        Ok(content)
    }

    async fn update(&self, content: pb::Content) -> Result<pb::Content, RepositoryError> {
        let mut state = self.state.write().await;
        let existing = state
            .contents
            .iter_mut()
            .find(|item| item.id == content.id)
            .ok_or_else(|| RepositoryError::NotFound(content.id.clone()))?;
        *existing = content.clone();
        Ok(content)
    }

    async fn publish(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, RepositoryError> {
        let mut state = self.state.write().await;
        if let Some(key) = idempotency_key {
            let scoped_key = (
                content.author_id.clone(),
                "publish".to_string(),
                key.clone(),
            );
            if let Some(existing) = state.idempotency.get(&scoped_key) {
                if existing.request_fingerprint != request_fingerprint {
                    return Err(RepositoryError::IdempotencyConflict(key));
                }
                return existing.response.clone().ok_or_else(|| {
                    RepositoryError::InvalidContent(
                        "publish idempotency record is missing its response snapshot".to_string(),
                    )
                });
            }
            let existing = state
                .contents
                .iter_mut()
                .find(|item| item.id == content.id)
                .ok_or_else(|| RepositoryError::NotFound(content.id.clone()))?;
            *existing = content.clone();
            state.idempotency.insert(
                scoped_key,
                IdempotencyRecord {
                    content_id: content.id.clone(),
                    request_fingerprint,
                    response: Some(content.clone()),
                },
            );
            return Ok(content);
        }
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
    async fn list(&self, query: &pb::ListRequest) -> Result<pb::ContentPage, RepositoryError> {
        let limit = query.limit.unwrap_or(20).clamp(1, 100) as i64;
        let offset = query
            .cursor
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0)
            .max(0);
        let status = query.status.map(content_status_name).transpose()?;
        let author_ids = (!query.author_ids.is_empty()).then(|| query.author_ids.clone());
        let order = if query.strategy.as_deref() == Some("fresh") {
            "created_at DESC, id DESC"
        } else {
            "quality_score DESC, created_at DESC, id DESC"
        };
        let sql = format!(
            "SELECT payload, COUNT(*) OVER() AS total_count FROM content_items WHERE deleted_at IS NULL AND ($1::text IS NULL OR status = $1) AND ($2::text IS NULL OR id = ANY(string_to_array($2, ','))) AND ($3::text IS NULL OR author_id = $3) AND ($6::text IS NULL OR content_type = $6) AND ($7::text IS NULL OR domain = $7) AND ($8::text[] IS NULL OR author_id = ANY($8)) ORDER BY {order} LIMIT $4 OFFSET $5"
        );
        let rows = sqlx::query_as::<_, (serde_json::Value, i64)>(&sql)
            .bind(status)
            .bind(query.ids.as_deref())
            .bind(query.author_id.as_deref())
            .bind(limit + 1)
            .bind(offset)
            .bind(query.content_type.map(content_type_name).transpose()?)
            .bind(query.domain.map(growth_domain_name).transpose()?)
            .bind(author_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::Database)?;
        let total = rows.first().map(|(_, total)| *total).unwrap_or(0);
        let items = rows
            .into_iter()
            .take(limit as usize)
            .map(|(value, _)| serde_json::from_value(value).map_err(RepositoryError::Serialization))
            .collect::<Result<Vec<pb::Content>, _>>()?;
        Ok(pb::ContentPage {
            total_estimate: u64::try_from(total).unwrap_or(u64::MAX),
            next_cursor: (offset + limit < total).then(|| (offset + limit).to_string()),
            items,
        })
    }

    async fn get(&self, id: &str) -> Result<pb::Content, RepositoryError> {
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

    async fn published_by_idempotency_key(
        &self,
        user_id: &str,
        idempotency_key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Content>, RepositoryError> {
        let existing = sqlx::query_as::<_, (String, Option<serde_json::Value>)>(
            "SELECT request_hash, response_payload FROM content_idempotency_keys WHERE user_id = $1 AND idempotency_key = $2 AND operation = 'publish'",
        )
        .bind(user_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        existing
            .map(|(stored_fingerprint, response)| {
                published_idempotency_response(
                    idempotency_key,
                    request_fingerprint,
                    stored_fingerprint,
                    response,
                )
            })
            .transpose()
    }

    async fn create(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, RepositoryError> {
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
        let post = content.post.as_ref().ok_or_else(|| {
            RepositoryError::InvalidContent("content is missing its post summary".to_string())
        })?;
        let payload = serde_json::to_value(&content).map_err(RepositoryError::Serialization)?;
        let published_at = parse_timestamp(content.published_at.as_deref())?;
        sqlx::query(
            "INSERT INTO content_items (id, author_id, content_type, status, title, summary, body, domain, cover_url, route_title, route_duration, version, quality_score, published_at, payload) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
        .bind(&content.id)
        .bind(&content.author_id)
        .bind(content_type_name(content.content_type)?)
        .bind(content_status_name(content.status)?)
        .bind(&post.title)
        .bind(&post.summary)
        .bind(&content.body)
        .bind(growth_domain_name(post.domain)?)
        .bind(&post.cover_url)
        .bind(&post.route_title)
        .bind(&post.route_duration)
        .bind(i64::from(content.version))
        .bind(content.quality_score)
        .bind(published_at)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .map_err(RepositoryError::Database)?;
        replace_content_media(&mut tx, &content).await?;
        queue_search_projection(&mut tx, &content).await?;
        if let Some(key) = idempotency_key {
            sqlx::query("INSERT INTO content_idempotency_keys (user_id,idempotency_key,operation,resource_id,request_hash) VALUES ($1,$2,'create',$3,$4)")
                .bind(&content.author_id)
                .bind(key)
                .bind(&content.id)
                .bind(request_fingerprint)
                .execute(&mut *tx)
                .await
                .map_err(RepositoryError::Database)?;
        }
        tx.commit().await.map_err(RepositoryError::Database)?;
        Ok(content)
    }

    async fn update(&self, content: pb::Content) -> Result<pb::Content, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(RepositoryError::Database)?;
        update_content_in_transaction(&mut tx, &content).await?;
        tx.commit().await.map_err(RepositoryError::Database)?;
        Ok(content)
    }

    async fn publish(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if let Some(key) = idempotency_key.as_deref() {
            // The row does not exist for the first request, so serialize the
            // key explicitly before observing or creating it.
            let lock_key = format!("content-publish:{}:{key}", content.author_id);
            sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
                .bind(lock_key)
                .execute(&mut *tx)
                .await
                .map_err(RepositoryError::Database)?;
            if let Some((stored_fingerprint, response)) =
                sqlx::query_as::<_, (String, Option<serde_json::Value>)>(
                    "SELECT request_hash, response_payload FROM content_idempotency_keys WHERE user_id = $1 AND idempotency_key = $2 AND operation = 'publish' FOR UPDATE",
                )
                .bind(&content.author_id)
                .bind(key)
                .fetch_optional(&mut *tx)
                .await
                .map_err(RepositoryError::Database)?
            {
                let existing = published_idempotency_response(
                    key,
                    &request_fingerprint,
                    stored_fingerprint,
                    response,
                )?;
                tx.commit().await.map_err(RepositoryError::Database)?;
                return Ok(existing);
            }
        }

        update_content_in_transaction(&mut tx, &content).await?;
        if let Some(key) = idempotency_key {
            let response =
                serde_json::to_value(&content).map_err(RepositoryError::Serialization)?;
            sqlx::query(
                "INSERT INTO content_idempotency_keys (user_id,idempotency_key,operation,resource_id,request_hash,response_payload) VALUES ($1,$2,'publish',$3,$4,$5)",
            )
            .bind(&content.author_id)
            .bind(key)
            .bind(&content.id)
            .bind(request_fingerprint)
            .bind(response)
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::Database)?;
        }
        tx.commit().await.map_err(RepositoryError::Database)?;
        Ok(content)
    }
}

fn published_idempotency_response(
    idempotency_key: &str,
    request_fingerprint: &str,
    stored_fingerprint: String,
    response: Option<serde_json::Value>,
) -> Result<pb::Content, RepositoryError> {
    if stored_fingerprint != request_fingerprint {
        return Err(RepositoryError::IdempotencyConflict(
            idempotency_key.to_string(),
        ));
    }
    let response = response.ok_or_else(|| {
        RepositoryError::InvalidContent(
            "publish idempotency record is missing its response snapshot".to_string(),
        )
    })?;
    serde_json::from_value(response).map_err(RepositoryError::Serialization)
}

async fn update_content_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    content: &pb::Content,
) -> Result<(), RepositoryError> {
    let post = content.post.as_ref().ok_or_else(|| {
        RepositoryError::InvalidContent("content is missing its post summary".to_string())
    })?;
    let payload = serde_json::to_value(content).map_err(RepositoryError::Serialization)?;
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
    .map_err(RepositoryError::Database)?;
    if updated.rows_affected() == 0 {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM content_items WHERE id=$1 AND deleted_at IS NULL)",
        )
        .bind(&content.id)
        .fetch_one(&mut **tx)
        .await
        .map_err(RepositoryError::Database)?;
        return Err(if exists {
            RepositoryError::VersionConflict
        } else {
            RepositoryError::NotFound(content.id.clone())
        });
    }
    replace_content_media(tx, content).await?;
    queue_search_projection(tx, content).await?;
    Ok(())
}

async fn replace_content_media(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    content: &pb::Content,
) -> Result<(), RepositoryError> {
    sqlx::query("DELETE FROM content_media WHERE content_id=$1")
        .bind(&content.id)
        .execute(&mut **tx)
        .await
        .map_err(RepositoryError::Database)?;
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
        .map_err(RepositoryError::Database)?;
        if inserted.rows_affected() != 1 {
            return Err(RepositoryError::InvalidContent(format!(
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
) -> Result<(), RepositoryError> {
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
    .map_err(RepositoryError::Database)?;
    Ok(())
}

fn parse_timestamp(value: Option<&str>) -> Result<Option<time::OffsetDateTime>, RepositoryError> {
    value
        .map(|value| {
            time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                .map_err(|_| RepositoryError::InvalidTimestamp(value.to_string()))
        })
        .transpose()
}

fn content_status_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::ContentStatus::try_from(value) {
        Ok(pb::ContentStatus::Draft) => Ok("draft"),
        Ok(pb::ContentStatus::Reviewing) => Ok("reviewing"),
        Ok(pb::ContentStatus::Published) => Ok("published"),
        Ok(pb::ContentStatus::Restricted) => Ok("restricted"),
        Ok(pb::ContentStatus::Deleted) => Ok("deleted"),
        Err(_) => Err(RepositoryError::InvalidContent(
            "invalid content status".to_string(),
        )),
    }
}

fn content_type_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::ContentType::try_from(value) {
        Ok(pb::ContentType::Note) => Ok("note"),
        Ok(pb::ContentType::Article) => Ok("article"),
        Ok(pb::ContentType::Video) => Ok("video"),
        Ok(pb::ContentType::Route) => Ok("route"),
        Ok(pb::ContentType::Milestone) => Ok("milestone"),
        Ok(pb::ContentType::Question) => Ok("question"),
        Err(_) => Err(RepositoryError::InvalidContent(
            "invalid content type".to_string(),
        )),
    }
}

fn growth_domain_name(value: i32) -> Result<&'static str, RepositoryError> {
    match pb::GrowthDomain::try_from(value) {
        Ok(pb::GrowthDomain::Learning) => Ok("learning"),
        Ok(pb::GrowthDomain::Movement) => Ok("movement"),
        Ok(pb::GrowthDomain::Wellness) => Ok("wellness"),
        Ok(pb::GrowthDomain::Travel) => Ok("travel"),
        Ok(pb::GrowthDomain::Leisure) => Ok("leisure"),
        Err(_) => Err(RepositoryError::InvalidContent(
            "invalid growth domain".to_string(),
        )),
    }
}
