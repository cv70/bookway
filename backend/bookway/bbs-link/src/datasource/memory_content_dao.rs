use super::*;

pub(crate) struct MemoryContentDao {
    state: RwLock<State>,
}

impl MemoryContentDao {
    pub(crate) fn seeded() -> Self {
        Self {
            state: RwLock::new(State {
                contents: vec![
                    seed(SeedContent {
                        id: "post-city-walk",
                        author_name: "木川",
                        author_id: "author-muchuan",
                        title: "我用 7 次散步重新认识了杭州",
                        summary: "不赶景点，只沿着水系和旧城慢慢走。每次回来，我都画一张自己的城市地图。",
                        domain: pb::GrowthDomain::Travel,
                        route_title: "7 次城市观察散步",
                        route_duration: "3 周",
                        join_count: 4862,
                        like_count: 9128,
                        freshness: 0.94,
                        tags: "城市漫游,观察",
                        created_at: "2026-08-10T08:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1537531383496-f4749b8032cf?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1500648767791-00dcc994a43e?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-reading",
                        author_name: "一册",
                        author_id: "author-yice",
                        title: "读完 12 本书后，我留下了这套主题阅读法",
                        summary: "从问题出发选择三本结构不同的书，每周只整理一个能用于生活的结论。",
                        domain: pb::GrowthDomain::Learning,
                        route_title: "四周主题阅读实验",
                        route_duration: "4 周",
                        join_count: 7130,
                        like_count: 15420,
                        freshness: 0.88,
                        tags: "阅读,知识管理",
                        created_at: "2026-08-09T09:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1495446815901-a7297e633e8d?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1494790108377-be9c29b29330?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-running",
                        author_name: "长风",
                        author_id: "author-changfeng",
                        title: "从跑不动两公里，到享受清晨的五公里",
                        summary: "真正有用的不是逼自己更快，而是给身体足够的恢复时间，并记录每次感受。",
                        domain: pb::GrowthDomain::Movement,
                        route_title: "零压力晨跑计划",
                        route_duration: "6 周",
                        join_count: 9854,
                        like_count: 22180,
                        freshness: 0.91,
                        tags: "跑步,晨间",
                        created_at: "2026-08-08T06:30:00Z",
                        cover_url: "https://images.unsplash.com/photo-1552674605-db6ffd4facb5?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1534528741775-53994a69daeb?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-sleep",
                        author_name: "林间钟",
                        author_id: "author-linjian",
                        title: "把睡前一小时还给自己之后",
                        summary: "我没有追求完美作息，只做了三个小调整，白天的注意力却明显回来了。",
                        domain: pb::GrowthDomain::Wellness,
                        route_title: "温和睡眠修复",
                        route_duration: "14 天",
                        join_count: 6321,
                        like_count: 10438,
                        freshness: 0.84,
                        tags: "睡眠,精力",
                        created_at: "2026-08-07T21:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1455642305367-68834a9d9aab?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1506794778202-cad84cf45f1d?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-pottery",
                        author_name: "未名",
                        author_id: "author-weiming",
                        title: "周末做陶，让时间重新慢下来",
                        summary: "手上的泥总有自己的脾气。两个周末之后，我不再急着控制最后的样子。",
                        domain: pb::GrowthDomain::Leisure,
                        route_title: "陶艺初体验",
                        route_duration: "2 周",
                        join_count: 2176,
                        like_count: 6890,
                        freshness: 0.96,
                        tags: "手作,放松",
                        created_at: "2026-08-06T10:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1610701596007-11502861dcfa?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1531123897727-8f129e1688ce?w=160&h=160&fit=crop",
                    }),
                    seed(SeedContent {
                        id: "post-museum",
                        author_name: "知也",
                        author_id: "author-zhiy",
                        title: "不做功课，也能认真看完一场展",
                        summary: "从一件真正好奇的作品开始，先描述看到什么，再去读作品背后的故事。",
                        domain: pb::GrowthDomain::Learning,
                        route_title: "三次博物馆观察练习",
                        route_duration: "3 周",
                        join_count: 3952,
                        like_count: 10582,
                        freshness: 0.82,
                        tags: "博物馆,观察",
                        created_at: "2026-08-05T14:00:00Z",
                        cover_url: "https://images.unsplash.com/photo-1564399579883-451a5d44ec08?w=1200&h=900&fit=crop",
                        avatar_url: "https://images.unsplash.com/photo-1534528741775-53994a69daeb?w=160&h=160&fit=crop",
                    }),
                ],
                idempotency: HashMap::new(),
            }),
        }
    }
}

