use bookway_content_audit_api::pb as audit_pb;
use bookway_media_api::pb as media_pb;
use uuid::Uuid;

use crate::{
    api::pb,
    datasource::RepositoryError,
    domain::{ContentError, Domain},
};

impl Domain {
    pub(crate) async fn list(
        &self,
        mut query: pb::ListRequest,
    ) -> Result<pb::ContentPage, ContentError> {
        query.author_ids = normalize_list_author_ids(query.author_ids)?;
        if query.author_id.is_some() && !query.author_ids.is_empty() {
            return Err(ContentError::Validation(
                "author_id 和 author_ids 不能同时使用".to_string(),
            ));
        }
        let mut page = self.repository.list(&query).await?;
        page.items = page
            .items
            .into_iter()
            .map(normalize_route_summary)
            .collect();
        Ok(page)
    }

    pub(crate) async fn get_public_summaries(
        &self,
        request: pb::PublicContentSummariesRequest,
    ) -> Result<pb::PublicContentSummaries, ContentError> {
        let ids = normalize_public_summary_ids(request.ids)?;
        if ids.is_empty() {
            return Ok(pb::PublicContentSummaries { items: Vec::new() });
        }
        let page = self
            .list(pb::ListRequest {
                cursor: None,
                // `normalize_public_summary_ids` limits this batch to 100.
                limit: Some(ids.len() as u32),
                status: Some(pb::ContentStatus::Published as i32),
                strategy: Some("fresh".to_string()),
                ids: Some(ids.join(",")),
                author_id: None,
                content_type: None,
                domain: None,
                author_ids: Vec::new(),
            })
            .await?;
        let mut summaries = std::collections::HashMap::with_capacity(page.items.len());
        for content in page.items {
            let Some(post) = content.post else {
                continue;
            };
            summaries.insert(
                content.id.clone(),
                pb::PublicContentSummary {
                    id: content.id,
                    post: Some(post),
                    author_id: content.author_id,
                    content_type: content.content_type,
                    topics: content.topics,
                    quality_score: content.quality_score,
                },
            );
        }
        Ok(pb::PublicContentSummaries {
            // Preserve the request order so batch consumers can correlate the
            // response without relying on datastore sorting rules.
            items: ids
                .into_iter()
                .filter_map(|id| summaries.remove(&id))
                .collect(),
        })
    }

    pub(crate) async fn get(&self, id: &str) -> Result<pb::Content, ContentError> {
        Ok(normalize_route_summary(self.repository.get(id).await?))
    }

    pub(crate) async fn get_public(&self, id: &str) -> Result<pb::Content, ContentError> {
        let content = normalize_route_summary(self.repository.get(id).await?);
        if content.status != pb::ContentStatus::Published as i32 {
            return Err(ContentError::Repository(RepositoryError::NotFound(
                id.to_string(),
            )));
        }
        Ok(content)
    }

