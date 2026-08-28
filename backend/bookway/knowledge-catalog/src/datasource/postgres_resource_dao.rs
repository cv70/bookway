use super::*;
use sqlx::FromRow;

#[derive(FromRow)]
struct ResourceRow {
    id: String,
    title: String,
    kind: String,
    provider: String,
    summary: String,
    url: String,
    license: String,
    version: String,
    citation: String,
    topics: Vec<String>,
    status: String,
    published_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(FromRow)]
struct AttachmentRow {
    id: String,
    route_id: String,
    action_node_id: String,
    scene_equipment: String,
    resource_id: String,
    kind: String,
    title_override: String,
    note: String,
    sort_rank: i32,
    rag_enabled: bool,
    embedding_collection: String,
    retrieval_scope: String,
    created_by: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

#[derive(FromRow)]
struct RagEmbeddingRow {
    attachment_id: String,
    embedding: Vec<f32>,
}

fn row_to_resource(row: ResourceRow) -> Result<pb::Resource, DaoError> {
    Ok(pb::Resource {
        id: row.id,
        title: row.title,
        kind: parse_kind(&row.kind)?,
        provider: row.provider,
        summary: row.summary,
        url: row.url,
        license: row.license,
        version: row.version,
        citation: row.citation,
        topics: row.topics,
        status: parse_status(&row.status)?,
        published_at: format_time(row.published_at),
        updated_at: format_time(row.updated_at),
    })
}

fn row_to_attachment(
    row: AttachmentRow,
    resource: Option<pb::Resource>,
) -> Result<pb::RouteNodeResourceAttachment, DaoError> {
    Ok(pb::RouteNodeResourceAttachment {
        id: row.id,
        route_id: row.route_id,
        action_node_id: row.action_node_id,
        scene_equipment: row.scene_equipment,
        resource_id: row.resource_id,
        kind: parse_attachment_kind(&row.kind)?,
        title_override: row.title_override,
        note: row.note,
        sort_rank: row.sort_rank,
        rag_enabled: row.rag_enabled,
        embedding_collection: row.embedding_collection,
        retrieval_scope: row.retrieval_scope,
        created_by: row.created_by,
        created_at: format_time(row.created_at),
        updated_at: format_time(row.updated_at),
        resource,
    })
}

fn attachment_row_matches_request(
    row: &AttachmentRow,
    request: &NewNodeResourceAttachment,
) -> bool {
    row.route_id == request.route_id
        && row.action_node_id == request.action_node_id
        && row.scene_equipment == request.scene_equipment
        && row.resource_id == request.resource_id
        && row.kind == attachment_kind_name(request.kind)
        && row.title_override == request.title_override
        && row.note == request.note
        && row.sort_rank == request.sort_rank
        && row.rag_enabled == request.rag_enabled
        && row.embedding_collection == request.embedding_collection
        && row.retrieval_scope == request.retrieval_scope
        && row.created_by == request.created_by
}

pub(crate) struct PostgresResourceDao {
    pool: sqlx::PgPool,
}

impl PostgresResourceDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResourceDao for PostgresResourceDao {
    async fn search(&self, request: &pb::SearchRequest) -> Result<pb::SearchResponse, DaoError> {
        let offset = parse_cursor(&request.cursor)?;
        let limit = i64::from(request.limit.unwrap_or(20).clamp(1, 50));
        let kind = request
            .kind
            .and_then(|kind| pb::ResourceKind::try_from(kind).ok())
            .filter(|kind| *kind != pb::ResourceKind::Unspecified)
            .map(kind_name);
        let pattern = format!("%{}%", escape_like(&request.query.trim().to_lowercase()));
        let topic = request.topic.trim().to_string();
        // Relevance first: trigram similarity of the searchable projection.
        // An empty query scores every row 0, so directory browsing degrades
        // to the recency order the ILIKE-only path used before.
        let rows = sqlx::query_as::<_, ResourceRow>("SELECT id,title,kind,provider,summary,url,license,version,citation,topics,status,published_at,updated_at FROM public_resources WHERE status='published' AND ($1='' OR search_text ILIKE $2 ESCAPE '\\') AND ($3::TEXT IS NULL OR kind=$3) AND ($4='' OR $4 = ANY(topics)) ORDER BY similarity(search_text, $2) DESC, updated_at DESC, id DESC LIMIT $5 OFFSET $6")
            .bind(request.query.trim())
            .bind(pattern)
            .bind(kind)
            .bind(topic)
            .bind(limit + 1)
            .bind(i64::try_from(offset).unwrap_or(i64::MAX))
            .fetch_all(&self.pool)
            .await
            .map_err(DaoError::Database)?;
        let has_more = rows.len() > usize::try_from(limit).unwrap_or(50);
        let items = rows
            .into_iter()
            .take(usize::try_from(limit).unwrap_or(50))
            .map(row_to_resource)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(pb::SearchResponse {
            items,
            next_cursor: has_more.then(|| {
                offset
                    .saturating_add(usize::try_from(limit).unwrap_or(50))
                    .to_string()
            }),
        })
    }

