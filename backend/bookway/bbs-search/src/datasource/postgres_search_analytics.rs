use super::*;

pub(crate) struct PostgresSearchAnalytics {
    pool: sqlx::PgPool,
}

impl PostgresSearchAnalytics {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SearchAnalytics for PostgresSearchAnalytics {
    async fn record(
        &self,
        user_id: Option<&str>,
        query: &str,
        search_type: pb::SearchType,
        zero_results: bool,
    ) {
        let search_type = search_type_name(search_type);
        let hash = format!("{:016x}", stable_hash(&format!("{search_type}\0{query}")));
        if let Err(error) = sqlx::query(
            "INSERT INTO search_query_stats (query_hash,query_text,search_type,request_count,zero_result_count,last_seen_at) VALUES ($1,$2,$3,1,$4,now()) ON CONFLICT (query_hash) DO UPDATE SET request_count=search_query_stats.request_count+1, zero_result_count=search_query_stats.zero_result_count+EXCLUDED.zero_result_count, last_seen_at=now()",
        )
        .bind(&hash)
        .bind(query)
        .bind(search_type)
        .bind(i64::from(zero_results))
        .execute(&self.pool)
        .await
        {
            tracing::warn!(%error, "search analytics write degraded");
        }
        if let Some(user_id) = user_id.map(str::trim).filter(|id| !id.is_empty()) {
            let history_hash = format!("{:016x}", stable_hash(&format!("{user_id}\0{query}")));
            if let Err(error) = sqlx::query(
                "INSERT INTO search_query_history (history_hash,user_id,query_text,search_type,request_count,last_seen_at) VALUES ($1,$2,$3,$4,1,now()) ON CONFLICT (history_hash) DO UPDATE SET request_count=search_query_history.request_count+1,last_seen_at=now()",
            )
            .bind(history_hash)
            .bind(user_id)
            .bind(query)
            .bind(search_type)
            .execute(&self.pool)
            .await
            {
                tracing::warn!(%error, "personal search history write degraded");
            }
            if let Err(error) = sqlx::query(
                "INSERT INTO search_query_stats_users (query_hash,user_id,last_seen_at) VALUES ($1,$2,now()) ON CONFLICT (query_hash,user_id) DO UPDATE SET last_seen_at=now()",
            )
            .bind(&hash)
            .bind(user_id)
            .execute(&self.pool)
            .await
            {
                tracing::warn!(%error, "search query unique-user write degraded");
            }
            if let Err(error) = sqlx::query(
                "DELETE FROM search_query_history WHERE user_id = $1 AND last_seen_at < now() - interval '90 days'",
            )
            .bind(user_id)
            .execute(&self.pool)
            .await
            {
                tracing::warn!(%error, "personal search history retention cleanup degraded");
            }
        }
    }

    async fn suggestions(
        &self,
        user_id: Option<&str>,
        prefix: &str,
        limit: usize,
    ) -> Vec<pb::Suggestion> {
        let pattern = format!("%{}%", escape_like(prefix));
        let mut items = Vec::new();
        if let Some(user_id) = user_id.map(str::trim).filter(|id| !id.is_empty()) {
            let rows = sqlx::query_as::<_, (String, String, i64)>(&format!(
                "SELECT query_text,search_type,request_count FROM search_query_history WHERE user_id = $1 AND query_text ILIKE $2 ESCAPE '\\\\' AND last_seen_at > now() - interval '90 days' ORDER BY last_seen_at DESC LIMIT {}",
                limit
            ))
            .bind(user_id)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await;
            if let Ok(rows) = rows {
                items.extend(rows.into_iter().map(|(text, search_type, requests)| {
                    pb::Suggestion {
                        text,
                        result_type: result_type_from_name(&search_type) as i32,
                        score: 10.0 + suggestion_score(requests.max(0) as u64, 0),
                        personal: true,
                    }
                }));
            }
        }
        let rows = sqlx::query_as::<_, (String, String, i64, i64)>(
            "SELECT stats.query_text,stats.search_type,stats.request_count,stats.zero_result_count FROM search_query_stats AS stats LEFT JOIN (SELECT query_hash,COUNT(*) AS unique_users FROM search_query_stats_users GROUP BY query_hash) AS users ON users.query_hash = stats.query_hash WHERE stats.request_count >= 2 AND (COALESCE(users.unique_users, 0) = 0 OR users.unique_users >= 2) AND stats.query_text ILIKE $1 ESCAPE '\\' AND stats.last_seen_at > now() - interval '90 days' ORDER BY (stats.request_count-stats.zero_result_count) DESC,stats.last_seen_at DESC LIMIT $2",
        )
        .bind(pattern)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await;
        match rows {
            Ok(rows) => {
                items.extend(rows.into_iter().map(
                    |(text, search_type, requests, zero_results)| pb::Suggestion {
                        text,
                        result_type: result_type_from_name(&search_type) as i32,
                        score: suggestion_score(requests.max(0) as u64, zero_results.max(0) as u64),
                        personal: false,
                    },
                ));
                items.sort_by(|left, right| right.score.total_cmp(&left.score));
                items.dedup_by(|left, right| left.text == right.text);
                items.truncate(limit);
                items
            }
            Err(error) => {
                tracing::warn!(%error, "search analytics suggestions degraded");
                items.truncate(limit);
                items
            }
        }
    }
}