    pub(crate) async fn create(
        &self,
        request: pb::CreateRequest,
    ) -> Result<pb::Content, ContentError> {
        if request.user_id.trim().is_empty() {
            return Err(ContentError::Validation("作者不能为空".to_string()));
        }
        if request.title.trim().is_empty() || request.body.trim().is_empty() {
            return Err(ContentError::Validation("标题和正文不能为空".to_string()));
        }
        if request.title.chars().count() > 120 {
            return Err(ContentError::Validation(
                "标题不能超过 120 个字符".to_string(),
            ));
        }
        if request.tags.len() > 12 || request.topics.len() > 12 {
            return Err(ContentError::Validation(
                "标签和主题最多各 12 个".to_string(),
            ));
        }
        reject_legacy_cover_url(request.cover_url.as_deref())?;
        validate_enum::<pb::GrowthDomain>(request.domain, "growth domain")?;
        validate_enum::<pb::ContentType>(request.content_type, "content type")?;
        let route_template = match request.route_template.clone() {
            Some(template) => validate_route_template(request.content_type, Some(template))?,
            None if request.content_type == pb::ContentType::Route as i32 => {
                validate_route_template(
                    request.content_type,
                    Some(legacy_route_template(&request.title, &request.summary)),
                )?
            }
            None => None,
        };
        let media_asset_ids = normalize_media_asset_ids(&request.media_asset_ids)?;
        let media = self
            .owned_ready_media(request.user_id.clone(), media_asset_ids)
            .await?;
        let media = content_media_from_assets(request.content_type, media)?;
        let request_fingerprint = serde_json::to_string(&request)
            .map_err(|error| ContentError::Validation(error.to_string()))?;
        let id = Uuid::now_v7().to_string();
        let now = now_rfc3339();
        let is_route = request.content_type == pb::ContentType::Route as i32;
        let route_title = if is_route {
            request
                .route_title
                .clone()
                .unwrap_or_else(|| request.title.trim().to_string())
        } else {
            String::new()
        };
        let route_duration = if is_route {
            request.route_duration.clone().unwrap_or_default()
        } else {
            String::new()
        };
        let content = pb::Content {
            id: id.clone(),
            post: Some(pb::PostSummary {
                id: id.clone(),
                author_name: request.user_id.clone(),
                author_avatar_url: String::new(),
                title: request.title.trim().to_string(),
                summary: request.summary.trim().to_string(),
                domain: request.domain,
                cover_url: media
                    .first()
                    .map(|item| item.url.clone())
                    .unwrap_or_default(),
                route_title,
                route_duration,
                join_count: 0,
                like_count: 0,
                freshness: 1.0,
                tags: request.tags,
                is_route,
            }),
            author_id: request.user_id,
            content_type: request.content_type,
            status: pb::ContentStatus::Draft as i32,
            body: request.body,
            media,
            topics: request.topics,
            created_at: now,
            published_at: None,
            version: 1,
            quality_score: 0.0,
            route_template,
        };
        Ok(normalize_route_summary(
            self.repository
                .create(content, request.idempotency_key, request_fingerprint)
                .await?,
        ))
    }

    pub(crate) async fn update(
        &self,
        request: pb::UpdateRequest,
    ) -> Result<pb::Content, ContentError> {
        let mut content = normalize_route_summary(self.repository.get(&request.id).await?);
        if content.author_id != request.user_id {
            return Err(ContentError::Forbidden);
        }
        let post = content.post.as_mut().ok_or_else(|| {
            ContentError::Repository(RepositoryError::InvalidContent(
                "content is missing its post summary".to_string(),
            ))
        })?;
        if let Some(title) = request.title {
            if title.trim().is_empty() {
                return Err(ContentError::Validation("标题不能为空".to_string()));
            }
            post.title = title.trim().to_string();
        }
        if let Some(summary) = request.summary {
            post.summary = summary;
        }
        if let Some(body) = request.body {
            if body.trim().is_empty() {
                return Err(ContentError::Validation("正文不能为空".to_string()));
            }
            content.body = body;
        }
        if let Some(tags) = request.tags {
            post.tags = tags.values;
        }
        if let Some(topics) = request.topics {
            content.topics = topics.values;
        }
        if let Some(route_template) = request.route_template {
            content.route_template =
                validate_route_template(content.content_type, Some(route_template))?;
        }
        reject_legacy_cover_url(request.cover_url.as_deref())?;
        if let Some(media_asset_ids) = request.media_asset_ids {
            let media_asset_ids = normalize_media_asset_ids(&media_asset_ids.values)?;
            let media = self
                .owned_ready_media(content.author_id.clone(), media_asset_ids)
                .await?;
            content.media = content_media_from_assets(content.content_type, media)?;
            post.cover_url = content
                .media
                .first()
                .map(|item| item.url.clone())
                .unwrap_or_default();
        }
        content.version = content.version.saturating_add(1);
        if content.status == pb::ContentStatus::Published as i32 {
            content.status = pb::ContentStatus::Reviewing as i32;
            content.published_at = None;
        }
        Ok(normalize_route_summary(
            self.repository.update(content).await?,
        ))
    }

