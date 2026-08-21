use super::*;

#[derive(Default)]
pub(crate) struct MemoryCommentDao {
    state: RwLock<MemoryCommentState>,
}

#[async_trait]
impl CommentDao for MemoryCommentDao {
    async fn get(
        &self,
        post_id: &str,
        comment_id: &str,
        excluded_author_ids: &[String],
    ) -> Result<pb::CommentItem, DaoError> {
        let state = self.state.read().await;
        let items = state
            .comments
            .get(post_id)
            .ok_or_else(|| DaoError::NotFound(comment_id.to_string()))?;
        public_comment_items(items, excluded_author_ids)
            .into_iter()
            .find(|comment| comment.id == comment_id)
            .ok_or_else(|| DaoError::NotFound(comment_id.to_string()))
    }

    async fn list(
        &self,
        post_id: &str,
        cursor: Option<&CommentCursor>,
        limit: usize,
        excluded_author_ids: &[String],
    ) -> Result<Vec<pb::CommentItem>, DaoError> {
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

    async fn list_moderation(
        &self,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentItem>, DaoError> {
        let state = self.state.read().await;
        let mut items = state
            .comments
            .values()
            .flatten()
            .filter(|comment| comment.status == pb::CommentStatus::Reviewing as i32)
            .cloned()
            .collect::<Vec<_>>();
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
    ) -> Result<pb::CreateCommentResult, DaoError> {
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
                .ok_or(DaoError::IdempotencyConflict)?;
            if existing.post_id == post_id
                && existing.body == body
                && existing.parent_id == parent_id
            {
                return Ok(pb::CreateCommentResult {
                    parent_author_id: memory_parent_author_id(
                        &state,
                        existing.parent_id.as_deref(),
                    ),
                    comment: Some(existing),
                });
            }
            return Err(DaoError::IdempotencyConflict);
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
                        && comment.status == pb::CommentStatus::Published as i32
                        && !excluded_author_ids.contains(&comment.author_id)
                })
                .ok_or_else(|| DaoError::ParentNotFound(parent_id.to_string()))?;
            if memory_comment_depth(items, parent)? >= MAX_REPLY_DEPTH {
                return Err(DaoError::ReplyDepthExceeded);
            }
            Some(parent.author_id.clone())
        } else {
            None
        };
        let comment = pb::CommentItem {
            id: uuid::Uuid::now_v7().to_string(),
            post_id: post_id.to_string(),
            author_id: user_id.to_string(),
            author_name: author_name.to_string(),
            body,
            parent_id,
            like_count: 0,
            created_at: now_timestamp(),
            status: pb::CommentStatus::Reviewing as i32,
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
        Ok(pb::CreateCommentResult {
            comment: Some(comment),
            parent_author_id,
        })
    }

    async fn delete(&self, user_id: &str, post_id: &str, comment_id: &str) -> Result<(), DaoError> {
        let mut state = self.state.write().await;
        let comment = {
            let comment = state
                .comments
                .get_mut(post_id)
                .into_iter()
                .flatten()
                .find(|comment| comment.id == comment_id && comment.author_id == user_id)
                .ok_or_else(|| DaoError::NotFound(comment_id.to_string()))?;
            comment.status = pb::CommentStatus::Deleted as i32;
            comment.clone()
        };
        update_memory_comment_records(&mut state, &comment);
        Ok(())
    }

    async fn set_moderation_status(
        &self,
        comment_id: &str,
        status: i32,
    ) -> Result<pb::CommentItem, DaoError> {
        if moderation_state_name(status).is_none() {
            return Err(DaoError::InvalidModerationState(format!("{status:?}")));
        }
        let mut state = self.state.write().await;
        let comment = state
            .comments
            .values_mut()
            .flatten()
            .find(|comment| comment.id == comment_id)
            .ok_or_else(|| DaoError::NotFound(comment_id.to_string()))?;
        if comment.status == pb::CommentStatus::Reviewing as i32 {
            comment.status = status;
        }
        Ok(comment.clone())
    }