#[async_trait]
impl ContentDao for MemoryContentDao {
    async fn list(&self, query: &pb::ListRequest) -> Result<pb::ContentPage, DaoError> {
        let state = self.state.read().await;
        let mut items: Vec<_> = state
            .contents
            .iter()
            .filter(|content| query.status.is_none_or(|status| content.status == status))
            .filter(|content| {
                query
                    .ids
                    .as_deref()
                    .is_none_or(|ids| ids.split(',').any(|id| id.trim() == content.id))
            })
            .filter(|content| {
                query
                    .author_id
                    .as_deref()
                    .is_none_or(|author_id| content.author_id == author_id)
            })
            .filter(|content| {
                query.author_ids.is_empty() || query.author_ids.contains(&content.author_id)
            })
            .filter(|content| {
                query
                    .content_type
                    .is_none_or(|content_type| content.content_type == content_type)
            })
            .filter(|content| {
                query.domain.is_none_or(|domain| {
                    content
                        .post
                        .as_ref()
                        .is_some_and(|post| post.domain == domain)
                })
            })
            .cloned()
            .collect();
        match query.strategy.as_deref() {
            Some("fresh") => items.sort_by(|left, right| right.created_at.cmp(&left.created_at)),
            _ => items.sort_by(|left, right| {
                right
                    .quality_score
                    .total_cmp(&left.quality_score)
                    .then_with(|| right.created_at.cmp(&left.created_at))
            }),
        }
        let total = items.len();
        let offset = query
            .cursor
            .as_deref()
            .and_then(|cursor| cursor.parse::<usize>().ok())
            .unwrap_or(0)
            .min(total);
        let limit = query.limit.unwrap_or(20).clamp(1, 100) as usize;
        let page: Vec<_> = items.into_iter().skip(offset).take(limit).collect();
        Ok(pb::ContentPage {
            next_cursor: (offset + page.len() < total).then(|| (offset + page.len()).to_string()),
            items: page,
            total_estimate: total as u64,
        })
    }

    async fn get(&self, id: &str) -> Result<pb::Content, DaoError> {
        self.state
            .read()
            .await
            .contents
            .iter()
            .find(|content| content.id == id)
            .cloned()
            .ok_or_else(|| DaoError::NotFound(id.to_string()))
    }

    async fn published_by_idempotency_key(
        &self,
        user_id: &str,
        idempotency_key: &str,
        request_fingerprint: &str,
    ) -> Result<Option<pb::Content>, DaoError> {
        let state = self.state.read().await;
        let key = (
            user_id.to_string(),
            "publish".to_string(),
            idempotency_key.to_string(),
        );
        let Some(existing) = state.idempotency.get(&key) else {
            return Ok(None);
        };
        if existing.request_fingerprint != request_fingerprint {
            return Err(DaoError::IdempotencyConflict(idempotency_key.to_string()));
        }
        existing.response.clone().map(Some).ok_or_else(|| {
            DaoError::InvalidContent(
                "publish idempotency record is missing its response snapshot".to_string(),
            )
        })
    }

    async fn create(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, DaoError> {
        let mut state = self.state.write().await;
        if let Some(key) = idempotency_key {
            let scoped_key = (content.author_id.clone(), "create".to_string(), key.clone());
            if let Some(existing) = state.idempotency.get(&scoped_key) {
                if existing.request_fingerprint != request_fingerprint {
                    return Err(DaoError::IdempotencyConflict(key));
                }
                return state
                    .contents
                    .iter()
                    .find(|item| item.id == existing.content_id)
                    .cloned()
                    .ok_or_else(|| DaoError::NotFound(existing.content_id.clone()));
            }
            state.idempotency.insert(
                scoped_key,
                IdempotencyRecord {
                    content_id: content.id.clone(),
                    request_fingerprint,
                    response: None,
                },
            );
        }
        state.contents.push(content.clone());
        Ok(content)
    }

    async fn update(&self, content: pb::Content) -> Result<pb::Content, DaoError> {
        let mut state = self.state.write().await;
        let existing = state
            .contents
            .iter_mut()
            .find(|item| item.id == content.id)
            .ok_or_else(|| DaoError::NotFound(content.id.clone()))?;
        *existing = content.clone();
        bump_source_fork_count(&mut state, &content);
        Ok(content)
    }

    async fn publish(
        &self,
        content: pb::Content,
        idempotency_key: Option<String>,
        request_fingerprint: String,
    ) -> Result<pb::Content, DaoError> {
        let mut state = self.state.write().await;
        if let Some(key) = idempotency_key {
            let scoped_key = (
                content.author_id.clone(),
                "publish".to_string(),
                key.clone(),
            );
            if let Some(existing) = state.idempotency.get(&scoped_key) {
                if existing.request_fingerprint != request_fingerprint {
                    return Err(DaoError::IdempotencyConflict(key));
                }
                return existing.response.clone().ok_or_else(|| {
                    DaoError::InvalidContent(
                        "publish idempotency record is missing its response snapshot".to_string(),
                    )
                });
            }
            let existing = state
                .contents
                .iter_mut()
                .find(|item| item.id == content.id)
                .ok_or_else(|| DaoError::NotFound(content.id.clone()))?;
            *existing = content.clone();
            state.idempotency.insert(
                scoped_key,
                IdempotencyRecord {
                    content_id: content.id.clone(),
                    request_fingerprint,
                    response: Some(content.clone()),
                },
            );
            return Ok(content);
        }
        let existing = state
            .contents
            .iter_mut()
            .find(|item| item.id == content.id)
            .ok_or_else(|| DaoError::NotFound(content.id.clone()))?;
        *existing = content.clone();
        Ok(content)
    }
}

/// Mirrors the Postgres publish transaction: a published fork increments its
/// source route's fork_count so local mode stays consistent with production.
fn bump_source_fork_count(state: &mut State, fork: &pb::Content) {
    let Some(source_route_id) = fork
        .route_fork
        .as_ref()
        .map(|fork| fork.source_route_id.clone())
        .filter(|id| !id.is_empty())
    else {
        return;
    };
    if let Some(source) = state
        .contents
        .iter_mut()
        .find(|item| item.id == source_route_id)
    {
        if let Some(post) = source.post.as_mut() {
            post.fork_count = post.fork_count.saturating_add(1);
        }
    }
}
