use std::collections::BTreeSet;

use bookway_api::{
    AuditDecisionDto, CommentDto, CommentPageDto, CommentQueryRequest, ContentAuditRequest,
    ContentStatusDto, CreateCommentRequest, CreateCommentResult,
};

use crate::{
    datasource::{CommentCursor, CreateCommentInput},
    domain::{CommentError, Domain},
};

const DEFAULT_PAGE_SIZE: usize = 30;
const MAX_PAGE_SIZE: usize = 50;

impl Domain {
    pub(crate) async fn list(
        &self,
        post_id: &str,
        request: CommentQueryRequest,
    ) -> Result<CommentPageDto, CommentError> {
        let cursor = request
            .cursor
            .as_deref()
            .map(|value| {
                CommentCursor::decode(value)
                    .ok_or_else(|| CommentError::Validation("评论游标无效".to_string()))
            })
            .transpose()?;
        let limit = request
            .limit
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);
        let excluded_author_ids = request
            .excluded_author_ids
            .into_iter()
            .map(|author_id| author_id.trim().to_string())
            .filter(|author_id| !author_id.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut items = self
            .repository
            .list(post_id, cursor.as_ref(), limit + 1, &excluded_author_ids)
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(CommentCursor::from_comment))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(CommentPageDto { items, next_cursor })
    }

    pub(crate) async fn create(
        &self,
        user_id: &str,
        post_id: &str,
        request: CreateCommentRequest,
        idempotency_key: Option<String>,
    ) -> Result<CommentDto, CommentError> {
        Ok(self
            .create_with_context(user_id, post_id, request, idempotency_key)
            .await?
            .comment)
    }

    pub(crate) async fn create_with_context(
        &self,
        user_id: &str,
        post_id: &str,
        request: CreateCommentRequest,
        idempotency_key: Option<String>,
    ) -> Result<CreateCommentResult, CommentError> {
        let CreateCommentRequest {
            body,
            parent_id,
            excluded_author_ids,
        } = request;
        let body = body.trim();
        if body.is_empty() {
            return Err(CommentError::Validation("评论不能为空".to_string()));
        }
        if body.chars().count() > 1000 {
            return Err(CommentError::Validation(
                "评论不能超过 1000 个字符".to_string(),
            ));
        }
        let idempotency_key = idempotency_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if idempotency_key.as_ref().is_some_and(|key| {
            key.len() > 128
                || !key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
        }) {
            return Err(CommentError::Validation("评论幂等键格式无效".to_string()));
        }
        let excluded_author_ids = excluded_author_ids
            .into_iter()
            .map(|author_id| author_id.trim().to_string())
            .filter(|author_id| !author_id.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut result = self
            .repository
            .create(CreateCommentInput {
                user_id,
                post_id,
                author_name: user_id,
                body: body.to_string(),
                parent_id,
                excluded_author_ids: &excluded_author_ids,
                idempotency_key,
            })
            .await?;
        if result.comment.status != ContentStatusDto::Reviewing {
            return Ok(result);
        }
        let status = match self
            .auditor
            .audit(ContentAuditRequest {
                content_id: result.comment.id.clone(),
                version: 1,
                title: "评论".to_string(),
                body: result.comment.body.clone(),
            })
            .await
        {
            Ok(response) => moderation_status(response.decision),
            Err(error) => {
                // The database default is reviewing. Audit outages therefore fail closed
                // without turning an accepted write into an ambiguous client failure.
                tracing::warn!(comment_id = %result.comment.id, %error, "comment audit degraded");
                return Ok(result);
            }
        };
        if result.comment.status == status {
            return Ok(result);
        }
        let comment_id = result.comment.id.clone();
        result.comment = self
            .repository
            .set_moderation_status(&comment_id, status)
            .await?;
        Ok(result)
    }

    pub(crate) async fn delete(
        &self,
        user_id: &str,
        post_id: &str,
        comment_id: &str,
    ) -> Result<(), CommentError> {
        if comment_id.trim().is_empty() {
            return Err(CommentError::Validation("评论 ID 不能为空".to_string()));
        }
        self.repository.delete(user_id, post_id, comment_id).await?;
        Ok(())
    }
}

