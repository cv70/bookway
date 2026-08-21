use super::*;

pub(crate) struct PostgresQueryRewriteDao {
    pool: sqlx::PgPool,
}

impl PostgresQueryRewriteDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl QueryRewriteDao for PostgresQueryRewriteDao {
    async fn active(&self) -> Result<Option<QueryRewriteDictionary>, QueryRewriteError> {
        let rows = sqlx::query_as::<_, (String, Option<String>, Option<Vec<String>>)>(
            "SELECT active.version, rule.trigger, rule.expansion_terms FROM search_query_rewrite_active AS active INNER JOIN search_query_rewrite_versions AS version ON version.version = active.version AND version.status = 'ready' LEFT JOIN search_query_rewrite_rules AS rule ON rule.version = active.version ORDER BY rule.trigger",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| QueryRewriteError::Storage(error.to_string()))?;
        let Some((version, _, _)) = rows.first() else {
            return Ok(None);
        };
        let version = version.clone();
        let rules = rows
            .into_iter()
            .filter_map(|(_, trigger, expansion_terms)| {
                Some(QueryRewriteRule {
                    trigger: trigger?,
                    expansion_terms: expansion_terms?,
                })
            })
            .collect();
        Ok(Some(QueryRewriteDictionary { version, rules }))
    }
}
