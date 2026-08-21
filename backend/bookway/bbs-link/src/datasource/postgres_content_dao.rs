use super::*;

pub(crate) struct PostgresContentDao {
    pool: sqlx::PgPool,
}

impl PostgresContentDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ContentDao for PostgresContentDao {
    async fn list(&self, query: &pb::ListRequest) -> Result<pb::ContentPage, DaoError> {
        let limit = query.limit.unwrap_or(20).clamp(1, 100) as i64;
        let offset = query
            .cursor
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0)
            .max(0);
        let status = query.status.map(content_status_name).transpose()?;
        let author_ids = (!query.author_ids.is_empty()).then(|| query.author_ids.clone());
        let order = if query.strategy.as_deref() == Some("fresh") {
            "created_at DESC, id DESC"
        } else {
            "quality_score DESC, created_at DESC, id DESC"
        };
        let sql = format!(
            "SELECT payload, COUNT(*) OVER() AS total_count FROM content_items WHERE deleted_at IS NULL AND ($1::text IS NULL OR status = $1) AND ($2::text IS NULL OR id = ANY(string_to_array($2, ','))) AND ($3::text IS NULL OR author_id = $3) AND ($6::text IS NULL OR content_type = $6) AND ($7::text IS NULL OR domain = $7) AND ($8::text[] IS NULL OR author_id = ANY($8)) ORDER BY {order} LIMIT $4 OFFSET $5"
        );
        let rows = sqlx::query_as::<_, (serde_json::Value, i64)>(&sql)
            .bind(status)
            .bind(query.ids.as_deref())
            .bind(query.author_id.as_deref())
            .bind(limit + 1)
            .bind(offset)
            .bind(query.content_type.map(content_type_name).transpose()?)
            .bind(query.domain.map(growth_domain_name).transpose()?)
            .bind(author_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(DaoError::Database)?;
        let total = rows.first().map(|(_, total)| *total).unwrap_or(0);
        let items = rows
            .into_iter()
            .take(limit as usize)
            .map(|(value, _)| serde_json::from_value(value).map_err(DaoError::Serialization))
            .collect::<Result<Vec<pb::Content>, _>>()?;
        Ok(pb::ContentPage {
            total_estimate: u64::try_from(total).unwrap_or(u64::MAX),
            next_cursor: (offset + limit < total).then(|| (offset + limit).to_string()),
            items,
        })
    }

    async fn get(&self, id: &str) -> Result<pb::Content, DaoError> {
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM content_items WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?
        .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        serde_json::from_value(payload).map_err(DaoError::Serialization)
    }

