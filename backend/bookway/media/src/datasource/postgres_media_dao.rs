use super::*;

pub(crate) struct PostgresMediaDao {
    pool: sqlx::PgPool,
}

impl PostgresMediaDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MediaDao for PostgresMediaDao {
    async fn create(&self, media: NewMedia) -> Result<pb::MediaResource, DaoError> {
        sqlx::query("INSERT INTO media_assets (id,owner_id,object_key,bucket,mime_type,size_bytes,cdn_url) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(&media.id).bind(&media.owner_id).bind(&media.object_key).bind(&media.bucket).bind(&media.mime_type).bind(i64::try_from(media.size_bytes).unwrap_or(i64::MAX)).bind(&media.cdn_url).execute(&self.pool).await.map_err(DaoError::Database)?;
        Ok(to_response(&media, "pending"))
    }
    async fn pending(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, DaoError> {
        load(&self.pool, id, Some(owner_id), Some("pending")).await
    }
    async fn owned(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, DaoError> {
        load(&self.pool, id, Some(owner_id), None).await
    }
    async fn get(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, DaoError> {
        load_visible(&self.pool, id, owner_id).await
    }
    async fn mark_processing(&self, id: &str) -> Result<pb::MediaResource, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let row = sqlx::query_as::<_, (String, String, i64, String, Option<i32>, Option<i32>, Option<i64>)>(
            "UPDATE media_assets SET status='processing',updated_at=now() WHERE id=$1 AND status='pending' RETURNING object_key,mime_type,size_bytes,cdn_url,width,height,duration_ms",
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        let Some(row) = row else {
            transaction.commit().await.map_err(DaoError::Database)?;
            return load(&self.pool, id, None, None).await;
        };
        sqlx::query(
            "INSERT INTO media_processing_jobs (asset_id) VALUES ($1) ON CONFLICT (asset_id) DO UPDATE SET status='pending',available_at=now(),locked_at=NULL,lease_id=NULL,last_error=NULL,updated_at=now()",
        )
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(media_response(id, row, "processing"))
    }

    async fn owned_ready_batch(
        &self,
        owner_id: &str,
        ids: &[String],
    ) -> Result<Vec<pb::MediaResource>, DaoError> {
        let rows = sqlx::query_as::<_, (String, String, String, i64, String, Option<i32>, Option<i32>, Option<i64>)>(
            "SELECT id,object_key,mime_type,size_bytes,cdn_url,width,height,duration_ms FROM media_assets WHERE owner_id=$1 AND status='ready' AND id = ANY($2) AND status <> 'deleted'",
        )
        .bind(owner_id)
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        let by_id = rows
            .into_iter()
            .map(
                |(id, object_key, mime_type, size_bytes, cdn_url, width, height, duration_ms)| {
                    (
                        id.clone(),
                        pb::MediaResource {
                            id,
                            object_key,
                            mime_type,
                            size_bytes: size_bytes.max(0) as u64,
                            status: "ready".to_string(),
                            cdn_url,
                            width: width.unwrap_or_default().max(0) as u32,
                            height: height.unwrap_or_default().max(0) as u32,
                            duration_ms: duration_ms.map(|value| value.max(0) as u64),
                        },
                    )
                },
            )
            .collect::<HashMap<_, _>>();
        ids.iter()
            .map(|id| by_id.get(id).cloned().ok_or(DaoError::NotFound))
            .collect()
    }
}
