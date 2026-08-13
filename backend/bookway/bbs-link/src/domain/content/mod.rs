use bookway_api::{
    AuditDecisionDto, ContentAuditRequest, ContentDto, ContentPageDto, ContentQueryRequest,
    ContentStatusDto, CreateContentRequest, UpdateContentRequest,
};
use uuid::Uuid;

use crate::datasource::RepositoryError;
use crate::domain::{ContentError, Domain};

impl Domain {
    pub(crate) async fn list(
        &self,
        query: ContentQueryRequest,
    ) -> Result<ContentPageDto, ContentError> {
        Ok(self.repository.list(&query).await?)
    }

    pub(crate) async fn get(&self, id: &str) -> Result<ContentDto, ContentError> {
        Ok(self.repository.get(id).await?)
    }

    pub(crate) async fn get_public(&self, id: &str) -> Result<ContentDto, ContentError> {
        let content = self.repository.get(id).await?;
        if content.status != ContentStatusDto::Published {
            return Err(ContentError::Repository(RepositoryError::NotFound(
                id.to_string(),
            )));
        }
        Ok(content)
    }

    pub(crate) async fn create(
        &self,
        author_id: &str,
        request: CreateContentRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentDto, ContentError> {
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
        let request_fingerprint = serde_json::to_string(&request)
            .map_err(|error| ContentError::Validation(error.to_string()))?;
        let id = Uuid::now_v7().to_string();
        let now = now_rfc3339();
        let post = bookway_api::PostSummaryDto {
            id: id.clone(),
            author_name: author_id.to_string(),
            author_avatar_url: String::new(),
            title: request.title.trim().to_string(),
            summary: request.summary.trim().to_string(),
            domain: request.domain,
            cover_url: request.cover_url.clone().unwrap_or_default(),
            route_title: request.route_title.clone().unwrap_or_default(),
            route_duration: request.route_duration.clone().unwrap_or_default(),
            join_count: 0,
            like_count: 0,
            freshness: 1.0,
            tags: request.tags.clone(),
        };
        let content = ContentDto {
            id,
            post,
            author_id: author_id.to_string(),
            content_type: request.content_type,
            status: ContentStatusDto::Draft,
            body: request.body,
            media: Vec::new(),
            topics: request.topics,
            created_at: now.clone(),
            published_at: None,
            version: 1,
            quality_score: 0.0,
        };
        Ok(self
            .repository
            .create(content, idempotency_key, request_fingerprint)
            .await?)
    }

    pub(crate) async fn update(
        &self,
        author_id: &str,
        id: &str,
        request: UpdateContentRequest,
    ) -> Result<ContentDto, ContentError> {
        let mut content = self.repository.get(id).await?;
        if content.author_id != author_id {
            return Err(ContentError::Forbidden);
        }
        if let Some(title) = request.title {
            if title.trim().is_empty() {
                return Err(ContentError::Validation("标题不能为空".to_string()));
            }
            content.post.title = title.trim().to_string();
        }
        if let Some(summary) = request.summary {
            content.post.summary = summary;
        }
        if let Some(body) = request.body {
            if body.trim().is_empty() {
                return Err(ContentError::Validation("正文不能为空".to_string()));
            }
            content.body = body;
        }
        if let Some(tags) = request.tags {
            content.post.tags = tags;
        }
        if let Some(topics) = request.topics {
            content.topics = topics;
        }
        if let Some(cover_url) = request.cover_url {
            content.post.cover_url = cover_url;
        }
        content.version = content.version.saturating_add(1);
        // Any edit to a published item changes the reviewed bytes. Keep the
        // previous public version out of feeds until the new version passes
        // the same audit gate as an initial publication.
        if matches!(content.status, ContentStatusDto::Published) {
            content.status = ContentStatusDto::Reviewing;
            content.published_at = None;
        }
        Ok(self.repository.update(content).await?)
    }

    pub(crate) async fn publish(
        &self,
        author_id: &str,
        id: &str,
    ) -> Result<ContentDto, ContentError> {
        let mut content = self.repository.get(id).await?;
        if content.author_id != author_id {
            return Err(ContentError::Forbidden);
        }
        if content.body.trim().is_empty() {
            return Err(ContentError::Validation(
                "没有正文的内容不能发布".to_string(),
            ));
        }
        let next_version = content.version.saturating_add(1);
        let audit = self
            .auditor
            .audit(ContentAuditRequest {
                content_id: content.id.clone(),
                version: next_version,
                title: content.post.title.clone(),
                body: content.body.clone(),
            })
            .await
            .map_err(ContentError::Audit)?;
        let (status, published_at) = match audit.decision {
            AuditDecisionDto::Approved => (ContentStatusDto::Published, Some(now_rfc3339())),
            AuditDecisionDto::Reviewing => (ContentStatusDto::Reviewing, None),
            AuditDecisionDto::Restricted => (ContentStatusDto::Restricted, None),
        };
        content.status = status;
        content.published_at = published_at;
        content.version = next_version;
        Ok(self.repository.update(content).await?)
    }
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use bookway_api::ContentAuditResponse;

    use super::*;
    use crate::{
        conf::Config,
        datasource::{ContentAuditor, LocalContentAuditor, MemoryContentRepository},
    };

    fn config() -> Config {
        Config {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            grpc_addr: "127.0.0.1:0".parse().unwrap(),
            content_audit_grpc_url: None,
        }
    }

    struct StaticAuditor(AuditDecisionDto);

    #[async_trait]
    impl ContentAuditor for StaticAuditor {
        async fn audit(
            &self,
            _request: ContentAuditRequest,
        ) -> Result<ContentAuditResponse, String> {
            Ok(ContentAuditResponse {
                decision: self.0,
                risk_score: 0.5,
                reasons: Vec::new(),
                provider: "test".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn idempotency_returns_the_same_draft() {
        let domain = Domain::from_repositories(
            config(),
            Arc::new(MemoryContentRepository::seeded()),
            Arc::new(LocalContentAuditor),
        );
        let input = CreateContentRequest {
            title: "一个新练习".to_string(),
            summary: "记录变化".to_string(),
            body: "今天完成了第一个小步骤".to_string(),
            domain: bookway_api::GrowthDomainDto::Learning,
            content_type: bookway_api::ContentTypeDto::Note,
            cover_url: None,
            tags: vec!["成长".to_string()],
            topics: vec!["记录".to_string()],
            route_title: None,
            route_duration: None,
        };
        let first = domain
            .create("user-a", input.clone(), Some("operation-1".to_string()))
            .await
            .expect("first create");
        let second = domain
            .create("user-a", input, Some("operation-1".to_string()))
            .await
            .expect("retry create");
        assert_eq!(first.id, second.id);
        assert_eq!(first.status, ContentStatusDto::Draft);
    }

    #[tokio::test]
    async fn editing_published_content_reopens_audit() {
        let repository = Arc::new(MemoryContentRepository::seeded());
        let domain = Domain::from_repositories(config(), repository, Arc::new(LocalContentAuditor));
        let draft = domain
            .create(
                "user-a",
                CreateContentRequest {
                    title: "可复盘的学习笔记".to_string(),
                    summary: "摘要".to_string(),
                    body: "正文".to_string(),
                    content_type: bookway_api::ContentTypeDto::Note,
                    domain: bookway_api::GrowthDomainDto::Learning,
                    tags: vec![],
                    topics: vec![],
                    cover_url: None,
                    route_title: None,
                    route_duration: None,
                },
                None,
            )
            .await
            .expect("create draft");
        let published = domain.publish("user-a", &draft.id).await.expect("publish");
        assert_eq!(published.status, ContentStatusDto::Published);
        let edited = domain
            .update(
                "user-a",
                &draft.id,
                UpdateContentRequest {
                    summary: Some("更新后的摘要".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("edit published content");
        assert_eq!(edited.status, ContentStatusDto::Reviewing);
        assert!(edited.published_at.is_none());
    }

    #[tokio::test]
    async fn only_approved_content_gets_a_published_timestamp() {
        for (decision, expected_status, should_have_timestamp) in [
            (
                AuditDecisionDto::Approved,
                ContentStatusDto::Published,
                true,
            ),
            (
                AuditDecisionDto::Reviewing,
                ContentStatusDto::Reviewing,
                false,
            ),
            (
                AuditDecisionDto::Restricted,
                ContentStatusDto::Restricted,
                false,
            ),
        ] {
            let domain = Domain::from_repositories(
                config(),
                Arc::new(MemoryContentRepository::seeded()),
                Arc::new(LocalContentAuditor),
            )
            .with_auditor(Arc::new(StaticAuditor(decision)));
            let draft = domain
                .create(
                    "user-a",
                    CreateContentRequest {
                        title: "审核状态测试".to_string(),
                        summary: "状态语义".to_string(),
                        body: "包含正文".to_string(),
                        domain: bookway_api::GrowthDomainDto::Learning,
                        content_type: bookway_api::ContentTypeDto::Note,
                        cover_url: None,
                        tags: Vec::new(),
                        topics: Vec::new(),
                        route_title: None,
                        route_duration: None,
                    },
                    None,
                )
                .await
                .expect("create draft");

            let published = domain
                .publish("user-a", &draft.id)
                .await
                .expect("publish content");

            assert_eq!(published.status, expected_status);
            assert_eq!(published.published_at.is_some(), should_have_timestamp);
        }
    }
}
