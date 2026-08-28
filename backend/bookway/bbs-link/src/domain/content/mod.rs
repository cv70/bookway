use bookway_content_audit_api::pb as audit_pb;
use bookway_media_api::pb as media_pb;
use uuid::Uuid;

use crate::{
    api::pb,
    datasource::DaoError,
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
        let mut page = self.dao.list(&query).await?;
        page.items = page
            .items
            .into_iter()
            .map(normalize_content_summary)
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
            let route_actions = content
                .route_template
                .as_ref()
                .map(|template| template.actions.clone())
                .unwrap_or_default();
            summaries.insert(
                content.id.clone(),
                pb::PublicContentSummary {
                    id: content.id,
                    post: Some(post),
                    author_id: content.author_id,
                    content_type: content.content_type,
                    topics: content.topics,
                    quality_score: content.quality_score,
                    route_actions,
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
        Ok(normalize_content_summary(self.dao.get(id).await?))
    }

    pub(crate) async fn get_public(&self, id: &str) -> Result<pb::Content, ContentError> {
        let content = normalize_content_summary(self.dao.get(id).await?);
        if content.status != pb::ContentStatus::Published as i32 {
            return Err(ContentError::Repository(DaoError::NotFound(id.to_string())));
        }
        Ok(content)
    }

    pub(crate) async fn create(
        &self,
        mut request: pb::CreateRequest,
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
        request.idempotency_key = normalize_create_idempotency_key(request.idempotency_key)?;
        validate_enum::<pb::GrowthDomain>(request.domain, "growth domain")?;
        validate_enum::<pb::ContentType>(request.content_type, "content type")?;
        let route_template =
            validate_route_template(request.content_type, request.route_template.clone())?;
        let milestone = self
            .resolve_milestone(
                request.content_type,
                request.domain,
                request.milestone.clone(),
            )
            .await?;
        let question_context = self
            .resolve_question_context(
                request.content_type,
                request.domain,
                request.question_context.clone(),
            )
            .await?;
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
                is_milestone: request.content_type == pb::ContentType::Milestone as i32,
                is_question: request.content_type == pb::ContentType::Question as i32,
                fork_count: 0,
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
            milestone,
            accepted_answer_id: None,
            question_context,
            route_fork: None,
        };
        Ok(normalize_content_summary(
            self.dao
                .create(content, request.idempotency_key, request_fingerprint)
                .await?,
        ))
    }

    pub(crate) async fn fork_route(
        &self,
        mut request: pb::ForkRouteRequest,
    ) -> Result<pb::Content, ContentError> {
        request.user_id = request.user_id.trim().to_string();
        request.source_route_id = request.source_route_id.trim().to_string();
        request.idempotency_key = request.idempotency_key.trim().to_string();
        if let Some(title) = request.title.as_mut() {
            *title = title.trim().to_string();
            if title.is_empty() {
                request.title = None;
            }
        }
        if let Some(summary) = request.summary.as_mut() {
            *summary = summary.trim().to_string();
        }
        let user_id = request.user_id.as_str();
        let source_route_id = request.source_route_id.as_str();
        let idempotency_key = request.idempotency_key.as_str();
        if user_id.is_empty() || source_route_id.is_empty() || idempotency_key.is_empty() {
            return Err(ContentError::Validation(
                "用户、来源路线和幂等键不能为空".to_string(),
            ));
        }
        if idempotency_key.chars().count() > 200 {
            return Err(ContentError::Validation(
                "Fork 幂等键不能超过 200 个字符".to_string(),
            ));
        }

        let source = self.get_public(source_route_id).await?;
        if source.content_type != pb::ContentType::Route as i32 {
            return Err(ContentError::Validation(
                "只能 Fork 当前公开的路线".to_string(),
            ));
        }
        if source.author_id == user_id {
            return Err(ContentError::Validation(
                "不能 Fork 自己的路线；请直接编辑原路线".to_string(),
            ));
        }
        let source_post = source.post.as_ref().ok_or_else(|| {
            ContentError::Repository(DaoError::InvalidContent(
                "public route is missing its post summary".to_string(),
            ))
        })?;
        let route_template =
            validate_route_template(pb::ContentType::Route as i32, source.route_template.clone())?;
        let title = request
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}（分支）", source_post.title));
        validate_text(&title, 1, 120, "Fork 路线标题")?;
        let summary = request
            .summary
            .as_deref()
            .map(str::trim)
            .map(str::to_string)
            .unwrap_or_else(|| source_post.summary.clone());
        validate_text(&summary, 0, 300, "Fork 路线摘要")?;
        if source.body.trim().is_empty() {
            return Err(ContentError::Validation(
                "来源路线没有可 Fork 的正文".to_string(),
            ));
        }
        let source_route_title = source_post.title.clone();
        let source_domain = source_post.domain;
        let source_duration = source_post.route_duration.clone();
        let source_tags = source_post.tags.clone();
        let source_route_id = source.id.clone();
        let source_route_version = source.version;
        let source_author_id = source.author_id.clone();

        let request_fingerprint = serde_json::to_string(&request)
            .map_err(|error| ContentError::Validation(error.to_string()))?;
        let id = Uuid::now_v7().to_string();
        let forked_at = now_rfc3339();
        let content = pb::Content {
            id: id.clone(),
            post: Some(pb::PostSummary {
                id,
                author_name: user_id.to_string(),
                author_avatar_url: String::new(),
                title: title.clone(),
                summary,
                domain: source_domain,
                cover_url: String::new(),
                route_title: title,
                route_duration: source_duration,
                join_count: 0,
                like_count: 0,
                freshness: 1.0,
                tags: source_tags,
                is_route: true,
                is_milestone: false,
                is_question: false,
            fork_count: 0,
            }),
            author_id: user_id.to_string(),
            content_type: pb::ContentType::Route as i32,
            status: pb::ContentStatus::Draft as i32,
            body: source.body,
            // Source media is owned by another author. A Fork copies public
            // route structure and text, never an asset ownership reference.
            media: Vec::new(),
            topics: source.topics,
            created_at: forked_at.clone(),
            published_at: None,
            version: 1,
            quality_score: 0.0,
            route_template,
            milestone: None,
            accepted_answer_id: None,
            question_context: None,
            route_fork: Some(pb::RouteFork {
                source_route_id,
                source_route_version,
                source_route_title,
                source_route_author_id: source_author_id,
                forked_at,
            }),
        };
        Ok(normalize_content_summary(
            self.dao
                .create(
                    content,
                    Some(idempotency_key.to_string()),
                    request_fingerprint,
                )
                .await?,
        ))
    }

    pub(crate) async fn update(
        &self,
        request: pb::UpdateRequest,
    ) -> Result<pb::Content, ContentError> {
        let mut content = normalize_content_summary(self.dao.get(&request.id).await?);
        if content.author_id != request.user_id {
            return Err(ContentError::Forbidden);
        }
        let post = content.post.as_mut().ok_or_else(|| {
            ContentError::Repository(DaoError::InvalidContent(
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
            let route_template =
                validate_route_template(content.content_type, Some(route_template))?;
            if content.status == pb::ContentStatus::Published as i32
                && action_node_commercial_context(content.route_template.as_ref())
                    != action_node_commercial_context(route_template.as_ref())
            {
                return Err(ContentError::Validation(
                    "已发布路线不能变更行动节点或其场景装备；请新建路线版本".to_string(),
                ));
            }
            content.route_template = route_template;
        }
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
        if let Some(milestone) = request.milestone {
            content.milestone = self
                .resolve_milestone(content.content_type, post.domain, Some(milestone))
                .await?;
        }
        content.version = content.version.saturating_add(1);
        if content.status == pb::ContentStatus::Published as i32 {
            content.status = pb::ContentStatus::Reviewing as i32;
            content.published_at = None;
        }
        Ok(normalize_content_summary(self.dao.update(content).await?))
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
                .dao
                .published_by_idempotency_key(&request.user_id, key, &request_fingerprint)
                .await?
        {
            return Ok(normalize_content_summary(content));
        }
        let mut content = normalize_content_summary(self.dao.get(&request.id).await?);
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
                return Err(ContentError::Repository(DaoError::InvalidContent(
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
                ContentError::Repository(DaoError::InvalidContent(
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
        Ok(normalize_content_summary(
            self.dao
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
        let mut content = normalize_content_summary(self.dao.get(request.content_id.trim()).await?);
        if matches!(
            pb::ContentStatus::try_from(content.status),
            Ok(pb::ContentStatus::Restricted | pb::ContentStatus::Deleted)
        ) {
            return Ok(content);
        }
        content.status = pb::ContentStatus::Restricted as i32;
        content.published_at = None;
        content.version = content.version.saturating_add(1);
        Ok(normalize_content_summary(self.dao.update(content).await?))
    }

    pub(crate) async fn accept_answer(
        &self,
        request: pb::AcceptAnswerRequest,
    ) -> Result<pb::Content, ContentError> {
        if request.user_id.trim().is_empty()
            || request.question_id.trim().is_empty()
            || request.answer_id.trim().is_empty()
        {
            return Err(ContentError::Validation(
                "问题、回答和用户不能为空".to_string(),
            ));
        }
        if request.answer_id.len() > 160 {
            return Err(ContentError::Validation("回答 ID 无效".to_string()));
        }
        let mut question = normalize_content_summary(self.dao.get(&request.question_id).await?);
        if question.author_id != request.user_id {
            return Err(ContentError::Forbidden);
        }
        if question.content_type != pb::ContentType::Question as i32
            || question.status != pb::ContentStatus::Published as i32
        {
            return Err(ContentError::Validation(
                "只能为已发布的问题采纳回答".to_string(),
            ));
        }
        if question.accepted_answer_id.as_deref() == Some(request.answer_id.trim()) {
            return Ok(question);
        }
        question.accepted_answer_id = Some(request.answer_id.trim().to_string());
        question.version = question.version.saturating_add(1);
        Ok(normalize_content_summary(self.dao.update(question).await?))
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
        let mut content = normalize_content_summary(self.dao.get(request.content_id.trim()).await?);
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
        Ok(normalize_content_summary(self.dao.update(content).await?))
    }

    async fn resolve_milestone(
        &self,
        content_type: i32,
        domain: i32,
        milestone: Option<pb::MilestoneDraft>,
    ) -> Result<Option<pb::Milestone>, ContentError> {
        let content_type = pb::ContentType::try_from(content_type)
            .map_err(|_| ContentError::Validation("invalid content type".to_string()))?;
        let draft = match (content_type, milestone) {
            (pb::ContentType::Milestone, Some(draft)) => draft,
            (pb::ContentType::Milestone, None) => {
                return Err(ContentError::Validation(
                    "阶段成果必须关联一条公开路线和阶段".to_string(),
                ));
            }
            (_, Some(_)) => {
                return Err(ContentError::Validation(
                    "仅阶段成果内容可以携带阶段成果结构".to_string(),
                ));
            }
            (_, None) => return Ok(None),
        };
        let route_id = normalize_public_reference_id(&draft.route_id, "关联路线 ID")?;
        validate_text(&draft.effort_summary, 1, 300, "阶段投入")?;
        validate_text(&draft.outcome_summary, 1, 1_000, "阶段结果")?;
        validate_text(&draft.adjustment_summary, 0, 600, "阶段调整")?;
        validate_text(&draft.evidence_scope, 1, 300, "证据范围")?;
        let stage_index = draft.stage_index.ok_or_else(|| {
            ContentError::Validation("阶段成果必须关联路线中的一个阶段".to_string())
        })?;

        // A milestone can only point to content that is currently public. The
        // snapshot below is public route metadata, never a private plan read.
        let route = match self.dao.get(&route_id).await {
            Ok(route) => normalize_content_summary(route),
            Err(DaoError::NotFound(_)) => {
                return Err(ContentError::Validation(
                    "关联路线不存在或当前不可公开访问".to_string(),
                ));
            }
            Err(error) => return Err(ContentError::Repository(error)),
        };
        if route.status != pb::ContentStatus::Published as i32
            || route.content_type != pb::ContentType::Route as i32
        {
            return Err(ContentError::Validation(
                "阶段成果只能关联当前公开的路线".to_string(),
            ));
        }
        let post = route.post.as_ref().ok_or_else(|| {
            ContentError::Repository(DaoError::InvalidContent(
                "public route is missing its post summary".to_string(),
            ))
        })?;
        if post.domain != domain {
            return Err(ContentError::Validation(
                "阶段成果的成长领域必须与关联路线一致".to_string(),
            ));
        }
        let route_template = route
            .route_template
            .as_ref()
            .ok_or_else(|| ContentError::Validation("关联路线没有可验证的阶段模板".to_string()))?;
        let stage = route_template
            .stages
            .get(stage_index as usize)
            .ok_or_else(|| ContentError::Validation("关联路线中不存在该阶段".to_string()))?;
        Ok(Some(pb::Milestone {
            route_id,
            route_title: post.title.clone(),
            stage_index,
            stage_title: stage.title.clone(),
            effort_summary: draft.effort_summary.trim().to_string(),
            outcome_summary: draft.outcome_summary.trim().to_string(),
            adjustment_summary: draft.adjustment_summary.trim().to_string(),
            evidence_scope: draft.evidence_scope.trim().to_string(),
        }))
    }

    async fn resolve_question_context(
        &self,
        content_type: i32,
        domain: i32,
        question_context: Option<pb::QuestionContextDraft>,
    ) -> Result<Option<pb::QuestionContext>, ContentError> {
        let content_type = pb::ContentType::try_from(content_type)
            .map_err(|_| ContentError::Validation("invalid content type".to_string()))?;
        let draft = match (content_type, question_context) {
            (pb::ContentType::Question, Some(draft)) => draft,
            (_, Some(_)) => {
                return Err(ContentError::Validation(
                    "只有问题内容可以关联路线上下文".to_string(),
                ));
            }
            (_, None) => return Ok(None),
        };
        let route_id = normalize_public_reference_id(&draft.route_id, "关联路线 ID")?;
        // This is intentionally a public content read. A question may describe
        // an execution blockage without exposing a private plan's progress.
        let route = match self.dao.get(&route_id).await {
            Ok(route) => normalize_content_summary(route),
            Err(DaoError::NotFound(_)) => {
                return Err(ContentError::Validation(
                    "关联路线不存在或当前不可公开访问".to_string(),
                ));
            }
            Err(error) => return Err(ContentError::Repository(error)),
        };
        if route.status != pb::ContentStatus::Published as i32
            || route.content_type != pb::ContentType::Route as i32
        {
            return Err(ContentError::Validation(
                "问题只能关联当前公开的路线".to_string(),
            ));
        }
        let post = route.post.as_ref().ok_or_else(|| {
            ContentError::Repository(DaoError::InvalidContent(
                "public route is missing its post summary".to_string(),
            ))
        })?;
        if post.domain != domain {
            return Err(ContentError::Validation(
                "问题的成长领域必须与关联路线一致".to_string(),
            ));
        }
        let (stage_index, stage_title) = if let Some(stage_index) = draft.stage_index {
            let route_template = route.route_template.as_ref().ok_or_else(|| {
                ContentError::Validation("关联路线没有可验证的阶段模板".to_string())
            })?;
            let stage = route_template
                .stages
                .get(stage_index as usize)
                .ok_or_else(|| ContentError::Validation("关联路线中不存在该阶段".to_string()))?;
            (Some(stage_index), Some(stage.title.clone()))
        } else {
            (None, None)
        };
        Ok(Some(pb::QuestionContext {
            route_id,
            route_title: post.title.clone(),
            stage_index,
            stage_title,
        }))
    }
}

fn normalize_content_summary(mut content: pb::Content) -> pb::Content {
    let is_route = content.content_type == pb::ContentType::Route as i32;
    let is_milestone = content.content_type == pb::ContentType::Milestone as i32;
    let is_question = content.content_type == pb::ContentType::Question as i32;
    if let Some(post) = content.post.as_mut() {
        post.is_route = is_route;
        post.is_milestone = is_milestone;
        post.is_question = is_question;
        if !is_route {
            // Only reusable public routes can advertise route adoption metadata.
            post.route_title.clear();
            post.route_duration.clear();
        }
    }
    if !is_milestone {
        content.milestone = None;
    }
    if !is_question {
        content.accepted_answer_id = None;
        content.question_context = None;
    }
    if !is_route {
        content.route_fork = None;
    }
    content
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

fn normalize_create_idempotency_key(value: Option<String>) -> Result<Option<String>, ContentError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_string();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(ContentError::Validation(
            "创建幂等键不能为空且不能超过 200 个字符".to_string(),
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

fn normalize_public_reference_id(value: &str, label: &str) -> Result<String, ContentError> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > 160
        || value.chars().any(char::is_control)
        || value.contains(',')
    {
        return Err(ContentError::Validation(format!("{label}无效")));
    }
    Ok(value.to_string())
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
        validate_action_node_id(&action.id)?;
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
        validate_scene_equipment(&action.scene_equipment)?;
    }
    let action_ids = template
        .actions
        .iter()
        .map(|action| action.id.trim())
        .collect::<std::collections::HashSet<_>>();
    if action_ids.len() != template.actions.len() {
        return Err(ContentError::Validation(
            "路线行动节点 ID 必须唯一".to_string(),
        ));
    }
    Ok(Some(template))
}

fn validate_action_node_id(value: &str) -> Result<(), ContentError> {
    let raw = value;
    let value = raw.trim();
    if value.is_empty()
        || raw != value
        || value.chars().count() > 160
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(ContentError::Validation(
            "路线行动节点 ID 只能包含字母、数字、-、_ 或 .，长度不超过 160".to_string(),
        ));
    }
    Ok(())
}

fn action_node_commercial_context(
    template: Option<&pb::RouteTemplate>,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    template
        .into_iter()
        .flat_map(|template| template.actions.iter())
        .map(|action| {
            let equipment = action
                .scene_equipment
                .iter()
                .map(|value| scene_equipment_key(value))
                .collect();
            (action.id.trim().to_string(), equipment)
        })
        .collect()
}

fn validate_scene_equipment(values: &[String]) -> Result<(), ContentError> {
    if values.len() > 12 {
        return Err(ContentError::Validation(
            "每个行动节点最多配置 12 项场景装备".to_string(),
        ));
    }
    let mut seen = std::collections::HashSet::with_capacity(values.len());
    for value in values {
        validate_text(value, 1, 80, "场景装备")?;
        if value.trim() != value || !seen.insert(scene_equipment_key(value)) {
            return Err(ContentError::Validation(
                "场景装备必须去重且不能包含首尾空白".to_string(),
            ));
        }
    }
    Ok(())
}

fn scene_equipment_key(value: &str) -> String {
    value.trim().to_lowercase()
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
    use crate::{conf::Config, datasource::MemoryContentDao};

    fn domain() -> Domain {
        Domain::from_dao(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
                content_audit_grpc_url: None,
                media_grpc_url: "http://127.0.0.1:18091".to_string(),
            },
            Arc::new(MemoryContentDao::seeded()),
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
            tags: vec!["成长".to_string()],
            topics: vec!["记录".to_string()],
            route_title: None,
            route_duration: None,
            media_asset_ids: Vec::new(),
            route_template: None,
            milestone: None,
            question_context: None,
        }
    }

    fn milestone_draft(route_id: &str) -> pb::MilestoneDraft {
        pb::MilestoneDraft {
            route_id: route_id.to_string(),
            stage_index: Some(0),
            effort_summary: "连续 7 天每天阅读 20 分钟，并完成两次整理".to_string(),
            outcome_summary: "从零散阅读转为能复述一个主题的完整观点".to_string(),
            adjustment_summary: "把周中整理改为周末集中完成".to_string(),
            evidence_scope: "本帖图片和正文仅覆盖本周公开的阅读产出".to_string(),
        }
    }

    fn route_template() -> pb::RouteTemplate {
        pb::RouteTemplate {
            intent: "用小步建立阅读节奏".to_string(),
            completion_criteria: "完成四周的主题阅读和三次复盘".to_string(),
            stages: vec![pb::RouteTemplateStage {
                title: "起步".to_string(),
                detail: "先找到可持续的时段".to_string(),
                completion_criteria: "完成三次阅读".to_string(),
            }],
            actions: vec![pb::RouteTemplateAction {
                id: "read-20".to_string(),
                title: "读二十分钟".to_string(),
                detail: "只标记一个有用观点".to_string(),
                estimated_minutes: 20,
                scheduled_label: "今晚".to_string(),
                stage_index: Some(0),
                scene_equipment: vec!["阅读灯".to_string()],
            }],
            journey_type: pb::RouteTemplateKind::Project as i32,
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
    async fn fork_copies_a_public_route_snapshot_into_an_editable_draft() {
        let service = domain();
        let request = pb::ForkRouteRequest {
            user_id: "user-a".to_string(),
            source_route_id: "post-reading".to_string(),
            idempotency_key: "fork-reading-1".to_string(),
            title: Some("我自己的主题阅读分支".to_string()),
            summary: None,
        };

        let first = service
            .fork_route(request.clone())
            .await
            .expect("public route should be forkable");
        let retry = service
            .fork_route(request)
            .await
            .expect("fork retry should replay the draft");

        assert_eq!(first.id, retry.id);
        assert_eq!(first.author_id, "user-a");
        assert_eq!(first.status, pb::ContentStatus::Draft as i32);
        assert!(
            first.media.is_empty(),
            "a fork cannot copy another user's media"
        );
        assert_eq!(
            first
                .route_fork
                .as_ref()
                .map(|fork| fork.source_route_id.as_str()),
            Some("post-reading")
        );
        assert_eq!(
            first
                .route_template
                .as_ref()
                .map(|template| template.actions[0].scene_equipment.as_slice()),
            Some(["行动记录工具".to_string()].as_slice())
        );
        assert_eq!(
            service
                .get_public("post-reading")
                .await
                .expect("source remains public")
                .author_id,
            "author-yice"
        );
    }

    #[tokio::test]
    async fn fork_rejects_a_private_or_self_owned_route() {
        let service = domain();
        let private_route = service
            .create(pb::CreateRequest {
                content_type: pb::ContentType::Route as i32,
                route_template: Some(route_template()),
                ..create_request()
            })
            .await
            .expect("draft route should be created");
        let private_fork = service
            .fork_route(pb::ForkRouteRequest {
                user_id: "user-b".to_string(),
                source_route_id: private_route.id,
                idempotency_key: "fork-private-1".to_string(),
                title: None,
                summary: None,
            })
            .await;
        assert!(matches!(
            private_fork,
            Err(ContentError::Repository(DaoError::NotFound(_)))
        ));

        let own_fork = service
            .fork_route(pb::ForkRouteRequest {
                user_id: "author-yice".to_string(),
                source_route_id: "post-reading".to_string(),
                idempotency_key: "fork-own-1".to_string(),
                title: None,
                summary: None,
            })
            .await;
        assert!(matches!(own_fork, Err(ContentError::Validation(_))));
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

    #[tokio::test]
    async fn question_author_can_select_an_answer_without_republishing() {
        let service = domain();
        let question = service
            .create(pb::CreateRequest {
                idempotency_key: Some("question-create-1".to_string()),
                title: "跑步后膝盖不适，该怎样调整训练？".to_string(),
                summary: "希望得到循序渐进的训练建议。".to_string(),
                body: "连续两周跑步后出现轻微不适，想知道应该先调整哪些变量。".to_string(),
                content_type: pb::ContentType::Question as i32,
                ..create_request()
            })
            .await
            .expect("question should be created");
        let question = service
            .publish(pb::PublishRequest {
                user_id: "user-a".to_string(),
                id: question.id,
                idempotency_key: Some("question-publish-1".to_string()),
            })
            .await
            .expect("question should publish");
        let accepted = service
            .accept_answer(pb::AcceptAnswerRequest {
                user_id: "user-a".to_string(),
                question_id: question.id,
                answer_id: "answer-1".to_string(),
            })
            .await
            .expect("question author can select an answer");

        assert_eq!(accepted.accepted_answer_id.as_deref(), Some("answer-1"));
        assert!(accepted.post.expect("question summary").is_question);
        assert_eq!(accepted.status, pb::ContentStatus::Published as i32);
    }

    #[tokio::test]
    async fn question_snapshots_a_public_route_stage_without_reading_private_progress() {
        let service = domain();
        let question = service
            .create(pb::CreateRequest {
                idempotency_key: Some("question-context-create-1".to_string()),
                title: "主题阅读的起步阶段总是拖延，应该先改哪里？".to_string(),
                summary: "想在真正放弃前调整路线。".to_string(),
                body: "我只希望讨论路线的第一阶段，不公开自己的日程或阅读记录。".to_string(),
                content_type: pb::ContentType::Question as i32,
                question_context: Some(pb::QuestionContextDraft {
                    route_id: "post-reading".to_string(),
                    stage_index: Some(0),
                }),
                ..create_request()
            })
            .await
            .expect("question context should resolve from a public route");

        let context = question
            .question_context
            .expect("resolved question context");
        assert_eq!(context.route_id, "post-reading");
        assert_eq!(
            context.route_title,
            "读完 12 本书后，我留下了这套主题阅读法"
        );
        assert_eq!(context.stage_index, Some(0));
        assert_eq!(context.stage_title.as_deref(), Some("从第一步开始"));
    }

    #[tokio::test]
    async fn question_context_rejects_private_or_cross_domain_routes() {
        let service = domain();
        let private_route = service
            .create(pb::CreateRequest {
                idempotency_key: Some("private-question-route-1".to_string()),
                content_type: pb::ContentType::Route as i32,
                route_template: Some(route_template()),
                ..create_request()
            })
            .await
            .expect("draft route should be created");
        let private_result = service
            .create(pb::CreateRequest {
                idempotency_key: Some("question-private-context-1".to_string()),
                content_type: pb::ContentType::Question as i32,
                question_context: Some(pb::QuestionContextDraft {
                    route_id: private_route.id,
                    stage_index: None,
                }),
                ..create_request()
            })
            .await;
        assert!(matches!(private_result, Err(ContentError::Validation(_))));

        let cross_domain = service
            .create(pb::CreateRequest {
                idempotency_key: Some("question-domain-context-1".to_string()),
                content_type: pb::ContentType::Question as i32,
                domain: pb::GrowthDomain::Travel as i32,
                question_context: Some(pb::QuestionContextDraft {
                    route_id: "post-reading".to_string(),
                    stage_index: None,
                }),
                ..create_request()
            })
            .await;
        assert!(matches!(cross_domain, Err(ContentError::Validation(_))));
    }

    #[tokio::test]
    async fn only_published_question_author_can_select_an_answer() {
        let service = domain();
        let note = service.create(create_request()).await.expect("create note");
        let error = service
            .accept_answer(pb::AcceptAnswerRequest {
                user_id: "user-a".to_string(),
                question_id: note.id,
                answer_id: "answer-1".to_string(),
            })
            .await
            .expect_err("a draft note cannot accept an answer");
        assert!(matches!(error, ContentError::Validation(_)));
    }

    #[test]
    fn historical_notes_are_normalized_before_being_served() {
        let content = normalize_content_summary(pb::Content {
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
    async fn milestones_snapshot_a_public_route_stage_without_copying_private_plans() {
        let service = domain();
        let content = service
            .create(pb::CreateRequest {
                idempotency_key: Some("milestone-create-1".to_string()),
                title: "第一周终于找到了阅读节奏".to_string(),
                summary: "完成起步阶段后，对方法做了一次小调整。".to_string(),
                body: "我把每天的阅读缩短到二十分钟，并在周末统一整理。".to_string(),
                content_type: pb::ContentType::Milestone as i32,
                milestone: Some(milestone_draft("post-reading")),
                ..create_request()
            })
            .await
            .expect("public route milestone should be creatable");

        let post = content.post.expect("milestone summary");
        assert!(!post.is_route);
        assert!(post.is_milestone);
        assert!(post.route_title.is_empty());
        let milestone = content.milestone.expect("resolved milestone");
        assert_eq!(milestone.route_id, "post-reading");
        assert_eq!(
            milestone.route_title,
            "读完 12 本书后，我留下了这套主题阅读法"
        );
        assert_eq!(milestone.stage_index, 0);
        assert_eq!(milestone.stage_title, "从第一步开始");
        assert!(milestone.effort_summary.contains("7 天"));
    }

    #[tokio::test]
    async fn milestones_reject_nonpublic_routes_before_exposing_an_association() {
        let service = domain();
        let private_route = service
            .create(pb::CreateRequest {
                idempotency_key: Some("private-route-create-1".to_string()),
                content_type: pb::ContentType::Route as i32,
                route_template: Some(route_template()),
                ..create_request()
            })
            .await
            .expect("draft route should be created");

        let result = service
            .create(pb::CreateRequest {
                idempotency_key: Some("milestone-private-route-1".to_string()),
                content_type: pb::ContentType::Milestone as i32,
                milestone: Some(milestone_draft(&private_route.id)),
                ..create_request()
            })
            .await;

        assert!(matches!(result, Err(ContentError::Validation(_))));
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
            Err(ContentError::Repository(DaoError::NotFound(id))) if id == draft.id
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
            Err(ContentError::Repository(DaoError::IdempotencyConflict(_)))
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
        let template = route_template();
        assert!(validate_route_template(pb::ContentType::Route as i32, Some(template)).is_ok());
        assert!(validate_route_template(pb::ContentType::Route as i32, None).is_err());
        assert!(
            validate_route_template(
                pb::ContentType::Note as i32,
                Some(pb::RouteTemplate::default()),
            )
            .is_err()
        );
        let mut duplicate_equipment = route_template();
        duplicate_equipment.actions[0].scene_equipment =
            vec!["阅读灯".to_string(), "阅读灯".to_string()];
        assert!(
            validate_route_template(pb::ContentType::Route as i32, Some(duplicate_equipment),)
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
                id: "read-20".to_string(),
                title: "读二十分钟".to_string(),
                detail: "只记录一个值得保留的观点".to_string(),
                estimated_minutes: 20,
                scheduled_label: "今晚".to_string(),
                stage_index: Some(0),
                scene_equipment: vec!["阅读笔记本".to_string()],
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
    async fn route_creation_requires_a_structured_action_template() {
        let service = domain();
        let result = service
            .create(pb::CreateRequest {
                content_type: pb::ContentType::Route as i32,
                ..create_request()
            })
            .await;
        assert!(matches!(result, Err(ContentError::Validation(_))));
    }
}