    pub(crate) async fn publish(
        &self,
        mut request: pb::PublishRequest,
    ) -> Result<pb::Content, ContentError> {
        request.idempotency_key = normalize_publish_idempotency_key(request.idempotency_key)?;
        let request_fingerprint = serde_json::to_string(&request)
            .map_err(|error| ContentError::Validation(error.to_string()))?;
        if let Some(key) = request.idempotency_key.as_deref()
            && let Some(content) = self
                .repository
                .published_by_idempotency_key(&request.user_id, key, &request_fingerprint)
                .await?
        {
            return Ok(normalize_route_summary(content));
        }
        let mut content = normalize_route_summary(self.repository.get(&request.id).await?);
        if content.author_id != request.user_id {
            return Err(ContentError::Forbidden);
        }
        match pb::ContentStatus::try_from(content.status) {
            Ok(pb::ContentStatus::Published) => return Ok(content),
            Ok(pb::ContentStatus::Restricted | pb::ContentStatus::Deleted) => {
                return Err(ContentError::Validation(
                    "受限或已删除的内容不能重新发布，请通过申诉流程处理".to_string(),
                ));
            }
            Ok(pb::ContentStatus::Draft | pb::ContentStatus::Reviewing) => {}
            Err(_) => {
                return Err(ContentError::Repository(RepositoryError::InvalidContent(
                    "content has an invalid status".to_string(),
                )));
            }
        }
        if content.body.trim().is_empty() {
            return Err(ContentError::Validation(
                "没有正文的内容不能发布".to_string(),
            ));
        }
        let title = content
            .post
            .as_ref()
            .ok_or_else(|| {
                ContentError::Repository(RepositoryError::InvalidContent(
                    "content is missing its post summary".to_string(),
                ))
            })?
            .title
            .clone();
        let next_version = content.version.saturating_add(1);
        let audit = self
            .audit(audit_pb::AuditRequest {
                content_id: content.id.clone(),
                version: next_version,
                title,
                body: content.body.clone(),
            })
            .await?;
        let decision = audit_pb::AuditDecision::try_from(audit.decision).map_err(|_| {
            ContentError::Audit("content-audit returned an invalid decision".to_string())
        })?;
        let (status, published_at) = match decision {
            audit_pb::AuditDecision::Approved => {
                (pb::ContentStatus::Published as i32, Some(now_rfc3339()))
            }
            audit_pb::AuditDecision::Reviewing => (pb::ContentStatus::Reviewing as i32, None),
            audit_pb::AuditDecision::Restricted => (pb::ContentStatus::Restricted as i32, None),
        };
        content.status = status;
        content.published_at = published_at;
        content.version = next_version;
        Ok(normalize_route_summary(
            self.repository
                .publish(content, request.idempotency_key, request_fingerprint)
                .await?,
        ))
    }

    pub(crate) async fn restrict(
        &self,
        request: pb::RestrictRequest,
    ) -> Result<pb::Content, ContentError> {
        if request.content_id.trim().is_empty() {
            return Err(ContentError::Validation(
                "content id must not be empty".to_string(),
            ));
        }
        let mut content =
            normalize_route_summary(self.repository.get(request.content_id.trim()).await?);
        if matches!(
            pb::ContentStatus::try_from(content.status),
            Ok(pb::ContentStatus::Restricted | pb::ContentStatus::Deleted)
        ) {
            return Ok(content);
        }
        content.status = pb::ContentStatus::Restricted as i32;
        content.published_at = None;
        content.version = content.version.saturating_add(1);
        Ok(normalize_route_summary(
            self.repository.update(content).await?,
        ))
    }

    pub(crate) async fn restore(
        &self,
        request: pb::RestoreRequest,
    ) -> Result<pb::Content, ContentError> {
        if request.content_id.trim().is_empty() {
            return Err(ContentError::Validation(
                "content id must not be empty".to_string(),
            ));
        }
        let mut content =
            normalize_route_summary(self.repository.get(request.content_id.trim()).await?);
        if content.status == pb::ContentStatus::Published as i32 {
            return Ok(content);
        }
        if content.status != pb::ContentStatus::Restricted as i32 {
            return Err(ContentError::Validation(
                "only restricted content can be restored".to_string(),
            ));
        }
        content.status = pb::ContentStatus::Published as i32;
        content.published_at = Some(now_rfc3339());
        content.version = content.version.saturating_add(1);
        Ok(normalize_route_summary(
            self.repository.update(content).await?,
        ))
    }
}

fn normalize_route_summary(mut content: pb::Content) -> pb::Content {
    let is_route = content.content_type == pb::ContentType::Route as i32;
    if let Some(post) = content.post.as_mut() {
        post.is_route = is_route;
        if !is_route {
            // Notes and articles cannot inherit a private journey's route metadata.
            post.route_title.clear();
            post.route_duration.clear();
        }
    }
    content
}

fn reject_legacy_cover_url(cover_url: Option<&str>) -> Result<(), ContentError> {
    if cover_url.is_some_and(|value| !value.trim().is_empty()) {
        return Err(ContentError::Validation(
            "封面必须使用已上传的媒体资源 ID，不能直接提供 URL".to_string(),
        ));
    }
    Ok(())
}