    async fn get(&self, resource_id: &str) -> Result<pb::Resource, DaoError> {
        sqlx::query_as::<_, ResourceRow>("SELECT id,title,kind,provider,summary,url,license,version,citation,topics,status,published_at,updated_at FROM public_resources WHERE id=$1 AND status='published'")
            .bind(resource_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(DaoError::Database)?
            .map(row_to_resource)
            .transpose()?
            .ok_or_else(|| DaoError::NotFound(resource_id.to_string()))
    }

    async fn upsert_public_resource(
        &self,
        request: NewPublicResource,
    ) -> Result<pb::Resource, DaoError> {
        // The canonical URL is the catalog's identity anchor: an entry that
        // already owns the URL is updated in place instead of duplicated. An
        // existing caller-addressed id still wins so PATCH cannot silently
        // rewrite a different row.
        let mut target = request.resource_id.clone();
        if let Some(id) = &target {
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM public_resources WHERE id=$1)",
            )
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(DaoError::Database)?;
            if !exists {
                target = None;
            }
        }
        if target.is_none() {
            target = sqlx::query_scalar::<_, Option<String>>(
                "SELECT id FROM public_resources WHERE url=$1",
            )
            .bind(&request.url)
            .fetch_optional(&self.pool)
            .await
            .map_err(DaoError::Database)?
            .flatten();
        }
        let id = target.unwrap_or_else(|| Uuid::now_v7().to_string());
        // published_at is set once at creation and never rewritten by updates.
        sqlx::query_as::<_, ResourceRow>("INSERT INTO public_resources (id,title,kind,provider,summary,url,license,version,citation,topics,status,published_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,now(),now()) ON CONFLICT (id) DO UPDATE SET title=EXCLUDED.title,kind=EXCLUDED.kind,provider=EXCLUDED.provider,summary=EXCLUDED.summary,url=EXCLUDED.url,license=EXCLUDED.license,version=EXCLUDED.version,citation=EXCLUDED.citation,topics=EXCLUDED.topics,status=EXCLUDED.status,updated_at=now() RETURNING id,title,kind,provider,summary,url,license,version,citation,topics,status,published_at,updated_at")
            .bind(id)
            .bind(&request.title)
            .bind(kind_name(request.kind))
            .bind(&request.provider)
            .bind(&request.summary)
            .bind(&request.url)
            .bind(&request.license)
            .bind(&request.version)
            .bind(&request.citation)
            .bind(&request.topics)
            .bind(status_name(request.status))
            .fetch_one(&self.pool)
            .await
            .map_err(|error| {
                if error.as_database_error().is_some_and(|database| database.is_unique_violation())
                {
                    // Lost the create race for this URL: refuse rather than
                    // guess which of the two competing identities to keep.
                    DaoError::Conflict(
                        "resource url already belongs to a different resource".to_string(),
                    )
                } else {
                    DaoError::Database(error)
                }
            })
            .and_then(row_to_resource)
    }

