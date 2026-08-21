use super::*;

type ModerationRow = (
    serde_json::Value,
    String,
    Option<String>,
    Option<String>,
    OffsetDateTime,
    OffsetDateTime,
);

fn hydrate_report(row: ModerationRow) -> Result<pb::ContentReport, DaoError> {
    let (payload, status, assignee_id, resolution, created_at, updated_at) = row;
    let mut report =
        serde_json::from_value::<pb::ContentReport>(payload).map_err(DaoError::Serialization)?;
    report.status = parse_report_status(status)?;
    report.assignee_id = assignee_id;
    report.resolution = resolution;
    report.created_at = format_timestamp(created_at);
    report.updated_at = format_timestamp(updated_at);
    Ok(report)
}

fn hydrate_appeal(row: ModerationRow) -> Result<pb::ContentAppeal, DaoError> {
    let (payload, status, assignee_id, resolution, created_at, updated_at) = row;
    let mut appeal =
        serde_json::from_value::<pb::ContentAppeal>(payload).map_err(DaoError::Serialization)?;
    appeal.status = parse_appeal_status(status)?;
    appeal.assignee_id = assignee_id;
    appeal.resolution = resolution;
    appeal.created_at = format_timestamp(created_at);
    appeal.updated_at = format_timestamp(updated_at);
    Ok(appeal)
}

#[derive(Clone)]
pub(crate) struct AuditDao {
    pool: Option<sqlx::PgPool>,
    reports: Arc<RwLock<MemoryReports>>,
    appeals: Arc<RwLock<MemoryAppeals>>,
}

impl AuditDao {
    pub(crate) fn new(pool: Option<sqlx::PgPool>) -> Self {
        Self {
            pool,
            reports: Arc::new(RwLock::new(MemoryReports::default())),
            appeals: Arc::new(RwLock::new(MemoryAppeals::default())),
        }
    }

