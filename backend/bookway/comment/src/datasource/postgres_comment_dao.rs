use super::*;

#[derive(sqlx::FromRow)]
pub(super) struct CommentReportRow {
    report_id: String,
    reporter_id: String,
    reason: String,
    details: String,
    status: String,
    reviewer_id: Option<String>,
    resolution: Option<String>,
    action: String,
    report_created_at: OffsetDateTime,
    report_updated_at: OffsetDateTime,
    comment_id: String,
    post_id: String,
    author_id: String,
    parent_id: Option<String>,
    body: String,
    like_count: i64,
    comment_created_at: OffsetDateTime,
    moderation_state: String,
    comment_deleted: bool,
}

#[derive(sqlx::FromRow)]
pub(super) struct CommentAppealRow {
    appeal_id: String,
    appeal_author_id: String,
    details: String,
    status: String,
    reviewer_id: Option<String>,
    resolution: Option<String>,
    action: String,
    appeal_created_at: OffsetDateTime,
    appeal_updated_at: OffsetDateTime,
    comment_id: String,
    post_id: String,
    comment_author_id: String,
    parent_id: Option<String>,
    body: String,
    like_count: i64,
    comment_created_at: OffsetDateTime,
    moderation_state: String,
    comment_deleted: bool,
}

pub(super) type StoredCommentRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    i64,
    OffsetDateTime,
    String,
    bool,
);

impl CommentReportRow {
    pub(super) fn into_report(self) -> Result<pb::CommentReport, DaoError> {
        Ok(pb::CommentReport {
            id: self.report_id,
            reporter_id: self.reporter_id,
            reported_comment: Some(moderation_comment_from_stored_row((
                self.comment_id,
                self.post_id,
                self.author_id,
                self.parent_id,
                self.body,
                self.like_count,
                self.comment_created_at,
                self.moderation_state,
                self.comment_deleted,
            ))?),
            reason: parse_comment_report_reason(&self.reason)?,
            details: self.details,
            status: parse_comment_report_status(&self.status)?,
            reviewer_id: self.reviewer_id,
            resolution: self.resolution,
            action: parse_comment_report_action(&self.action)?,
            created_at: format_timestamp(self.report_created_at),
            updated_at: format_timestamp(self.report_updated_at),
        })
    }
}

impl CommentAppealRow {
    pub(super) fn into_appeal(self) -> Result<pb::CommentAppeal, DaoError> {
        Ok(pb::CommentAppeal {
            id: self.appeal_id,
            author_id: self.appeal_author_id,
            appealed_comment: Some(moderation_comment_from_stored_row((
                self.comment_id,
                self.post_id,
                self.comment_author_id,
                self.parent_id,
                self.body,
                self.like_count,
                self.comment_created_at,
                self.moderation_state,
                self.comment_deleted,
            ))?),
            details: self.details,
            status: parse_comment_appeal_status(&self.status)?,
            reviewer_id: self.reviewer_id,
            resolution: self.resolution,
            action: parse_comment_appeal_action(&self.action)?,
            created_at: format_timestamp(self.appeal_created_at),
            updated_at: format_timestamp(self.appeal_updated_at),
        })
    }
}

pub(crate) struct PostgresCommentDao {
    pool: sqlx::PgPool,
}

