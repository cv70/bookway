use super::*;

#[derive(Default)]
pub(crate) struct MemorySearchAnalytics {
    stats: RwLock<HashMap<SearchStatsKey, SearchCounters>>,
    global_users: RwLock<HashMap<SearchStatsKey, HashSet<String>>>,
    history: RwLock<HashMap<SearchHistoryKey, SearchCounters>>,
    sequence: RwLock<u64>,
}

#[async_trait]
impl SearchAnalytics for MemorySearchAnalytics {
    async fn record(
        &self,
        user_id: Option<&str>,
        query: &str,
        search_type: pb::SearchType,
        zero_results: bool,
    ) {
        let mut stats = self.stats.write().await;
        let value = stats
            .entry((query.to_string(), search_type))
            .or_insert((0, 0));
        value.0 = value.0.saturating_add(1);
        value.1 = value.1.saturating_add(u64::from(zero_results));
        let user_id = user_id.map(str::trim).filter(|id| !id.is_empty());
        if let Some(user_id) = user_id {
            self.global_users
                .write()
                .await
                .entry((query.to_string(), search_type))
                .or_default()
                .insert(user_id.to_string());
        }
        let Some(user_id) = user_id else { return };
        let mut sequence = self.sequence.write().await;
        *sequence = sequence.saturating_add(1);
        self.history.write().await.insert(
            (user_id.to_string(), query.to_string(), search_type),
            (value.0, *sequence),
        );
    }

    async fn suggestions(
        &self,
        user_id: Option<&str>,
        prefix: &str,
        limit: usize,
    ) -> Vec<pb::Suggestion> {
        let prefix = prefix.to_lowercase();
        let user_id = user_id.map(str::trim).filter(|id| !id.is_empty());
        let mut personal = {
            let history = self.history.read().await;
            history
                .iter()
                .filter(|((owner, query, _), _)| {
                    Some(owner.as_str()) == user_id && query.to_lowercase().contains(&prefix)
                })
                .map(
                    |((_, query, search_type), (requests, sequence))| pb::Suggestion {
                        text: query.clone(),
                        result_type: result_type(*search_type) as i32,
                        score: 10.0
                            + (*sequence as f64 / 1_000_000_000.0)
                            + suggestion_score(*requests, 0),
                        personal: true,
                    },
                )
                .collect::<Vec<_>>()
        };
        personal.sort_by(|left, right| right.score.total_cmp(&left.score));
        personal.truncate(limit);
        let stats = self.stats.read().await.clone();
        let global_users = self.global_users.read().await;
        let mut items = stats
            .iter()
            .filter(|((query, search_type), (requests, _))| {
                let unique_users = global_users
                    .get(&(query.clone(), *search_type))
                    .map_or(0, HashSet::len);
                *requests >= 2
                    && (unique_users == 0 || unique_users >= 2)
                    && query.to_lowercase().contains(&prefix)
            })
            .map(
                |((query, search_type), (requests, zero_results))| pb::Suggestion {
                    text: query.clone(),
                    result_type: result_type(*search_type) as i32,
                    score: suggestion_score(*requests, *zero_results),
                    personal: false,
                },
            )
            .collect::<Vec<_>>();
        items.retain(|item| !personal.iter().any(|owned| owned.text == item.text));
        personal.extend(items);
        let mut items = personal;
        items.sort_by(|left, right| right.score.total_cmp(&left.score));
        items.truncate(limit);
        items
    }
}
