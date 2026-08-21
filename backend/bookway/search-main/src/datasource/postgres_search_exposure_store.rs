use super::*;

pub(crate) struct PostgresSearchExposureStore {
    pool: sqlx::PgPool,
}

impl PostgresSearchExposureStore {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SearchExposureStore for PostgresSearchExposureStore {
    async fn record(&self, exposure: SearchExposure) -> Result<(), SearchExposureError> {
        let mut tx = self.pool.begin().await?;
        // Retain only the short attribution window while bounding cleanup work
        // for a single search response.
        sqlx::query("WITH expired AS (DELETE FROM search_exposures WHERE request_id IN (SELECT request_id FROM search_exposures WHERE expires_at <= now() ORDER BY expires_at LIMIT $1 FOR UPDATE SKIP LOCKED)) INSERT INTO search_exposures (request_id,user_id,session_id,query_hash,query_rewrite_version,result_count,degraded,expires_at) VALUES ($2,$3,$4,$5,$6,$7,$8,now() + ($9 * interval '1 second'))")
            .bind(SEARCH_EXPOSURE_CLEANUP_BATCH_SIZE)
            .bind(&exposure.request_id)
            .bind(&exposure.user_id)
            .bind(&exposure.session_id)
            .bind(&exposure.query_hash)
            .bind(&exposure.query_rewrite_version)
            .bind(i32::try_from(exposure.items.len()).unwrap_or(i32::MAX))
            .bind(exposure.degraded)
            .bind(SEARCH_EXPOSURE_TTL.as_secs() as i64)
            .execute(&mut *tx)
            .await?;
        for item in &exposure.items {
            sqlx::query("INSERT INTO search_exposure_items (request_id,position,result_id,result_type) VALUES ($1,$2,$3,$4)")
                .bind(&exposure.request_id)
                .bind(i32::try_from(item.position).unwrap_or(i32::MAX))
                .bind(&item.result_id)
                .bind(&item.result_type)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn validate(
        &self,
        user_id: &str,
        attributions: &[SearchAttribution],
    ) -> Result<Vec<bool>, SearchExposureError> {
        if attributions.is_empty() {
            return Ok(Vec::new());
        }
        let positions = attributions
            .iter()
            .map(|item| i32::try_from(item.position))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| SearchExposureError::PositionOutOfRange)?;
        let rows = sqlx::query_as::<_, (i64, bool)>(
            "SELECT input.ordinality, EXISTS (SELECT 1 FROM search_exposures AS exposure INNER JOIN search_exposure_items AS item ON item.request_id = exposure.request_id WHERE exposure.request_id = input.request_id AND exposure.user_id = $1 AND exposure.session_id = input.session_id AND item.position = input.position AND item.result_id = input.result_id) AS valid FROM unnest($2::text[], $3::text[], $4::text[], $5::integer[]) WITH ORDINALITY AS input(request_id, session_id, result_id, position, ordinality) ORDER BY input.ordinality",
        )
        .bind(user_id)
        .bind(attributions.iter().map(|item| item.request_id.clone()).collect::<Vec<_>>())
        .bind(attributions.iter().map(|item| item.session_id.clone()).collect::<Vec<_>>())
        .bind(attributions.iter().map(|item| item.result_id.clone()).collect::<Vec<_>>())
        .bind(positions)
        .fetch_all(&self.pool)
        .await?;
        let mut valid = vec![false; attributions.len()];
        for (ordinality, is_valid) in rows {
            if let Some(index) = usize::try_from(ordinality)
                .ok()
                .and_then(|value| value.checked_sub(1))
                .filter(|index| *index < valid.len())
            {
                valid[index] = is_valid;
            }
        }
        Ok(valid)
    }
}