impl PostgresCommentDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CommentDao for PostgresCommentDao {
    async fn get(
        &self,
        post_id: &str,
        comment_id: &str,
        excluded_author_ids: &[String],
    ) -> Result<pb::CommentItem, DaoError> {
        let row = sqlx::query_as::<_, StoredCommentRow>(
            "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state, deleted_at IS NOT NULL FROM comments WHERE id = $1 AND post_id = $2 AND deleted_at IS NULL AND moderation_state = 'published' AND (cardinality($3::TEXT[]) = 0 OR author_id <> ALL($3::TEXT[]))",
        )
        .bind(comment_id)
        .bind(post_id)
        .bind(excluded_author_ids)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::NotFound(comment_id.to_string()))?;
        comment_from_stored_row(row)
    }

    async fn list(
        &self,
        post_id: &str,
        cursor: Option<&CommentCursor>,
        limit: usize,
        excluded_author_ids: &[String],
    ) -> Result<Vec<pb::CommentItem>, DaoError> {
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
        .map_err(DaoError::Database)?;
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
                        Ok(pb::CommentItem {
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
            .collect::<Result<Vec<_>, DaoError>>()
    }

    async fn list_moderation(
        &self,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentItem>, DaoError> {
        let rows = sqlx::query_as::<
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
            "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state FROM comments WHERE deleted_at IS NULL AND moderation_state = 'reviewing' AND ($1::TIMESTAMPTZ IS NULL OR (created_at, id) > ($1::TIMESTAMPTZ, $2)) ORDER BY created_at ASC, id ASC LIMIT $3",
        )
        .bind(cursor.map(|value| format_timestamp(value.created_at)))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        rows.into_iter()
            .map(comment_from_row)
            .collect::<Result<Vec<_>, DaoError>>()
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
        let mut tx = self.pool.begin().await.map_err(DaoError::Database)?;
        if let Some(key) = idempotency_key.as_deref()
            && let Some(existing) = find_idempotent_comment(&mut tx, user_id, key).await?
        {
            if existing.comment.post_id == post_id
                && existing.request_body == body
                && existing.comment.parent_id == parent_id
            {
                let parent_author_id =
                    find_parent_author_id(&mut tx, existing.comment.parent_id.as_deref()).await?;
                tx.commit().await.map_err(DaoError::Database)?;
                return Ok(pb::CreateCommentResult {
                    comment: Some(existing.comment),
                    parent_author_id,
                });
            }
            return Err(DaoError::IdempotencyConflict);
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
            .map_err(DaoError::Database)?
            .ok_or_else(|| DaoError::ParentNotFound(parent.to_string()))?
            ;
            if parent_depth < 0 {
                return Err(DaoError::InvalidReplyHierarchy);
            }
            if parent_depth as usize >= MAX_REPLY_DEPTH {
                return Err(DaoError::ReplyDepthExceeded);
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
        .fetch_optional(&mut *tx).await.map_err(DaoError::Database)?;
        let Some((created_at, moderation_state)) = inserted else {
            let existing = find_idempotent_comment(
                &mut tx,
                user_id,
                idempotency_key.as_deref().unwrap_or_default(),
            )
            .await?
            .ok_or(DaoError::IdempotencyConflict)?;
            if existing.comment.post_id != post_id
                || existing.request_body != body
                || existing.comment.parent_id != parent_id
            {
                return Err(DaoError::IdempotencyConflict);
            }
            let parent_author_id =
                find_parent_author_id(&mut tx, existing.comment.parent_id.as_deref()).await?;
            tx.commit().await.map_err(DaoError::Database)?;
            return Ok(pb::CreateCommentResult {
                comment: Some(existing.comment),
                parent_author_id,
            });
        };
        tx.commit().await.map_err(DaoError::Database)?;
        Ok(pb::CreateCommentResult {
            comment: Some(pb::CommentItem {
                id,
                post_id: post_id.to_string(),
                author_id: user_id.to_string(),
                author_name: author_name.to_string(),
                body,
                parent_id,
                like_count: 0,
                created_at: format_timestamp(created_at),
                status: moderation_status(&moderation_state)?,
            }),
            parent_author_id,
        })
    }

    async fn delete(&self, user_id: &str, post_id: &str, comment_id: &str) -> Result<(), DaoError> {
        sqlx::query_scalar::<_, String>(
            "UPDATE comments SET deleted_at = COALESCE(deleted_at, now()), updated_at = CASE WHEN deleted_at IS NULL THEN now() ELSE updated_at END WHERE id = $1 AND post_id = $2 AND author_id = $3 RETURNING id",
        )
        .bind(comment_id)
        .bind(post_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::NotFound(comment_id.to_string()))?;
        Ok(())
    }

    async fn set_moderation_status(
        &self,
        comment_id: &str,
        status: i32,
    ) -> Result<pb::CommentItem, DaoError> {
        let state = moderation_state_name(status)
            .ok_or_else(|| DaoError::InvalidModerationState(format!("{status:?}")))?;
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
            "UPDATE comments SET moderation_state = $2, updated_at = now() WHERE id = $1 AND deleted_at IS NULL AND moderation_state = 'reviewing' RETURNING id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state",
        )
        .bind(comment_id)
        .bind(state)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        if let Some(row) = row {
            return comment_from_row(row);
        }
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
            "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state FROM comments WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(comment_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::NotFound(comment_id.to_string()))?;
        comment_from_row(row)
    }

    async fn review(
        &self,
        comment_id: &str,
        reviewer_id: &str,
        status: i32,
    ) -> Result<pb::ReviewCommentResult, DaoError> {
        let state = moderation_state_name(status)
            .filter(|state| *state != "reviewing")
            .ok_or_else(|| DaoError::InvalidModerationState(format!("{status:?}")))?;
        let mut tx = self.pool.begin().await.map_err(DaoError::Database)?;
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
            "UPDATE comments SET moderation_state = $2, updated_at = now() WHERE id = $1 AND deleted_at IS NULL AND moderation_state = 'reviewing' RETURNING id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state",
        )
        .bind(comment_id)
        .bind(state)
        .fetch_optional(&mut *tx)
        .await
        .map_err(DaoError::Database)?;
        let comment = if let Some(row) = row {
            sqlx::query(
                "INSERT INTO comment_moderation_reviews (comment_id, reviewer_id, decision) VALUES ($1, $2, $3) ON CONFLICT (comment_id) DO NOTHING",
            )
            .bind(comment_id)
            .bind(reviewer_id)
            .bind(state)
            .execute(&mut *tx)
            .await
            .map_err(DaoError::Database)?;
            comment_from_row(row)?
        } else {
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
                "SELECT id, post_id, author_id, parent_id, body, like_count, created_at, moderation_state FROM comments WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(comment_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(DaoError::Database)?
            .ok_or_else(|| DaoError::NotFound(comment_id.to_string()))?;
            let comment = comment_from_row(row)?;
            if comment.status != status {
                return Err(DaoError::ModerationConflict);
            }
            comment
        };
        let parent_author_id = find_parent_author_id(&mut tx, comment.parent_id.as_deref()).await?;
        tx.commit().await.map_err(DaoError::Database)?;
        Ok(pb::ReviewCommentResult {
            comment: Some(comment),
            parent_author_id,
        })
    }

    async fn create_report(
        &self,
        input: CreateCommentReportInput,
    ) -> Result<pb::CommentReport, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        if let Some(existing) = select_comment_report_by_idempotency(
            &mut transaction,
            &input.reporter_id,
            &input.idempotency_key,
        )
        .await?
        {
            let report = existing.into_report()?;
            let same_request = report.reported_comment.as_ref().is_some_and(|comment| {
                comment.id == input.comment_id && comment.post_id == input.post_id
            }) && report.reason == input.reason
                && report.details == input.details;
            if !same_request {
                return Err(DaoError::ReportIdempotencyConflict);
            }
            transaction.commit().await.map_err(DaoError::Database)?;
            return Ok(report);
        }

        let comment = select_stored_comment(&mut transaction, &input.comment_id)
            .await?
            .filter(|comment| comment.1 == input.post_id)
            .ok_or_else(|| DaoError::NotFound(input.comment_id.clone()))?;
        let reportable = comment.2 != input.reporter_id
            && !comment.8
            && comment.7 == "published"
            && !input.excluded_author_ids.contains(&comment.2);
        if comment.2 == input.reporter_id {
            return Err(DaoError::SelfReport);
        }
        if !reportable {
            return Err(DaoError::NotReportable(input.comment_id));
        }

        let inserted = sqlx::query_scalar::<_, String>(
            "INSERT INTO comment_reports (id,comment_id,reporter_id,reason,details,idempotency_key,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7::TIMESTAMPTZ,$7::TIMESTAMPTZ) ON CONFLICT (reporter_id,idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING RETURNING id",
        )
        .bind(&input.id)
        .bind(&input.comment_id)
        .bind(&input.reporter_id)
        .bind(comment_report_reason_name(input.reason)?)
        .bind(&input.details)
        .bind(&input.idempotency_key)
        .bind(&input.created_at)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        let report_id = match inserted {
            Some(report_id) => report_id,
            None => {
                let existing = select_comment_report_by_idempotency(
                    &mut transaction,
                    &input.reporter_id,
                    &input.idempotency_key,
                )
                .await?
                .ok_or(DaoError::ReportIdempotencyConflict)?
                .into_report()?;
                let same_request = existing.reported_comment.as_ref().is_some_and(|comment| {
                    comment.id == input.comment_id && comment.post_id == input.post_id
                }) && existing.reason == input.reason
                    && existing.details == input.details;
                if !same_request {
                    return Err(DaoError::ReportIdempotencyConflict);
                }
                transaction.commit().await.map_err(DaoError::Database)?;
                return Ok(existing);
            }
        };
        let report = select_comment_report(&mut transaction, &report_id)
            .await?
            .into_report()?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(report)
    }

    async fn list_reports(
        &self,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentReport>, DaoError> {
        let rows = sqlx::query_as::<_, CommentReportRow>(
            "SELECT r.id AS report_id,r.reporter_id,r.reason,r.details,r.status,r.reviewer_id,r.resolution,r.action,r.created_at AS report_created_at,r.updated_at AS report_updated_at,c.id AS comment_id,c.post_id,c.author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_reports AS r JOIN comments AS c ON c.id = r.comment_id WHERE ($1::TEXT IS NULL OR r.status = $1) AND ($2::TIMESTAMPTZ IS NULL OR (r.created_at,r.id) > ($2,$3)) ORDER BY r.created_at ASC,r.id ASC LIMIT $4",
        )
        .bind(status.map(comment_report_status_name).transpose()?)
        .bind(cursor.map(|value| value.created_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        rows.into_iter()
            .map(CommentReportRow::into_report)
            .collect()
    }

    async fn review_report(
        &self,
        report_id: &str,
        input: ReviewCommentReportInput,
    ) -> Result<pb::CommentReport, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let mut report = select_comment_report_for_update(&mut transaction, report_id)
            .await?
            .into_report()?;
        let was_terminal = is_terminal_report(report.status);
        let reviewed = apply_report_review(&mut report, &input)?;
        if was_terminal {
            transaction.commit().await.map_err(DaoError::Database)?;
            return Ok(reviewed);
        }
        if reviewed.status == pb::CommentReportStatus::Resolved as i32
            && reviewed.action == pb::CommentReportAction::RestrictComment as i32
        {
            let comment_id = reviewed
                .reported_comment
                .as_ref()
                .map(|comment| comment.id.as_str())
                .ok_or_else(|| DaoError::NotFound(report_id.to_string()))?;
            let changed = sqlx::query_scalar::<_, String>(
                "UPDATE comments SET moderation_state = 'restricted',updated_at = now() WHERE id = $1 AND deleted_at IS NULL AND moderation_state = 'published' RETURNING id",
            )
            .bind(comment_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
            if changed.is_none() {
                return Err(DaoError::ActionConflict);
            }
        }
        sqlx::query(
            "UPDATE comment_reports SET status = $2,reviewer_id = $3,resolution = $4,action = $5,updated_at = now() WHERE id = $1",
        )
        .bind(report_id)
        .bind(comment_report_status_name(reviewed.status)?)
        .bind(&reviewed.reviewer_id)
        .bind(&reviewed.resolution)
        .bind(comment_report_action_name(reviewed.action)?)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        let reviewed = select_comment_report(&mut transaction, report_id)
            .await?
            .into_report()?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(reviewed)
    }

    async fn create_appeal(
        &self,
        input: CreateCommentAppealInput,
    ) -> Result<pb::CommentAppeal, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        if let Some(existing) = select_comment_appeal_by_idempotency(
            &mut transaction,
            &input.author_id,
            &input.idempotency_key,
        )
        .await?
        {
            let appeal = existing.into_appeal()?;
            let same_request = appeal
                .appealed_comment
                .as_ref()
                .is_some_and(|comment| comment.id == input.comment_id)
                && appeal.details == input.details;
            if !same_request {
                return Err(DaoError::AppealIdempotencyConflict);
            }
            transaction.commit().await.map_err(DaoError::Database)?;
            return Ok(appeal);
        }
        let comment = select_stored_comment(&mut transaction, &input.comment_id)
            .await?
            .ok_or_else(|| DaoError::NotFound(input.comment_id.clone()))?;
        if comment.2 != input.author_id || comment.8 || comment.7 != "restricted" {
            return Err(DaoError::ActionConflict);
        }
        let inserted = sqlx::query_scalar::<_, String>(
            "INSERT INTO comment_appeals (id,comment_id,author_id,details,idempotency_key,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6::TIMESTAMPTZ,$6::TIMESTAMPTZ) ON CONFLICT (author_id,idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING RETURNING id",
        )
        .bind(&input.id)
        .bind(&input.comment_id)
        .bind(&input.author_id)
        .bind(&input.details)
        .bind(&input.idempotency_key)
        .bind(&input.created_at)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        let appeal_id = match inserted {
            Some(appeal_id) => appeal_id,
            None => {
                let existing = select_comment_appeal_by_idempotency(
                    &mut transaction,
                    &input.author_id,
                    &input.idempotency_key,
                )
                .await?
                .ok_or(DaoError::AppealIdempotencyConflict)?
                .into_appeal()?;
                let same_request = existing
                    .appealed_comment
                    .as_ref()
                    .is_some_and(|comment| comment.id == input.comment_id)
                    && existing.details == input.details;
                if !same_request {
                    return Err(DaoError::AppealIdempotencyConflict);
                }
                transaction.commit().await.map_err(DaoError::Database)?;
                return Ok(existing);
            }
        };
        let appeal = select_comment_appeal(&mut transaction, &appeal_id)
            .await?
            .into_appeal()?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(appeal)
    }

    async fn list_appeals(
        &self,
        author_id: Option<&str>,
        status: Option<i32>,
        cursor: Option<&CommentCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CommentAppeal>, DaoError> {
        let rows = sqlx::query_as::<_, CommentAppealRow>(
            "SELECT a.id AS appeal_id,a.author_id AS appeal_author_id,a.details,a.status,a.reviewer_id,a.resolution,a.action,a.created_at AS appeal_created_at,a.updated_at AS appeal_updated_at,c.id AS comment_id,c.post_id,c.author_id AS comment_author_id,c.parent_id,c.body,c.like_count,c.created_at AS comment_created_at,c.moderation_state,c.deleted_at IS NOT NULL AS comment_deleted FROM comment_appeals AS a JOIN comments AS c ON c.id = a.comment_id WHERE ($1::TEXT IS NULL OR a.author_id = $1) AND ($2::TEXT IS NULL OR a.status = $2) AND ($3::TIMESTAMPTZ IS NULL OR (a.created_at,a.id) > ($3,$4)) ORDER BY a.created_at ASC,a.id ASC LIMIT $5",
        )
        .bind(author_id)
        .bind(status.map(comment_appeal_status_name).transpose()?)
        .bind(cursor.map(|value| value.created_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        rows.into_iter()
            .map(CommentAppealRow::into_appeal)
            .collect()
    }

    async fn review_appeal(
        &self,
        appeal_id: &str,
        input: ReviewCommentAppealInput,
    ) -> Result<pb::CommentAppeal, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let mut appeal = select_comment_appeal_for_update(&mut transaction, appeal_id)
            .await?
            .into_appeal()?;
        let was_terminal = is_terminal_appeal(appeal.status);
        let reviewed = apply_appeal_review(&mut appeal, &input)?;
        if was_terminal {
            transaction.commit().await.map_err(DaoError::Database)?;
            return Ok(reviewed);
        }
        if reviewed.status == pb::CommentAppealStatus::Resolved as i32
            && reviewed.action == pb::CommentAppealAction::RestoreComment as i32
        {
            let comment_id = reviewed
                .appealed_comment
                .as_ref()
                .map(|comment| comment.id.as_str())
                .ok_or_else(|| DaoError::NotFound(appeal_id.to_string()))?;
            let changed = sqlx::query_scalar::<_, String>(
                "UPDATE comments SET moderation_state = 'published',updated_at = now() WHERE id = $1 AND deleted_at IS NULL AND moderation_state = 'restricted' RETURNING id",
            )
            .bind(comment_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
            if changed.is_none() {
                return Err(DaoError::ActionConflict);
            }
        }
        sqlx::query(
            "UPDATE comment_appeals SET status = $2,reviewer_id = $3,resolution = $4,action = $5,updated_at = now() WHERE id = $1",
        )
        .bind(appeal_id)
        .bind(comment_appeal_status_name(reviewed.status)?)
        .bind(&reviewed.reviewer_id)
        .bind(&reviewed.resolution)
        .bind(comment_appeal_action_name(reviewed.action)?)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        let reviewed = select_comment_appeal(&mut transaction, appeal_id)
            .await?
            .into_appeal()?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(reviewed)
    }
}
