use super::*;

#[derive(Clone)]
pub(crate) struct CommunityNotificationJobDao {
    pool: PgPool,
}

impl CommunityNotificationJobDao {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn enqueue(&self, job: CommunityNotificationJob) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO community_notification_jobs (source_id,recipient_user_id,title,body,data) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (source_id) DO NOTHING",
        )
        .bind(job.source_id)
        .bind(job.recipient_user_id)
        .bind(job.title)
        .bind(job.body)
        .bind(job.data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
