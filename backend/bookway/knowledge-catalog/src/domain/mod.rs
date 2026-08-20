use std::sync::Arc;

use bookway_bbs_link_api::pb::{self as bbs_link, bbs_link_client::BbsLinkClient};
use bookway_knowledge_catalog_api::pb;

use crate::{
    conf::Config,
    datasource::{
        MemoryResourceRepository, NewNodeResourceAttachment, PostgresResourceRepository,
        RepositoryError, ResourceRepository,
    },
};

#[derive(Clone)]
pub(crate) struct Domain {
    config: Config,
    repository: Arc<dyn ResourceRepository>,
    bbs_link: Option<BbsLinkClient<tonic::transport::Channel>>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DomainInitError {
    #[error("could not initialize resource storage: {0}")]
    Data(#[from] bookway_data::DataError),
    #[error("could not connect to BBS Link: {0}")]
    Transport(#[from] tonic::transport::Error),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DomainError {
    #[error("invalid request: {0}")]
    Validation(String),
    #[error("{0}")]
    Repository(#[from] RepositoryError),
    #[error("BBS Link request failed: {0}")]
    Upstream(String),
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, DomainInitError> {
        let repository: Arc<dyn ResourceRepository> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryResourceRepository::seeded()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresResourceRepository::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let bbs_link = BbsLinkClient::connect(config.bbs_link_url.clone()).await?;
        Ok(Self {
            config,
            repository,
            bbs_link: Some(bbs_link),
        })
    }
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    pub(crate) async fn search(
        &self,
        mut request: pb::SearchRequest,
    ) -> Result<pb::SearchResponse, DomainError> {
        request.query = request.query.trim().chars().take(200).collect();
        request.topic = request.topic.trim().chars().take(80).collect();
        request.cursor = request.cursor.trim().to_string();
        request.limit = Some(request.limit.unwrap_or(20).clamp(1, 50));
        Ok(self.repository.search(&request).await?)
    }
    pub(crate) async fn get(&self, request: pb::GetRequest) -> Result<pb::Resource, DomainError> {
        let id = request.resource_id.trim();
        if id.is_empty() || id.len() > 160 {
            return Err(DomainError::Validation(
                "resource_id is required".to_string(),
            ));
        }
        Ok(self.repository.get(id).await?)
    }

    pub(crate) async fn list_node_resources(
        &self,
        request: pb::ListNodeResourcesRequest,
    ) -> Result<pb::ListNodeResourcesResponse, DomainError> {
        let route_id = bounded_required("route_id", &request.route_id, 160)?;
        let action_node_id = bounded_required("action_node_id", &request.action_node_id, 160)?;
        self.validate_public_action_node(&route_id, &action_node_id, None)
            .await?;
        Ok(self
            .repository
            .list_node_resources(&route_id, &action_node_id, request.include_archived)
            .await?)
    }

    pub(crate) async fn attach_node_resource(
        &self,
        request: pb::AttachNodeResourceRequest,
    ) -> Result<pb::RouteNodeResourceAttachment, DomainError> {
        let route_id = bounded_required("route_id", &request.route_id, 160)?;
        let action_node_id = bounded_required("action_node_id", &request.action_node_id, 160)?;
        let resource_id = bounded_required("resource_id", &request.resource_id, 160)?;
        let idempotency_key = bounded_required("idempotency_key", &request.idempotency_key, 220)?;
        let created_by = bounded_required("created_by", &request.created_by, 160)?;
        self.validate_public_action_node(&route_id, &action_node_id, Some(&created_by))
            .await?;
        let kind = pb::AttachmentKind::try_from(request.kind)
            .ok()
            .filter(|kind| *kind != pb::AttachmentKind::Unspecified)
            .ok_or_else(|| DomainError::Validation("attachment kind is required".to_string()))?;
        let title_override = bounded_optional(&request.title_override, 200);
        let note = bounded_optional(&request.note, 1_000);
        let retrieval_scope = bounded_optional(&request.retrieval_scope, 240);
        let embedding_collection = if request.rag_enabled || kind == pb::AttachmentKind::RagCorpus {
            format!(
                "route_node:{}:{}",
                stable_token(&route_id),
                stable_token(&action_node_id)
            )
        } else {
            String::new()
        };
        Ok(self
            .repository
            .attach_node_resource(NewNodeResourceAttachment {
                route_id,
                action_node_id,
                resource_id,
                kind,
                title_override,
                note,
                sort_rank: request.sort_rank.clamp(-10_000, 10_000),
                rag_enabled: request.rag_enabled || kind == pb::AttachmentKind::RagCorpus,
                embedding_collection,
                retrieval_scope,
                idempotency_key,
                created_by,
            })
            .await?)
    }

    pub(crate) async fn detach_node_resource(
        &self,
        request: pb::DetachNodeResourceRequest,
    ) -> Result<pb::DetachNodeResourceResponse, DomainError> {
        let route_id = bounded_required("route_id", &request.route_id, 160)?;
        let action_node_id = bounded_required("action_node_id", &request.action_node_id, 160)?;
        let attachment_id = bounded_required("attachment_id", &request.attachment_id, 160)?;
        let operator_id = bounded_required("operator_id", &request.operator_id, 160)?;
        self.validate_public_action_node(&route_id, &action_node_id, Some(&operator_id))
            .await?;
        Ok(pb::DetachNodeResourceResponse {
            detached: self
                .repository
                .detach_node_resource(&route_id, &action_node_id, &attachment_id)
                .await?,
        })
    }

    pub(crate) async fn retrieve_rag_context(
        &self,
        request: pb::RetrieveRagContextRequest,
    ) -> Result<pb::RetrieveRagContextResponse, DomainError> {
        let route_id = bounded_required("route_id", &request.route_id, 160)?;
        let action_node_id = bounded_required("action_node_id", &request.action_node_id, 160)?;
        let question = bounded_required("question", &request.question, 600)?;
        self.validate_public_action_node(&route_id, &action_node_id, None)
            .await?;
        let limit = usize::try_from(request.limit.unwrap_or(6).clamp(1, 12)).unwrap_or(6);
        let mut contexts = self
            .repository
            .list_node_resources(&route_id, &action_node_id, false)
            .await?
            .items
            .into_iter()
            .filter(|attachment| attachment.rag_enabled)
            .filter_map(|attachment| {
                let resource = attachment.resource.as_ref()?;
                let relevance = rag_relevance(&question, &attachment, resource);
                Some(pb::RagContext {
                    excerpt: rag_excerpt(&attachment, resource),
                    attachment: Some(attachment),
                    relevance,
                })
            })
            .collect::<Vec<_>>();
        contexts.sort_by(|left, right| right.relevance.total_cmp(&left.relevance));
        contexts.truncate(limit);
        let mut embedding_collections = contexts
            .iter()
            .filter_map(|context| context.attachment.as_ref())
            .map(|attachment| attachment.embedding_collection.clone())
            .filter(|collection| !collection.is_empty())
            .collect::<Vec<_>>();
        embedding_collections.sort();
        embedding_collections.dedup();
        Ok(pb::RetrieveRagContextResponse {
            contexts,
            embedding_collections,
            retrieval_mode: "attachment_lexical_fallback".to_string(),
        })
    }

    async fn validate_public_action_node(
        &self,
        route_id: &str,
        action_node_id: &str,
        expected_owner_id: Option<&str>,
    ) -> Result<(), DomainError> {
        let Some(client) = &self.bbs_link else {
            return Ok(());
        };
        let mut client = client.clone();
        let route = client
            .get_public(
                bookway_runtime::grpc_service_request(bbs_link::IdRequest {
                    id: route_id.to_string(),
                })
                .map_err(|error| DomainError::Validation(error.to_string()))?,
            )
            .await
            .map_err(|error| match error.code() {
                tonic::Code::NotFound => {
                    DomainError::Validation("public route not found".to_string())
                }
                _ => DomainError::Upstream(error.to_string()),
            })?
            .into_inner();
        validate_route_action_node(&route, action_node_id, expected_owner_id)
    }
}

fn validate_route_action_node(
    route: &bbs_link::Content,
    action_node_id: &str,
    expected_owner_id: Option<&str>,
) -> Result<(), DomainError> {
    if route.content_type != bbs_link::ContentType::Route as i32
        || !route.route_template.as_ref().is_some_and(|template| {
            template
                .actions
                .iter()
                .any(|action| action.id == action_node_id)
        })
    {
        return Err(DomainError::Validation(
            "resource attachment must target an action node on a public route".to_string(),
        ));
    }
    if expected_owner_id.is_some_and(|owner_id| route.author_id != owner_id) {
        return Err(DomainError::Validation(
            "only the route author may attach or detach node resources".to_string(),
        ));
    }
    Ok(())
}

fn bounded_required(name: &str, value: &str, max_chars: usize) -> Result<String, DomainError> {
    let value = bounded_optional(value, max_chars);
    if value.is_empty() {
        return Err(DomainError::Validation(format!("{name} is required")));
    }
    Ok(value)
}

fn bounded_optional(value: &str, max_chars: usize) -> String {
    value.trim().chars().take(max_chars).collect()
}

fn stable_token(value: &str) -> String {
    // Collection names are persisted and used as tenant boundaries by the
    // future vector backend. Encode every byte so distinct route/node IDs
    // cannot collide after sanitization (for example `route/one` and
    // `route_one`).
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(1 + value.len() * 2);
    encoded.push('r');
    for byte in value.as_bytes() {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn rag_relevance(
    question: &str,
    attachment: &pb::RouteNodeResourceAttachment,
    resource: &pb::Resource,
) -> f64 {
    let haystack = format!(
        "{} {} {} {} {}",
        attachment.title_override,
        attachment.note,
        resource.title,
        resource.summary,
        resource.topics.join(" ")
    )
    .to_lowercase();
    let matches = rag_terms(question)
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count();
    matches as f64 + f64::from((-attachment.sort_rank).max(0)) / 10_000.0
}

fn rag_terms(value: &str) -> Vec<String> {
    let mut terms = std::collections::BTreeSet::new();
    let mut ascii_word = String::new();
    let mut cjk_run = Vec::new();
    let flush_ascii = |word: &mut String, terms: &mut std::collections::BTreeSet<String>| {
        if word.len() > 1 {
            terms.insert(std::mem::take(word));
        } else {
            word.clear();
        }
    };
    let flush_cjk = |run: &mut Vec<char>, terms: &mut std::collections::BTreeSet<String>| {
        if run.len() == 1 {
            terms.insert(run[0].to_string());
        } else {
            for pair in run.windows(2) {
                terms.insert(pair.iter().collect());
            }
        }
        run.clear();
    };

    for character in value.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            flush_cjk(&mut cjk_run, &mut terms);
            ascii_word.push(character);
        } else if is_cjk(character) {
            flush_ascii(&mut ascii_word, &mut terms);
            cjk_run.push(character);
        } else {
            flush_ascii(&mut ascii_word, &mut terms);
            flush_cjk(&mut cjk_run, &mut terms);
        }
    }
    flush_ascii(&mut ascii_word, &mut terms);
    flush_cjk(&mut cjk_run, &mut terms);
    terms.into_iter().collect()
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
    )
}

fn rag_excerpt(attachment: &pb::RouteNodeResourceAttachment, resource: &pb::Resource) -> String {
    let value = if attachment.note.trim().is_empty() {
        &resource.summary
    } else {
        &attachment.note
    };
    value.chars().take(600).collect()
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use bookway_bbs_link_api::pb as bbs_link;
    use bookway_knowledge_catalog_api::pb;

    use super::{
        Domain, bounded_required, rag_relevance, stable_token, validate_route_action_node,
    };
    use crate::domain::DomainError;
    use crate::{conf::Config, datasource::MemoryResourceRepository};

    #[test]
    fn required_values_are_trimmed_and_bounded() {
        assert_eq!(
            bounded_required("route_id", "  route-1  ", 20).expect("route id"),
            "route-1"
        );
        assert!(matches!(
            bounded_required("resource_id", "   ", 20),
            Err(DomainError::Validation(message)) if message == "resource_id is required"
        ));
    }

    #[test]
    fn route_node_embedding_collection_is_safe_for_identifiers() {
        assert_eq!(stable_token("route/one"), "r726f7574652f6f6e65");
        assert_eq!(stable_token("node-1_v2"), "r6e6f64652d315f7632");
        assert_ne!(stable_token("route/one"), stable_token("route_one"));
    }

    #[test]
    fn node_resource_mutations_require_the_public_route_author() {
        let route = bbs_link::Content {
            author_id: "route-author".to_string(),
            content_type: bbs_link::ContentType::Route as i32,
            route_template: Some(bbs_link::RouteTemplate {
                actions: vec![bbs_link::RouteTemplateAction {
                    id: "action-1".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        validate_route_action_node(&route, "action-1", Some("route-author"))
            .expect("route author may mutate an attached resource");
        assert!(matches!(
            validate_route_action_node(&route, "action-1", Some("another-user")),
            Err(DomainError::Validation(message))
                if message == "only the route author may attach or detach node resources"
        ));
        assert!(matches!(
            validate_route_action_node(&route, "missing-action", Some("route-author")),
            Err(DomainError::Validation(message))
                if message == "resource attachment must target an action node on a public route"
        ));
    }

    #[test]
    fn rag_lexical_fallback_ranks_cjk_terms_without_whitespace() {
        let attachment = pb::RouteNodeResourceAttachment::default();
        let running = pb::Resource {
            title: "跑步装备清单".to_string(),
            summary: "适合入门跑步训练的鞋服与补给选择。".to_string(),
            ..Default::default()
        };
        let reading = pb::Resource {
            title: "阅读复盘方法".to_string(),
            summary: "建立长期阅读与笔记习惯。".to_string(),
            ..Default::default()
        };

        assert!(
            rag_relevance("跑步装备怎么选", &attachment, &running)
                > rag_relevance("跑步装备怎么选", &attachment, &reading)
        );
    }

    #[tokio::test]
    async fn node_resource_lifecycle_is_idempotent_and_archivable() {
        let domain = Domain {
            config: Config {
                listen_addr: "127.0.0.1:0".parse::<SocketAddr>().expect("socket address"),
                bbs_link_url: "http://127.0.0.1:18004".to_string(),
            },
            repository: Arc::new(MemoryResourceRepository::seeded()),
            bbs_link: None,
        };
        let request = pb::AttachNodeResourceRequest {
            route_id: "route/one".to_string(),
            action_node_id: "node/one".to_string(),
            resource_id: "resource-mdn-web".to_string(),
            kind: pb::AttachmentKind::RagCorpus as i32,
            sort_rank: 10_001,
            idempotency_key: "attach-1".to_string(),
            created_by: "creator-1".to_string(),
            ..Default::default()
        };

        let first = domain
            .attach_node_resource(request.clone())
            .await
            .expect("resource should attach");
        assert!(first.rag_enabled);
        assert_eq!(
            first.embedding_collection,
            "route_node:r726f7574652f6f6e65:r6e6f64652f6f6e65"
        );
        assert_eq!(first.sort_rank, 10_000);

        let retry = domain
            .attach_node_resource(request)
            .await
            .expect("retry should return the existing attachment");
        assert_eq!(retry.id, first.id);

        let conflicting_request = pb::AttachNodeResourceRequest {
            route_id: "route/other".to_string(),
            action_node_id: "node/other".to_string(),
            resource_id: "resource-mdn-web".to_string(),
            kind: pb::AttachmentKind::RagCorpus as i32,
            idempotency_key: "attach-1".to_string(),
            created_by: "creator-1".to_string(),
            ..Default::default()
        };
        let conflict = domain
            .attach_node_resource(conflicting_request.clone())
            .await;
        assert!(matches!(
            conflict,
            Err(DomainError::Repository(
                crate::datasource::RepositoryError::Conflict(_)
            ))
        ));
        let listed = domain
            .list_node_resources(pb::ListNodeResourcesRequest {
                route_id: "route/one".to_string(),
                action_node_id: "node/one".to_string(),
                include_archived: false,
            })
            .await
            .expect("attachment should be listed");
        assert_eq!(listed.items.len(), 1);
        let listed_resource = listed.items[0]
            .resource
            .as_ref()
            .expect("listed attachment should include its resource");
        assert_eq!(listed_resource.id, "resource-mdn-web");

        let rag_context = domain
            .retrieve_rag_context(pb::RetrieveRagContextRequest {
                route_id: "route/one".to_string(),
                action_node_id: "node/one".to_string(),
                question: "Web platform tools".to_string(),
                limit: Some(3),
            })
            .await
            .expect("RAG context should be available for enabled attachments");
        assert_eq!(rag_context.contexts.len(), 1);
        assert_eq!(rag_context.retrieval_mode, "attachment_lexical_fallback");
        assert_eq!(
            rag_context.embedding_collections,
            vec!["route_node:r726f7574652f6f6e65:r6e6f64652f6f6e65"]
        );

        let detached = domain
            .detach_node_resource(pb::DetachNodeResourceRequest {
                route_id: "route/one".to_string(),
                action_node_id: "node/one".to_string(),
                attachment_id: first.id.clone(),
                operator_id: "creator-1".to_string(),
            })
            .await
            .expect("attachment should detach");
        assert!(detached.detached);

        let listed = domain
            .list_node_resources(pb::ListNodeResourcesRequest {
                route_id: "route/one".to_string(),
                action_node_id: "node/one".to_string(),
                include_archived: false,
            })
            .await
            .expect("active attachments should be listed");
        assert!(listed.items.is_empty());
    }
}