fn normalize_media_asset_ids(ids: &[String]) -> Result<Vec<String>, ContentError> {
    if ids.len() > 12 {
        return Err(ContentError::Validation(
            "每条内容最多附带 12 个媒体资源".to_string(),
        ));
    }
    let mut seen = std::collections::HashSet::with_capacity(ids.len());
    ids.iter()
        .map(|value| value.trim().to_string())
        .map(|id| {
            if Uuid::parse_str(&id).is_err() || !seen.insert(id.clone()) {
                Err(ContentError::Validation(
                    "媒体资源 ID 无效或重复".to_string(),
                ))
            } else {
                Ok(id)
            }
        })
        .collect()
}

fn normalize_publish_idempotency_key(
    value: Option<String>,
) -> Result<Option<String>, ContentError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_string();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(ContentError::Validation(
            "发布幂等键不能为空且不能超过 200 个字符".to_string(),
        ));
    }
    Ok(Some(value))
}

fn normalize_public_summary_ids(ids: Vec<String>) -> Result<Vec<String>, ContentError> {
    if ids.len() > 100 {
        return Err(ContentError::Validation(
            "一次最多查询 100 条公开内容摘要".to_string(),
        ));
    }
    let mut seen = std::collections::HashSet::with_capacity(ids.len());
    ids.into_iter()
        .map(|id| id.trim().to_string())
        .map(|id| {
            if id.is_empty() || id.len() > 160 || id.contains(',') || !seen.insert(id.clone()) {
                Err(ContentError::Validation(
                    "公开内容摘要 ID 无效或重复".to_string(),
                ))
            } else {
                Ok(id)
            }
        })
        .collect()
}

fn normalize_list_author_ids(ids: Vec<String>) -> Result<Vec<String>, ContentError> {
    const MAX_BATCH_AUTHORS: usize = 5_000;
    if ids.len() > MAX_BATCH_AUTHORS {
        return Err(ContentError::Validation(format!(
            "一次最多查询 {MAX_BATCH_AUTHORS} 位作者的内容"
        )));
    }
    let mut seen = std::collections::HashSet::with_capacity(ids.len());
    let mut normalized = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id.trim().to_string();
        if id.is_empty() || id.chars().count() > 160 {
            return Err(ContentError::Validation("作者 ID 无效".to_string()));
        }
        if seen.insert(id.clone()) {
            normalized.push(id);
        }
    }
    normalized.sort();
    Ok(normalized)
}

fn validate_route_template(
    content_type: i32,
    template: Option<pb::RouteTemplate>,
) -> Result<Option<pb::RouteTemplate>, ContentError> {
    let content_type = pb::ContentType::try_from(content_type)
        .map_err(|_| ContentError::Validation("invalid content type".to_string()))?;
    let template = match (content_type, template) {
        (pb::ContentType::Route, Some(template)) => template,
        (pb::ContentType::Route, None) => {
            return Err(ContentError::Validation(
                "路线内容需要至少一项可执行的路线模板".to_string(),
            ));
        }
        (_, Some(_)) => {
            return Err(ContentError::Validation(
                "仅路线内容可以携带路线模板".to_string(),
            ));
        }
        (_, None) => return Ok(None),
    };
    validate_text(&template.intent, 1, 300, "路线意图")?;
    validate_text(&template.completion_criteria, 1, 300, "路线完成标准")?;
    validate_enum::<pb::RouteTemplateKind>(template.journey_type, "route template kind")?;
    if template.stages.len() > 12 || template.actions.is_empty() || template.actions.len() > 50 {
        return Err(ContentError::Validation(
            "路线最多包含 12 个阶段和 50 个行动，且至少要有一个行动".to_string(),
        ));
    }
    for stage in &template.stages {
        validate_text(&stage.title, 1, 100, "路线阶段名称")?;
        validate_text(&stage.detail, 0, 500, "路线阶段说明")?;
        validate_text(&stage.completion_criteria, 0, 200, "路线阶段完成标准")?;
    }
    for action in &template.actions {
        validate_text(&action.title, 1, 120, "路线行动名称")?;
        validate_text(&action.detail, 0, 1_000, "路线行动说明")?;
        validate_text(&action.scheduled_label, 1, 80, "路线行动安排")?;
        if action.estimated_minutes == 0 || action.estimated_minutes > 720 {
            return Err(ContentError::Validation(
                "路线行动时长需要在 1 到 720 分钟之间".to_string(),
            ));
        }
        if action.stage_index.is_some_and(|index| {
            usize::try_from(index).map_or(true, |index| index >= template.stages.len())
        }) {
            return Err(ContentError::Validation(
                "路线行动关联了不存在的阶段".to_string(),
            ));
        }
    }
    Ok(Some(template))
}