    async fn list_node_resources(
        &self,
        route_id: &str,
        action_node_id: &str,
        scene_equipment: Option<&str>,
        include_archived: bool,
    ) -> Result<pb::ListNodeResourcesResponse, DaoError> {
        let rows = sqlx::query_as::<_, AttachmentRow>("SELECT id,route_id,action_node_id,scene_equipment,resource_id,kind,title_override,note,sort_rank,rag_enabled,embedding_collection,retrieval_scope,created_by,created_at,updated_at FROM route_node_resource_attachments WHERE route_id=$1 AND action_node_id=$2 AND ($3::TEXT IS NULL OR scene_equipment=$3) AND ($4 OR archived_at IS NULL) ORDER BY sort_rank ASC, created_at ASC, id ASC")
            .bind(route_id)
            .bind(action_node_id)
            .bind(scene_equipment)
            .bind(include_archived)
            .fetch_all(&self.pool)
            .await
            .map_err(DaoError::Database)?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let resource = self.get(&row.resource_id).await.ok();
            items.push(row_to_attachment(row, resource)?);
        }
        Ok(pb::ListNodeResourcesResponse { items })
    }

    async fn attach_node_resource(
        &self,
        request: NewNodeResourceAttachment,
    ) -> Result<pb::RouteNodeResourceAttachment, DaoError> {
        let resource = self.get(&request.resource_id).await?;
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        // A first request has no idempotency row to lock. Serialize by the
        // operation key before observing or inserting it, otherwise concurrent
        // retries can race the unique idempotency constraint.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1 || chr(31) || $2, 0))")
            .bind("route-node-resource")
            .bind(&request.idempotency_key)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
        if let Some(row) = sqlx::query_as::<_, AttachmentRow>("SELECT id,route_id,action_node_id,scene_equipment,resource_id,kind,title_override,note,sort_rank,rag_enabled,embedding_collection,retrieval_scope,created_by,created_at,updated_at FROM route_node_resource_attachments WHERE idempotency_key=$1 FOR UPDATE")
            .bind(&request.idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(DaoError::Database)?
        {
            if !attachment_row_matches_request(&row, &request) {
                return Err(DaoError::Conflict(
                    "idempotency key is already bound to a different resource attachment"
                        .to_string(),
                ));
            }
            let existing = row_to_attachment(row, Some(resource))?;
            transaction.commit().await.map_err(DaoError::Database)?;
            return Ok(existing);
        }

        let id = Uuid::new_v4().to_string();
        let kind = attachment_kind_name(request.kind);
        let row = sqlx::query_as::<_, AttachmentRow>("INSERT INTO route_node_resource_attachments (id,route_id,action_node_id,scene_equipment,resource_id,kind,title_override,note,sort_rank,rag_enabled,embedding_collection,retrieval_scope,created_by,idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT (route_id, action_node_id, resource_id, scene_equipment) WHERE archived_at IS NULL DO UPDATE SET kind=EXCLUDED.kind,title_override=EXCLUDED.title_override,note=EXCLUDED.note,sort_rank=EXCLUDED.sort_rank,rag_enabled=EXCLUDED.rag_enabled,embedding_collection=EXCLUDED.embedding_collection,retrieval_scope=EXCLUDED.retrieval_scope,created_by=EXCLUDED.created_by,updated_at=now() RETURNING id,route_id,action_node_id,scene_equipment,resource_id,kind,title_override,note,sort_rank,rag_enabled,embedding_collection,retrieval_scope,created_by,created_at,updated_at")
            .bind(id)
            .bind(&request.route_id)
            .bind(&request.action_node_id)
            .bind(&request.scene_equipment)
            .bind(&request.resource_id)
            .bind(kind)
            .bind(&request.title_override)
            .bind(&request.note)
            .bind(request.sort_rank)
            .bind(request.rag_enabled)
            .bind(&request.embedding_collection)
            .bind(&request.retrieval_scope)
            .bind(&request.created_by)
            .bind(&request.idempotency_key)
            .fetch_one(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
        let attachment = row_to_attachment(row, Some(resource))?;
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(attachment)
    }

    async fn detach_node_resource(
        &self,
        route_id: &str,
        action_node_id: &str,
        attachment_id: &str,
    ) -> Result<bool, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let result = sqlx::query("UPDATE route_node_resource_attachments SET archived_at=now(), updated_at=now() WHERE id=$1 AND route_id=$2 AND action_node_id=$3 AND archived_at IS NULL")
            .bind(attachment_id)
            .bind(route_id)
            .bind(action_node_id)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
        let detached = result.rows_affected() > 0;
        if detached {
            sqlx::query("DELETE FROM route_node_resource_embeddings WHERE attachment_id=$1")
                .bind(attachment_id)
                .execute(&mut *transaction)
                .await
                .map_err(DaoError::Database)?;
        }
        transaction.commit().await.map_err(DaoError::Database)?;
        Ok(detached)
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
        sqlx::query("INSERT INTO route_node_resource_embeddings (attachment_id,route_id,action_node_id,embedding_collection,embedding_model,embedding) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (attachment_id) DO UPDATE SET route_id=EXCLUDED.route_id,action_node_id=EXCLUDED.action_node_id,embedding_collection=EXCLUDED.embedding_collection,embedding_model=EXCLUDED.embedding_model,embedding=EXCLUDED.embedding,updated_at=now()")
            .bind(&attachment.id)
            .bind(&attachment.route_id)
            .bind(&attachment.action_node_id)
            .bind(&attachment.embedding_collection)
            .bind(embedding_model)
            .bind(embedding)
            .execute(&self.pool)
            .await
            .map_err(DaoError::Database)?;
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
        let rows = sqlx::query_as::<_, RagEmbeddingRow>("SELECT attachment_id,embedding FROM route_node_resource_embeddings WHERE route_id=$1 AND action_node_id=$2 AND embedding_collection=$3 AND embedding_model=$4")
            .bind(route_id)
            .bind(action_node_id)
            .bind(embedding_collection)
            .bind(embedding_model)
            .fetch_all(&self.pool)
            .await
            .map_err(DaoError::Database)?;
        let mut hits = rows
            .into_iter()
            .filter_map(|row| {
                cosine_similarity(query, &row.embedding).map(|relevance| RagVectorHit {
                    attachment_id: row.attachment_id,
                    relevance,
                })
            })
            .collect::<Vec<_>>();
        sort_rag_hits(&mut hits);
        hits.truncate(limit);
        Ok(hits)
    }
}
