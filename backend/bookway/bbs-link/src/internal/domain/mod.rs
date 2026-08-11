use std::sync::Arc;

use bookway_api::{
    AuditDecisionDto, ContentAuditRequest, ContentDto, ContentPageDto, ContentQueryRequest,
    ContentStatusDto, CreateContentRequest, UpdateContentRequest,
};
use thiserror::Error;
use uuid::Uuid;

use super::datasource::{ContentAuditor, ContentRepository, RepositoryError};

#[derive(Debug, Error)]
pub(crate) enum ContentError {
    #[error("{0}")]
    Validation(String),
    #[error("content belongs to another author")]
    Forbidden,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("content audit unavailable: {0}")]
    Audit(#[source] reqwest::Error),
}

#[derive(Clone)]
pub(crate) struct ContentService {
    repository: Arc<dyn ContentRepository>,
    auditor: Arc<dyn ContentAuditor>,
}

impl ContentService {
    pub(crate) fn new(repository: Arc<dyn ContentRepository>) -> Self {
        Self {
            repository,
            auditor: Arc::new(super::datasource::LocalContentAuditor),
        }
    }

    pub(crate) fn with_auditor(mut self, auditor: Arc<dyn ContentAuditor>) -> Self {
        self.auditor = auditor;
        self
    }

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
    use async_trait::async_trait;
    use bookway_api::ContentAuditResponse;

    use super::*;
    use crate::internal::datasource::{ContentAuditor, MemoryContentRepository};

    struct StaticAuditor(AuditDecisionDto);

    #[async_trait]
    impl ContentAuditor for StaticAuditor {
        async fn audit(
            &self,
            _request: ContentAuditRequest,
        ) -> Result<ContentAuditResponse, reqwest::Error> {
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
        let service = ContentService::new(Arc::new(MemoryContentRepository::seeded()));
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
        let first = service
            .create("user-a", input.clone(), Some("operation-1".to_string()))
            .await
            .expect("first create");
        let second = service
            .create("user-a", input, Some("operation-1".to_string()))
            .await
            .expect("retry create");
        assert_eq!(first.id, second.id);
        assert_eq!(first.status, ContentStatusDto::Draft);
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
            let service = ContentService::new(Arc::new(MemoryContentRepository::seeded()))
                .with_auditor(Arc::new(StaticAuditor(decision)));
            let draft = service
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

            let published = service
                .publish("user-a", &draft.id)
                .await
                .expect("publish content");

            assert_eq!(published.status, expected_status);
            assert_eq!(published.published_at.is_some(), should_have_timestamp);
        }
    }
}
