use super::*;

pub(crate) struct PostgresExposureDataSource {
    pool: sqlx::PgPool,
}

impl PostgresExposureDataSource {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExposureDataSource for PostgresExposureDataSource {
    async fn record(&self, exposure: Exposure) -> Result<(), ExposureError> {
        let mut transaction = self.pool.begin().await?;
        let selected_count = i32::try_from(exposure.items.len()).unwrap_or(i32::MAX);
        let candidate_count = i32::try_from(exposure.candidate_count).unwrap_or(i32::MAX);
        sqlx::query(
            "INSERT INTO feed_exposures (request_id, user_id, session_id, surface, pipeline_id, model_version, experiment_bucket, candidate_count, selected_count, degraded) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(&exposure.request_id)
        .bind(&exposure.user_id)
        .bind(&exposure.session_id)
        .bind(&exposure.surface)
        .bind(&exposure.pipeline_id)
        .bind(&exposure.model_version)
        .bind(&exposure.experiment_bucket)
        .bind(candidate_count)
        .bind(selected_count)
        .bind(exposure.degraded)
        .execute(&mut *transaction)
        .await?;
        for item in &exposure.items {
            let position = i32::try_from(item.position).unwrap_or(i32::MAX);
            sqlx::query(
                "INSERT INTO feed_exposure_items (request_id, position, content_id, source, score, p_ctr, p_cvr, p_wegu, feature_snapshot, reasons) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(&exposure.request_id)
            .bind(position)
            .bind(&item.content_id)
            .bind(&item.source)
            .bind(item.score)
            .bind(item.p_ctr)
            .bind(item.p_cvr)
            .bind(item.p_wegu)
            .bind(&item.feature_snapshot)
            .bind(serde_json::json!(item.reasons))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn recent_content_ids(
        &self,
        user_id: &str,
        surface: &str,
        limit: usize,
    ) -> HashSet<String> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT item.content_id FROM feed_exposure_items AS item INNER JOIN feed_exposures AS exposure ON exposure.request_id = item.request_id WHERE exposure.user_id = $1 AND exposure.surface = $2 AND exposure.created_at > now() - interval '7 days' ORDER BY exposure.created_at DESC, item.position ASC LIMIT $3",
        )
        .bind(user_id)
        .bind(surface)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await;
        match rows {
            Ok(rows) => rows.into_iter().collect(),
            Err(error) => {
                tracing::warn!(%error, "served history read degraded");
                HashSet::new()
            }
        }
    }

    async fn validate_attributions(
        &self,
        user_id: &str,
        attributions: &[ExposureAttribution],
    ) -> Result<Vec<bool>, ExposureError> {
        if attributions.is_empty() {
            return Ok(Vec::new());
        }
        let positions = attributions
            .iter()
            .map(|attribution| i32::try_from(attribution.position))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ExposureError::PositionOutOfRange)?;
        let request_ids = attributions
            .iter()
            .map(|attribution| attribution.request_id.clone())
            .collect::<Vec<_>>();
        let session_ids = attributions
            .iter()
            .map(|attribution| attribution.session_id.clone())
            .collect::<Vec<_>>();
        let content_ids = attributions
            .iter()
            .map(|attribution| attribution.content_id.clone())
            .collect::<Vec<_>>();
        let rows = sqlx::query_as::<_, (i64, bool)>(
            "SELECT input.ordinality, EXISTS (SELECT 1 FROM feed_exposures AS exposure INNER JOIN feed_exposure_items AS item ON item.request_id = exposure.request_id WHERE exposure.request_id = input.request_id AND exposure.user_id = $1 AND exposure.session_id = input.session_id AND item.position = input.position AND item.content_id = input.content_id) AS valid FROM unnest($2::text[], $3::text[], $4::text[], $5::integer[]) WITH ORDINALITY AS input(request_id, session_id, content_id, position, ordinality) ORDER BY input.ordinality",
        )
        .bind(user_id)
        .bind(request_ids)
        .bind(session_ids)
        .bind(content_ids)
        .bind(positions)
        .fetch_all(&self.pool)
        .await?;
        let mut valid = vec![false; attributions.len()];
        for (ordinality, is_valid) in rows {
            let index = usize::try_from(ordinality)
                .ok()
                .and_then(|value| value.checked_sub(1));
            if let Some(index) = index.filter(|index| *index < valid.len()) {
                valid[index] = is_valid;
            }
        }
        Ok(valid)
    }
}
