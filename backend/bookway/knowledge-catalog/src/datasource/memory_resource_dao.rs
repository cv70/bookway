use super::*;

pub(crate) struct MemoryResourceDao {
    resources: RwLock<Vec<pb::Resource>>,
    attachments: RwLock<Vec<pb::RouteNodeResourceAttachment>>,
    attachment_idempotency: RwLock<HashMap<String, String>>,
    rag_embeddings: RwLock<HashMap<String, RagEmbedding>>,
}

impl MemoryResourceDao {
    pub(crate) fn seeded() -> Self {
        Self {
            resources: RwLock::new(vec![
                resource(
                    "resource-mdn-web",
                    "MDN Web Docs",
                    pb::ResourceKind::Article,
                    "Mozilla",
                    "面向开发者的开放 Web 平台文档与实践参考。",
                    "https://developer.mozilla.org/",
                    "CC BY-SA 2.5",
                    "2026.1",
                    "Mozilla Developer Network. MDN Web Docs. 2026.",
                    &["学习", "工具", "编程"],
                ),
                resource(
                    "resource-ocw-learning",
                    "MIT OpenCourseWare",
                    pb::ResourceKind::Course,
                    "MIT",
                    "公开课程资料，适合按主题建立长期学习路径。",
                    "https://ocw.mit.edu/",
                    "CC BY-NC-SA 4.0",
                    "2026",
                    "MIT OpenCourseWare. 2026.",
                    &["学习", "课程"],
                ),
                resource(
                    "resource-gutenberg",
                    "Project Gutenberg",
                    pb::ResourceKind::Book,
                    "Project Gutenberg",
                    "可合法阅读和下载的公共领域电子书目录。",
                    "https://www.gutenberg.org/",
                    "Public Domain",
                    "2026",
                    "Project Gutenberg. 2026.",
                    &["阅读", "书籍", "知识管理"],
                ),
            ]),
            attachments: RwLock::new(Vec::new()),
            attachment_idempotency: RwLock::new(HashMap::new()),
            rag_embeddings: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ResourceDao for MemoryResourceDao {
    async fn search(&self, request: &pb::SearchRequest) -> Result<pb::SearchResponse, DaoError> {
        let query = request.query.trim().to_lowercase();
        let topic = request.topic.trim().to_lowercase();
        let kind = request
            .kind
            .map(pb::ResourceKind::try_from)
            .transpose()
            .map_err(|_| DaoError::Invalid("invalid resource kind".to_string()))?;
        let offset = parse_cursor(&request.cursor)?;
        let limit = usize::try_from(request.limit.unwrap_or(20).clamp(1, 50)).unwrap_or(20);
        let resources = self.resources.read().await;
        let mut items = resources
            .iter()
            .filter(|item| item.status == pb::ResourceStatus::Published as i32)
            .filter(|item| kind.is_none() || kind == pb::ResourceKind::try_from(item.kind).ok())
            .filter(|item| {
                topic.is_empty()
                    || item
                        .topics
                        .iter()
                        .any(|value| value.to_lowercase() == topic)
            })
            .filter(|item| {
                query.is_empty()
                    || [
                        item.title.as_str(),
                        item.summary.as_str(),
                        item.provider.as_str(),
                        item.citation.as_str(),
                    ]
                    .into_iter()
                    .any(|value| value.to_lowercase().contains(&query))
                    || item
                        .topics
                        .iter()
                        .any(|value| value.to_lowercase().contains(&query))
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        let has_more = offset.saturating_add(limit) < items.len();
        let page = items
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        Ok(pb::SearchResponse {
            items: page,
            next_cursor: has_more.then(|| offset.saturating_add(limit).to_string()),
        })
    }

    async fn get(&self, resource_id: &str) -> Result<pb::Resource, DaoError> {
        self.resources
            .read()
            .await
            .iter()
            .find(|resource| {
                resource.id == resource_id
                    && resource.status == pb::ResourceStatus::Published as i32
            })
            .cloned()
            .ok_or_else(|| DaoError::NotFound(resource_id.to_string()))
    }

    async fn list_node_resources(
        &self,
        route_id: &str,
        action_node_id: &str,
        include_archived: bool,
    ) -> Result<pb::ListNodeResourcesResponse, DaoError> {
        let resources = self.resources.read().await;
        let resource_by_id = resources
            .iter()
            .map(|resource| (resource.id.as_str(), resource.clone()))
            .collect::<HashMap<_, _>>();
        let mut items = self
            .attachments
            .read()
            .await
            .iter()
            .filter(|attachment| {
                attachment.route_id == route_id
                    && attachment.action_node_id == action_node_id
                    && (include_archived || !attachment.updated_at.starts_with("archived:"))
            })
            .cloned()
            .map(|mut attachment| {
                attachment.resource = resource_by_id.get(attachment.resource_id.as_str()).cloned();
                attachment
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            left.sort_rank
                .cmp(&right.sort_rank)
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(pb::ListNodeResourcesResponse { items })
    }

    async fn attach_node_resource(
        &self,
        request: NewNodeResourceAttachment,
    ) -> Result<pb::RouteNodeResourceAttachment, DaoError> {
        let resource = self.get(&request.resource_id).await?;
        if let Some(existing_id) = self
            .attachment_idempotency
            .read()
            .await
            .get(&request.idempotency_key)
            .cloned()
        {
            let attachments = self.attachments.read().await;
            let existing = attachments
                .iter()
                .find(|item| item.id == existing_id)
                .ok_or_else(|| {
                    DaoError::Invalid("missing node resource idempotency target".to_string())
                })?;
            if !attachment_matches_request(existing, &request) {
                return Err(DaoError::Conflict(
                    "idempotency key is already bound to a different resource attachment"
                        .to_string(),
                ));
            }
            let mut existing = existing.clone();
            existing.resource = Some(resource);
            return Ok(existing);
        }

        let mut attachments = self.attachments.write().await;
        if let Some(existing) = attachments.iter_mut().find(|item| {
            item.route_id == request.route_id
                && item.action_node_id == request.action_node_id
                && item.resource_id == request.resource_id
                && !item.updated_at.starts_with("archived:")
        }) {
            existing.kind = request.kind as i32;
            existing.title_override = request.title_override;
            existing.note = request.note;
            existing.sort_rank = request.sort_rank;
            existing.rag_enabled = request.rag_enabled;
            existing.embedding_collection = request.embedding_collection;
            existing.retrieval_scope = request.retrieval_scope;
            existing.created_by = request.created_by;
            existing.updated_at = "2026-08-18T00:00:00Z".to_string();
            existing.resource = Some(resource);
            self.attachment_idempotency
                .write()
                .await
                .insert(request.idempotency_key, existing.id.clone());
            return Ok(existing.clone());
        }

        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "bookway:route-node-resource:{}:{}:{}:{}",
                request.route_id,
                request.action_node_id,
                request.resource_id,
                request.idempotency_key
            )
            .as_bytes(),
        )
        .to_string();
        let now = "2026-08-18T00:00:00Z".to_string();
        let attachment = pb::RouteNodeResourceAttachment {
            id: id.clone(),
            route_id: request.route_id,
            action_node_id: request.action_node_id,
            resource_id: request.resource_id,
            kind: request.kind as i32,
            title_override: request.title_override,
            note: request.note,
            sort_rank: request.sort_rank,
            rag_enabled: request.rag_enabled,
            embedding_collection: request.embedding_collection,
            retrieval_scope: request.retrieval_scope,
            created_by: request.created_by,
            created_at: now.clone(),
            updated_at: now,
            resource: Some(resource),
        };
        attachments.push(attachment.clone());
        self.attachment_idempotency
            .write()
            .await
            .insert(request.idempotency_key, id);
        Ok(attachment)
    }

    async fn detach_node_resource(
        &self,
        route_id: &str,
        action_node_id: &str,
        attachment_id: &str,
    ) -> Result<bool, DaoError> {
        let mut attachments = self.attachments.write().await;
        let Some(attachment) = attachments.iter_mut().find(|attachment| {
            attachment.id == attachment_id
                && attachment.route_id == route_id
                && attachment.action_node_id == action_node_id
        }) else {
            return Ok(false);
        };
        if attachment.updated_at.starts_with("archived:") {
            return Ok(false);
        }
        attachment.updated_at = "archived:2026-08-18T00:00:00Z".to_string();
        attachment.resource = None;
        self.rag_embeddings.write().await.remove(attachment_id);
        Ok(true)
    }

    async fn upsert_rag_embedding(
        &self,
        attachment: &pb::RouteNodeResourceAttachment,
        embedding_model: &str,
        embedding: Vec<f32>,
    ) -> Result<(), DaoError> {
        validate_embedding(embedding_model, &embedding)?;
        if attachment.id.trim().is_empty()
            || attachment.route_id.trim().is_empty()
            || attachment.action_node_id.trim().is_empty()
            || attachment.embedding_collection.trim().is_empty()
        {
            return Err(DaoError::Invalid(
                "RAG embedding attachment scope is incomplete".to_string(),
            ));
        }
        self.rag_embeddings.write().await.insert(
            attachment.id.clone(),
            RagEmbedding {
                route_id: attachment.route_id.clone(),
                action_node_id: attachment.action_node_id.clone(),
                embedding_collection: attachment.embedding_collection.clone(),
                embedding_model: embedding_model.to_string(),
                embedding,
            },
        );
        Ok(())
    }

    async fn search_rag_embeddings(
        &self,
        route_id: &str,
        action_node_id: &str,
        embedding_collection: &str,
        embedding_model: &str,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<RagVectorHit>, DaoError> {
        validate_embedding(embedding_model, query)?;
        let mut hits = self
            .rag_embeddings
            .read()
            .await
            .iter()
            .filter(|(_, embedding)| {
                embedding.route_id == route_id
                    && embedding.action_node_id == action_node_id
                    && embedding.embedding_collection == embedding_collection
                    && embedding.embedding_model == embedding_model
            })
            .filter_map(|(attachment_id, embedding)| {
                cosine_similarity(query, &embedding.embedding).map(|relevance| RagVectorHit {
                    attachment_id: attachment_id.clone(),
                    relevance,
                })
            })
            .collect::<Vec<_>>();
        sort_rag_hits(&mut hits);
        hits.truncate(limit);
        Ok(hits)
    }
}