    async fn review(
        &self,
        comment_id: &str,
        _reviewer_id: &str,
        status: i32,
    ) -> Result<pb::ReviewCommentResult, DaoError> {
        if !matches!(
            pb::CommentStatus::try_from(status),
            Ok(pb::CommentStatus::Published | pb::CommentStatus::Restricted)
        ) {
            return Err(DaoError::InvalidModerationState(format!("{status:?}")));
        }
        let mut state = self.state.write().await;
        let (comment, parent_id) = {
            let comment = state
                .comments
                .values_mut()
                .flatten()
                .find(|comment| comment.id == comment_id)
                .ok_or_else(|| DaoError::NotFound(comment_id.to_string()))?;
            if comment.status == pb::CommentStatus::Deleted as i32 {
                return Err(DaoError::NotFound(comment_id.to_string()));
            }
            if comment.status == pb::CommentStatus::Reviewing as i32 {
                comment.status = status;
            } else if comment.status != status {
                return Err(DaoError::ModerationConflict);
            }
            (comment.clone(), comment.parent_id.clone())
        };
        Ok(pb::ReviewCommentResult {
            parent_author_id: memory_parent_author_id(&state, parent_id.as_deref()),
            comment: Some(comment),
        })
    }

    async fn create_report(
        &self,
        input: CreateCommentReportInput,
    ) -> Result<pb::CommentReport, DaoError> {
        let mut state = self.state.write().await;
        let request_key = (input.reporter_id.clone(), input.idempotency_key.clone());
        if let Some(report_id) = state.report_requests.get(&request_key)
            && let Some(report) = state.reports.get(report_id)
        {
            if report.reported_comment.as_ref().is_some_and(|comment| {
                comment.id == input.comment_id && comment.post_id == input.post_id
            }) && report.reason == input.reason
                && report.details == input.details
            {
                return Ok(report.clone());
            }
            return Err(DaoError::ReportIdempotencyConflict);
        }
        let comment = memory_comment(&state, &input.comment_id)
            .filter(|comment| comment.post_id == input.post_id)
            .ok_or_else(|| DaoError::NotFound(input.comment_id.clone()))?;
        if comment.author_id == input.reporter_id {
            return Err(DaoError::SelfReport);
        }
        if comment.status != pb::CommentStatus::Published as i32
            || input.excluded_author_ids.contains(&comment.author_id)
        {
            return Err(DaoError::NotReportable(input.comment_id));
        }
        let report = pb::CommentReport {
            id: input.id.clone(),
            reporter_id: input.reporter_id.clone(),
            reported_comment: Some(comment),
            reason: input.reason,
            details: input.details,
            status: pb::CommentReportStatus::Pending as i32,
            reviewer_id: None,
            resolution: None,
            action: pb::CommentReportAction::NoAction as i32,
            created_at: input.created_at.clone(),
            updated_at: input.created_at,
        };
        state.report_requests.insert(request_key, report.id.clone());
        state.reports.insert(report.id.clone(), report.clone());
        Ok(report)
    }

    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentReport>, DaoError> {
        let state = self.state.read().await;
        let mut reports = state
            .reports
            .values()
            .filter(|report| status.is_none_or(|status| report.status == status))
            .filter(|report| {
                cursor.is_none_or(|cursor| {
                    CommentCursor::from_values(&report.created_at, &report.id).is_some_and(
                        |value| {
                            (value.created_at, value.id.as_str())
                                > (cursor.created_at, cursor.id.as_str())
                        },
                    )
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        reports.sort_by_key(|report| CommentCursor::from_values(&report.created_at, &report.id));
        reports.truncate(limit);
        Ok(reports)
    }

    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewCommentReportInput,
    ) -> Result<pb::CommentReport, DaoError> {
        let mut state = self.state.write().await;
        let mut report = state
            .reports
            .get(report_id)
            .cloned()
            .ok_or_else(|| DaoError::ReportNotFound(report_id.to_string()))?;
        let was_terminal = is_terminal_report(report.status);
        let mut reviewed = apply_report_review(&mut report, &input)?;
        if was_terminal {
            return Ok(reviewed);
        }
        if reviewed.status == pb::CommentReportStatus::Resolved as i32
            && reviewed.action == pb::CommentReportAction::RestrictComment as i32
        {
            let comment_id = reviewed
                .reported_comment
                .as_ref()
                .map(|comment| comment.id.clone())
                .ok_or_else(|| DaoError::NotFound(report_id.to_string()))?;
            let comment = memory_comment_mut(&mut state, &comment_id)
                .ok_or_else(|| DaoError::NotFound(comment_id.clone()))?;
            if comment.status != pb::CommentStatus::Published as i32 {
                return Err(DaoError::ActionConflict);
            }
            comment.status = pb::CommentStatus::Restricted as i32;
            let comment = comment.clone();
            reviewed.reported_comment = Some(comment.clone());
            update_memory_comment_records(&mut state, &comment);
        }
        state
            .reports
            .insert(report_id.to_string(), reviewed.clone());
        Ok(reviewed)
    }

    async fn create_appeal(
        &self,
        input: CreateCommentAppealInput,
    ) -> Result<pb::CommentAppeal, DaoError> {
        let mut state = self.state.write().await;
        let request_key = (input.author_id.clone(), input.idempotency_key.clone());
        if let Some(appeal_id) = state.appeal_requests.get(&request_key)
            && let Some(appeal) = state.appeals.get(appeal_id)
        {
            if appeal
                .appealed_comment
                .as_ref()
                .is_some_and(|comment| comment.id == input.comment_id)
                && appeal.details == input.details
            {
                return Ok(appeal.clone());
            }
            return Err(DaoError::AppealIdempotencyConflict);
        }
        let comment = memory_comment(&state, &input.comment_id)
            .ok_or_else(|| DaoError::NotFound(input.comment_id.clone()))?;
        if comment.author_id != input.author_id
            || comment.status != pb::CommentStatus::Restricted as i32
        {
            return Err(DaoError::ActionConflict);
        }
        let appeal = pb::CommentAppeal {
            id: input.id.clone(),
            author_id: input.author_id.clone(),
            appealed_comment: Some(comment),
            details: input.details,
            status: pb::CommentAppealStatus::Pending as i32,
            reviewer_id: None,
            resolution: None,
            action: pb::CommentAppealAction::NoAction as i32,
            created_at: input.created_at.clone(),
            updated_at: input.created_at,
        };
        state.appeal_requests.insert(request_key, appeal.id.clone());
        state.appeals.insert(appeal.id.clone(), appeal.clone());
        Ok(appeal)
    }

    async fn list_appeals(
        &self,
        author_id: Option<&str>,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentAppeal>, DaoError> {
        let state = self.state.read().await;
        let mut appeals = state
            .appeals
            .values()
            .filter(|appeal| author_id.is_none_or(|author_id| appeal.author_id == author_id))
            .filter(|appeal| status.is_none_or(|status| appeal.status == status))
            .filter(|appeal| {
                cursor.is_none_or(|cursor| {
                    CommentCursor::from_values(&appeal.created_at, &appeal.id).is_some_and(
                        |value| {
                            (value.created_at, value.id.as_str())
                                > (cursor.created_at, cursor.id.as_str())
                        },
                    )
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        appeals.sort_by_key(|appeal| CommentCursor::from_values(&appeal.created_at, &appeal.id));
        appeals.truncate(limit);
        Ok(appeals)
    }

    async fn review_appeal(
        &self,
        appeal_id: &str,
        input: ReviewCommentAppealInput,
    ) -> Result<pb::CommentAppeal, DaoError> {
        let mut state = self.state.write().await;
        let mut appeal = state
            .appeals
            .get(appeal_id)
            .cloned()
            .ok_or_else(|| DaoError::AppealNotFound(appeal_id.to_string()))?;
        let was_terminal = is_terminal_appeal(appeal.status);
        let mut reviewed = apply_appeal_review(&mut appeal, &input)?;
        if was_terminal {
            return Ok(reviewed);
        }
        if reviewed.status == pb::CommentAppealStatus::Resolved as i32
            && reviewed.action == pb::CommentAppealAction::RestoreComment as i32
        {
            let comment_id = reviewed
                .appealed_comment
                .as_ref()
                .map(|comment| comment.id.clone())
                .ok_or_else(|| DaoError::NotFound(appeal_id.to_string()))?;
            let comment = memory_comment_mut(&mut state, &comment_id)
                .ok_or_else(|| DaoError::NotFound(comment_id.clone()))?;
            if comment.status != pb::CommentStatus::Restricted as i32 {
                return Err(DaoError::ActionConflict);
            }
            comment.status = pb::CommentStatus::Published as i32;
            let comment = comment.clone();
            reviewed.appealed_comment = Some(comment.clone());
            update_memory_comment_records(&mut state, &comment);
        }
        state
            .appeals
            .insert(appeal_id.to_string(), reviewed.clone());
        Ok(reviewed)
    }
}