    pub(crate) async fn store(
        &self,
        request: &pb::AuditRequest,
        response: &pb::AuditResponse,
    ) -> Result<(), DaoError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        sqlx::query("INSERT INTO content_audits (content_id,version,decision,risk_score,reasons,provider) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (content_id,version) DO UPDATE SET decision=excluded.decision,risk_score=excluded.risk_score,reasons=excluded.reasons,provider=excluded.provider")
            .bind(&request.content_id)
            .bind(i32::try_from(request.version).unwrap_or(i32::MAX))
            .bind(audit_decision_name(response.decision)?)
            .bind(response.risk_score)
            .bind(serde_json::json!(response.reasons))
            .bind(&response.provider)
            .execute(pool)
            .await
            .map_err(DaoError::Database)?;
        Ok(())
    }

    pub(crate) async fn store_report(
        &self,
        report: pb::ContentReport,
        idempotency_key: Option<String>,
    ) -> Result<pb::ContentReport, DaoError> {
        let Some(pool) = &self.pool else {
            let Some(idempotency_key) = idempotency_key else {
                self.reports
                    .write()
                    .await
                    .by_id
                    .insert(report.id.clone(), report.clone());
                return Ok(report);
            };
            let mut reports = self.reports.write().await;
            let key = (report.reporter_id.clone(), idempotency_key);
            if let Some(existing) = reports.idempotency.get(&key)
                && let Some(existing) = reports.by_id.get(existing)
            {
                return Ok(existing.clone());
            }
            reports.idempotency.insert(key, report.id.clone());
            reports.by_id.insert(report.id.clone(), report.clone());
            return Ok(report);
        };
        let payload = serde_json::to_value(&report).map_err(DaoError::Serialization)?;
        let stored = sqlx::query_as::<_, ModerationRow>(
            "INSERT INTO community_reports (id,reporter_id,content_id,reason,details,status,idempotency_key,payload,created_at) VALUES ($1,$2,$3,$4,$5,'pending',$6,$7,$8::timestamptz) ON CONFLICT (reporter_id,idempotency_key) DO UPDATE SET reporter_id=EXCLUDED.reporter_id RETURNING payload,status,assignee_id,resolution,created_at,updated_at",
        )
        .bind(&report.id)
        .bind(&report.reporter_id)
        .bind(&report.content_id)
        .bind(report_reason_name(report.reason)?)
        .bind(&report.details)
        .bind(idempotency_key)
        .bind(payload)
        .bind(&report.created_at)
        .fetch_one(pool)
        .await
        .map_err(DaoError::Database)?;
        hydrate_report(stored)
    }

    pub(crate) async fn list_reports(
        &self,
        query: &pb::ListReportsRequest,
        cursor: Option<&ReportCursor>,
        limit: usize,
    ) -> Result<Vec<pb::ContentReport>, DaoError> {
        let status = query.status.unwrap_or(pb::ReportStatus::Pending as i32);
        let Some(pool) = &self.pool else {
            let mut reports = self
                .reports
                .read()
                .await
                .by_id
                .values()
                .filter(|report| report.status == status)
                .cloned()
                .collect::<Vec<_>>();
            reports.sort_by_key(ReportCursor::from_report);
            if let Some(cursor) = cursor {
                reports.retain(|report| {
                    ReportCursor::from_report(report).is_some_and(|value| value > cursor.clone())
                });
            }
            reports.truncate(limit);
            return Ok(reports);
        };
        let rows = sqlx::query_as::<_, ModerationRow>(
            "SELECT payload,status,assignee_id,resolution,created_at,updated_at FROM community_reports WHERE status = $1 AND ($2::TIMESTAMPTZ IS NULL OR (created_at,id) > ($2::TIMESTAMPTZ,$3)) ORDER BY created_at ASC,id ASC LIMIT $4",
        )
        .bind(report_status_name(status)?)
        .bind(cursor.map(|value| format_timestamp(value.created_at)))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(pool)
        .await
        .map_err(DaoError::Database)?;
        rows.into_iter()
            .map(hydrate_report)
            .collect::<Result<Vec<_>, DaoError>>()
    }

    pub(crate) async fn review_report(
        &self,
        report_id: &str,
        reviewer_id: &str,
        request: pb::ReviewReportRequest,
    ) -> Result<pb::ContentReport, DaoError> {
        let Some(pool) = &self.pool else {
            let mut reports = self.reports.write().await;
            let report = reports
                .by_id
                .get_mut(report_id)
                .ok_or_else(|| DaoError::ReportNotFound(report_id.to_string()))?;
            return apply_review(report, reviewer_id, &request);
        };
        let mut tx = pool.begin().await.map_err(DaoError::Database)?;
        let row = sqlx::query_as::<_, ModerationRow>(
            "SELECT payload,status,assignee_id,resolution,created_at,updated_at FROM community_reports WHERE id = $1 FOR UPDATE",
        )
        .bind(report_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::ReportNotFound(report_id.to_string()))?;
        let mut report = hydrate_report(row)?;
        if is_terminal(report.status) {
            let reviewed = apply_review(&mut report, reviewer_id, &request)?;
            ensure_report_restriction_job(&mut tx, &reviewed).await?;
            tx.commit().await.map_err(DaoError::Database)?;
            return Ok(reviewed);
        }
        let reviewed = apply_review(&mut report, reviewer_id, &request)?;
        let payload = serde_json::to_value(&reviewed).map_err(DaoError::Serialization)?;
        let updated_at = sqlx::query_scalar::<_, OffsetDateTime>(
            "UPDATE community_reports SET status = $2,assignee_id = $3,resolution = $4,payload = $5,updated_at = now() WHERE id = $1 RETURNING updated_at",
        )
        .bind(report_id)
        .bind(report_status_name(reviewed.status)?)
        .bind(&reviewed.assignee_id)
        .bind(&reviewed.resolution)
        .bind(payload)
        .fetch_one(&mut *tx)
        .await
        .map_err(DaoError::Database)?;
        ensure_report_restriction_job(&mut tx, &reviewed).await?;
        tx.commit().await.map_err(DaoError::Database)?;
        report.updated_at = format_timestamp(updated_at);
        Ok(report)
    }

    pub(crate) async fn store_appeal(
        &self,
        appeal: pb::ContentAppeal,
        idempotency_key: Option<String>,
    ) -> Result<pb::ContentAppeal, DaoError> {
        let Some(pool) = &self.pool else {
            let Some(idempotency_key) = idempotency_key else {
                self.appeals
                    .write()
                    .await
                    .by_id
                    .insert(appeal.id.clone(), appeal.clone());
                return Ok(appeal);
            };
            let mut appeals = self.appeals.write().await;
            let key = (appeal.appellant_id.clone(), idempotency_key);
            if let Some(existing) = appeals.idempotency.get(&key)
                && let Some(existing) = appeals.by_id.get(existing)
            {
                return Ok(existing.clone());
            }
            appeals.idempotency.insert(key, appeal.id.clone());
            appeals.by_id.insert(appeal.id.clone(), appeal.clone());
            return Ok(appeal);
        };
        let payload = serde_json::to_value(&appeal).map_err(DaoError::Serialization)?;
        let stored = sqlx::query_as::<_, ModerationRow>(
            "INSERT INTO content_appeals (id,appellant_id,content_id,details,status,idempotency_key,payload,created_at) VALUES ($1,$2,$3,$4,'pending',$5,$6,$7::timestamptz) ON CONFLICT (appellant_id,idempotency_key) DO UPDATE SET appellant_id=EXCLUDED.appellant_id RETURNING payload,status,assignee_id,resolution,created_at,updated_at",
        )
        .bind(&appeal.id)
        .bind(&appeal.appellant_id)
        .bind(&appeal.content_id)
        .bind(&appeal.details)
        .bind(idempotency_key)
        .bind(payload)
        .bind(&appeal.created_at)
        .fetch_one(pool)
        .await
        .map_err(DaoError::Database)?;
        hydrate_appeal(stored)
    }

    pub(crate) async fn list_appeals(
        &self,
        query: &pb::ListAppealsRequest,
        cursor: Option<&ReportCursor>,
        limit: usize,
    ) -> Result<Vec<pb::ContentAppeal>, DaoError> {
        let Some(pool) = &self.pool else {
            let mut appeals = self
                .appeals
                .read()
                .await
                .by_id
                .values()
                .filter(|appeal| query.status.is_none_or(|status| appeal.status == status))
                .filter(|appeal| {
                    query
                        .appellant_id
                        .as_deref()
                        .is_none_or(|appellant_id| appeal.appellant_id == appellant_id)
                })
                .filter(|appeal| {
                    query
                        .content_id
                        .as_deref()
                        .is_none_or(|content_id| appeal.content_id == content_id)
                })
                .cloned()
                .collect::<Vec<_>>();
            appeals.sort_by_key(ReportCursor::from_appeal);
            if let Some(cursor) = cursor {
                appeals.retain(|appeal| {
                    ReportCursor::from_appeal(appeal).is_some_and(|value| value > cursor.clone())
                });
            }
            appeals.truncate(limit);
            return Ok(appeals);
        };
        let rows = sqlx::query_as::<_, ModerationRow>(
            "SELECT payload,status,assignee_id,resolution,created_at,updated_at FROM content_appeals WHERE ($1::text IS NULL OR status = $1) AND ($2::text IS NULL OR appellant_id = $2) AND ($3::text IS NULL OR content_id = $3) AND ($4::TIMESTAMPTZ IS NULL OR (created_at,id) > ($4::TIMESTAMPTZ,$5)) ORDER BY created_at ASC,id ASC LIMIT $6",
        )
        .bind(query.status.map(appeal_status_name).transpose()?)
        .bind(query.appellant_id.as_deref())
        .bind(query.content_id.as_deref())
        .bind(cursor.map(|value| format_timestamp(value.created_at)))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(pool)
        .await
        .map_err(DaoError::Database)?;
        rows.into_iter()
            .map(hydrate_appeal)
            .collect::<Result<Vec<_>, DaoError>>()
    }

    pub(crate) async fn review_appeal(
        &self,
        appeal_id: &str,
        reviewer_id: &str,
        request: pb::ReviewAppealRequest,
    ) -> Result<pb::ContentAppeal, DaoError> {
        let Some(pool) = &self.pool else {
            let mut appeals = self.appeals.write().await;
            let appeal = appeals
                .by_id
                .get_mut(appeal_id)
                .ok_or_else(|| DaoError::AppealNotFound(appeal_id.to_string()))?;
            return apply_appeal_review(appeal, reviewer_id, &request);
        };
        let mut tx = pool.begin().await.map_err(DaoError::Database)?;
        let row = sqlx::query_as::<_, ModerationRow>(
            "SELECT payload,status,assignee_id,resolution,created_at,updated_at FROM content_appeals WHERE id = $1 FOR UPDATE",
        )
        .bind(appeal_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::AppealNotFound(appeal_id.to_string()))?;
        let mut appeal = hydrate_appeal(row)?;
        if is_appeal_terminal(appeal.status) {
            let reviewed = apply_appeal_review(&mut appeal, reviewer_id, &request)?;
            ensure_appeal_notification_job(&mut tx, &reviewed).await?;
            tx.commit().await.map_err(DaoError::Database)?;
            return Ok(reviewed);
        }
        let reviewed = apply_appeal_review(&mut appeal, reviewer_id, &request)?;
        let payload = serde_json::to_value(&reviewed).map_err(DaoError::Serialization)?;
        let updated_at = sqlx::query_scalar::<_, OffsetDateTime>(
            "UPDATE content_appeals SET status = $2,assignee_id = $3,resolution = $4,payload = $5,updated_at = now() WHERE id = $1 RETURNING updated_at",
        )
        .bind(appeal_id)
        .bind(appeal_status_name(reviewed.status)?)
        .bind(&reviewed.assignee_id)
        .bind(&reviewed.resolution)
        .bind(payload)
        .fetch_one(&mut *tx)
        .await
        .map_err(DaoError::Database)?;
        ensure_appeal_notification_job(&mut tx, &reviewed).await?;
        tx.commit().await.map_err(DaoError::Database)?;
        appeal.updated_at = format_timestamp(updated_at);
        Ok(appeal)
    }
}