fn moderation_status(decision: AuditDecisionDto) -> ContentStatusDto {
    match decision {
        AuditDecisionDto::Approved => ContentStatusDto::Published,
        AuditDecisionDto::Reviewing => ContentStatusDto::Reviewing,
        AuditDecisionDto::Restricted => ContentStatusDto::Restricted,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use bookway_api::{AuditDecisionDto, ContentAuditResponse};

    use super::*;
    use crate::{
        conf::Config,
        datasource::{CommentAuditor, MAX_REPLY_DEPTH, MemoryCommentRepository, RepositoryError},
    };

    struct StaticAuditor {
        decision: AuditDecisionDto,
    }

    #[async_trait]
    impl CommentAuditor for StaticAuditor {
        async fn audit(
            &self,
            _request: ContentAuditRequest,
        ) -> Result<ContentAuditResponse, String> {
            Ok(ContentAuditResponse {
                decision: self.decision,
                risk_score: 0.7,
                reasons: vec!["test".to_string()],
                provider: "test".to_string(),
            })
        }
    }

    struct FailingAuditor;

    #[async_trait]
    impl CommentAuditor for FailingAuditor {
        async fn audit(
            &self,
            _request: ContentAuditRequest,
        ) -> Result<ContentAuditResponse, String> {
            Err("audit unavailable".to_string())
        }
    }

    fn domain() -> Domain {
        Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
                content_audit_grpc_url: None,
            },
            Arc::new(MemoryCommentRepository::default()),
        )
    }

    #[tokio::test]
    async fn rejects_empty_comments() {
        let result = domain()
            .create(
                "user-a",
                "post-a",
                CreateCommentRequest {
                    body: "   ".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await;
        assert!(matches!(result, Err(CommentError::Validation(_))));
    }

    #[tokio::test]
    async fn validates_parent_on_the_same_post() {
        let result = domain()
            .create(
                "user-a",
                "post-a",
                CreateCommentRequest {
                    body: "回复".to_string(),
                    parent_id: Some("missing".to_string()),
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await;
        assert!(matches!(result, Err(CommentError::Repository(_))));
    }

    #[tokio::test]
    async fn caps_reply_nesting_before_it_can_create_an_unbounded_thread() {
        let domain = domain();
        let mut parent = domain
            .create(
                "root-author",
                "post-a",
                CreateCommentRequest {
                    body: "根评论".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("root comment should be created");
        for depth in 1..=MAX_REPLY_DEPTH {
            parent = domain
                .create(
                    &format!("reply-author-{depth}"),
                    "post-a",
                    CreateCommentRequest {
                        body: format!("第 {depth} 层回复"),
                        parent_id: Some(parent.id),
                        excluded_author_ids: Vec::new(),
                    },
                    None,
                )
                .await
                .expect("reply within the limit should be created");
        }

        let result = domain
            .create(
                "reply-author-overflow",
                "post-a",
                CreateCommentRequest {
                    body: "超出层级的回复".to_string(),
                    parent_id: Some(parent.id),
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await;

        assert!(matches!(
            result,
            Err(CommentError::Repository(
                RepositoryError::ReplyDepthExceeded
            ))
        ));
    }

    #[tokio::test]
    async fn refuses_replies_to_comments_hidden_from_the_viewer() {
        let domain = domain();
        let parent = domain
            .create(
                "author-hidden",
                "post-a",
                CreateCommentRequest {
                    body: "隐藏评论".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("parent should be published");

        let result = domain
            .create(
                "viewer",
                "post-a",
                CreateCommentRequest {
                    body: "不应创建的回复".to_string(),
                    parent_id: Some(parent.id),
                    excluded_author_ids: vec!["author-hidden".to_string()],
                },
                None,
            )
            .await;

        assert!(matches!(result, Err(CommentError::Repository(_))));
    }

    #[tokio::test]
    async fn pages_comments_without_duplicates() {
        let domain = domain();
        for body in ["一", "二", "三"] {
            domain
                .create(
                    "user-a",
                    "post-a",
                    CreateCommentRequest {
                        body: body.to_string(),
                        parent_id: None,
                        excluded_author_ids: Vec::new(),
                    },
                    None,
                )
                .await
                .expect("comment should be created");
        }
        let first = domain
            .list(
                "post-a",
                CommentQueryRequest {
                    cursor: None,
                    limit: Some(2),
                    excluded_author_ids: Vec::new(),
                },
            )
            .await
            .expect("first page should load");
        let second = domain
            .list(
                "post-a",
                CommentQueryRequest {
                    cursor: first.next_cursor.clone(),
                    limit: Some(2),
                    excluded_author_ids: Vec::new(),
                },
            )
            .await
            .expect("second page should load");

        assert_eq!(first.items.len(), 2);
        assert_eq!(second.items.len(), 1);
        assert_ne!(first.items[1].id, second.items[0].id);
        assert!(second.next_cursor.is_none());
    }

    #[tokio::test]
    async fn filters_excluded_authors_before_paginating_comments() {
        let domain = domain();
        for (author, body) in [
            ("author-hidden", "隐藏一"),
            ("author-visible", "可见一"),
            ("author-hidden", "隐藏二"),
            ("author-visible", "可见二"),
        ] {
            domain
                .create(
                    author,
                    "post-a",
                    CreateCommentRequest {
                        body: body.to_string(),
                        parent_id: None,
                        excluded_author_ids: Vec::new(),
                    },
                    None,
                )
                .await
                .expect("comment should be created");
        }
        let first = domain
            .list(
                "post-a",
                CommentQueryRequest {
                    cursor: None,
                    limit: Some(1),
                    excluded_author_ids: vec!["author-hidden".to_string()],
                },
            )
            .await
            .expect("first visible comment page");
        let second = domain
            .list(
                "post-a",
                CommentQueryRequest {
                    cursor: first.next_cursor.clone(),
                    limit: Some(1),
                    excluded_author_ids: vec!["author-hidden".to_string()],
                },
            )
            .await
            .expect("second visible comment page");

        assert_eq!(first.items[0].body, "可见一");
        assert_eq!(second.items[0].body, "可见二");
        assert!(second.next_cursor.is_none());
    }

    #[tokio::test]
    async fn deduplicates_retried_comment_creates() {
        let domain = domain();
        let request = CreateCommentRequest {
            body: "同一条评论".to_string(),
            parent_id: None,
            excluded_author_ids: Vec::new(),
        };
        let first = domain
            .create(
                "user-a",
                "post-a",
                request.clone(),
                Some("comment-request-a".to_string()),
            )
            .await
            .expect("first create should succeed");
        let retried = domain
            .create(
                "user-a",
                "post-a",
                request,
                Some("comment-request-a".to_string()),
            )
            .await
            .expect("retry should return the first comment");

        assert_eq!(first.id, retried.id);
    }

    #[tokio::test]
    async fn returns_parent_author_for_new_and_retried_replies() {
        let domain = domain();
        let parent = domain
            .create(
                "parent-author",
                "post-a",
                CreateCommentRequest {
                    body: "父评论".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("parent should be published");
        let request = CreateCommentRequest {
            body: "回复父评论".to_string(),
            parent_id: Some(parent.id),
            excluded_author_ids: Vec::new(),
        };

        let created = domain
            .create_with_context(
                "reply-author",
                "post-a",
                request.clone(),
                Some("reply-request-a".to_string()),
            )
            .await
            .expect("reply should be created");
        let retried = domain
            .create_with_context(
                "reply-author",
                "post-a",
                request,
                Some("reply-request-a".to_string()),
            )
            .await
            .expect("reply retry should return the original result");

        assert_eq!(created.parent_author_id.as_deref(), Some("parent-author"));
        assert_eq!(retried.parent_author_id.as_deref(), Some("parent-author"));
        assert_eq!(created.comment.id, retried.comment.id);
    }

    #[test]
    fn create_result_is_compatible_with_legacy_comment_payloads() {
        let result = CreateCommentResult {
            comment: CommentDto {
                id: "comment-a".to_string(),
                post_id: "post-a".to_string(),
                author_id: "reply-author".to_string(),
                author_name: "reply-author".to_string(),
                body: "回复".to_string(),
                parent_id: Some("parent-a".to_string()),
                like_count: 0,
                created_at: "2026-08-15T00:00:00Z".to_string(),
                status: ContentStatusDto::Published,
            },
            parent_author_id: Some("parent-author".to_string()),
        };

        let response_json = serde_json::to_string(&result).expect("result should serialize");
        let legacy: CommentDto =
            serde_json::from_str(&response_json).expect("legacy client should decode result");
        assert_eq!(legacy.id, "comment-a");

        let legacy_json = serde_json::to_string(&result.comment).expect("comment should serialize");
        let upgraded: CreateCommentResult = serde_json::from_str(&legacy_json)
            .expect("upgraded client should decode legacy payload");
        assert_eq!(upgraded.parent_author_id, None);
        assert_eq!(upgraded.comment.id, "comment-a");
    }

    #[tokio::test]
    async fn deleting_a_parent_keeps_an_anonymous_tombstone_for_visible_replies() {
        let domain = domain();
        let parent = domain
            .create(
                "parent-author",
                "post-a",
                CreateCommentRequest {
                    body: "会被删除的父评论".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("parent should be created");
        let reply = domain
            .create(
                "reply-author",
                "post-a",
                CreateCommentRequest {
                    body: "仍然可见的回复".to_string(),
                    parent_id: Some(parent.id.clone()),
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("reply should be created");

        domain
            .delete("parent-author", "post-a", &parent.id)
            .await
            .expect("owner should delete parent");
        let page = domain
            .list("post-a", CommentQueryRequest::default())
            .await
            .expect("public list should load");

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].id, parent.id);
        assert_eq!(page.items[0].status, ContentStatusDto::Deleted);
        assert_eq!(page.items[0].author_id, "");
        assert_eq!(page.items[0].body, "该评论已删除");
        assert_eq!(page.items[1].id, reply.id);
        let result = domain
            .create(
                "another-user",
                "post-a",
                CreateCommentRequest {
                    body: "不能回复已删除评论".to_string(),
                    parent_id: Some(parent.id),
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await;
        assert!(matches!(
            result,
            Err(CommentError::Repository(RepositoryError::ParentNotFound(_)))
        ));
    }

    #[tokio::test]
    async fn only_the_author_can_delete_and_retries_are_idempotent() {
        let domain = domain();
        let comment = domain
            .create(
                "owner",
                "post-a",
                CreateCommentRequest {
                    body: "可删除评论".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("comment should be created");

        let unauthorized = domain.delete("other-user", "post-a", &comment.id).await;
        assert!(matches!(
            unauthorized,
            Err(CommentError::Repository(RepositoryError::NotFound(_)))
        ));
        domain
            .delete("owner", "post-a", &comment.id)
            .await
            .expect("first delete should succeed");
        domain
            .delete("owner", "post-a", &comment.id)
            .await
            .expect("retried delete should also succeed");
        let page = domain
            .list("post-a", CommentQueryRequest::default())
            .await
            .expect("public list should load");
        assert!(page.items.is_empty());
    }

    #[tokio::test]
    async fn deleted_tombstones_never_bypass_author_visibility() {
        let domain = domain();
        let parent = domain
            .create(
                "hidden-parent-author",
                "post-a",
                CreateCommentRequest {
                    body: "隐藏作者的父评论".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("parent should be created");
        let reply = domain
            .create(
                "visible-reply-author",
                "post-a",
                CreateCommentRequest {
                    body: "可见回复".to_string(),
                    parent_id: Some(parent.id.clone()),
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("reply should be created");
        domain
            .delete("hidden-parent-author", "post-a", &parent.id)
            .await
            .expect("parent should be deleted");

        let page = domain
            .list(
                "post-a",
                CommentQueryRequest {
                    excluded_author_ids: vec!["hidden-parent-author".to_string()],
                    ..CommentQueryRequest::default()
                },
            )
            .await
            .expect("visible list should load");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, reply.id);
    }

    #[tokio::test]
    async fn holds_reviewing_comments_out_of_the_public_list() {
        let domain = domain().with_auditor(Arc::new(StaticAuditor {
            decision: AuditDecisionDto::Reviewing,
        }));
        let comment = domain
            .create(
                "user-a",
                "post-a",
                CreateCommentRequest {
                    body: "请审核这条评论".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("comment should be accepted for review");
        let page = domain
            .list("post-a", CommentQueryRequest::default())
            .await
            .expect("public list");

        assert_eq!(comment.status, ContentStatusDto::Reviewing);
        assert!(page.items.is_empty());
    }

    #[tokio::test]
    async fn holds_comments_when_audit_is_unavailable() {
        let domain = domain().with_auditor(Arc::new(FailingAuditor));
        let comment = domain
            .create(
                "user-a",
                "post-a",
                CreateCommentRequest {
                    body: "审核服务故障时也不能公开".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("comment should remain queued");
        let page = domain
            .list("post-a", CommentQueryRequest::default())
            .await
            .expect("public list");

        assert_eq!(comment.status, ContentStatusDto::Reviewing);
        assert!(page.items.is_empty());
    }

    #[tokio::test]
    async fn restricts_a_rejected_comment_without_exposing_it() {
        let domain = domain().with_auditor(Arc::new(StaticAuditor {
            decision: AuditDecisionDto::Restricted,
        }));
        let comment = domain
            .create(
                "user-a",
                "post-a",
                CreateCommentRequest {
                    body: "受限评论".to_string(),
                    parent_id: None,
                    excluded_author_ids: Vec::new(),
                },
                None,
            )
            .await
            .expect("comment should be stored for moderation");
        let page = domain
            .list("post-a", CommentQueryRequest::default())
            .await
            .expect("public list");

        assert_eq!(comment.status, ContentStatusDto::Restricted);
        assert!(page.items.is_empty());
    }
}