    async fn published_by_idempotency_key(
        &self,
        user_id: &str,
        idempotency_key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Content>, DaoError> {
        let existing = sqlx::query_as::<_, (String, Option<serde_json::Value>)>(
            "SELECT request_hash, response_payload FROM content_idempotency_keys WHERE user_id = $1 AND idempotency_key = $2 AND operation = 'publish'",
        )
        .bind(user_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        existing
            .map(|(stored_fingerprint, response)| {
                published_idempotency_response(
                    idempotency_key,
                    request_fingerprint,
                    stored_fingerprint,
                    response,
                )
            })
            .transpose()
    }

    async fn create(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, DaoError> {
        let mut tx = self.pool.begin().await.map_err(DaoError::Database)?;
        if let Some(key) = idempotency_key.as_deref()
            && let Some((resource_id, fingerprint)) = sqlx::query_as::<_, (String, String)>(
                "SELECT resource_id, request_hash FROM content_idempotency_keys WHERE user_id = $1 AND idempotency_key = $2 AND operation = 'create' FOR UPDATE",
            )
            .bind(&content.author_id)
            .bind(key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(DaoError::Database)?
        {
            if fingerprint != request_fingerprint {
                return Err(DaoError::IdempotencyConflict(key.to_string()));
            }
            let payload = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT payload FROM content_items WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(resource_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(DaoError::Database)?;
            return serde_json::from_value(payload).map_err(DaoError::Serialization);
        }
        let post = content.post.as_ref().ok_or_else(|| {
            DaoError::InvalidContent("content is missing its post summary".to_string())
        })?;
        let payload = serde_json::to_value(&content).map_err(DaoError::Serialization)?;
        let published_at = parse_timestamp(content.published_at.as_deref())?;
        sqlx::query(
            "INSERT INTO content_items (id, author_id, content_type, status, title, summary, body, domain, cover_url, route_title, route_duration, version, quality_score, published_at, payload) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
        .bind(&content.id)
        .bind(&content.author_id)
        .bind(content_type_name(content.content_type)?)
        .bind(content_status_name(content.status)?)
        .bind(&post.title)
        .bind(&post.summary)
        .bind(&content.body)
        .bind(growth_domain_name(post.domain)?)
        .bind(&post.cover_url)
        .bind(&post.route_title)
        .bind(&post.route_duration)
        .bind(i64::from(content.version))
        .bind(content.quality_score)
        .bind(published_at)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .map_err(DaoError::Database)?;
        replace_content_media(&mut tx, &content).await?;
        queue_search_projection(&mut tx, &content).await?;
        if let Some(key) = idempotency_key {
            sqlx::query("INSERT INTO content_idempotency_keys (user_id,idempotency_key,operation,resource_id,request_hash) VALUES ($1,$2,'create',$3,$4)")
                .bind(&content.author_id)
                .bind(key)
                .bind(&content.id)
                .bind(request_fingerprint)
                .execute(&mut *tx)
                .await
                .map_err(DaoError::Database)?;
        }
        tx.commit().await.map_err(DaoError::Database)?;
        Ok(content)
    }

    async fn update(&self, content: pb::Content) -> Result<pb::Content, DaoError> {
        let mut tx = self.pool.begin().await.map_err(DaoError::Database)?;
        update_content_in_transaction(&mut tx, &content).await?;
        tx.commit().await.map_err(DaoError::Database)?;
        Ok(content)
    }

    async fn publish(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, DaoError> {
        let mut tx = self.pool.begin().await.map_err(DaoError::Database)?;
        if let Some(key) = idempotency_key.as_deref() {
            // The row does not exist for the first request, so serialize the
            // key explicitly before observing or creating it.
            let lock_key = format!("content-publish:{}:{key}", content.author_id);
            sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
                .bind(lock_key)
                .execute(&mut *tx)
                .await
                .map_err(DaoError::Database)?;
            if let Some((stored_fingerprint, response)) =
                sqlx::query_as::<_, (String, Option<serde_json::Value>)>(
                    "SELECT request_hash, response_payload FROM content_idempotency_keys WHERE user_id = $1 AND idempotency_key = $2 AND operation = 'publish' FOR UPDATE",
                )
                .bind(&content.author_id)
                .bind(key)
                .fetch_optional(&mut *tx)
                .await
                .map_err(DaoError::Database)?
            {
                let existing = published_idempotency_response(
                    key,
                    &request_fingerprint,
                    stored_fingerprint,
                    response,
                )?;
                tx.commit().await.map_err(DaoError::Database)?;
                return Ok(existing);
            }
        }

        update_content_in_transaction(&mut tx, &content).await?;
        if let Some(key) = idempotency_key {
            let response = serde_json::to_value(&content).map_err(DaoError::Serialization)?;
            sqlx::query(
                "INSERT INTO content_idempotency_keys (user_id,idempotency_key,operation,resource_id,request_hash,response_payload) VALUES ($1,$2,'publish',$3,$4,$5)",
            )
            .bind(&content.author_id)
            .bind(key)
            .bind(&content.id)
            .bind(request_fingerprint)
            .bind(response)
            .execute(&mut *tx)
            .await
            .map_err(DaoError::Database)?;
        }
        tx.commit().await.map_err(DaoError::Database)?;
        Ok(content)
    }
}