fn legacy_route_template(title: &str, summary: &str) -> pb::RouteTemplate {
    let intent = if summary.trim().is_empty() {
        title.trim().to_string()
    } else {
        summary.trim().to_string()
    };
    pb::RouteTemplate {
        intent: intent.clone(),
        completion_criteria: "完成路线中的必要阶段和行动".to_string(),
        stages: Vec::new(),
        actions: vec![pb::RouteTemplateAction {
            title: title.trim().to_string(),
            detail: intent,
            estimated_minutes: 20,
            scheduled_label: "开始时".to_string(),
            stage_index: None,
        }],
        journey_type: pb::RouteTemplateKind::Project as i32,
    }
}

fn validate_text(
    value: &str,
    minimum: usize,
    maximum: usize,
    label: &str,
) -> Result<(), ContentError> {
    let length = value.trim().chars().count();
    if length < minimum || length > maximum {
        return Err(ContentError::Validation(format!(
            "{label}长度需要在 {minimum} 到 {maximum} 个字符之间"
        )));
    }
    Ok(())
}

fn content_media_from_assets(
    content_type: i32,
    assets: Vec<media_pb::MediaResource>,
) -> Result<Vec<pb::ContentMedia>, ContentError> {
    let content_type = pb::ContentType::try_from(content_type)
        .map_err(|_| ContentError::Validation("invalid content type".to_string()))?;
    let media = assets
        .into_iter()
        .map(|asset| {
            if asset.status != "ready" || asset.cdn_url.trim().is_empty() {
                return Err(ContentError::Validation(
                    "媒体资源尚未通过公开引用校验".to_string(),
                ));
            }
            let kind = if asset.mime_type.starts_with("image/") {
                "image"
            } else if asset.mime_type.starts_with("video/") {
                "video"
            } else {
                return Err(ContentError::Validation(
                    "公开内容仅支持图片或视频媒体".to_string(),
                ));
            };
            Ok(pb::ContentMedia {
                id: asset.id,
                url: asset.cdn_url,
                kind: kind.to_string(),
                width: asset.width,
                height: asset.height,
                duration_ms: asset.duration_ms,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    match content_type {
        pb::ContentType::Video if !media.iter().any(|item| item.kind == "video") => Err(
            ContentError::Validation("视频内容至少需要一个已处理完成的视频资源".to_string()),
        ),
        pb::ContentType::Video => Ok(media),
        _ if media.iter().any(|item| item.kind != "image") => Err(ContentError::Validation(
            "非视频内容只能附带图片资源".to_string(),
        )),
        _ => Ok(media),
    }
}

fn validate_enum<T>(value: i32, label: &str) -> Result<(), ContentError>
where
    T: TryFrom<i32>,
    T::Error: std::fmt::Debug,
{
    T::try_from(value)
        .map(|_| ())
        .map_err(|_| ContentError::Validation(format!("invalid {label}")))
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{conf::Config, datasource::MemoryContentRepository};

    fn domain() -> Domain {
        Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
                content_audit_grpc_url: None,
                media_grpc_url: "http://127.0.0.1:18091".to_string(),
            },
            Arc::new(MemoryContentRepository::seeded()),
        )
    }

    fn create_request() -> pb::CreateRequest {
        pb::CreateRequest {
            user_id: "user-a".to_string(),
            idempotency_key: Some("operation-1".to_string()),
            title: "一个新练习".to_string(),
            summary: "记录变化".to_string(),
            body: "今天完成了第一个小步骤".to_string(),
            domain: pb::GrowthDomain::Learning as i32,
            content_type: pb::ContentType::Note as i32,
            cover_url: None,
            tags: vec!["成长".to_string()],
            topics: vec!["记录".to_string()],
            route_title: None,
            route_duration: None,
            media_asset_ids: Vec::new(),
            route_template: None,
        }
    }

    #[tokio::test]
    async fn idempotency_returns_the_same_draft() {
        let service = domain();
        let first = service
            .create(create_request())
            .await
            .expect("first create");
        let second = service
            .create(create_request())
            .await
            .expect("retry create");
        assert_eq!(first.id, second.id);
        assert_eq!(first.status, pb::ContentStatus::Draft as i32);
    }

    #[tokio::test]
    async fn notes_drop_route_metadata_and_never_advertise_adoption() {
        let content = domain()
            .create(pb::CreateRequest {
                route_title: Some("作者的私人计划".to_string()),
                route_duration: Some("每天 7:00".to_string()),
                ..create_request()
            })
            .await
            .expect("note should be created");
        let post = content.post.expect("content summary");

        assert!(!post.is_route);
        assert!(post.route_title.is_empty());
        assert!(post.route_duration.is_empty());
    }

    #[test]
    fn historical_notes_are_normalized_before_being_served() {
        let content = normalize_route_summary(pb::Content {
            content_type: pb::ContentType::Note as i32,
            post: Some(pb::PostSummary {
                route_title: "过期路线标题".to_string(),
                route_duration: "每天 7:00".to_string(),
                is_route: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        let post = content.post.expect("content summary");

        assert!(!post.is_route);
        assert!(post.route_title.is_empty());
        assert!(post.route_duration.is_empty());
    }

    #[tokio::test]
    async fn public_reads_do_not_expose_a_draft() {
        let service = domain();
        let draft = service
            .create(create_request())
            .await
            .expect("create draft");
        assert!(matches!(
            service.get_public(&draft.id).await,
            Err(ContentError::Repository(RepositoryError::NotFound(id))) if id == draft.id
        ));
    }

    #[tokio::test]
    async fn batch_author_list_is_fresh_and_does_not_leak_other_authors() {
        let service = domain();
        let page = service
            .list(pb::ListRequest {
                status: Some(pb::ContentStatus::Published as i32),
                strategy: Some("fresh".to_string()),
                author_ids: vec!["author-zhiy".to_string(), "author-yice".to_string()],
                ..Default::default()
            })
            .await
            .expect("batch author list");

        assert_eq!(
            page.items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["post-reading", "post-museum"],
            "the author batch is merged in newest-first order"
        );
        assert!(
            page.items
                .iter()
                .all(|item| matches!(item.author_id.as_str(), "author-yice" | "author-zhiy"))
        );
        assert!(matches!(
            service
                .list(pb::ListRequest {
                    author_id: Some("author-yice".to_string()),
                    author_ids: vec!["author-yice".to_string()],
                    ..Default::default()
                })
                .await,
            Err(ContentError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn public_summary_batch_is_ordered_and_omits_nonpublic_content() {
        let service = domain();
        let draft = service
            .create(create_request())
            .await
            .expect("create draft");
        service
            .restrict(pb::RestrictRequest {
                content_id: "post-city-walk".to_string(),
            })
            .await
            .expect("restrict seeded content");

        let summaries = service
            .get_public_summaries(pb::PublicContentSummariesRequest {
                ids: vec![
                    draft.id,
                    "post-city-walk".to_string(),
                    "missing-content".to_string(),
                    "post-museum".to_string(),
                    "post-reading".to_string(),
                ],
            })
            .await
            .expect("public summaries");

        assert_eq!(
            summaries
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["post-museum", "post-reading"]
        );
        let museum = &summaries.items[0];
        assert_eq!(museum.author_id, "author-zhiy");
        assert_eq!(museum.content_type, pb::ContentType::Route as i32);
        assert_eq!(museum.topics, vec!["博物馆", "观察"]);
        assert_eq!(
            museum.post.as_ref().map(|post| post.title.as_str()),
            Some("不做功课，也能认真看完一场展")
        );
    }

    #[test]
    fn public_summary_ids_are_bounded_unique_and_safe_for_batch_filtering() {
        assert_eq!(
            normalize_public_summary_ids(vec![" post-a ".to_string(), "post-b".to_string()])
                .expect("valid IDs"),
            vec!["post-a", "post-b"]
        );
        for ids in [
            vec![" ".to_string()],
            vec!["post-a".to_string(), " post-a ".to_string()],
            vec!["post-a,post-b".to_string()],
            vec!["a".repeat(161)],
            (0..101).map(|index| format!("post-{index}")).collect(),
        ] {
            assert!(matches!(
                normalize_public_summary_ids(ids),
                Err(ContentError::Validation(_))
            ));
        }
    }

    #[test]
    fn batch_author_ids_are_normalized_and_bounded() {
        assert_eq!(
            normalize_list_author_ids(vec![
                " author-b ".to_string(),
                "author-a".to_string(),
                "author-b".to_string(),
            ])
            .expect("valid batch authors"),
            vec!["author-a", "author-b"]
        );
        assert!(normalize_list_author_ids(vec![" ".to_string()]).is_err());
        assert!(
            normalize_list_author_ids((0..5_001).map(|index| format!("author-{index}")).collect(),)
                .is_err()
        );
    }

    #[tokio::test]
    async fn editing_published_content_reopens_audit() {
        let service = domain();
        let draft = service
            .create(create_request())
            .await
            .expect("create draft");
        let published = service
            .publish(pb::PublishRequest {
                user_id: "user-a".to_string(),
                id: draft.id.clone(),
                idempotency_key: None,
            })
            .await
            .expect("publish");
        assert_eq!(published.status, pb::ContentStatus::Published as i32);
        let edited = service
            .update(pb::UpdateRequest {
                user_id: "user-a".to_string(),
                id: draft.id,
                summary: Some("更新后的摘要".to_string()),
                ..Default::default()
            })
            .await
            .expect("update");
        assert_eq!(edited.status, pb::ContentStatus::Reviewing as i32);
        assert!(edited.published_at.is_none());
    }

    #[tokio::test]
    async fn publishing_with_the_same_key_returns_the_original_audit_snapshot() {
        let service = domain();
        let draft = service
            .create(create_request())
            .await
            .expect("create draft");
        let request = pb::PublishRequest {
            user_id: "user-a".to_string(),
            id: draft.id.clone(),
            idempotency_key: Some("publish-operation-1".to_string()),
        };
        let first = service
            .publish(request.clone())
            .await
            .expect("first publish");
        let retry = service.publish(request).await.expect("retry publish");

        assert_eq!(first, retry);
        assert_eq!(first.status, pb::ContentStatus::Published as i32);
        assert_eq!(first.version, draft.version + 1);

        let edited = service
            .update(pb::UpdateRequest {
                user_id: "user-a".to_string(),
                id: draft.id.clone(),
                summary: Some("发布后的编辑必须重新审核".to_string()),
                ..Default::default()
            })
            .await
            .expect("edit published content");
        assert_eq!(edited.status, pb::ContentStatus::Reviewing as i32);

        let delayed_retry = service
            .publish(pb::PublishRequest {
                user_id: "user-a".to_string(),
                id: draft.id,
                idempotency_key: Some("publish-operation-1".to_string()),
            })
            .await
            .expect("delayed retry");
        assert_eq!(delayed_retry, first);
    }

    #[tokio::test]
    async fn restricted_content_cannot_be_republished_by_its_author() {
        let service = domain();
        let draft = service
            .create(create_request())
            .await
            .expect("create draft");
        service
            .restrict(pb::RestrictRequest {
                content_id: draft.id.clone(),
            })
            .await
            .expect("restrict content");

        assert!(matches!(
            service
                .publish(pb::PublishRequest {
                    user_id: "user-a".to_string(),
                    id: draft.id,
                    idempotency_key: Some("publish-restricted".to_string()),
                })
                .await,
            Err(ContentError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn publish_key_cannot_be_reused_for_another_content() {
        let service = domain();
        let first = service
            .create(create_request())
            .await
            .expect("create first draft");
        service
            .publish(pb::PublishRequest {
                user_id: "user-a".to_string(),
                id: first.id,
                idempotency_key: Some("publish-operation-conflict".to_string()),
            })
            .await
            .expect("publish first draft");
        let second = service
            .create(pb::CreateRequest {
                idempotency_key: None,
                title: "第二个练习".to_string(),
                ..create_request()
            })
            .await
            .expect("create second draft");

        assert!(matches!(
            service
                .publish(pb::PublishRequest {
                    user_id: "user-a".to_string(),
                    id: second.id,
                    idempotency_key: Some("publish-operation-conflict".to_string()),
                })
                .await,
            Err(ContentError::Repository(
                RepositoryError::IdempotencyConflict(_)
            ))
        ));
    }

    #[test]
    fn arbitrary_cover_urls_are_rejected() {
        assert!(matches!(
            reject_legacy_cover_url(Some("https://untrusted.example/cover.jpg")),
            Err(ContentError::Validation(_))
        ));
    }

    #[test]
    fn content_media_enforces_compatible_mime_kinds() {
        let image = media_pb::MediaResource {
            id: "0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b".to_string(),
            object_key: "author-a/asset".to_string(),
            mime_type: "image/jpeg".to_string(),
            size_bytes: 128,
            status: "ready".to_string(),
            cdn_url: "https://cdn.example/asset.jpg".to_string(),
            width: 1200,
            height: 900,
            duration_ms: None,
        };
        let video = media_pb::MediaResource {
            id: "0184c5bc-76e7-7c77-8d0d-7a03e1d2a13b".to_string(),
            object_key: "author-a/video".to_string(),
            mime_type: "video/mp4".to_string(),
            size_bytes: 512,
            status: "ready".to_string(),
            cdn_url: "https://cdn.example/video.mp4".to_string(),
            width: 1920,
            height: 1080,
            duration_ms: Some(3_000),
        };

        assert!(
            content_media_from_assets(pb::ContentType::Note as i32, vec![video.clone()]).is_err()
        );
        assert!(content_media_from_assets(pb::ContentType::Video as i32, vec![image]).is_err());
        let accepted = content_media_from_assets(pb::ContentType::Video as i32, vec![video])
            .expect("video post accepts a ready video asset");
        assert_eq!(accepted[0].kind, "video");
    }

    #[test]
    fn route_templates_require_valid_stages_and_actions() {
        let template = pb::RouteTemplate {
            intent: "用小步建立阅读节奏".to_string(),
            completion_criteria: "完成四周的主题阅读和三次复盘".to_string(),
            stages: vec![pb::RouteTemplateStage {
                title: "起步".to_string(),
                detail: "先找到可持续的时段".to_string(),
                completion_criteria: "完成三次阅读".to_string(),
            }],
            actions: vec![pb::RouteTemplateAction {
                title: "读二十分钟".to_string(),
                detail: "只标记一个有用观点".to_string(),
                estimated_minutes: 20,
                scheduled_label: "今晚".to_string(),
                stage_index: Some(0),
            }],
            journey_type: pb::RouteTemplateKind::Project as i32,
        };
        assert!(validate_route_template(pb::ContentType::Route as i32, Some(template)).is_ok());
        assert!(validate_route_template(pb::ContentType::Route as i32, None).is_err());
        assert!(
            validate_route_template(
                pb::ContentType::Note as i32,
                Some(pb::RouteTemplate::default()),
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn route_content_persists_a_structured_template_and_uses_its_title_by_default() {
        let service = domain();
        let route_template = pb::RouteTemplate {
            intent: "在忙碌中恢复阅读".to_string(),
            completion_criteria: "完成四次阅读和一次回望".to_string(),
            stages: vec![pb::RouteTemplateStage {
                title: "起步".to_string(),
                detail: "先找到最小可行时间".to_string(),
                completion_criteria: "完成第一次阅读".to_string(),
            }],
            actions: vec![pb::RouteTemplateAction {
                title: "读二十分钟".to_string(),
                detail: "只记录一个值得保留的观点".to_string(),
                estimated_minutes: 20,
                scheduled_label: "今晚".to_string(),
                stage_index: Some(0),
            }],
            journey_type: pb::RouteTemplateKind::Habit as i32,
        };
        let content = service
            .create(pb::CreateRequest {
                content_type: pb::ContentType::Route as i32,
                route_template: Some(route_template.clone()),
                route_title: None,
                ..create_request()
            })
            .await
            .expect("route content should be created");

        assert_eq!(
            content.post.as_ref().map(|post| post.route_title.as_str()),
            Some("一个新练习")
        );
        assert_eq!(content.post.as_ref().map(|post| post.is_route), Some(true));
        assert_eq!(content.route_template, Some(route_template));
    }

    #[tokio::test]
    async fn legacy_route_creation_receives_a_safe_single_action_template() {
        let service = domain();
        let content = service
            .create(pb::CreateRequest {
                content_type: pb::ContentType::Route as i32,
                ..create_request()
            })
            .await
            .expect("legacy route content should remain creatable");

        let template = content
            .route_template
            .expect("a legacy template is generated");
        assert_eq!(template.actions.len(), 1);
        assert_eq!(template.actions[0].title, "一个新练习");
    }
}
