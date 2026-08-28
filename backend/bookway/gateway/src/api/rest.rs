//! Stable JSON shapes for Gateway's mobile-facing REST endpoints.
//!
//! Protobuf keeps enums as integers in generated Rust messages. REST clients
//! use the lower-case names below, so conversion lives at this transport edge
//! instead of leaking generated representation details outside Gateway.

use bookway_account_api::pb as account_pb;
use bookway_bbs_api::pb as bbs_pb;
use bookway_bbs_creator_api::pb as creator_pb;
use bookway_bbs_link_api::pb as bbs_link_pb;
use bookway_bbs_message_api::pb as message_pb;
use bookway_bbs_search_api::pb as search_pb;
use bookway_comment_api::pb as comment_pb;
use bookway_content_audit_api::pb as audit_pb;
use bookway_feedback_api::pb as feedback_pb;
use bookway_growth_api::pb as growth_pb;
use bookway_interaction_status_api::pb as like_pb;
use bookway_knowledge_catalog_api::pb as catalog_pb;
use bookway_mall_api::pb as mall_pb;
use bookway_mall_order_api::pb as mall_order_pb;
use bookway_media_api::pb as media_pb;
use bookway_recommend_main_api::pb as recommend_pb;
use bookway_user_event_api::pb as user_event_pb;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GrowthDomain {
    Learning,
    Movement,
    Wellness,
    Travel,
    Leisure,
}

impl GrowthDomain {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::GrowthDomain::try_from(value).ok() {
            Some(growth_pb::GrowthDomain::Learning) => Ok(Self::Learning),
            Some(growth_pb::GrowthDomain::Movement) => Ok(Self::Movement),
            Some(growth_pb::GrowthDomain::Wellness) => Ok(Self::Wellness),
            Some(growth_pb::GrowthDomain::Travel) => Ok(Self::Travel),
            Some(growth_pb::GrowthDomain::Leisure) => Ok(Self::Leisure),
            None => Err(format!("growth returned unknown domain {value}")),
        }
    }

    fn from_link(value: i32) -> Result<Self, String> {
        match bbs_link_pb::GrowthDomain::try_from(value).ok() {
            Some(bbs_link_pb::GrowthDomain::Learning) => Ok(Self::Learning),
            Some(bbs_link_pb::GrowthDomain::Movement) => Ok(Self::Movement),
            Some(bbs_link_pb::GrowthDomain::Wellness) => Ok(Self::Wellness),
            Some(bbs_link_pb::GrowthDomain::Travel) => Ok(Self::Travel),
            Some(bbs_link_pb::GrowthDomain::Leisure) => Ok(Self::Leisure),
            None => Err(format!("bbs-link returned unknown growth domain {value}")),
        }
    }

    fn from_search(value: i32) -> Result<Self, String> {
        match search_pb::GrowthDomain::try_from(value).ok() {
            Some(search_pb::GrowthDomain::Learning) => Ok(Self::Learning),
            Some(search_pb::GrowthDomain::Movement) => Ok(Self::Movement),
            Some(search_pb::GrowthDomain::Wellness) => Ok(Self::Wellness),
            Some(search_pb::GrowthDomain::Travel) => Ok(Self::Travel),
            Some(search_pb::GrowthDomain::Leisure) => Ok(Self::Leisure),
            Some(search_pb::GrowthDomain::Unspecified) | None => {
                Err(format!("bbs-search returned unknown growth domain {value}"))
            }
        }
    }

    pub(crate) fn into_link(self) -> i32 {
        match self {
            Self::Learning => bbs_link_pb::GrowthDomain::Learning as i32,
            Self::Movement => bbs_link_pb::GrowthDomain::Movement as i32,
            Self::Wellness => bbs_link_pb::GrowthDomain::Wellness as i32,
            Self::Travel => bbs_link_pb::GrowthDomain::Travel as i32,
            Self::Leisure => bbs_link_pb::GrowthDomain::Leisure as i32,
        }
    }

    pub(crate) fn into_growth(self) -> i32 {
        match self {
            Self::Learning => growth_pb::GrowthDomain::Learning as i32,
            Self::Movement => growth_pb::GrowthDomain::Movement as i32,
            Self::Wellness => growth_pb::GrowthDomain::Wellness as i32,
            Self::Travel => growth_pb::GrowthDomain::Travel as i32,
            Self::Leisure => growth_pb::GrowthDomain::Leisure as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContentType {
    Note,
    Article,
    Video,
    Route,
    Milestone,
    Question,
}

impl ContentType {
    fn from_link(value: i32) -> Result<Self, String> {
        match bbs_link_pb::ContentType::try_from(value).ok() {
            Some(bbs_link_pb::ContentType::Note) => Ok(Self::Note),
            Some(bbs_link_pb::ContentType::Article) => Ok(Self::Article),
            Some(bbs_link_pb::ContentType::Video) => Ok(Self::Video),
            Some(bbs_link_pb::ContentType::Route) => Ok(Self::Route),
            Some(bbs_link_pb::ContentType::Milestone) => Ok(Self::Milestone),
            Some(bbs_link_pb::ContentType::Question) => Ok(Self::Question),
            None => Err(format!("bbs-link returned unknown content type {value}")),
        }
    }

    pub(crate) fn into_link(self) -> i32 {
        match self {
            Self::Note => bbs_link_pb::ContentType::Note as i32,
            Self::Article => bbs_link_pb::ContentType::Article as i32,
            Self::Video => bbs_link_pb::ContentType::Video as i32,
            Self::Route => bbs_link_pb::ContentType::Route as i32,
            Self::Milestone => bbs_link_pb::ContentType::Milestone as i32,
            Self::Question => bbs_link_pb::ContentType::Question as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContentStatus {
    Draft,
    Reviewing,
    Published,
    Restricted,
    Deleted,
}

impl ContentStatus {
    fn from_link(value: i32) -> Result<Self, String> {
        match bbs_link_pb::ContentStatus::try_from(value).ok() {
            Some(bbs_link_pb::ContentStatus::Draft) => Ok(Self::Draft),
            Some(bbs_link_pb::ContentStatus::Reviewing) => Ok(Self::Reviewing),
            Some(bbs_link_pb::ContentStatus::Published) => Ok(Self::Published),
            Some(bbs_link_pb::ContentStatus::Restricted) => Ok(Self::Restricted),
            Some(bbs_link_pb::ContentStatus::Deleted) => Ok(Self::Deleted),
            None => Err(format!("bbs-link returned unknown content status {value}")),
        }
    }

    pub(crate) fn into_link(self) -> i32 {
        match self {
            Self::Draft => bbs_link_pb::ContentStatus::Draft as i32,
            Self::Reviewing => bbs_link_pb::ContentStatus::Reviewing as i32,
            Self::Published => bbs_link_pb::ContentStatus::Published as i32,
            Self::Restricted => bbs_link_pb::ContentStatus::Restricted as i32,
            Self::Deleted => bbs_link_pb::ContentStatus::Deleted as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteTemplateKind {
    Project,
    Habit,
    Quantity,
    Travel,
    Challenge,
}

impl RouteTemplateKind {
    fn from_link(value: i32) -> Result<Self, String> {
        match bbs_link_pb::RouteTemplateKind::try_from(value).ok() {
            Some(bbs_link_pb::RouteTemplateKind::Project) => Ok(Self::Project),
            Some(bbs_link_pb::RouteTemplateKind::Habit) => Ok(Self::Habit),
            Some(bbs_link_pb::RouteTemplateKind::Quantity) => Ok(Self::Quantity),
            Some(bbs_link_pb::RouteTemplateKind::Travel) => Ok(Self::Travel),
            Some(bbs_link_pb::RouteTemplateKind::Challenge) => Ok(Self::Challenge),
            None => Err(format!(
                "bbs-link returned unknown route template kind {value}"
            )),
        }
    }

    fn into_link(self) -> i32 {
        match self {
            Self::Project => bbs_link_pb::RouteTemplateKind::Project as i32,
            Self::Habit => bbs_link_pb::RouteTemplateKind::Habit as i32,
            Self::Quantity => bbs_link_pb::RouteTemplateKind::Quantity as i32,
            Self::Travel => bbs_link_pb::RouteTemplateKind::Travel as i32,
            Self::Challenge => bbs_link_pb::RouteTemplateKind::Challenge as i32,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct RouteTemplateStage {
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) completion_criteria: String,
}

impl From<RouteTemplateStage> for bbs_link_pb::RouteTemplateStage {
    fn from(value: RouteTemplateStage) -> Self {
        Self {
            title: value.title,
            detail: value.detail,
            completion_criteria: value.completion_criteria,
        }
    }
}

impl From<bbs_link_pb::RouteTemplateStage> for RouteTemplateStage {
    fn from(value: bbs_link_pb::RouteTemplateStage) -> Self {
        Self {
            title: value.title,
            detail: value.detail,
            completion_criteria: value.completion_criteria,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct RouteTemplateAction {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) estimated_minutes: u32,
    pub(crate) scheduled_label: String,
    pub(crate) stage_index: Option<u32>,
    #[serde(default)]
    pub(crate) scene_equipment: Vec<String>,
}

impl From<RouteTemplateAction> for bbs_link_pb::RouteTemplateAction {
    fn from(value: RouteTemplateAction) -> Self {
        Self {
            id: value.id,
            title: value.title,
            detail: value.detail,
            estimated_minutes: value.estimated_minutes,
            scheduled_label: value.scheduled_label,
            stage_index: value.stage_index,
            scene_equipment: value.scene_equipment,
        }
    }
}

impl From<bbs_link_pb::RouteTemplateAction> for RouteTemplateAction {
    fn from(value: bbs_link_pb::RouteTemplateAction) -> Self {
        Self {
            id: value.id,
            title: value.title,
            detail: value.detail,
            estimated_minutes: value.estimated_minutes,
            scheduled_label: value.scheduled_label,
            stage_index: value.stage_index,
            scene_equipment: value.scene_equipment,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct RouteTemplate {
    pub(crate) intent: String,
    pub(crate) completion_criteria: String,
    pub(crate) stages: Vec<RouteTemplateStage>,
    pub(crate) actions: Vec<RouteTemplateAction>,
    pub(crate) journey_type: RouteTemplateKind,
}

impl From<RouteTemplate> for bbs_link_pb::RouteTemplate {
    fn from(value: RouteTemplate) -> Self {
        Self {
            intent: value.intent,
            completion_criteria: value.completion_criteria,
            stages: value.stages.into_iter().map(Into::into).collect(),
            actions: value.actions.into_iter().map(Into::into).collect(),
            journey_type: value.journey_type.into_link(),
        }
    }
}

impl TryFrom<bbs_link_pb::RouteTemplate> for RouteTemplate {
    type Error = String;

    fn try_from(value: bbs_link_pb::RouteTemplate) -> Result<Self, Self::Error> {
        Ok(Self {
            intent: value.intent,
            completion_criteria: value.completion_criteria,
            stages: value.stages.into_iter().map(Into::into).collect(),
            actions: value.actions.into_iter().map(Into::into).collect(),
            journey_type: RouteTemplateKind::from_link(value.journey_type)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct MilestoneDraft {
    pub(crate) route_id: String,
    pub(crate) stage_index: Option<u32>,
    pub(crate) effort_summary: String,
    pub(crate) outcome_summary: String,
    pub(crate) adjustment_summary: String,
    pub(crate) evidence_scope: String,
}

impl From<MilestoneDraft> for bbs_link_pb::MilestoneDraft {
    fn from(value: MilestoneDraft) -> Self {
        Self {
            route_id: value.route_id,
            stage_index: value.stage_index,
            effort_summary: value.effort_summary,
            outcome_summary: value.outcome_summary,
            adjustment_summary: value.adjustment_summary,
            evidence_scope: value.evidence_scope,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct Milestone {
    route_id: String,
    route_title: String,
    stage_index: u32,
    stage_title: String,
    effort_summary: String,
    outcome_summary: String,
    adjustment_summary: String,
    evidence_scope: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct QuestionContextDraft {
    pub(crate) route_id: String,
    pub(crate) stage_index: Option<u32>,
}

impl From<QuestionContextDraft> for bbs_link_pb::QuestionContextDraft {
    fn from(value: QuestionContextDraft) -> Self {
        Self {
            route_id: value.route_id,
            stage_index: value.stage_index,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct QuestionContext {
    route_id: String,
    route_title: String,
    stage_index: Option<u32>,
    stage_title: Option<String>,
}

impl From<bbs_link_pb::QuestionContext> for QuestionContext {
    fn from(value: bbs_link_pb::QuestionContext) -> Self {
        Self {
            route_id: value.route_id,
            route_title: value.route_title,
            stage_index: value.stage_index,
            stage_title: value.stage_title,
        }
    }
}

/// Immutable server-owned provenance for a route created from a public fork.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct RouteFork {
    source_route_id: String,
    source_route_version: u32,
    source_route_title: String,
    forked_at: String,
}

impl From<bbs_link_pb::RouteFork> for RouteFork {
    fn from(value: bbs_link_pb::RouteFork) -> Self {
        Self {
            source_route_id: value.source_route_id,
            source_route_version: value.source_route_version,
            source_route_title: value.source_route_title,
            forked_at: value.forked_at,
        }
    }
}

impl From<bbs_link_pb::Milestone> for Milestone {
    fn from(value: bbs_link_pb::Milestone) -> Self {
        Self {
            route_id: value.route_id,
            route_title: value.route_title,
            stage_index: value.stage_index,
            stage_title: value.stage_title,
            effort_summary: value.effort_summary,
            outcome_summary: value.outcome_summary,
            adjustment_summary: value.adjustment_summary,
            evidence_scope: value.evidence_scope,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateContentRequest {
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) body: String,
    pub(crate) domain: GrowthDomain,
    pub(crate) content_type: ContentType,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) topics: Vec<String>,
    pub(crate) route_title: Option<String>,
    pub(crate) route_duration: Option<String>,
    #[serde(default)]
    pub(crate) media_asset_ids: Vec<String>,
    pub(crate) route_template: Option<RouteTemplate>,
    pub(crate) milestone: Option<MilestoneDraft>,
    pub(crate) question_context: Option<QuestionContextDraft>,
}

impl CreateContentRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        idempotency_key: Option<String>,
    ) -> bbs_link_pb::CreateRequest {
        bbs_link_pb::CreateRequest {
            user_id,
            idempotency_key,
            title: self.title,
            summary: self.summary,
            body: self.body,
            domain: self.domain.into_link(),
            content_type: self.content_type.into_link(),
            tags: self.tags,
            topics: self.topics,
            route_title: self.route_title,
            route_duration: self.route_duration,
            media_asset_ids: self.media_asset_ids,
            route_template: self.route_template.map(Into::into),
            milestone: self.milestone.map(Into::into),
            question_context: self.question_context.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateContentRequest {
    pub(crate) title: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) topics: Option<Vec<String>>,
    pub(crate) media_asset_ids: Option<Vec<String>>,
    pub(crate) route_template: Option<RouteTemplate>,
    pub(crate) milestone: Option<MilestoneDraft>,
}

impl UpdateContentRequest {
    pub(crate) fn into_pb(self, user_id: String, id: String) -> bbs_link_pb::UpdateRequest {
        bbs_link_pb::UpdateRequest {
            user_id,
            id,
            title: self.title,
            summary: self.summary,
            body: self.body,
            tags: self.tags.map(|values| bbs_link_pb::StringList { values }),
            topics: self.topics.map(|values| bbs_link_pb::StringList { values }),
            media_asset_ids: self
                .media_asset_ids
                .map(|values| bbs_link_pb::StringList { values }),
            route_template: self.route_template.map(Into::into),
            milestone: self.milestone.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ForkRouteRequest {
    pub(crate) title: Option<String>,
    pub(crate) summary: Option<String>,
}

impl ForkRouteRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        source_route_id: String,
        idempotency_key: String,
    ) -> bbs_link_pb::ForkRouteRequest {
        bbs_link_pb::ForkRouteRequest {
            user_id,
            source_route_id,
            idempotency_key,
            title: self.title,
            summary: self.summary,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct ContentMedia {
    id: String,
    url: String,
    kind: String,
    width: u32,
    height: u32,
    duration_ms: Option<u64>,
}

impl From<bbs_link_pb::ContentMedia> for ContentMedia {
    fn from(value: bbs_link_pb::ContentMedia) -> Self {
        Self {
            id: value.id,
            url: value.url,
            kind: value.kind,
            width: value.width,
            height: value.height,
            duration_ms: value.duration_ms,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct PostSummary {
    id: String,
    author_name: String,
    author_avatar_url: String,
    title: String,
    summary: String,
    domain: GrowthDomain,
    cover_url: String,
    route_title: String,
    route_duration: String,
    join_count: u32,
    like_count: u32,
    freshness: f64,
    tags: Vec<String>,
    is_route: bool,
    is_milestone: bool,
    is_question: bool,
}

impl PostSummary {
    fn from_link(value: bbs_link_pb::PostSummary) -> Result<Self, String> {
        Ok(Self {
            id: value.id,
            author_name: value.author_name,
            author_avatar_url: value.author_avatar_url,
            title: value.title,
            summary: value.summary,
            domain: GrowthDomain::from_link(value.domain)?,
            cover_url: value.cover_url,
            route_title: value.route_title,
            route_duration: value.route_duration,
            join_count: value.join_count,
            like_count: value.like_count,
            freshness: value.freshness,
            tags: value.tags,
            is_route: value.is_route,
            is_milestone: value.is_milestone,
            is_question: value.is_question,
        })
    }

    fn from_search(value: search_pb::PostSummary) -> Result<Self, String> {
        Ok(Self {
            id: value.id,
            author_name: value.author_name,
            author_avatar_url: value.author_avatar_url,
            title: value.title,
            summary: value.summary,
            domain: GrowthDomain::from_search(value.domain)?,
            cover_url: value.cover_url,
            route_title: value.route_title,
            route_duration: value.route_duration,
            join_count: value.join_count,
            like_count: value.like_count,
            freshness: value.freshness,
            tags: value.tags,
            is_route: value.is_route,
            is_milestone: value.is_milestone,
            is_question: value.is_question,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Content {
    id: String,
    post: Option<PostSummary>,
    author_id: String,
    content_type: ContentType,
    status: ContentStatus,
    body: String,
    media: Vec<ContentMedia>,
    topics: Vec<String>,
    created_at: String,
    published_at: Option<String>,
    version: u32,
    quality_score: f64,
    route_template: Option<RouteTemplate>,
    milestone: Option<Milestone>,
    accepted_answer_id: Option<String>,
    question_context: Option<QuestionContext>,
    route_fork: Option<RouteFork>,
}

impl TryFrom<bbs_link_pb::Content> for Content {
    type Error = String;

    fn try_from(value: bbs_link_pb::Content) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            post: value.post.map(PostSummary::from_link).transpose()?,
            author_id: value.author_id,
            content_type: ContentType::from_link(value.content_type)?,
            status: ContentStatus::from_link(value.status)?,
            body: value.body,
            media: value.media.into_iter().map(Into::into).collect(),
            topics: value.topics,
            created_at: value.created_at,
            published_at: value.published_at,
            version: value.version,
            quality_score: value.quality_score,
            route_template: value.route_template.map(TryInto::try_into).transpose()?,
            milestone: value.milestone.map(Into::into),
            accepted_answer_id: value.accepted_answer_id,
            question_context: value.question_context.map(Into::into),
            route_fork: value.route_fork.map(Into::into),
        })
    }
}

/// Merchant-facing order projection for `/v1/admin/mall/orders`. The buyer's
/// platform identity (`user_id`) and their payment reference are intentionally
/// absent: fulfilment needs neither, and minimal disclosure keeps buyer
/// accounts out of merchant reach.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct MerchantOrderView {
    pub(crate) id: String,
    pub(crate) status: i32,
    pub(crate) currency: String,
    pub(crate) total_cents: i64,
    pub(crate) items: Vec<MerchantOrderItemView>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) node_offer_id: String,
    pub(crate) affiliate_creator_id: String,
    pub(crate) commission_cents: i64,
    pub(crate) fulfillment_status: i32,
    pub(crate) tracking_number: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct MerchantOrderItemView {
    pub(crate) title: String,
    pub(crate) quantity: u32,
    pub(crate) line_total_cents: i64,
}

impl TryFrom<mall_order_pb::Order> for MerchantOrderView {
    type Error = String;

    fn try_from(value: mall_order_pb::Order) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            status: value.status,
            currency: value.currency,
            total_cents: value.total_cents,
            items: value
                .items
                .into_iter()
                .map(|item| MerchantOrderItemView {
                    title: item.title,
                    quantity: item.quantity,
                    line_total_cents: item.line_total_cents,
                })
                .collect(),
            created_at: value.created_at,
            updated_at: value.updated_at,
            node_offer_id: value.node_offer_id,
            affiliate_creator_id: value.affiliate_creator_id,
            commission_cents: value.commission_cents,
            fulfillment_status: value.fulfillment_status,
            tracking_number: value.tracking_number,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct MerchantOrderPage {
    pub(crate) items: Vec<MerchantOrderView>,
    pub(crate) next_cursor: Option<String>,
}

impl TryFrom<mall_order_pb::MerchantOrderListResponse> for MerchantOrderPage {
    type Error = String;

    fn try_from(value: mall_order_pb::MerchantOrderListResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(MerchantOrderView::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

/// Public storefront projection of a contextual node offer. `commission_bps`
/// is the creator's commercial rate and `merchant_id` the internal shop
/// identity — both exist for checkout snapshots and settlement only, so the
/// unauthenticated route surface never prices the relationship. The admin
/// surface keeps the full internal message.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct PublicNodeOffer {
    pub(crate) id: String,
    pub(crate) product_id: String,
    pub(crate) sku_id: String,
    pub(crate) route_id: String,
    pub(crate) action_node_id: String,
    pub(crate) scene_equipment: String,
    pub(crate) created_at: String,
    pub(crate) product: Option<PublicNodeOfferProduct>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct PublicNodeOfferProduct {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) image_url: String,
    pub(crate) product_kind: i32,
    pub(crate) course_resource_id: String,
    pub(crate) skus: Vec<PublicNodeOfferSku>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct PublicNodeOfferSku {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) price_cents: i64,
    pub(crate) currency: String,
    pub(crate) attributes: std::collections::HashMap<String, String>,
    pub(crate) saleable: bool,
}

impl TryFrom<mall_pb::NodeOffer> for PublicNodeOffer {
    type Error = String;

    fn try_from(value: mall_pb::NodeOffer) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            product_id: value.product_id,
            sku_id: value.sku_id,
            route_id: value.route_id,
            action_node_id: value.action_node_id,
            scene_equipment: value.scene_equipment,
            created_at: value.created_at,
            product: value.product.map(|product| PublicNodeOfferProduct {
                id: product.id,
                title: product.title,
                description: product.description,
                image_url: product.image_url,
                product_kind: product.product_kind,
                course_resource_id: product.course_resource_id,
                skus: product
                    .skus
                    .into_iter()
                    .map(|sku| PublicNodeOfferSku {
                        id: sku.id,
                        title: sku.title,
                        price_cents: sku.price_cents,
                        currency: sku.currency,
                        attributes: sku.attributes,
                        saleable: sku.saleable,
                    })
                    .collect(),
            }),
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct PublicNodeOfferList {
    pub(crate) items: Vec<PublicNodeOffer>,
}

impl TryFrom<mall_pb::NodeOfferList> for PublicNodeOfferList {
    type Error = String;

    fn try_from(value: mall_pb::NodeOfferList) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(PublicNodeOffer::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct ContentPage {
    items: Vec<Content>,
    next_cursor: Option<String>,
    total_estimate: u64,
}

impl TryFrom<bbs_link_pb::ContentPage> for ContentPage {
    type Error = String;

    fn try_from(value: bbs_link_pb::ContentPage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
            total_estimate: value.total_estimate,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FeedQuery {
    pub(crate) interests: Option<String>,
    pub(crate) seen: Option<String>,
    pub(crate) limit: Option<u32>,
    pub(crate) session_id: Option<String>,
    pub(crate) surface: Option<String>,
    pub(crate) cursor: Option<String>,
    pub(crate) route_id: Option<String>,
    pub(crate) action_node_id: Option<String>,
    pub(crate) placement: Option<String>,
    pub(crate) action_domain: Option<String>,
    pub(crate) scene_equipment: Option<String>,
}

impl FeedQuery {
    pub(crate) fn into_pb(self, user_id: String) -> Result<recommend_pb::FeedRequest, String> {
        let surface = self.surface.unwrap_or_else(|| "home".to_string());
        let action_context = match (self.route_id, self.action_node_id) {
            (Some(route_id), Some(action_node_id))
                if !route_id.trim().is_empty() && !action_node_id.trim().is_empty() =>
            {
                Some(recommend_pb::FeedActionContext {
                    route_id,
                    action_node_id,
                    placement: self.placement.unwrap_or_else(|| surface.clone()),
                    domain: self.action_domain,
                    scene_equipment: self.scene_equipment,
                })
            }
            (None, None) => None,
            _ => {
                return Err("route_id and action_node_id must be provided together".to_string());
            }
        };
        Ok(recommend_pb::FeedRequest {
            user_id,
            interests: parse_domains(self.interests)?
                .into_iter()
                .map(GrowthDomain::into_link)
                .collect(),
            seen: parse_csv(self.seen),
            limit: self.limit,
            session_id: self.session_id.unwrap_or_default(),
            surface,
            cursor: self.cursor,
            action_context,
            // Delivery context is stamped by the handlers from edge headers.
            geo_region: String::new(),
            device_os: String::new(),
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct FeedItem {
    author_id: String,
    post: Option<PostSummary>,
    ad: Option<FeedAd>,
    score: f64,
    source: String,
    reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct FeedAd {
    request_id: String,
    campaign_id: String,
    placement: String,
    title: String,
    body: String,
    image_url: String,
    landing_url: String,
    ecpm: f64,
    model_version: String,
    route_id: String,
    action_node_id: String,
    scene_equipment: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct FeedMeta {
    sourced: u32,
    filtered: u32,
    selected: u32,
    next_cursor: Option<String>,
    pipeline_id: String,
    degraded: bool,
    model_version: Option<String>,
    experiment_bucket: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct FeedResponse {
    request_id: String,
    items: Vec<FeedItem>,
    meta: Option<FeedMeta>,
}

impl TryFrom<recommend_pb::FeedResponse> for FeedResponse {
    type Error = String;

    fn try_from(value: recommend_pb::FeedResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            request_id: value.request_id,
            items: value
                .items
                .into_iter()
                .map(|item| {
                    let post = item.post.map(PostSummary::from_link).transpose()?;
                    let ad = item.ad.map(|ad| FeedAd {
                        request_id: ad.request_id,
                        campaign_id: ad.campaign_id,
                        placement: ad.placement,
                        title: ad.title,
                        body: ad.body,
                        image_url: ad.image_url,
                        landing_url: ad.landing_url,
                        ecpm: ad.ecpm,
                        model_version: ad.model_version,
                        route_id: ad.route_id,
                        action_node_id: ad.action_node_id,
                        scene_equipment: ad.scene_equipment,
                    });
                    if post.is_some() == ad.is_some() {
                        return Err(
                            "feed item must contain exactly one post or contextual ad".to_string()
                        );
                    }
                    Ok(FeedItem {
                        author_id: item.author_id,
                        post,
                        ad,
                        score: item.score,
                        source: item.source,
                        reasons: item.reasons,
                    })
                })
                .collect::<Result<_, String>>()?,
            meta: value.meta.map(|meta| FeedMeta {
                sourced: meta.sourced,
                filtered: meta.filtered,
                selected: meta.selected,
                next_cursor: meta.next_cursor,
                pipeline_id: meta.pipeline_id,
                degraded: meta.degraded,
                model_version: meta.model_version,
                experiment_bucket: meta.experiment_bucket,
            }),
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SearchType {
    All,
    Posts,
    Journeys,
    Users,
    Topics,
    Resources,
    Nodes,
    Equipment,
}

impl SearchType {
    fn into_pb(self) -> i32 {
        match self {
            Self::All => search_pb::SearchType::All as i32,
            Self::Posts => search_pb::SearchType::Posts as i32,
            Self::Journeys => search_pb::SearchType::Journeys as i32,
            Self::Users => search_pb::SearchType::Users as i32,
            Self::Topics => search_pb::SearchType::Topics as i32,
            Self::Resources => search_pb::SearchType::Resources as i32,
            Self::Nodes => search_pb::SearchType::Nodes as i32,
            Self::Equipment => search_pb::SearchType::Equipment as i32,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SearchQuery {
    pub(crate) q: String,
    pub(crate) search_type: SearchType,
    pub(crate) cursor: Option<String>,
    pub(crate) limit: Option<u32>,
    pub(crate) session_id: Option<String>,
    pub(crate) route_id: Option<String>,
    pub(crate) action_node_id: Option<String>,
    pub(crate) scene_equipment: Option<String>,
    pub(crate) ad_placement: Option<String>,
}

impl SearchQuery {
    pub(crate) fn into_pb(self, user_id: String) -> search_pb::SearchRequest {
        search_pb::SearchRequest {
            q: self.q,
            search_type: self.search_type.into_pb(),
            cursor: self.cursor,
            limit: self.limit,
            user_id: Some(user_id),
            excluded_author_ids: Vec::new(),
            session_id: self.session_id,
            route_id: self.route_id,
            action_node_id: self.action_node_id,
            scene_equipment: self.scene_equipment,
            ad_placement: self.ad_placement,
            // Delivery context is stamped by the handlers from edge headers.
            geo_region: None,
            device_os: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SearchResultType {
    Post,
    Journey,
    User,
    Topic,
    Resource,
    Ad,
    ActionNode,
    SceneEquipment,
}

impl SearchResultType {
    fn from_pb(value: i32) -> Result<Self, String> {
        match search_pb::SearchResultType::try_from(value).ok() {
            Some(search_pb::SearchResultType::Post) => Ok(Self::Post),
            Some(search_pb::SearchResultType::Journey) => Ok(Self::Journey),
            Some(search_pb::SearchResultType::User) => Ok(Self::User),
            Some(search_pb::SearchResultType::Topic) => Ok(Self::Topic),
            Some(search_pb::SearchResultType::Resource) => Ok(Self::Resource),
            Some(search_pb::SearchResultType::Ad) => Ok(Self::Ad),
            Some(search_pb::SearchResultType::ActionNode) => Ok(Self::ActionNode),
            Some(search_pb::SearchResultType::SceneEquipment) => Ok(Self::SceneEquipment),
            None => Err(format!("bbs-search returned unknown result type {value}")),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SearchResult {
    id: String,
    result_type: SearchResultType,
    title: String,
    snippet: String,
    cover_url: Option<String>,
    author_id: Option<String>,
    author_name: Option<String>,
    domain: Option<GrowthDomain>,
    score: f64,
    highlights: Vec<String>,
    post: Option<PostSummary>,
    resource: Option<SearchResourceSummary>,
    ad: Option<SearchAd>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SearchAd {
    request_id: String,
    campaign_id: String,
    placement: String,
    title: String,
    body: String,
    image_url: String,
    landing_url: String,
    ecpm: f64,
    model_version: String,
    route_id: String,
    action_node_id: String,
    scene_equipment: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SearchResourceSummary {
    id: String,
    kind: String,
    provider: String,
    url: String,
    license: String,
    version: String,
    citation: String,
    topics: Vec<String>,
    published_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SearchResponse {
    query: String,
    items: Vec<SearchResult>,
    creator_profiles: Vec<CreatorProfile>,
    next_cursor: Option<String>,
    total_estimate: u64,
    took_ms: u64,
    degraded: bool,
    request_id: String,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub(crate) struct ResourceSearchQuery {
    pub(crate) query: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) topic: Option<String>,
    pub(crate) cursor: Option<String>,
    pub(crate) limit: Option<u32>,
}

impl ResourceSearchQuery {
    pub(crate) fn into_pb(self) -> Result<catalog_pb::SearchRequest, String> {
        let kind = self
            .kind
            .map(|value| match value.trim() {
                "book" => Ok(catalog_pb::ResourceKind::Book as i32),
                "course" => Ok(catalog_pb::ResourceKind::Course as i32),
                "tool" => Ok(catalog_pb::ResourceKind::Tool as i32),
                "article" => Ok(catalog_pb::ResourceKind::Article as i32),
                "podcast" => Ok(catalog_pb::ResourceKind::Podcast as i32),
                _ => Err("kind must be book, course, tool, article or podcast".to_string()),
            })
            .transpose()?;
        Ok(catalog_pb::SearchRequest {
            query: self.query.unwrap_or_default(),
            kind,
            topic: self.topic.unwrap_or_default(),
            cursor: self.cursor.unwrap_or_default(),
            limit: self.limit,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct PublicResource {
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
    published_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct PublicResourcePage {
    items: Vec<PublicResource>,
    next_cursor: Option<String>,
}

impl TryFrom<catalog_pb::Resource> for PublicResource {
    type Error = String;

    fn try_from(value: catalog_pb::Resource) -> Result<Self, Self::Error> {
        let kind = match catalog_pb::ResourceKind::try_from(value.kind).ok() {
            Some(catalog_pb::ResourceKind::Book) => "book",
            Some(catalog_pb::ResourceKind::Course) => "course",
            Some(catalog_pb::ResourceKind::Tool) => "tool",
            Some(catalog_pb::ResourceKind::Article) => "article",
            Some(catalog_pb::ResourceKind::Podcast) => "podcast",
            _ => {
                return Err(format!(
                    "catalog returned unknown resource kind {}",
                    value.kind
                ));
            }
        };
        if value.url.trim().is_empty()
            || value.license.trim().is_empty()
            || value.citation.trim().is_empty()
        {
            return Err("catalog returned incomplete public resource metadata".to_string());
        }
        Ok(Self {
            id: value.id,
            title: value.title,
            kind: kind.to_string(),
            provider: value.provider,
            summary: value.summary,
            url: value.url,
            license: value.license,
            version: value.version,
            citation: value.citation,
            topics: value.topics,
            published_at: value.published_at,
            updated_at: value.updated_at,
        })
    }
}

impl TryFrom<catalog_pb::SearchResponse> for PublicResourcePage {
    type Error = String;

    fn try_from(value: catalog_pb::SearchResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

/// Admin catalog write. An absent `resource_id` (PATCH path fills it from the
/// URL) creates a new entry; the catalog treats a duplicate canonical URL as
/// an update of the entry that already owns it.
#[derive(Clone, Debug, Deserialize, Default)]
pub(crate) struct UpsertResourceRequest {
    pub(crate) title: String,
    pub(crate) kind: String,
    pub(crate) provider: String,
    #[serde(default)]
    pub(crate) summary: String,
    pub(crate) url: String,
    pub(crate) license: String,
    pub(crate) version: String,
    pub(crate) citation: String,
    #[serde(default)]
    pub(crate) topics: Vec<String>,
    pub(crate) status: Option<String>,
}

impl UpsertResourceRequest {
    pub(crate) fn into_pb(
        self,
        resource_id: String,
        operator_id: String,
    ) -> Result<catalog_pb::UpsertPublicResourceRequest, String> {
        let kind = match self.kind.trim() {
            "book" => catalog_pb::ResourceKind::Book,
            "course" => catalog_pb::ResourceKind::Course,
            "tool" => catalog_pb::ResourceKind::Tool,
            "article" => catalog_pb::ResourceKind::Article,
            "podcast" => catalog_pb::ResourceKind::Podcast,
            _ => {
                return Err(
                    "kind must be book, course, tool, article or podcast".to_string(),
                );
            }
        };
        let status = match self.status.as_deref().map(str::trim) {
            None | Some("") => catalog_pb::ResourceStatus::Unspecified,
            Some("published") => catalog_pb::ResourceStatus::Published,
            Some("archived") => catalog_pb::ResourceStatus::Archived,
            Some(other) => {
                return Err(format!("status must be published or archived, got {other}"));
            }
        };
        Ok(catalog_pb::UpsertPublicResourceRequest {
            resource_id,
            title: self.title,
            kind: kind as i32,
            provider: self.provider,
            summary: self.summary,
            url: self.url,
            license: self.license,
            version: self.version,
            citation: self.citation,
            topics: self.topics,
            status: status as i32,
            operator_id,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
pub(crate) struct RouteNodeResourceQuery {
    pub(crate) include_archived: bool,
    pub(crate) scene_equipment: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AttachRouteNodeResourceRequest {
    pub(crate) resource_id: String,
    pub(crate) kind: String,
    pub(crate) scene_equipment: String,
    #[serde(default)]
    pub(crate) title_override: String,
    #[serde(default)]
    pub(crate) note: String,
    #[serde(default)]
    pub(crate) sort_rank: i32,
    #[serde(default)]
    pub(crate) rag_enabled: bool,
    #[serde(default)]
    pub(crate) retrieval_scope: String,
}

impl AttachRouteNodeResourceRequest {
    pub(crate) fn into_pb(
        self,
        route_id: String,
        action_node_id: String,
        created_by: String,
        idempotency_key: Option<String>,
    ) -> Result<catalog_pb::AttachNodeResourceRequest, String> {
        let kind = match self.kind.trim() {
            "document" => catalog_pb::AttachmentKind::Document,
            "pdf" => catalog_pb::AttachmentKind::Pdf,
            "external_link" => catalog_pb::AttachmentKind::ExternalLink,
            "tool_checklist" => catalog_pb::AttachmentKind::ToolChecklist,
            "ai_action_guide" => catalog_pb::AttachmentKind::AiActionGuide,
            "rag_corpus" => catalog_pb::AttachmentKind::RagCorpus,
            "resource_package" => catalog_pb::AttachmentKind::ResourcePackage,
            _ => {
                return Err(
                    "kind must be document, pdf, external_link, tool_checklist, ai_action_guide, resource_package or rag_corpus"
                        .to_string(),
                );
            }
        };
        let idempotency_key = idempotency_key
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Idempotency-Key header is required".to_string())?;
        Ok(catalog_pb::AttachNodeResourceRequest {
            route_id,
            action_node_id,
            resource_id: self.resource_id,
            kind: kind as i32,
            scene_equipment: self.scene_equipment,
            title_override: self.title_override,
            note: self.note,
            sort_rank: self.sort_rank,
            rag_enabled: self.rag_enabled,
            retrieval_scope: self.retrieval_scope,
            idempotency_key,
            created_by,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct RouteNodeResourceAttachment {
    id: String,
    route_id: String,
    action_node_id: String,
    resource_id: String,
    kind: String,
    scene_equipment: String,
    title_override: String,
    note: String,
    sort_rank: i32,
    rag_enabled: bool,
    embedding_collection: String,
    retrieval_scope: String,
    created_by: String,
    created_at: String,
    updated_at: String,
    resource: Option<PublicResource>,
}

impl TryFrom<catalog_pb::RouteNodeResourceAttachment> for RouteNodeResourceAttachment {
    type Error = String;

    fn try_from(value: catalog_pb::RouteNodeResourceAttachment) -> Result<Self, Self::Error> {
        let kind = match catalog_pb::AttachmentKind::try_from(value.kind).ok() {
            Some(catalog_pb::AttachmentKind::Document) => "document",
            Some(catalog_pb::AttachmentKind::Pdf) => "pdf",
            Some(catalog_pb::AttachmentKind::ExternalLink) => "external_link",
            Some(catalog_pb::AttachmentKind::ToolChecklist) => "tool_checklist",
            Some(catalog_pb::AttachmentKind::AiActionGuide) => "ai_action_guide",
            Some(catalog_pb::AttachmentKind::RagCorpus) => "rag_corpus",
            Some(catalog_pb::AttachmentKind::ResourcePackage) => "resource_package",
            _ => {
                return Err(format!(
                    "catalog returned unknown attachment kind {}",
                    value.kind
                ));
            }
        };
        Ok(Self {
            id: value.id,
            route_id: value.route_id,
            action_node_id: value.action_node_id,
            resource_id: value.resource_id,
            kind: kind.to_string(),
            scene_equipment: value.scene_equipment,
            title_override: value.title_override,
            note: value.note,
            sort_rank: value.sort_rank,
            rag_enabled: value.rag_enabled,
            embedding_collection: value.embedding_collection,
            retrieval_scope: value.retrieval_scope,
            created_by: value.created_by,
            created_at: value.created_at,
            updated_at: value.updated_at,
            resource: value.resource.map(TryInto::try_into).transpose()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct RouteNodeResourcePage {
    items: Vec<RouteNodeResourceAttachment>,
}

impl TryFrom<catalog_pb::ListNodeResourcesResponse> for RouteNodeResourcePage {
    type Error = String;

    fn try_from(value: catalog_pb::ListNodeResourcesResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct DetachRouteNodeResourceResponse {
    detached: bool,
}

impl From<catalog_pb::DetachNodeResourceResponse> for DetachRouteNodeResourceResponse {
    fn from(value: catalog_pb::DetachNodeResourceResponse) -> Self {
        Self {
            detached: value.detached,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RouteNodeRagContextRequest {
    pub(crate) question: String,
    pub(crate) limit: Option<u32>,
    pub(crate) scene_equipment: Option<String>,
}

impl RouteNodeRagContextRequest {
    pub(crate) fn into_pb(
        self,
        route_id: String,
        action_node_id: String,
    ) -> catalog_pb::RetrieveRagContextRequest {
        catalog_pb::RetrieveRagContextRequest {
            route_id,
            action_node_id,
            question: self.question,
            limit: self.limit,
            embedding_model: String::new(),
            query_embedding: Vec::new(),
            scene_equipment: self.scene_equipment,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct RouteNodeRagContext {
    attachment: Option<RouteNodeResourceAttachment>,
    excerpt: String,
    relevance: f64,
}

impl TryFrom<catalog_pb::RagContext> for RouteNodeRagContext {
    type Error = String;

    fn try_from(value: catalog_pb::RagContext) -> Result<Self, Self::Error> {
        Ok(Self {
            attachment: value.attachment.map(TryInto::try_into).transpose()?,
            excerpt: value.excerpt,
            relevance: value.relevance,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct RouteNodeRagContextResponse {
    contexts: Vec<RouteNodeRagContext>,
    embedding_collections: Vec<String>,
    retrieval_mode: String,
}

impl TryFrom<catalog_pb::RetrieveRagContextResponse> for RouteNodeRagContextResponse {
    type Error = String;

    fn try_from(value: catalog_pb::RetrieveRagContextResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            contexts: value
                .contexts
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            embedding_collections: value.embedding_collections,
            retrieval_mode: value.retrieval_mode,
        })
    }
}

impl TryFrom<search_pb::SearchResponse> for SearchResponse {
    type Error = String;

    fn try_from(value: search_pb::SearchResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            query: value.query,
            items: value
                .items
                .into_iter()
                .map(|item| {
                    let result_type = SearchResultType::from_pb(item.result_type)?;
                    let ad = item.ad.map(|ad| SearchAd {
                        request_id: ad.request_id,
                        campaign_id: ad.campaign_id,
                        placement: ad.placement,
                        title: ad.title,
                        body: ad.body,
                        image_url: ad.image_url,
                        landing_url: ad.landing_url,
                        ecpm: ad.ecpm,
                        model_version: ad.model_version,
                        route_id: ad.route_id,
                        action_node_id: ad.action_node_id,
                        scene_equipment: ad.scene_equipment,
                    });
                    if (result_type == SearchResultType::Ad) != ad.is_some() {
                        return Err("search ad result has an invalid ad payload".to_string());
                    }
                    Ok(SearchResult {
                        id: item.id,
                        result_type,
                        title: item.title,
                        snippet: item.snippet,
                        cover_url: item.cover_url,
                        author_id: item.author_id,
                        author_name: item.author_name,
                        domain: item.domain.map(GrowthDomain::from_search).transpose()?,
                        score: item.score,
                        highlights: item.highlights,
                        post: item.post.map(PostSummary::from_search).transpose()?,
                        resource: item.resource.map(Into::into),
                        ad,
                    })
                })
                .collect::<Result<_, String>>()?,
            creator_profiles: Vec::new(),
            next_cursor: value.next_cursor,
            total_estimate: value.total_estimate,
            took_ms: value.took_ms,
            degraded: value.degraded,
            request_id: value.request_id,
        })
    }
}

impl From<search_pb::ResourceSummary> for SearchResourceSummary {
    fn from(value: search_pb::ResourceSummary) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            provider: value.provider,
            url: value.url,
            license: value.license,
            version: value.version,
            citation: value.citation,
            topics: value.topics,
            published_at: value.published_at,
            updated_at: value.updated_at,
        }
    }
}

impl TryFrom<crate::domain::SearchDiscovery> for SearchResponse {
    type Error = String;

    fn try_from(value: crate::domain::SearchDiscovery) -> Result<Self, Self::Error> {
        let mut response = Self::try_from(value.response)?;
        response.creator_profiles = value
            .creator_profiles
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;
        Ok(response)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Suggestion {
    text: String,
    result_type: SearchResultType,
    score: f64,
    personal: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SuggestionsResponse {
    query: String,
    items: Vec<Suggestion>,
}

impl TryFrom<search_pb::SuggestionsResponse> for SuggestionsResponse {
    type Error = String;

    fn try_from(value: search_pb::SuggestionsResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            query: value.query,
            items: value
                .items
                .into_iter()
                .map(|item| {
                    Ok(Suggestion {
                        text: item.text,
                        result_type: SearchResultType::from_pb(item.result_type)?,
                        score: item.score,
                        personal: item.personal,
                    })
                })
                .collect::<Result<_, String>>()?,
        })
    }
}

fn parse_domains(value: Option<String>) -> Result<Vec<GrowthDomain>, String> {
    parse_csv(value)
        .into_iter()
        .map(|domain| match domain.as_str() {
            "learning" => Ok(GrowthDomain::Learning),
            "movement" => Ok(GrowthDomain::Movement),
            "wellness" => Ok(GrowthDomain::Wellness),
            "travel" => Ok(GrowthDomain::Travel),
            "leisure" => Ok(GrowthDomain::Leisure),
            _ => Err(format!("unsupported growth domain '{domain}'")),
        })
        .collect()
}

fn parse_csv(value: Option<String>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|value| value.split(',').map(str::to_owned).collect::<Vec<_>>())
        .filter(|value| !value.is_empty())
        .collect()
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JourneyStatus {
    Active,
    Paused,
    Completed,
}

impl JourneyStatus {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::JourneyStatus::try_from(value).ok() {
            Some(growth_pb::JourneyStatus::Active) => Ok(Self::Active),
            Some(growth_pb::JourneyStatus::Paused) => Ok(Self::Paused),
            Some(growth_pb::JourneyStatus::Completed) => Ok(Self::Completed),
            None => Err(format!("growth returned unknown journey status {value}")),
        }
    }

    fn into_growth(self) -> i32 {
        match self {
            Self::Active => growth_pb::JourneyStatus::Active as i32,
            Self::Paused => growth_pb::JourneyStatus::Paused as i32,
            Self::Completed => growth_pb::JourneyStatus::Completed as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JourneyType {
    Habit,
    Project,
    Quantity,
    Travel,
    Challenge,
}

impl JourneyType {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::JourneyType::try_from(value).ok() {
            Some(growth_pb::JourneyType::Habit) => Ok(Self::Habit),
            Some(growth_pb::JourneyType::Project) => Ok(Self::Project),
            Some(growth_pb::JourneyType::Quantity) => Ok(Self::Quantity),
            Some(growth_pb::JourneyType::Travel) => Ok(Self::Travel),
            Some(growth_pb::JourneyType::Challenge) => Ok(Self::Challenge),
            None => Err(format!("growth returned unknown journey type {value}")),
        }
    }

    fn into_growth(self) -> i32 {
        match self {
            Self::Habit => growth_pb::JourneyType::Habit as i32,
            Self::Project => growth_pb::JourneyType::Project as i32,
            Self::Quantity => growth_pb::JourneyType::Quantity as i32,
            Self::Travel => growth_pb::JourneyType::Travel as i32,
            Self::Challenge => growth_pb::JourneyType::Challenge as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActionState {
    Pending,
    Completed,
    Skipped,
}

impl ActionState {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::ActionState::try_from(value).ok() {
            Some(growth_pb::ActionState::Pending) => Ok(Self::Pending),
            Some(growth_pb::ActionState::Completed) => Ok(Self::Completed),
            Some(growth_pb::ActionState::Skipped) => Ok(Self::Skipped),
            None => Err(format!("growth returned unknown action state {value}")),
        }
    }

    fn into_growth(self) -> i32 {
        match self {
            Self::Pending => growth_pb::ActionState::Pending as i32,
            Self::Completed => growth_pb::ActionState::Completed as i32,
            Self::Skipped => growth_pb::ActionState::Skipped as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActionRecurrenceFrequency {
    Daily,
    Weekly,
}

impl ActionRecurrenceFrequency {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::ActionRecurrenceFrequency::try_from(value).ok() {
            Some(growth_pb::ActionRecurrenceFrequency::Daily) => Ok(Self::Daily),
            Some(growth_pb::ActionRecurrenceFrequency::Weekly) => Ok(Self::Weekly),
            None => Err(format!(
                "growth returned unknown recurrence frequency {value}"
            )),
        }
    }

    fn into_growth(self) -> i32 {
        match self {
            Self::Daily => growth_pb::ActionRecurrenceFrequency::Daily as i32,
            Self::Weekly => growth_pb::ActionRecurrenceFrequency::Weekly as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::Weekday::try_from(value).ok() {
            Some(growth_pb::Weekday::Monday) => Ok(Self::Monday),
            Some(growth_pb::Weekday::Tuesday) => Ok(Self::Tuesday),
            Some(growth_pb::Weekday::Wednesday) => Ok(Self::Wednesday),
            Some(growth_pb::Weekday::Thursday) => Ok(Self::Thursday),
            Some(growth_pb::Weekday::Friday) => Ok(Self::Friday),
            Some(growth_pb::Weekday::Saturday) => Ok(Self::Saturday),
            Some(growth_pb::Weekday::Sunday) => Ok(Self::Sunday),
            None => Err(format!("growth returned unknown weekday {value}")),
        }
    }

    fn into_growth(self) -> i32 {
        match self {
            Self::Monday => growth_pb::Weekday::Monday as i32,
            Self::Tuesday => growth_pb::Weekday::Tuesday as i32,
            Self::Wednesday => growth_pb::Weekday::Wednesday as i32,
            Self::Thursday => growth_pb::Weekday::Thursday as i32,
            Self::Friday => growth_pb::Weekday::Friday as i32,
            Self::Saturday => growth_pb::Weekday::Saturday as i32,
            Self::Sunday => growth_pb::Weekday::Sunday as i32,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct ActionRecurrence {
    frequency: ActionRecurrenceFrequency,
    interval: u32,
    weekdays: Vec<Weekday>,
    ends_on: Option<String>,
    anchor_date: Option<String>,
}

impl TryFrom<growth_pb::ActionRecurrence> for ActionRecurrence {
    type Error = String;

    fn try_from(value: growth_pb::ActionRecurrence) -> Result<Self, Self::Error> {
        Ok(Self {
            frequency: ActionRecurrenceFrequency::from_growth(value.frequency)?,
            interval: value.interval,
            weekdays: value
                .weekdays
                .into_iter()
                .map(Weekday::from_growth)
                .collect::<Result<_, _>>()?,
            ends_on: value.ends_on,
            anchor_date: value.anchor_date,
        })
    }
}

impl TryFrom<ActionRecurrence> for growth_pb::ActionRecurrence {
    type Error = String;

    fn try_from(value: ActionRecurrence) -> Result<Self, Self::Error> {
        Ok(Self {
            frequency: value.frequency.into_growth(),
            interval: value.interval,
            weekdays: value
                .weekdays
                .into_iter()
                .map(Weekday::into_growth)
                .collect(),
            ends_on: value.ends_on,
            anchor_date: value.anchor_date,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct JourneyStageInput {
    title: String,
    detail: String,
    completion_criteria: String,
}

impl From<JourneyStageInput> for growth_pb::JourneyStageInput {
    fn from(value: JourneyStageInput) -> Self {
        Self {
            title: value.title,
            detail: value.detail,
            completion_criteria: value.completion_criteria,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct JourneyStage {
    id: String,
    title: String,
    detail: String,
    completion_criteria: String,
    position: u32,
}

impl From<growth_pb::JourneyStage> for JourneyStage {
    fn from(value: growth_pb::JourneyStage) -> Self {
        Self {
            id: value.id,
            title: value.title,
            detail: value.detail,
            completion_criteria: value.completion_criteria,
            position: value.position,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Journey {
    id: String,
    title: String,
    intent: String,
    domain: GrowthDomain,
    journey_type: JourneyType,
    completion_criteria: String,
    stages: Vec<JourneyStage>,
    status: JourneyStatus,
    progress: u32,
    duration_label: String,
    next_action: String,
    participant_count: u32,
}

impl TryFrom<growth_pb::Journey> for Journey {
    type Error = String;

    fn try_from(value: growth_pb::Journey) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            title: value.title,
            intent: value.intent,
            domain: GrowthDomain::from_growth(value.domain)?,
            journey_type: JourneyType::from_growth(value.journey_type)?,
            completion_criteria: value.completion_criteria,
            stages: value.stages.into_iter().map(Into::into).collect(),
            status: JourneyStatus::from_growth(value.status)?,
            progress: value.progress,
            duration_label: value.duration_label,
            next_action: value.next_action,
            participant_count: value.participant_count,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Action {
    id: String,
    journey_id: String,
    stage_id: Option<String>,
    title: String,
    detail: String,
    estimated_minutes: u32,
    scheduled_label: String,
    scheduled_for: Option<String>,
    scheduled_timezone: Option<String>,
    recurrence: Option<ActionRecurrence>,
    state: ActionState,
}

impl TryFrom<growth_pb::Action> for Action {
    type Error = String;

    fn try_from(value: growth_pb::Action) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            journey_id: value.journey_id,
            stage_id: value.stage_id,
            title: value.title,
            detail: value.detail,
            estimated_minutes: value.estimated_minutes,
            scheduled_label: value.scheduled_label,
            scheduled_for: value.scheduled_for,
            scheduled_timezone: value.scheduled_timezone,
            recurrence: value.recurrence.map(TryInto::try_into).transpose()?,
            state: ActionState::from_growth(value.state)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct JourneyDetail {
    journey: Option<Journey>,
    actions: Vec<Action>,
}

impl TryFrom<growth_pb::JourneyDetail> for JourneyDetail {
    type Error = String;

    fn try_from(value: growth_pb::JourneyDetail) -> Result<Self, Self::Error> {
        Ok(Self {
            journey: value.journey.map(TryInto::try_into).transpose()?,
            actions: value
                .actions
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct JourneyList {
    items: Vec<Journey>,
}

impl TryFrom<growth_pb::JourneyList> for JourneyList {
    type Error = String;

    fn try_from(value: growth_pb::JourneyList) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateJourneyRequest {
    title: String,
    intent: String,
    domain: GrowthDomain,
    journey_type: JourneyType,
    completion_criteria: String,
    #[serde(default)]
    stages: Vec<JourneyStageInput>,
    duration_label: String,
    first_action_title: String,
    first_action_detail: String,
    estimated_minutes: u32,
    first_action_scheduled_label: Option<String>,
    first_action_scheduled_for: Option<String>,
    first_action_scheduled_timezone: Option<String>,
    first_action_stage_index: Option<u32>,
    first_action_recurrence: Option<ActionRecurrence>,
}

impl CreateJourneyRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        idempotency_key: Option<String>,
    ) -> Result<growth_pb::CreateJourneyRequest, String> {
        Ok(growth_pb::CreateJourneyRequest {
            user_id,
            title: self.title,
            intent: self.intent,
            domain: self.domain.into_growth(),
            journey_type: self.journey_type.into_growth(),
            completion_criteria: self.completion_criteria,
            stages: self.stages.into_iter().map(Into::into).collect(),
            duration_label: self.duration_label,
            first_action_title: self.first_action_title,
            first_action_detail: self.first_action_detail,
            estimated_minutes: self.estimated_minutes,
            first_action_scheduled_label: self.first_action_scheduled_label,
            first_action_scheduled_for: self.first_action_scheduled_for,
            first_action_scheduled_timezone: self.first_action_scheduled_timezone,
            first_action_stage_index: self.first_action_stage_index,
            first_action_recurrence: self
                .first_action_recurrence
                .map(TryInto::try_into)
                .transpose()?,
            idempotency_key,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpdateJourneyRequest {
    title: Option<String>,
    intent: Option<String>,
    duration_label: Option<String>,
    status: Option<JourneyStatus>,
}

impl UpdateJourneyRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        journey_id: String,
    ) -> growth_pb::UpdateJourneyRequest {
        growth_pb::UpdateJourneyRequest {
            user_id,
            journey_id,
            title: self.title,
            intent: self.intent,
            duration_label: self.duration_label,
            status: self.status.map(JourneyStatus::into_growth),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateActionRequest {
    stage_id: Option<String>,
    title: String,
    detail: String,
    estimated_minutes: u32,
    scheduled_label: String,
    scheduled_for: Option<String>,
    scheduled_timezone: Option<String>,
    recurrence: Option<ActionRecurrence>,
}

impl CreateActionRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        journey_id: String,
        idempotency_key: Option<String>,
    ) -> Result<growth_pb::CreateActionRequest, String> {
        Ok(growth_pb::CreateActionRequest {
            user_id,
            journey_id,
            stage_id: self.stage_id,
            title: self.title,
            detail: self.detail,
            estimated_minutes: self.estimated_minutes,
            scheduled_label: self.scheduled_label,
            scheduled_for: self.scheduled_for,
            scheduled_timezone: self.scheduled_timezone,
            recurrence: self.recurrence.map(TryInto::try_into).transpose()?,
            idempotency_key,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpdateActionRequest {
    title: Option<String>,
    detail: Option<String>,
    estimated_minutes: Option<u32>,
    scheduled_label: Option<String>,
    scheduled_for: Option<String>,
    scheduled_timezone: Option<String>,
    state: Option<ActionState>,
}

impl UpdateActionRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        action_id: String,
    ) -> growth_pb::UpdateActionRequest {
        growth_pb::UpdateActionRequest {
            user_id,
            action_id,
            title: self.title,
            detail: self.detail,
            estimated_minutes: self.estimated_minutes,
            scheduled_label: self.scheduled_label,
            scheduled_for: self.scheduled_for,
            scheduled_timezone: self.scheduled_timezone,
            state: self.state.map(ActionState::into_growth),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EntryMood {
    Clear,
    Steady,
    Tired,
    Energized,
    Calm,
}

impl EntryMood {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::EntryMood::try_from(value).ok() {
            Some(growth_pb::EntryMood::Clear) => Ok(Self::Clear),
            Some(growth_pb::EntryMood::Steady) => Ok(Self::Steady),
            Some(growth_pb::EntryMood::Tired) => Ok(Self::Tired),
            Some(growth_pb::EntryMood::Energized) => Ok(Self::Energized),
            Some(growth_pb::EntryMood::Calm) => Ok(Self::Calm),
            None => Err(format!("growth returned unknown entry mood {value}")),
        }
    }

    fn into_growth(self) -> i32 {
        match self {
            Self::Clear => growth_pb::EntryMood::Clear as i32,
            Self::Steady => growth_pb::EntryMood::Steady as i32,
            Self::Tired => growth_pb::EntryMood::Tired as i32,
            Self::Energized => growth_pb::EntryMood::Energized as i32,
            Self::Calm => growth_pb::EntryMood::Calm as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EntryPublicationStatus {
    Private,
    Pending,
    Reviewing,
    Published,
    Restricted,
    Failed,
}

impl EntryPublicationStatus {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::EntryPublicationStatus::try_from(value).ok() {
            Some(growth_pb::EntryPublicationStatus::Private) => Ok(Self::Private),
            Some(growth_pb::EntryPublicationStatus::Pending) => Ok(Self::Pending),
            Some(growth_pb::EntryPublicationStatus::Reviewing) => Ok(Self::Reviewing),
            Some(growth_pb::EntryPublicationStatus::Published) => Ok(Self::Published),
            Some(growth_pb::EntryPublicationStatus::Restricted) => Ok(Self::Restricted),
            Some(growth_pb::EntryPublicationStatus::Failed) => Ok(Self::Failed),
            None => Err(format!(
                "growth returned unknown entry publication status {value}"
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct GrowthEntry {
    id: String,
    action_id: Option<String>,
    journey_id: Option<String>,
    body: String,
    mood: EntryMood,
    duration_minutes: Option<u32>,
    quantity: Option<String>,
    location: Option<String>,
    photo_url: Option<String>,
    created_at: String,
    published: bool,
    publication_status: EntryPublicationStatus,
    public_content_id: Option<String>,
    publication_error: Option<String>,
    photo_media_id: Option<String>,
}

impl TryFrom<growth_pb::GrowthEntry> for GrowthEntry {
    type Error = String;

    fn try_from(value: growth_pb::GrowthEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            action_id: value.action_id,
            journey_id: value.journey_id,
            body: value.body,
            mood: EntryMood::from_growth(value.mood)?,
            duration_minutes: value.duration_minutes,
            quantity: value.quantity,
            location: value.location,
            photo_url: value.photo_url,
            created_at: value.created_at,
            published: value.published,
            publication_status: EntryPublicationStatus::from_growth(value.publication_status)?,
            public_content_id: value.public_content_id,
            publication_error: value.publication_error,
            photo_media_id: value.photo_media_id,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct EntryList {
    items: Vec<GrowthEntry>,
}

impl TryFrom<growth_pb::EntryList> for EntryList {
    type Error = String;

    fn try_from(value: growth_pb::EntryList) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateEntryRequest {
    action_id: Option<String>,
    journey_id: Option<String>,
    body: String,
    mood: EntryMood,
    duration_minutes: Option<u32>,
    quantity: Option<String>,
    location: Option<String>,
    photo_url: Option<String>,
    published: bool,
    photo_media_id: Option<String>,
}

impl CreateEntryRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        idempotency_key: Option<String>,
    ) -> growth_pb::CreateEntryRequest {
        growth_pb::CreateEntryRequest {
            user_id,
            action_id: self.action_id,
            journey_id: self.journey_id,
            body: self.body,
            mood: self.mood.into_growth(),
            duration_minutes: self.duration_minutes,
            quantity: self.quantity,
            location: self.location,
            photo_url: self.photo_url,
            published: self.published,
            photo_media_id: self.photo_media_id,
            idempotency_key,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct TodaySummary {
    completed: u32,
    total: u32,
    focus_minutes: u32,
    actions: Vec<Action>,
}

impl TryFrom<growth_pb::TodaySummary> for TodaySummary {
    type Error = String;

    fn try_from(value: growth_pb::TodaySummary) -> Result<Self, Self::Error> {
        Ok(Self {
            completed: value.completed,
            total: value.total,
            focus_minutes: value.focus_minutes,
            actions: value
                .actions
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CompanionMode {
    StartSmall,
    KeepGoing,
    Celebrate,
    PlanNext,
}

impl CompanionMode {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::CompanionMode::try_from(value).ok() {
            Some(growth_pb::CompanionMode::StartSmall) => Ok(Self::StartSmall),
            Some(growth_pb::CompanionMode::KeepGoing) => Ok(Self::KeepGoing),
            Some(growth_pb::CompanionMode::Celebrate) => Ok(Self::Celebrate),
            Some(growth_pb::CompanionMode::PlanNext) => Ok(Self::PlanNext),
            None => Err(format!("growth returned unknown companion mode {value}")),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CompanionBrief {
    mode: CompanionMode,
    headline: String,
    message: String,
    reason: String,
    suggested_action: Option<Action>,
    suggested_minutes: Option<u32>,
    completed_actions: u32,
    total_actions: u32,
    active_journeys: u32,
    reflection_prompt: String,
}

impl TryFrom<growth_pb::CompanionBrief> for CompanionBrief {
    type Error = String;

    fn try_from(value: growth_pb::CompanionBrief) -> Result<Self, Self::Error> {
        Ok(Self {
            mode: CompanionMode::from_growth(value.mode)?,
            headline: value.headline,
            message: value.message,
            reason: value.reason,
            suggested_action: value.suggested_action.map(TryInto::try_into).transpose()?,
            suggested_minutes: value.suggested_minutes,
            completed_actions: value.completed_actions,
            total_actions: value.total_actions,
            active_journeys: value.active_journeys,
            reflection_prompt: value.reflection_prompt,
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReviewAdjustmentKind {
    ReduceActionDuration,
    RescheduleAction,
    PauseJourney,
}

impl ReviewAdjustmentKind {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::ReviewAdjustmentKind::try_from(value).ok() {
            Some(growth_pb::ReviewAdjustmentKind::ReduceActionDuration) => {
                Ok(Self::ReduceActionDuration)
            }
            Some(growth_pb::ReviewAdjustmentKind::RescheduleAction) => Ok(Self::RescheduleAction),
            Some(growth_pb::ReviewAdjustmentKind::PauseJourney) => Ok(Self::PauseJourney),
            None => Err(format!(
                "growth returned unknown review adjustment kind {value}"
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct ReviewActionPatch {
    action_id: String,
    estimated_minutes: Option<u32>,
    scheduled_label: Option<String>,
}

impl From<growth_pb::ReviewActionPatch> for ReviewActionPatch {
    fn from(value: growth_pb::ReviewActionPatch) -> Self {
        Self {
            action_id: value.action_id,
            estimated_minutes: value.estimated_minutes,
            scheduled_label: value.scheduled_label,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct ReviewJourneyPatch {
    journey_id: String,
    status: JourneyStatus,
}

impl TryFrom<growth_pb::ReviewJourneyPatch> for ReviewJourneyPatch {
    type Error = String;

    fn try_from(value: growth_pb::ReviewJourneyPatch) -> Result<Self, Self::Error> {
        Ok(Self {
            journey_id: value.journey_id,
            status: JourneyStatus::from_growth(value.status)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct ReviewAdjustmentSuggestion {
    kind: ReviewAdjustmentKind,
    title: String,
    rationale: String,
    action_patch: Option<ReviewActionPatch>,
    journey_patch: Option<ReviewJourneyPatch>,
}

impl TryFrom<growth_pb::ReviewAdjustmentSuggestion> for ReviewAdjustmentSuggestion {
    type Error = String;

    fn try_from(value: growth_pb::ReviewAdjustmentSuggestion) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: ReviewAdjustmentKind::from_growth(value.kind)?,
            title: value.title,
            rationale: value.rationale,
            action_patch: value.action_patch.map(Into::into),
            journey_patch: value.journey_patch.map(TryInto::try_into).transpose()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct WeeklyReviewSummary {
    period_start: String,
    period_end: String,
    completed_actions: u32,
    skipped_actions: u32,
    focus_minutes: u32,
    entry_count: u32,
    active_journeys: u32,
    completion_rate: f64,
    domains: Vec<ReviewDomainProgress>,
    reflection_prompts: Vec<String>,
    adjustment_suggestions: Vec<ReviewAdjustmentSuggestion>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct ReviewDomainProgress {
    domain: GrowthDomain,
    completed_actions: u32,
    total_actions: u32,
}

impl TryFrom<growth_pb::ReviewDomainProgress> for ReviewDomainProgress {
    type Error = String;

    fn try_from(value: growth_pb::ReviewDomainProgress) -> Result<Self, Self::Error> {
        Ok(Self {
            domain: GrowthDomain::from_growth(value.domain)?,
            completed_actions: value.completed_actions,
            total_actions: value.total_actions,
        })
    }
}

impl TryFrom<growth_pb::WeeklyReviewSummary> for WeeklyReviewSummary {
    type Error = String;

    fn try_from(value: growth_pb::WeeklyReviewSummary) -> Result<Self, Self::Error> {
        Ok(Self {
            period_start: value.period_start,
            period_end: value.period_end,
            completed_actions: value.completed_actions,
            skipped_actions: value.skipped_actions,
            focus_minutes: value.focus_minutes,
            entry_count: value.entry_count,
            active_journeys: value.active_journeys,
            completion_rate: value.completion_rate,
            domains: value
                .domains
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            reflection_prompts: value.reflection_prompts,
            adjustment_suggestions: value
                .adjustment_suggestions
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveWeeklyReviewRequest {
    reflection: String,
    #[serde(default)]
    next_focus: String,
}

impl SaveWeeklyReviewRequest {
    pub(crate) fn into_pb(self, user_id: String) -> growth_pb::SaveWeeklyReviewRequest {
        growth_pb::SaveWeeklyReviewRequest {
            user_id,
            reflection: self.reflection,
            next_focus: self.next_focus,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct WeeklyReview {
    id: String,
    summary: WeeklyReviewSummary,
    reflection: String,
    next_focus: String,
    created_at: String,
    updated_at: String,
    applied_adjustments: Vec<ReviewAdjustmentDecision>,
}

impl TryFrom<growth_pb::ReviewRecord> for WeeklyReview {
    type Error = String;

    fn try_from(value: growth_pb::ReviewRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            summary: value
                .summary
                .ok_or_else(|| "growth review is missing its summary".to_string())?
                .try_into()?,
            reflection: value.reflection,
            next_focus: value.next_focus,
            created_at: value.created_at,
            updated_at: value.updated_at,
            applied_adjustments: value
                .applied_adjustments
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct ReviewAdjustmentDecision {
    suggestion_index: u32,
    applied_at: String,
    action: Option<Action>,
    journey: Option<Journey>,
}

impl TryFrom<growth_pb::ReviewAdjustmentDecision> for ReviewAdjustmentDecision {
    type Error = String;

    fn try_from(value: growth_pb::ReviewAdjustmentDecision) -> Result<Self, Self::Error> {
        Ok(Self {
            suggestion_index: value.suggestion_index,
            applied_at: value.applied_at,
            action: value.action.map(TryInto::try_into).transpose()?,
            journey: value.journey.map(TryInto::try_into).transpose()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct ReviewAdjustmentApplication {
    review: WeeklyReview,
    decision: ReviewAdjustmentDecision,
}

impl TryFrom<growth_pb::ApplyWeeklyReviewAdjustmentResponse> for ReviewAdjustmentApplication {
    type Error = String;

    fn try_from(
        value: growth_pb::ApplyWeeklyReviewAdjustmentResponse,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            review: value
                .review
                .ok_or_else(|| "growth adjustment response is missing its review".to_string())?
                .try_into()?,
            decision: value
                .decision
                .ok_or_else(|| "growth adjustment response is missing its decision".to_string())?
                .try_into()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KnowledgeResourceKind {
    Book,
    Article,
    Course,
    Video,
    Link,
    Note,
}

impl KnowledgeResourceKind {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::KnowledgeResourceKind::try_from(value).ok() {
            Some(growth_pb::KnowledgeResourceKind::Book) => Ok(Self::Book),
            Some(growth_pb::KnowledgeResourceKind::Article) => Ok(Self::Article),
            Some(growth_pb::KnowledgeResourceKind::Course) => Ok(Self::Course),
            Some(growth_pb::KnowledgeResourceKind::Video) => Ok(Self::Video),
            Some(growth_pb::KnowledgeResourceKind::Link) => Ok(Self::Link),
            Some(growth_pb::KnowledgeResourceKind::Note) => Ok(Self::Note),
            None => Err(format!("growth returned unknown knowledge kind {value}")),
        }
    }

    fn into_growth(self) -> i32 {
        match self {
            Self::Book => growth_pb::KnowledgeResourceKind::Book as i32,
            Self::Article => growth_pb::KnowledgeResourceKind::Article as i32,
            Self::Course => growth_pb::KnowledgeResourceKind::Course as i32,
            Self::Video => growth_pb::KnowledgeResourceKind::Video as i32,
            Self::Link => growth_pb::KnowledgeResourceKind::Link as i32,
            Self::Note => growth_pb::KnowledgeResourceKind::Note as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KnowledgeResourceStatus {
    Inbox,
    Active,
    Completed,
    Archived,
}

impl KnowledgeResourceStatus {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::KnowledgeResourceStatus::try_from(value).ok() {
            Some(growth_pb::KnowledgeResourceStatus::Inbox) => Ok(Self::Inbox),
            Some(growth_pb::KnowledgeResourceStatus::Active) => Ok(Self::Active),
            Some(growth_pb::KnowledgeResourceStatus::Completed) => Ok(Self::Completed),
            Some(growth_pb::KnowledgeResourceStatus::Archived) => Ok(Self::Archived),
            None => Err(format!("growth returned unknown knowledge status {value}")),
        }
    }

    fn into_growth(self) -> i32 {
        match self {
            Self::Inbox => growth_pb::KnowledgeResourceStatus::Inbox as i32,
            Self::Active => growth_pb::KnowledgeResourceStatus::Active as i32,
            Self::Completed => growth_pb::KnowledgeResourceStatus::Completed as i32,
            Self::Archived => growth_pb::KnowledgeResourceStatus::Archived as i32,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct KnowledgeResource {
    id: String,
    title: String,
    creator: String,
    summary: String,
    kind: KnowledgeResourceKind,
    status: KnowledgeResourceStatus,
    source_url: Option<String>,
    body: Option<String>,
    tags: Vec<String>,
    journey_id: Option<String>,
    progress: u32,
    current_position: u32,
    reading_seconds: u64,
    bookmarks: Vec<String>,
    created_at: String,
    updated_at: String,
    last_opened_at: Option<String>,
    source_content_id: Option<String>,
}

impl TryFrom<growth_pb::KnowledgeResource> for KnowledgeResource {
    type Error = String;

    fn try_from(value: growth_pb::KnowledgeResource) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            title: value.title,
            creator: value.creator,
            summary: value.summary,
            kind: KnowledgeResourceKind::from_growth(value.kind)?,
            status: KnowledgeResourceStatus::from_growth(value.status)?,
            source_url: value.source_url,
            body: value.body,
            tags: value.tags,
            journey_id: value.journey_id,
            progress: value.progress,
            current_position: value.current_position,
            reading_seconds: value.reading_seconds,
            bookmarks: value.bookmarks,
            created_at: value.created_at,
            updated_at: value.updated_at,
            last_opened_at: value.last_opened_at,
            source_content_id: value.source_content_id,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct KnowledgeJourney {
    resource: KnowledgeResource,
    journey: JourneyDetail,
}

impl TryFrom<growth_pb::KnowledgeJourney> for KnowledgeJourney {
    type Error = String;

    fn try_from(value: growth_pb::KnowledgeJourney) -> Result<Self, Self::Error> {
        Ok(Self {
            resource: value
                .resource
                .ok_or_else(|| {
                    "growth returned a knowledge journey without its resource".to_string()
                })?
                .try_into()?,
            journey: value
                .journey
                .ok_or_else(|| {
                    "growth returned a knowledge journey without its journey".to_string()
                })?
                .try_into()?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct KnowledgeList {
    items: Vec<KnowledgeResource>,
}

impl TryFrom<growth_pb::KnowledgeList> for KnowledgeList {
    type Error = String;

    fn try_from(value: growth_pb::KnowledgeList) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct KnowledgeQuery {
    q: Option<String>,
    kind: Option<KnowledgeResourceKind>,
    status: Option<KnowledgeResourceStatus>,
    tag: Option<String>,
}

impl KnowledgeQuery {
    pub(crate) fn into_pb(self, user_id: String) -> growth_pb::KnowledgeQueryRequest {
        growth_pb::KnowledgeQueryRequest {
            user_id,
            q: self.q,
            kind: self.kind.map(KnowledgeResourceKind::into_growth),
            status: self.status.map(KnowledgeResourceStatus::into_growth),
            tag: self.tag,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateKnowledgeRequest {
    title: String,
    creator: String,
    summary: String,
    kind: KnowledgeResourceKind,
    status: KnowledgeResourceStatus,
    source_url: Option<String>,
    body: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    journey_id: Option<String>,
}

impl CreateKnowledgeRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        idempotency_key: Option<String>,
    ) -> growth_pb::CreateKnowledgeRequest {
        growth_pb::CreateKnowledgeRequest {
            user_id,
            idempotency_key,
            title: self.title,
            creator: self.creator,
            summary: self.summary,
            kind: self.kind.into_growth(),
            status: self.status.into_growth(),
            source_url: self.source_url,
            body: self.body,
            tags: self.tags,
            journey_id: self.journey_id,
            source_content_id: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct StartKnowledgeJourneyRequest {
    #[serde(flatten)]
    journey: CreateJourneyRequest,
}

impl StartKnowledgeJourneyRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        resource_id: String,
    ) -> Result<growth_pb::StartKnowledgeJourneyRequest, String> {
        // The resource ID is StartKnowledgeJourney's idempotency boundary.
        let journey = self.journey.into_pb(user_id.clone(), None)?;
        Ok(growth_pb::StartKnowledgeJourneyRequest {
            user_id,
            resource_id,
            title: journey.title,
            intent: journey.intent,
            domain: journey.domain,
            journey_type: journey.journey_type,
            completion_criteria: journey.completion_criteria,
            stages: journey.stages,
            duration_label: journey.duration_label,
            first_action_title: journey.first_action_title,
            first_action_detail: journey.first_action_detail,
            estimated_minutes: journey.estimated_minutes,
            first_action_scheduled_label: journey.first_action_scheduled_label,
            first_action_scheduled_for: journey.first_action_scheduled_for,
            first_action_scheduled_timezone: journey.first_action_scheduled_timezone,
            first_action_stage_index: journey.first_action_stage_index,
            first_action_recurrence: journey.first_action_recurrence,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpdateKnowledgeRequest {
    title: Option<String>,
    creator: Option<String>,
    summary: Option<String>,
    kind: Option<KnowledgeResourceKind>,
    status: Option<KnowledgeResourceStatus>,
    source_url: Option<String>,
    body: Option<String>,
    tags: Option<Vec<String>>,
    journey_id: Option<String>,
    progress: Option<u32>,
    current_position: Option<u32>,
    reading_seconds: Option<u64>,
    bookmarks: Option<Vec<String>>,
    last_opened_at: Option<String>,
}

impl UpdateKnowledgeRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        resource_id: String,
    ) -> growth_pb::UpdateKnowledgeRequest {
        growth_pb::UpdateKnowledgeRequest {
            user_id,
            resource_id,
            title: self.title,
            creator: self.creator,
            summary: self.summary,
            kind: self.kind.map(KnowledgeResourceKind::into_growth),
            status: self.status.map(KnowledgeResourceStatus::into_growth),
            source_url: self.source_url,
            body: self.body,
            tags: self.tags.map(|values| growth_pb::StringList { values }),
            journey_id: self.journey_id,
            progress: self.progress,
            current_position: self.current_position,
            reading_seconds: self.reading_seconds,
            bookmarks: self
                .bookmarks
                .map(|values| growth_pb::StringList { values }),
            last_opened_at: self.last_opened_at,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FeedbackCategory {
    Bug,
    Feature,
    Experience,
    Content,
    Other,
}

impl FeedbackCategory {
    fn from_pb(value: i32) -> Result<Self, String> {
        match feedback_pb::FeedbackCategory::try_from(value).ok() {
            Some(feedback_pb::FeedbackCategory::Bug) => Ok(Self::Bug),
            Some(feedback_pb::FeedbackCategory::Feature) => Ok(Self::Feature),
            Some(feedback_pb::FeedbackCategory::Experience) => Ok(Self::Experience),
            Some(feedback_pb::FeedbackCategory::Content) => Ok(Self::Content),
            Some(feedback_pb::FeedbackCategory::Other) => Ok(Self::Other),
            None => Err(format!("feedback returned unknown category {value}")),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Bug => feedback_pb::FeedbackCategory::Bug as i32,
            Self::Feature => feedback_pb::FeedbackCategory::Feature as i32,
            Self::Experience => feedback_pb::FeedbackCategory::Experience as i32,
            Self::Content => feedback_pb::FeedbackCategory::Content as i32,
            Self::Other => feedback_pb::FeedbackCategory::Other as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FeedbackStatus {
    Pending,
    Processing,
    Resolved,
    Closed,
}

impl FeedbackStatus {
    fn from_pb(value: i32) -> Result<Self, String> {
        match feedback_pb::FeedbackStatus::try_from(value).ok() {
            Some(feedback_pb::FeedbackStatus::Pending) => Ok(Self::Pending),
            Some(feedback_pb::FeedbackStatus::Processing) => Ok(Self::Processing),
            Some(feedback_pb::FeedbackStatus::Resolved) => Ok(Self::Resolved),
            Some(feedback_pb::FeedbackStatus::Closed) => Ok(Self::Closed),
            None => Err(format!("feedback returned unknown status {value}")),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Pending => feedback_pb::FeedbackStatus::Pending as i32,
            Self::Processing => feedback_pb::FeedbackStatus::Processing as i32,
            Self::Resolved => feedback_pb::FeedbackStatus::Resolved as i32,
            Self::Closed => feedback_pb::FeedbackStatus::Closed as i32,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct FeedbackItem {
    id: String,
    user_id: String,
    category: FeedbackCategory,
    content: String,
    contact: String,
    platform: String,
    app_version: String,
    status: FeedbackStatus,
    resolution: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<feedback_pb::FeedbackItem> for FeedbackItem {
    type Error = String;

    fn try_from(value: feedback_pb::FeedbackItem) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            user_id: value.user_id,
            category: FeedbackCategory::from_pb(value.category)?,
            content: value.content,
            contact: value.contact,
            platform: value.platform,
            app_version: value.app_version,
            status: FeedbackStatus::from_pb(value.status)?,
            resolution: value.resolution,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct FeedbackList {
    items: Vec<FeedbackItem>,
}

impl TryFrom<feedback_pb::FeedbackList> for FeedbackList {
    type Error = String;

    fn try_from(value: feedback_pb::FeedbackList) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateFeedbackRequest {
    category: FeedbackCategory,
    content: String,
    contact: String,
    platform: String,
    app_version: String,
}

impl CreateFeedbackRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        idempotency_key: Option<String>,
    ) -> feedback_pb::CreateFeedbackRequest {
        feedback_pb::CreateFeedbackRequest {
            user_id,
            idempotency_key,
            category: self.category.into_pb(),
            content: self.content,
            contact: self.contact,
            platform: self.platform,
            app_version: self.app_version,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FeedbackQuery {
    status: Option<FeedbackStatus>,
    limit: Option<u32>,
}

impl FeedbackQuery {
    pub(crate) fn into_own_pb(self, user_id: String) -> feedback_pb::ListOwnFeedbackRequest {
        feedback_pb::ListOwnFeedbackRequest {
            user_id,
            status: self.status.map(FeedbackStatus::into_pb),
            limit: self.limit,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReactionType {
    Like,
    Bookmark,
    Hide,
}

impl ReactionType {
    fn from_pb(value: i32) -> Result<Self, String> {
        match like_pb::ReactionType::try_from(value).ok() {
            Some(like_pb::ReactionType::Like) => Ok(Self::Like),
            Some(like_pb::ReactionType::Bookmark) => Ok(Self::Bookmark),
            Some(like_pb::ReactionType::Hide) => Ok(Self::Hide),
            None => Err(format!(
                "interaction-status returned unknown reaction type {value}"
            )),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Like => like_pb::ReactionType::Like as i32,
            Self::Bookmark => like_pb::ReactionType::Bookmark as i32,
            Self::Hide => like_pb::ReactionType::Hide as i32,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SetReactionRequest {
    reaction: ReactionType,
    active: bool,
    negative_feedback_reason: Option<NegativeFeedbackReason>,
    attribution: Option<ContentAttributionRequest>,
}

impl SetReactionRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        post_id: String,
    ) -> Result<
        (
            like_pb::SetReactionRequest,
            Option<user_event_pb::NegativeFeedbackReason>,
            Option<crate::domain::ContentAttribution>,
        ),
        String,
    > {
        let Self {
            reaction,
            active,
            negative_feedback_reason,
            attribution,
        } = self;
        let negative_feedback_reason = match (&reaction, negative_feedback_reason) {
            (ReactionType::Hide, reason) => reason.map(NegativeFeedbackReason::into_pb),
            (_, Some(_)) => {
                return Err("negative_feedback_reason is only valid for hide reactions".to_string());
            }
            (_, None) => None,
        };
        Ok((
            like_pb::SetReactionRequest {
                user_id,
                post_id,
                reaction: reaction.into_pb(),
                active,
            },
            negative_feedback_reason,
            attribution.map(ContentAttributionRequest::into_domain),
        ))
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ContentAttributionSource {
    Recommendation,
    Search,
}

impl ContentAttributionSource {
    fn into_pb(self) -> user_event_pb::AttributionSource {
        match self {
            Self::Recommendation => user_event_pb::AttributionSource::Recommendation,
            Self::Search => user_event_pb::AttributionSource::Search,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ContentAttributionRequest {
    session_id: String,
    request_id: String,
    position: u32,
    attribution_source: ContentAttributionSource,
}

impl ContentAttributionRequest {
    fn into_domain(self) -> crate::domain::ContentAttribution {
        crate::domain::ContentAttribution {
            session_id: self.session_id,
            request_id: self.request_id,
            position: self.position,
            source: self.attribution_source.into_pb(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct CaptureContentAsKnowledgeRequest {
    attribution: Option<ContentAttributionRequest>,
}

impl CaptureContentAsKnowledgeRequest {
    pub(crate) fn into_attribution(self) -> Option<crate::domain::ContentAttribution> {
        self.attribution.map(ContentAttributionRequest::into_domain)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct JoinRouteRequest {
    attribution: Option<ContentAttributionRequest>,
}

impl JoinRouteRequest {
    pub(crate) fn into_attribution(self) -> Option<crate::domain::ContentAttribution> {
        self.attribution.map(ContentAttributionRequest::into_domain)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Reaction {
    target_id: String,
    target_type: String,
    reaction: ReactionType,
    active: bool,
    count: u64,
}

impl TryFrom<like_pb::Reaction> for Reaction {
    type Error = String;

    fn try_from(value: like_pb::Reaction) -> Result<Self, Self::Error> {
        Ok(Self {
            target_id: value.target_id,
            target_type: value.target_type,
            reaction: ReactionType::from_pb(value.reaction)?,
            active: value.active,
            count: value.count,
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CommentStatus {
    Reviewing,
    Published,
    Restricted,
    Deleted,
}

impl CommentStatus {
    fn from_pb(value: i32) -> Result<Self, String> {
        match comment_pb::CommentStatus::try_from(value).ok() {
            Some(comment_pb::CommentStatus::Reviewing) => Ok(Self::Reviewing),
            Some(comment_pb::CommentStatus::Published) => Ok(Self::Published),
            Some(comment_pb::CommentStatus::Restricted) => Ok(Self::Restricted),
            Some(comment_pb::CommentStatus::Deleted) => Ok(Self::Deleted),
            None => Err(format!("comment returned unknown status {value}")),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CommentItem {
    id: String,
    post_id: String,
    author_id: String,
    author_name: String,
    body: String,
    parent_id: Option<String>,
    like_count: u64,
    created_at: String,
    status: CommentStatus,
}

impl TryFrom<comment_pb::CommentItem> for CommentItem {
    type Error = String;

    fn try_from(value: comment_pb::CommentItem) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            post_id: value.post_id,
            author_id: value.author_id,
            author_name: value.author_name,
            body: value.body,
            parent_id: value.parent_id,
            like_count: value.like_count,
            created_at: value.created_at,
            status: CommentStatus::from_pb(value.status)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CommentPage {
    items: Vec<CommentItem>,
    next_cursor: Option<String>,
}

impl TryFrom<comment_pb::CommentPage> for CommentPage {
    type Error = String;

    fn try_from(value: comment_pb::CommentPage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

impl TryFrom<comment_pb::ModerationCommentPage> for CommentPage {
    type Error = String;

    fn try_from(value: comment_pb::ModerationCommentPage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CommentQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

impl CommentQuery {
    pub(crate) fn into_pb(self, post_id: String) -> comment_pb::ListRequest {
        comment_pb::ListRequest {
            post_id,
            cursor: self.cursor,
            limit: self.limit,
            excluded_author_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ModerationCommentQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

impl ModerationCommentQuery {
    pub(crate) fn into_pb(self) -> comment_pb::ListModerationRequest {
        comment_pb::ListModerationRequest {
            cursor: self.cursor,
            limit: self.limit,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CommentModerationDecision {
    Approve,
    Restrict,
}

impl CommentModerationDecision {
    fn into_pb(self) -> i32 {
        match self {
            Self::Approve => comment_pb::CommentModerationDecision::Approve as i32,
            Self::Restrict => comment_pb::CommentModerationDecision::Restrict as i32,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ReviewCommentRequest {
    decision: CommentModerationDecision,
}

impl ReviewCommentRequest {
    pub(crate) fn into_pb(
        self,
        reviewer_id: String,
        comment_id: String,
    ) -> comment_pb::ReviewCommentRequest {
        comment_pb::ReviewCommentRequest {
            reviewer_id,
            comment_id,
            decision: self.decision.into_pb(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateCommentRequest {
    body: String,
    parent_id: Option<String>,
}

impl CreateCommentRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        post_id: String,
        idempotency_key: Option<String>,
    ) -> comment_pb::CreateRequest {
        comment_pb::CreateRequest {
            user_id,
            post_id,
            idempotency_key,
            body: self.body,
            parent_id: self.parent_id,
            excluded_author_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommentReportReason {
    Spam,
    Harassment,
    Unsafe,
    Fraud,
    Privacy,
    Other,
}

impl CommentReportReason {
    fn from_pb(value: i32) -> Result<Self, String> {
        match comment_pb::CommentReportReason::try_from(value).ok() {
            Some(comment_pb::CommentReportReason::Spam) => Ok(Self::Spam),
            Some(comment_pb::CommentReportReason::Harassment) => Ok(Self::Harassment),
            Some(comment_pb::CommentReportReason::Unsafe) => Ok(Self::Unsafe),
            Some(comment_pb::CommentReportReason::Fraud) => Ok(Self::Fraud),
            Some(comment_pb::CommentReportReason::Privacy) => Ok(Self::Privacy),
            Some(comment_pb::CommentReportReason::Other) => Ok(Self::Other),
            _ => Err(format!("comment returned unknown report reason {value}")),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Spam => comment_pb::CommentReportReason::Spam as i32,
            Self::Harassment => comment_pb::CommentReportReason::Harassment as i32,
            Self::Unsafe => comment_pb::CommentReportReason::Unsafe as i32,
            Self::Fraud => comment_pb::CommentReportReason::Fraud as i32,
            Self::Privacy => comment_pb::CommentReportReason::Privacy as i32,
            Self::Other => comment_pb::CommentReportReason::Other as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommentReportStatus {
    Pending,
    Reviewing,
    Resolved,
    Rejected,
}

impl CommentReportStatus {
    fn from_pb(value: i32) -> Result<Self, String> {
        match comment_pb::CommentReportStatus::try_from(value).ok() {
            Some(comment_pb::CommentReportStatus::Pending) => Ok(Self::Pending),
            Some(comment_pb::CommentReportStatus::Reviewing) => Ok(Self::Reviewing),
            Some(comment_pb::CommentReportStatus::Resolved) => Ok(Self::Resolved),
            Some(comment_pb::CommentReportStatus::Rejected) => Ok(Self::Rejected),
            None => Err(format!("comment returned unknown report status {value}")),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Pending => comment_pb::CommentReportStatus::Pending as i32,
            Self::Reviewing => comment_pb::CommentReportStatus::Reviewing as i32,
            Self::Resolved => comment_pb::CommentReportStatus::Resolved as i32,
            Self::Rejected => comment_pb::CommentReportStatus::Rejected as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommentReportAction {
    NoAction,
    RestrictComment,
}

impl CommentReportAction {
    fn from_pb(value: i32) -> Result<Self, String> {
        match comment_pb::CommentReportAction::try_from(value).ok() {
            Some(comment_pb::CommentReportAction::NoAction) => Ok(Self::NoAction),
            Some(comment_pb::CommentReportAction::RestrictComment) => Ok(Self::RestrictComment),
            None => Err(format!("comment returned unknown report action {value}")),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::NoAction => comment_pb::CommentReportAction::NoAction as i32,
            Self::RestrictComment => comment_pb::CommentReportAction::RestrictComment as i32,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateCommentReportRequest {
    reason: CommentReportReason,
    #[serde(default)]
    details: String,
}

impl CreateCommentReportRequest {
    pub(crate) fn into_pb(
        self,
        post_id: String,
        comment_id: String,
        idempotency_key: Option<String>,
    ) -> Result<comment_pb::CreateCommentReportRequest, String> {
        let idempotency_key = idempotency_key
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Idempotency-Key is required when reporting a comment".to_string())?;
        Ok(comment_pb::CreateCommentReportRequest {
            reporter_id: String::new(),
            post_id,
            comment_id,
            idempotency_key: Some(idempotency_key),
            reason: self.reason.into_pb(),
            details: self.details,
            excluded_author_ids: Vec::new(),
        })
    }
}

/// The report receipt is deliberately body-free; raw comment context is only
/// emitted from the moderator-only report endpoints below.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CommentReportReceipt {
    id: String,
    comment_id: String,
    reason: CommentReportReason,
    status: CommentReportStatus,
    created_at: String,
}

impl TryFrom<comment_pb::CommentReport> for CommentReportReceipt {
    type Error = String;

    fn try_from(value: comment_pb::CommentReport) -> Result<Self, Self::Error> {
        let comment_id = value
            .reported_comment
            .as_ref()
            .map(|comment| comment.id.clone())
            .ok_or_else(|| "comment report omitted its reported comment".to_string())?;
        Ok(Self {
            id: value.id,
            comment_id,
            reason: CommentReportReason::from_pb(value.reason)?,
            status: CommentReportStatus::from_pb(value.status)?,
            created_at: value.created_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CommentReport {
    id: String,
    reporter_id: String,
    reported_comment: CommentItem,
    reason: CommentReportReason,
    details: String,
    status: CommentReportStatus,
    reviewer_id: Option<String>,
    resolution: Option<String>,
    action: CommentReportAction,
    created_at: String,
    updated_at: String,
}

impl TryFrom<comment_pb::CommentReport> for CommentReport {
    type Error = String;

    fn try_from(value: comment_pb::CommentReport) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            reporter_id: value.reporter_id,
            reported_comment: value
                .reported_comment
                .ok_or_else(|| "comment report omitted its reported comment".to_string())?
                .try_into()?,
            reason: CommentReportReason::from_pb(value.reason)?,
            details: value.details,
            status: CommentReportStatus::from_pb(value.status)?,
            reviewer_id: value.reviewer_id,
            resolution: value.resolution,
            action: CommentReportAction::from_pb(value.action)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CommentReportPage {
    items: Vec<CommentReport>,
    next_cursor: Option<String>,
}

impl TryFrom<comment_pb::CommentReportPage> for CommentReportPage {
    type Error = String;

    fn try_from(value: comment_pb::CommentReportPage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ModerationCommentReportQuery {
    status: Option<CommentReportStatus>,
    cursor: Option<String>,
    limit: Option<u32>,
}

impl ModerationCommentReportQuery {
    pub(crate) fn into_pb(self) -> comment_pb::ListCommentReportsRequest {
        comment_pb::ListCommentReportsRequest {
            status: self.status.map(CommentReportStatus::into_pb),
            cursor: self.cursor,
            limit: self.limit,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ReviewCommentReportRequest {
    status: CommentReportStatus,
    resolution: String,
    action: CommentReportAction,
}

impl ReviewCommentReportRequest {
    pub(crate) fn into_pb(
        self,
        reviewer_id: String,
        report_id: String,
    ) -> comment_pb::ReviewCommentReportRequest {
        comment_pb::ReviewCommentReportRequest {
            reviewer_id,
            report_id,
            status: self.status.into_pb(),
            resolution: self.resolution,
            action: self.action.into_pb(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommentAppealStatus {
    Pending,
    Reviewing,
    Resolved,
    Rejected,
}

impl CommentAppealStatus {
    fn from_pb(value: i32) -> Result<Self, String> {
        match comment_pb::CommentAppealStatus::try_from(value).ok() {
            Some(comment_pb::CommentAppealStatus::Pending) => Ok(Self::Pending),
            Some(comment_pb::CommentAppealStatus::Reviewing) => Ok(Self::Reviewing),
            Some(comment_pb::CommentAppealStatus::Resolved) => Ok(Self::Resolved),
            Some(comment_pb::CommentAppealStatus::Rejected) => Ok(Self::Rejected),
            None => Err(format!("comment returned unknown appeal status {value}")),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Pending => comment_pb::CommentAppealStatus::Pending as i32,
            Self::Reviewing => comment_pb::CommentAppealStatus::Reviewing as i32,
            Self::Resolved => comment_pb::CommentAppealStatus::Resolved as i32,
            Self::Rejected => comment_pb::CommentAppealStatus::Rejected as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommentAppealAction {
    NoAction,
    RestoreComment,
}

impl CommentAppealAction {
    fn from_pb(value: i32) -> Result<Self, String> {
        match comment_pb::CommentAppealAction::try_from(value).ok() {
            Some(comment_pb::CommentAppealAction::NoAction) => Ok(Self::NoAction),
            Some(comment_pb::CommentAppealAction::RestoreComment) => Ok(Self::RestoreComment),
            None => Err(format!("comment returned unknown appeal action {value}")),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::NoAction => comment_pb::CommentAppealAction::NoAction as i32,
            Self::RestoreComment => comment_pb::CommentAppealAction::RestoreComment as i32,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateCommentAppealRequest {
    #[serde(default)]
    details: String,
}

impl CreateCommentAppealRequest {
    pub(crate) fn into_pb(
        self,
        comment_id: String,
        idempotency_key: Option<String>,
    ) -> Result<comment_pb::CreateCommentAppealRequest, String> {
        let idempotency_key = idempotency_key
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Idempotency-Key is required when appealing a comment".to_string())?;
        Ok(comment_pb::CreateCommentAppealRequest {
            author_id: String::new(),
            comment_id,
            idempotency_key: Some(idempotency_key),
            details: self.details,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CommentAppeal {
    id: String,
    author_id: String,
    appealed_comment: CommentItem,
    details: String,
    status: CommentAppealStatus,
    reviewer_id: Option<String>,
    resolution: Option<String>,
    action: CommentAppealAction,
    created_at: String,
    updated_at: String,
}

impl TryFrom<comment_pb::CommentAppeal> for CommentAppeal {
    type Error = String;

    fn try_from(value: comment_pb::CommentAppeal) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            author_id: value.author_id,
            appealed_comment: value
                .appealed_comment
                .ok_or_else(|| "comment appeal omitted its appealed comment".to_string())?
                .try_into()?,
            details: value.details,
            status: CommentAppealStatus::from_pb(value.status)?,
            reviewer_id: value.reviewer_id,
            resolution: value.resolution,
            action: CommentAppealAction::from_pb(value.action)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CommentAppealPage {
    items: Vec<CommentAppeal>,
    next_cursor: Option<String>,
}

impl TryFrom<comment_pb::CommentAppealPage> for CommentAppealPage {
    type Error = String;

    fn try_from(value: comment_pb::CommentAppealPage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct OwnCommentAppealQuery {
    status: Option<CommentAppealStatus>,
    cursor: Option<String>,
    limit: Option<u32>,
}

impl OwnCommentAppealQuery {
    pub(crate) fn into_pb(self) -> comment_pb::ListCommentAppealsRequest {
        comment_pb::ListCommentAppealsRequest {
            author_id: None,
            status: self.status.map(CommentAppealStatus::into_pb),
            cursor: self.cursor,
            limit: self.limit,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ModerationCommentAppealQuery {
    status: Option<CommentAppealStatus>,
    cursor: Option<String>,
    limit: Option<u32>,
}

impl ModerationCommentAppealQuery {
    pub(crate) fn into_pb(self) -> comment_pb::ListCommentAppealsRequest {
        comment_pb::ListCommentAppealsRequest {
            author_id: None,
            status: self.status.map(CommentAppealStatus::into_pb),
            cursor: self.cursor,
            limit: self.limit,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ReviewCommentAppealRequest {
    status: CommentAppealStatus,
    resolution: String,
    action: CommentAppealAction,
}

impl ReviewCommentAppealRequest {
    pub(crate) fn into_pb(
        self,
        reviewer_id: String,
        appeal_id: String,
    ) -> comment_pb::ReviewCommentAppealRequest {
        comment_pb::ReviewCommentAppealRequest {
            reviewer_id,
            appeal_id,
            status: self.status.into_pb(),
            resolution: self.resolution,
            action: self.action.into_pb(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportReason {
    Spam,
    Harassment,
    Unsafe,
    Misinformation,
    Copyright,
    Privacy,
    Other,
}

impl ReportReason {
    fn from_pb(value: i32) -> Result<Self, String> {
        match audit_pb::ReportReason::try_from(value).ok() {
            Some(audit_pb::ReportReason::Spam) => Ok(Self::Spam),
            Some(audit_pb::ReportReason::Harassment) => Ok(Self::Harassment),
            Some(audit_pb::ReportReason::Unsafe) => Ok(Self::Unsafe),
            Some(audit_pb::ReportReason::Misinformation) => Ok(Self::Misinformation),
            Some(audit_pb::ReportReason::Copyright) => Ok(Self::Copyright),
            Some(audit_pb::ReportReason::Privacy) => Ok(Self::Privacy),
            Some(audit_pb::ReportReason::Other) => Ok(Self::Other),
            None => Err(format!(
                "content-audit returned unknown report reason {value}"
            )),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Spam => audit_pb::ReportReason::Spam as i32,
            Self::Harassment => audit_pb::ReportReason::Harassment as i32,
            Self::Unsafe => audit_pb::ReportReason::Unsafe as i32,
            Self::Misinformation => audit_pb::ReportReason::Misinformation as i32,
            Self::Copyright => audit_pb::ReportReason::Copyright as i32,
            Self::Privacy => audit_pb::ReportReason::Privacy as i32,
            Self::Other => audit_pb::ReportReason::Other as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReportStatus {
    Pending,
    Reviewing,
    Resolved,
    Rejected,
}

impl ReportStatus {
    fn from_pb(value: i32) -> Result<Self, String> {
        match audit_pb::ReportStatus::try_from(value).ok() {
            Some(audit_pb::ReportStatus::Pending) => Ok(Self::Pending),
            Some(audit_pb::ReportStatus::Reviewing) => Ok(Self::Reviewing),
            Some(audit_pb::ReportStatus::Resolved) => Ok(Self::Resolved),
            Some(audit_pb::ReportStatus::Rejected) => Ok(Self::Rejected),
            None => Err(format!(
                "content-audit returned unknown report status {value}"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentAction {
    NoAction,
    Restrict,
    Restore,
}

impl Serialize for ContentAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = match self {
            Self::NoAction => "no_action",
            Self::Restrict => "restrict_content",
            Self::Restore => "restore_content",
        };
        serializer.serialize_str(value)
    }
}

impl ContentAction {
    fn from_pb(value: i32) -> Result<Self, String> {
        match audit_pb::ContentAction::try_from(value).ok() {
            Some(audit_pb::ContentAction::NoAction) => Ok(Self::NoAction),
            Some(audit_pb::ContentAction::Restrict) => Ok(Self::Restrict),
            Some(audit_pb::ContentAction::Restore) => Ok(Self::Restore),
            None => Err(format!(
                "content-audit returned unknown content action {value}"
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct ContentReport {
    id: String,
    reporter_id: String,
    content_id: String,
    reason: ReportReason,
    details: String,
    status: ReportStatus,
    created_at: String,
    assignee_id: Option<String>,
    resolution: Option<String>,
    action: ContentAction,
    updated_at: String,
}

impl TryFrom<audit_pb::ContentReport> for ContentReport {
    type Error = String;

    fn try_from(value: audit_pb::ContentReport) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            reporter_id: value.reporter_id,
            content_id: value.content_id,
            reason: ReportReason::from_pb(value.reason)?,
            details: value.details,
            status: ReportStatus::from_pb(value.status)?,
            created_at: value.created_at,
            assignee_id: value.assignee_id,
            resolution: value.resolution,
            action: ContentAction::from_pb(value.action)?,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateReportRequest {
    reason: ReportReason,
    details: String,
}

impl CreateReportRequest {
    pub(crate) fn into_pb(
        self,
        reporter_id: String,
        content_id: String,
        idempotency_key: Option<String>,
    ) -> audit_pb::CreateReportRequest {
        audit_pb::CreateReportRequest {
            reporter_id,
            content_id,
            idempotency_key,
            reason: self.reason.into_pb(),
            details: self.details,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AppealStatus {
    Pending,
    Reviewing,
    Resolved,
    Rejected,
}

impl AppealStatus {
    fn from_pb(value: i32) -> Result<Self, String> {
        match audit_pb::AppealStatus::try_from(value).ok() {
            Some(audit_pb::AppealStatus::Pending) => Ok(Self::Pending),
            Some(audit_pb::AppealStatus::Reviewing) => Ok(Self::Reviewing),
            Some(audit_pb::AppealStatus::Resolved) => Ok(Self::Resolved),
            Some(audit_pb::AppealStatus::Rejected) => Ok(Self::Rejected),
            None => Err(format!(
                "content-audit returned unknown appeal status {value}"
            )),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Pending => audit_pb::AppealStatus::Pending as i32,
            Self::Reviewing => audit_pb::AppealStatus::Reviewing as i32,
            Self::Resolved => audit_pb::AppealStatus::Resolved as i32,
            Self::Rejected => audit_pb::AppealStatus::Rejected as i32,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct ContentAppeal {
    id: String,
    content_id: String,
    appellant_id: String,
    details: String,
    status: AppealStatus,
    assignee_id: Option<String>,
    resolution: Option<String>,
    action: ContentAction,
    created_at: String,
    updated_at: String,
}

impl TryFrom<audit_pb::ContentAppeal> for ContentAppeal {
    type Error = String;

    fn try_from(value: audit_pb::ContentAppeal) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            content_id: value.content_id,
            appellant_id: value.appellant_id,
            details: value.details,
            status: AppealStatus::from_pb(value.status)?,
            assignee_id: value.assignee_id,
            resolution: value.resolution,
            action: ContentAction::from_pb(value.action)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct AppealPage {
    items: Vec<ContentAppeal>,
    next_cursor: Option<String>,
}

impl TryFrom<audit_pb::AppealPage> for AppealPage {
    type Error = String;

    fn try_from(value: audit_pb::AppealPage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateAppealRequest {
    details: String,
}

impl CreateAppealRequest {
    pub(crate) fn into_pb(
        self,
        appellant_id: String,
        content_id: String,
        idempotency_key: Option<String>,
    ) -> audit_pb::CreateAppealRequest {
        audit_pb::CreateAppealRequest {
            appellant_id,
            content_id,
            idempotency_key,
            details: self.details,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct OwnAppealQuery {
    status: Option<AppealStatus>,
    cursor: Option<String>,
    limit: Option<u32>,
}

impl OwnAppealQuery {
    pub(crate) fn into_pb(self) -> audit_pb::ListAppealsRequest {
        audit_pb::ListAppealsRequest {
            status: self.status.map(AppealStatus::into_pb),
            appellant_id: None,
            content_id: None,
            cursor: self.cursor,
            limit: self.limit,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FollowRequest {
    active: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FollowerPageQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RoutePeersQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

impl FollowerPageQuery {
    pub(crate) fn into_pb(self, user_id: String) -> bbs_pb::ListFollowersRequest {
        bbs_pb::ListFollowersRequest {
            user_id,
            cursor: self.cursor.unwrap_or_default(),
            limit: self.limit.unwrap_or(0),
        }
    }
}

impl RoutePeersQuery {
    pub(crate) fn into_pb(self, viewer_id: String, route_id: String) -> bbs_pb::ListRoutePeersRequest {
        bbs_pb::ListRoutePeersRequest {
            viewer_id,
            route_id,
            cursor: self.cursor.unwrap_or_default(),
            limit: self.limit.unwrap_or(0),
        }
    }
}

impl FollowRequest {
    pub(crate) fn into_pb(self, user_id: String, target_user_id: String) -> bbs_pb::SetEdgeRequest {
        bbs_pb::SetEdgeRequest {
            user_id,
            target_user_id,
            edge: bbs_pb::SocialEdgeType::Follow as i32,
            active: self.active,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RelationshipEdge {
    Follow,
    Block,
    Mute,
}

impl RelationshipEdge {
    fn into_pb(self) -> i32 {
        match self {
            Self::Follow => bbs_pb::SocialEdgeType::Follow as i32,
            Self::Block => bbs_pb::SocialEdgeType::Block as i32,
            Self::Mute => bbs_pb::SocialEdgeType::Mute as i32,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SetRelationshipRequest {
    edge: RelationshipEdge,
    active: bool,
}

impl SetRelationshipRequest {
    pub(crate) fn into_pb(self, user_id: String, target_user_id: String) -> bbs_pb::SetEdgeRequest {
        bbs_pb::SetEdgeRequest {
            user_id,
            target_user_id,
            edge: self.edge.into_pb(),
            active: self.active,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RouteParticipationRequest {
    active: bool,
    private_journey_id: Option<String>,
}

impl RouteParticipationRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        route_id: String,
    ) -> bbs_pb::RouteParticipationRequest {
        bbs_pb::RouteParticipationRequest {
            user_id,
            route_id,
            active: self.active,
            private_journey_id: self.private_journey_id,
            intent_version: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AttributionSource {
    Unspecified,
    Recommendation,
    Search,
}

impl AttributionSource {
    fn into_pb(self) -> i32 {
        match self {
            Self::Unspecified => user_event_pb::AttributionSource::Unspecified as i32,
            Self::Recommendation => user_event_pb::AttributionSource::Recommendation as i32,
            Self::Search => user_event_pb::AttributionSource::Search as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum NegativeFeedbackReason {
    NotRelevant,
    AlreadySeen,
    LowQuality,
}

impl NegativeFeedbackReason {
    fn into_pb(self) -> user_event_pb::NegativeFeedbackReason {
        match self {
            Self::NotRelevant => user_event_pb::NegativeFeedbackReason::NotRelevant,
            Self::AlreadySeen => user_event_pb::NegativeFeedbackReason::AlreadySeen,
            Self::LowQuality => user_event_pb::NegativeFeedbackReason::LowQuality,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ClientEvent {
    event_id: String,
    event_type: ClientEventType,
    session_id: String,
    request_id: Option<String>,
    component_id: String,
    content_id: Option<String>,
    position: Option<u32>,
    occurred_at: String,
    source: String,
    attribution_source: Option<AttributionSource>,
    negative_feedback_reason: Option<NegativeFeedbackReason>,
}

impl From<ClientEvent> for user_event_pb::Event {
    fn from(value: ClientEvent) -> Self {
        Self {
            event_id: value.event_id,
            event_type: value.event_type.as_str().to_string(),
            session_id: value.session_id,
            request_id: value.request_id,
            component_id: value.component_id,
            content_id: value.content_id,
            position: value.position,
            occurred_at: value.occurred_at,
            source: value.source,
            attribution_source: value
                .attribution_source
                .unwrap_or(AttributionSource::Unspecified)
                .into_pb(),
            negative_feedback_reason: value
                .negative_feedback_reason
                .map(|reason| reason.into_pb() as i32),
        }
    }
}

fn is_reserved_server_source(source: &str) -> bool {
    source.trim().starts_with("gateway-")
}

// Payments are authoritative server-side facts.  Keeping them out of this
// public request schema prevents a client from training recommendations with
// a fabricated purchase conversion.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ClientEventType {
    Impression,
    Click,
    View,
    Like,
    Bookmark,
    SaveKnowledge,
    Share,
    Hide,
    Complete,
    JoinRoute,
    Follow,
    Report,
    SearchSubmit,
}

impl ClientEventType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Impression => "impression",
            Self::Click => "click",
            Self::View => "view",
            Self::Like => "like",
            Self::Bookmark => "bookmark",
            Self::SaveKnowledge => "save_knowledge",
            Self::Share => "share",
            Self::Hide => "hide",
            Self::Complete => "complete",
            Self::JoinRoute => "join_route",
            Self::Follow => "follow",
            Self::Report => "report",
            Self::SearchSubmit => "search_submit",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct IngestEventsRequest {
    events: Vec<ClientEvent>,
}

impl IngestEventsRequest {
    pub(crate) fn into_pb(self, user_id: String) -> Result<user_event_pb::IngestRequest, String> {
        if self
            .events
            .iter()
            .any(|event| is_reserved_server_source(&event.source))
        {
            return Err("gateway-* event sources are reserved for server actions".to_string());
        }
        Ok(user_event_pb::IngestRequest {
            user_id,
            events: self.events.into_iter().map(Into::into).collect(),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpdateAccountProfileRequest {
    display_name: Option<String>,
    avatar_url: Option<String>,
    bio: Option<String>,
}

impl UpdateAccountProfileRequest {
    pub(crate) fn into_pb(self, user_id: String) -> account_pb::UpdateProfileRequest {
        account_pb::UpdateProfileRequest {
            user_id,
            display_name: self.display_name,
            avatar_url: self.avatar_url,
            bio: self.bio,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CreatorState {
    Active,
    Paused,
}

impl CreatorState {
    fn from_pb(value: i32) -> Result<Self, String> {
        match creator_pb::CreatorState::try_from(value).ok() {
            Some(creator_pb::CreatorState::Active) => Ok(Self::Active),
            Some(creator_pb::CreatorState::Paused) => Ok(Self::Paused),
            None => Err(format!("bbs-creator returned unknown state {value}")),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Active => creator_pb::CreatorState::Active as i32,
            Self::Paused => creator_pb::CreatorState::Paused as i32,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CreatorProfile {
    user_id: String,
    handle: String,
    headline: String,
    introduction: String,
    cover_url: String,
    specialties: Vec<String>,
    featured_content_ids: Vec<String>,
    state: CreatorState,
    created_at: String,
    updated_at: String,
}

impl TryFrom<creator_pb::CreatorProfile> for CreatorProfile {
    type Error = String;

    fn try_from(value: creator_pb::CreatorProfile) -> Result<Self, Self::Error> {
        Ok(Self {
            user_id: value.user_id,
            handle: value.handle,
            headline: value.headline,
            introduction: value.introduction,
            cover_url: value.cover_url,
            specialties: value.specialties,
            featured_content_ids: value.featured_content_ids,
            state: CreatorState::from_pb(value.state)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CreatorProfilePage {
    items: Vec<CreatorProfile>,
    next_cursor: Option<String>,
}

impl TryFrom<creator_pb::CreatorProfilePage> for CreatorProfilePage {
    type Error = String;

    fn try_from(value: creator_pb::CreatorProfilePage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreatorProfileQuery {
    user_ids: Option<String>,
    query: Option<String>,
    specialty: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
}

impl CreatorProfileQuery {
    pub(crate) fn into_pb(self) -> creator_pb::ListCreatorProfilesRequest {
        creator_pb::ListCreatorProfilesRequest {
            user_ids: parse_csv(self.user_ids),
            query: self.query,
            specialty: self.specialty,
            cursor: self.cursor,
            limit: self.limit,
            excluded_user_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpdateCreatorProfileRequest {
    handle: String,
    headline: String,
    introduction: String,
    cover_url: String,
    #[serde(default)]
    specialties: Vec<String>,
    #[serde(default)]
    featured_content_ids: Vec<String>,
    state: CreatorState,
}

impl UpdateCreatorProfileRequest {
    pub(crate) fn into_pb(self, user_id: String) -> creator_pb::UpsertCreatorProfileRequest {
        creator_pb::UpsertCreatorProfileRequest {
            user_id,
            handle: self.handle,
            headline: self.headline,
            introduction: self.introduction,
            cover_url: self.cover_url,
            specialties: self.specialties,
            featured_content_ids: self.featured_content_ids,
            state: self.state.into_pb(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DirectMessageKind {
    Text,
}

impl DirectMessageKind {
    fn from_pb(value: i32) -> Result<Self, String> {
        match message_pb::DirectMessageKind::try_from(value).ok() {
            Some(message_pb::DirectMessageKind::Text) => Ok(Self::Text),
            None => Err(format!("bbs-message returned unknown kind {value}")),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct DirectMessage {
    id: String,
    conversation_id: String,
    sender_user_id: String,
    recipient_user_id: String,
    kind: DirectMessageKind,
    body: String,
    created_at: String,
    read_at: Option<String>,
}

impl TryFrom<message_pb::DirectMessage> for DirectMessage {
    type Error = String;

    fn try_from(value: message_pb::DirectMessage) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            conversation_id: value.conversation_id,
            sender_user_id: value.sender_user_id,
            recipient_user_id: value.recipient_user_id,
            kind: DirectMessageKind::from_pb(value.kind)?,
            body: value.body,
            created_at: value.created_at,
            read_at: value.read_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct DirectMessagePage {
    items: Vec<DirectMessage>,
    next_cursor: Option<String>,
}

impl TryFrom<message_pb::DirectMessagePage> for DirectMessagePage {
    type Error = String;

    fn try_from(value: message_pb::DirectMessagePage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct DirectConversation {
    id: String,
    peer_user_id: String,
    last_message_preview: String,
    last_message_at: String,
    unread_count: u64,
}

impl From<message_pb::Conversation> for DirectConversation {
    fn from(value: message_pb::Conversation) -> Self {
        Self {
            id: value.id,
            peer_user_id: value.peer_user_id,
            last_message_preview: value.last_message_preview,
            last_message_at: value.last_message_at,
            unread_count: value.unread_count,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct DirectConversationPage {
    items: Vec<DirectConversation>,
    next_cursor: Option<String>,
}

impl From<message_pb::ConversationPage> for DirectConversationPage {
    fn from(value: message_pb::ConversationPage) -> Self {
        Self {
            items: value.items.into_iter().map(Into::into).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct DirectMessagePreferences {
    allow_direct_messages: bool,
    updated_at: String,
}

impl From<message_pb::DirectMessagePreferences> for DirectMessagePreferences {
    fn from(value: message_pb::DirectMessagePreferences) -> Self {
        Self {
            allow_direct_messages: value.allow_direct_messages,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct MarkConversationReadResponse {
    marked_count: u64,
    read_at: String,
}

impl From<message_pb::MarkConversationReadResponse> for MarkConversationReadResponse {
    fn from(value: message_pb::MarkConversationReadResponse) -> Self {
        Self {
            marked_count: value.marked_count,
            read_at: value.read_at,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SendDirectMessageRequest {
    recipient_user_id: String,
    body: String,
}

impl SendDirectMessageRequest {
    pub(crate) fn into_pb(
        self,
        sender_user_id: String,
        client_message_id: Option<String>,
    ) -> Result<message_pb::SendDirectMessageRequest, String> {
        let client_message_id = client_message_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "Idempotency-Key is required when sending a direct message".to_string()
            })?;
        Ok(message_pb::SendDirectMessageRequest {
            sender_user_id,
            recipient_user_id: self.recipient_user_id,
            client_message_id,
            kind: message_pb::DirectMessageKind::Text as i32,
            body: self.body,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DirectConversationQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

impl DirectConversationQuery {
    pub(crate) fn into_pb(self, user_id: String) -> message_pb::ListConversationsRequest {
        message_pb::ListConversationsRequest {
            user_id,
            cursor: self.cursor,
            limit: self.limit,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DirectMessageQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

impl DirectMessageQuery {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        conversation_id: String,
    ) -> message_pb::ListMessagesRequest {
        message_pb::ListMessagesRequest {
            user_id,
            conversation_id,
            cursor: self.cursor,
            limit: self.limit,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct MarkConversationReadRequest {
    through_message_id: Option<String>,
}

impl MarkConversationReadRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
        conversation_id: String,
    ) -> message_pb::MarkConversationReadRequest {
        message_pb::MarkConversationReadRequest {
            user_id,
            conversation_id,
            through_message_id: self.through_message_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpdateDirectMessagePreferencesRequest {
    allow_direct_messages: bool,
}

impl UpdateDirectMessagePreferencesRequest {
    pub(crate) fn into_pb(
        self,
        user_id: String,
    ) -> message_pb::UpdateDirectMessagePreferencesRequest {
        message_pb::UpdateDirectMessagePreferencesRequest {
            user_id,
            allow_direct_messages: self.allow_direct_messages,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DirectMessageReportReason {
    Spam,
    Harassment,
    Unsafe,
    Fraud,
    Privacy,
    Other,
}

impl DirectMessageReportReason {
    fn from_pb(value: i32) -> Result<Self, String> {
        match message_pb::DirectMessageReportReason::try_from(value).ok() {
            Some(message_pb::DirectMessageReportReason::Spam) => Ok(Self::Spam),
            Some(message_pb::DirectMessageReportReason::Harassment) => Ok(Self::Harassment),
            Some(message_pb::DirectMessageReportReason::Unsafe) => Ok(Self::Unsafe),
            Some(message_pb::DirectMessageReportReason::Fraud) => Ok(Self::Fraud),
            Some(message_pb::DirectMessageReportReason::Privacy) => Ok(Self::Privacy),
            Some(message_pb::DirectMessageReportReason::Other) => Ok(Self::Other),
            None => Err(format!(
                "bbs-message returned unknown report reason {value}"
            )),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Spam => message_pb::DirectMessageReportReason::Spam as i32,
            Self::Harassment => message_pb::DirectMessageReportReason::Harassment as i32,
            Self::Unsafe => message_pb::DirectMessageReportReason::Unsafe as i32,
            Self::Fraud => message_pb::DirectMessageReportReason::Fraud as i32,
            Self::Privacy => message_pb::DirectMessageReportReason::Privacy as i32,
            Self::Other => message_pb::DirectMessageReportReason::Other as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DirectMessageReportStatus {
    Pending,
    Reviewing,
    Resolved,
    Rejected,
}

impl DirectMessageReportStatus {
    fn from_pb(value: i32) -> Result<Self, String> {
        match message_pb::DirectMessageReportStatus::try_from(value).ok() {
            Some(message_pb::DirectMessageReportStatus::Pending) => Ok(Self::Pending),
            Some(message_pb::DirectMessageReportStatus::Reviewing) => Ok(Self::Reviewing),
            Some(message_pb::DirectMessageReportStatus::Resolved) => Ok(Self::Resolved),
            Some(message_pb::DirectMessageReportStatus::Rejected) => Ok(Self::Rejected),
            None => Err(format!(
                "bbs-message returned unknown report status {value}"
            )),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::Pending => message_pb::DirectMessageReportStatus::Pending as i32,
            Self::Reviewing => message_pb::DirectMessageReportStatus::Reviewing as i32,
            Self::Resolved => message_pb::DirectMessageReportStatus::Resolved as i32,
            Self::Rejected => message_pb::DirectMessageReportStatus::Rejected as i32,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DirectMessageModerationAction {
    NoAction,
    RestrictSender,
}

impl DirectMessageModerationAction {
    fn from_pb(value: i32) -> Result<Self, String> {
        match message_pb::DirectMessageModerationAction::try_from(value).ok() {
            Some(message_pb::DirectMessageModerationAction::NoAction) => Ok(Self::NoAction),
            Some(message_pb::DirectMessageModerationAction::RestrictSender) => {
                Ok(Self::RestrictSender)
            }
            None => Err(format!(
                "bbs-message returned unknown moderation action {value}"
            )),
        }
    }

    fn into_pb(self) -> i32 {
        match self {
            Self::NoAction => message_pb::DirectMessageModerationAction::NoAction as i32,
            Self::RestrictSender => {
                message_pb::DirectMessageModerationAction::RestrictSender as i32
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateDirectMessageReportRequest {
    reason: DirectMessageReportReason,
    #[serde(default)]
    details: String,
}

impl CreateDirectMessageReportRequest {
    pub(crate) fn into_pb(
        self,
        reporter_user_id: String,
        message_id: String,
        idempotency_key: Option<String>,
    ) -> Result<message_pb::ReportDirectMessageRequest, String> {
        let idempotency_key = idempotency_key
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "Idempotency-Key is required when reporting a direct message".to_string()
            })?;
        Ok(message_pb::ReportDirectMessageRequest {
            reporter_user_id,
            message_id,
            idempotency_key: Some(idempotency_key),
            reason: self.reason.into_pb(),
            details: self.details,
        })
    }
}

/// This intentionally excludes the reported message body. Reporters receive a
/// receipt; full message context is reserved for trusted moderator endpoints.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct DirectMessageReportReceipt {
    id: String,
    message_id: String,
    reason: DirectMessageReportReason,
    status: DirectMessageReportStatus,
    created_at: String,
}

impl TryFrom<message_pb::DirectMessageReport> for DirectMessageReportReceipt {
    type Error = String;

    fn try_from(value: message_pb::DirectMessageReport) -> Result<Self, Self::Error> {
        let message_id = value
            .reported_message
            .as_ref()
            .map(|message| message.id.clone())
            .ok_or_else(|| "bbs-message report omitted its reported message".to_string())?;
        Ok(Self {
            id: value.id,
            message_id,
            reason: DirectMessageReportReason::from_pb(value.reason)?,
            status: DirectMessageReportStatus::from_pb(value.status)?,
            created_at: value.created_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct DirectMessageReport {
    id: String,
    reporter_user_id: String,
    reported_user_id: String,
    reported_message: DirectMessage,
    reason: DirectMessageReportReason,
    details: String,
    status: DirectMessageReportStatus,
    reviewer_user_id: Option<String>,
    resolution: Option<String>,
    action: DirectMessageModerationAction,
    created_at: String,
    updated_at: String,
}

impl TryFrom<message_pb::DirectMessageReport> for DirectMessageReport {
    type Error = String;

    fn try_from(value: message_pb::DirectMessageReport) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            reporter_user_id: value.reporter_user_id,
            reported_user_id: value.reported_user_id,
            reported_message: value
                .reported_message
                .ok_or_else(|| "bbs-message report omitted its reported message".to_string())?
                .try_into()?,
            reason: DirectMessageReportReason::from_pb(value.reason)?,
            details: value.details,
            status: DirectMessageReportStatus::from_pb(value.status)?,
            reviewer_user_id: value.reviewer_user_id,
            resolution: value.resolution,
            action: DirectMessageModerationAction::from_pb(value.action)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct DirectMessageReportPage {
    items: Vec<DirectMessageReport>,
    next_cursor: Option<String>,
}

impl TryFrom<message_pb::DirectMessageReportPage> for DirectMessageReportPage {
    type Error = String;

    fn try_from(value: message_pb::DirectMessageReportPage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ModerationDirectMessageReportQuery {
    status: Option<DirectMessageReportStatus>,
    cursor: Option<String>,
    limit: Option<u32>,
}

impl ModerationDirectMessageReportQuery {
    pub(crate) fn into_pb(self) -> message_pb::ListDirectMessageReportsRequest {
        message_pb::ListDirectMessageReportsRequest {
            status: self.status.map(DirectMessageReportStatus::into_pb),
            cursor: self.cursor,
            limit: self.limit,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ReviewDirectMessageReportRequest {
    status: DirectMessageReportStatus,
    resolution: String,
    action: DirectMessageModerationAction,
}

impl ReviewDirectMessageReportRequest {
    pub(crate) fn into_pb(
        self,
        reviewer_user_id: String,
        report_id: String,
    ) -> message_pb::ReviewDirectMessageReportRequest {
        message_pb::ReviewDirectMessageReportRequest {
            reviewer_user_id,
            report_id,
            status: self.status.into_pb(),
            resolution: self.resolution,
            action: self.action.into_pb(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpdateReminderPreferencesRequest {
    enabled: bool,
    lead_minutes: u32,
    timezone: String,
    quiet_hours_start: Option<String>,
    quiet_hours_end: Option<String>,
}

impl UpdateReminderPreferencesRequest {
    pub(crate) fn into_pb(self, user_id: String) -> growth_pb::UpdateReminderPreferencesRequest {
        growth_pb::UpdateReminderPreferencesRequest {
            user_id,
            enabled: self.enabled,
            lead_minutes: self.lead_minutes,
            timezone: self.timezone,
            quiet_hours_start: self.quiet_hours_start,
            quiet_hours_end: self.quiet_hours_end,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct NotificationQuery {
    cursor: Option<String>,
    limit: Option<u32>,
    unread_only: Option<bool>,
}

impl NotificationQuery {
    pub(crate) fn into_pb(self, user_id: String) -> growth_pb::NotificationQueryRequest {
        growth_pb::NotificationQueryRequest {
            user_id,
            cursor: self.cursor,
            limit: self.limit,
            unread_only: self.unread_only,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum NotificationKind {
    ActionReminder,
    Community,
    System,
}

impl NotificationKind {
    fn from_growth(value: i32) -> Result<Self, String> {
        match growth_pb::NotificationKind::try_from(value).ok() {
            Some(growth_pb::NotificationKind::ActionReminder) => Ok(Self::ActionReminder),
            Some(growth_pb::NotificationKind::Community) => Ok(Self::Community),
            Some(growth_pb::NotificationKind::System) => Ok(Self::System),
            None => Err(format!("growth returned unknown notification kind {value}")),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct UserNotification {
    id: String,
    kind: NotificationKind,
    source_id: String,
    title: String,
    body: String,
    data: std::collections::HashMap<String, String>,
    read_at: Option<String>,
    created_at: String,
}

impl TryFrom<growth_pb::UserNotification> for UserNotification {
    type Error = String;

    fn try_from(value: growth_pb::UserNotification) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            kind: NotificationKind::from_growth(value.kind)?,
            source_id: value.source_id,
            title: value.title,
            body: value.body,
            data: value.data,
            read_at: value.read_at,
            created_at: value.created_at,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct NotificationPage {
    items: Vec<UserNotification>,
    next_cursor: Option<String>,
    unread_count: u32,
}

impl TryFrom<growth_pb::NotificationPage> for NotificationPage {
    type Error = String;

    fn try_from(value: growth_pb::NotificationPage) -> Result<Self, Self::Error> {
        Ok(Self {
            items: value
                .items
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_cursor: value.next_cursor,
            unread_count: value.unread_count,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CreateMediaUploadRequest {
    mime_type: String,
    size_bytes: u64,
}

impl CreateMediaUploadRequest {
    pub(crate) fn into_pb(self, user_id: String) -> media_pb::CreateUploadRequest {
        media_pb::CreateUploadRequest {
            user_id,
            mime_type: self.mime_type,
            size_bytes: self.size_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use bookway_mall_order_api::pb as mall_order_pb;

    use super::{
        Action, AttachRouteNodeResourceRequest, CommentReportReceipt, Content, ContentAppeal,
        ContentType, CreateActionRequest, CreateCommentAppealRequest, CreateCommentReportRequest,
        CreateCommentRequest, CreateContentRequest, CreateDirectMessageReportRequest,
        CreateEntryRequest, CreateJourneyRequest, CreateMediaUploadRequest,
        DirectMessageReportReceipt, FeedQuery, FeedbackItem, FollowRequest, ForkRouteRequest,
        GrowthDomain, IngestEventsRequest, KnowledgeResource, ModerationCommentQuery,
        NotificationPage, OwnCommentAppealQuery, PublicNodeOfferList, PublicResource, Reaction,
        ResourceSearchQuery, MerchantOrderView, ReviewCommentReportRequest, ReviewCommentRequest,
        ReviewDirectMessageReportRequest, RouteNodeRagContextRequest,
        RouteNodeResourceAttachment, RouteParticipationRequest, RouteTemplate, RouteTemplateKind,
        SaveWeeklyReviewRequest, SearchQuery, SetReactionRequest, SetRelationshipRequest,
        StartKnowledgeJourneyRequest, UpdateAccountProfileRequest,
        UpdateReminderPreferencesRequest, UpsertResourceRequest,
    };
    use bookway_bbs_link_api::pb as bbs_link_pb;
    use bookway_bbs_message_api::pb as message_pb;
    use bookway_comment_api::pb as comment_pb;
    use bookway_content_audit_api::pb as audit_pb;
    use bookway_feedback_api::pb as feedback_pb;
    use bookway_growth_api::pb as growth_pb;
    use bookway_interaction_status_api::pb as like_pb;
    use bookway_knowledge_catalog_api::pb as catalog_pb;
    use bookway_mall_api::pb as mall_pb;

    #[test]
    fn content_creation_accepts_mobile_string_enums() {
        let request: CreateContentRequest = serde_json::from_value(serde_json::json!({
            "title": "完成 10 公里徒步",
            "summary": "适合周末",
            "body": "从简单的一步开始。",
            "domain": "travel",
            "content_type": "route",
            "tags": ["徒步"],
            "topics": ["周末"],
            "route_template": {
                "intent": "走完这条路线",
                "completion_criteria": "完成全部行动",
                "stages": [],
                "actions": [],
                "journey_type": "project"
            }
        }))
        .expect("mobile route JSON should deserialize");

        let request = request.into_pb("user-1".to_string(), None);
        assert_eq!(request.domain, bbs_link_pb::GrowthDomain::Travel as i32);
        assert_eq!(request.content_type, bbs_link_pb::ContentType::Route as i32);
        assert_eq!(
            request.route_template.expect("route template").journey_type,
            bbs_link_pb::RouteTemplateKind::Project as i32
        );
    }

    #[test]
    fn legacy_cover_url_is_rejected_at_the_rest_boundary() {
        let result = serde_json::from_value::<CreateContentRequest>(serde_json::json!({
            "title": "路线",
            "summary": "摘要",
            "body": "正文",
            "domain": "learning",
            "content_type": "note",
            "cover_url": "https://legacy.example/cover.jpg",
            "tags": [],
            "topics": []
        }));
        assert!(result.is_err(), "deprecated cover_url must not be accepted");
    }

    #[test]
    fn milestone_requests_and_responses_keep_route_snapshots_server_owned() {
        let request: CreateContentRequest = serde_json::from_value(serde_json::json!({
            "title": "第一周的阅读成果",
            "summary": "找到适合自己的整理节奏",
            "body": "每天二十分钟，周末统一整理。",
            "domain": "learning",
            "content_type": "milestone",
            "milestone": {
                "route_id": "public-route-1",
                "stage_index": 0,
                "effort_summary": "连续七天阅读并完成两次整理",
                "outcome_summary": "能够复述一个完整主题",
                "adjustment_summary": "把零散整理改到周末",
                "evidence_scope": "本帖图文覆盖本周公开产出"
            }
        }))
        .expect("mobile milestone JSON should deserialize");

        let request = request.into_pb("user-1".to_string(), None);
        assert_eq!(
            request.content_type,
            bbs_link_pb::ContentType::Milestone as i32
        );
        let draft = request.milestone.expect("milestone draft");
        assert_eq!(draft.route_id, "public-route-1");
        assert_eq!(draft.stage_index, Some(0));

        let content = Content::try_from(bbs_link_pb::Content {
            post: Some(bbs_link_pb::PostSummary {
                id: "milestone-1".to_string(),
                title: "第一周的阅读成果".to_string(),
                domain: bbs_link_pb::GrowthDomain::Learning as i32,
                is_milestone: true,
                fork_count: 0,
                ..Default::default()
            }),
            content_type: bbs_link_pb::ContentType::Milestone as i32,
            milestone: Some(bbs_link_pb::Milestone {
                route_id: "public-route-1".to_string(),
                route_title: "四周主题阅读".to_string(),
                stage_index: 0,
                stage_title: "起步".to_string(),
                effort_summary: "连续七天阅读并完成两次整理".to_string(),
                outcome_summary: "能够复述一个完整主题".to_string(),
                adjustment_summary: "把零散整理改到周末".to_string(),
                evidence_scope: "本帖图文覆盖本周公开产出".to_string(),
            }),
            ..Default::default()
        })
        .expect("valid milestone response");
        let json = serde_json::to_value(content).expect("milestone JSON");
        assert_eq!(json["content_type"], "milestone");
        assert_eq!(json["post"]["is_milestone"], true);
        assert_eq!(json["milestone"]["route_title"], "四周主题阅读");
    }

    #[test]
    fn question_context_requests_and_responses_keep_public_route_snapshots_server_owned() {
        let request: CreateContentRequest = serde_json::from_value(serde_json::json!({
            "title": "第一阶段总是拖延，怎么调整？",
            "summary": "想先把开始变得更轻。",
            "body": "我只想讨论公开路线的第一阶段。",
            "domain": "learning",
            "content_type": "question",
            "question_context": { "route_id": "public-route-1", "stage_index": 0 }
        }))
        .expect("mobile question JSON should deserialize");
        let request = request.into_pb("user-1".to_string(), None);
        let draft = request.question_context.expect("question context draft");
        assert_eq!(draft.route_id, "public-route-1");
        assert_eq!(draft.stage_index, Some(0));

        let content = Content::try_from(bbs_link_pb::Content {
            content_type: bbs_link_pb::ContentType::Question as i32,
            question_context: Some(bbs_link_pb::QuestionContext {
                route_id: "public-route-1".to_string(),
                route_title: "四周主题阅读".to_string(),
                stage_index: Some(0),
                stage_title: Some("起步".to_string()),
            }),
            ..Default::default()
        })
        .expect("valid question response");
        let json = serde_json::to_value(content).expect("question JSON");
        assert_eq!(json["question_context"]["route_title"], "四周主题阅读");
        assert_eq!(json["question_context"]["stage_title"], "起步");
    }

    #[test]
    fn fork_request_and_provenance_keep_ownership_fields_server_owned() {
        let request: ForkRouteRequest = serde_json::from_value(serde_json::json!({
            "title": "我的阅读分支",
            "summary": "从周末开始",
            "user_id": "attacker",
            "source_route_id": "attacker-route",
            "idempotency_key": "attacker-key"
        }))
        .expect("fork JSON should deserialize");
        let request = request.into_pb(
            "member-1".to_string(),
            "public-route-1".to_string(),
            "fork-1".to_string(),
        );
        assert_eq!(request.user_id, "member-1");
        assert_eq!(request.source_route_id, "public-route-1");
        assert_eq!(request.idempotency_key, "fork-1");

        let content = Content::try_from(bbs_link_pb::Content {
            content_type: bbs_link_pb::ContentType::Route as i32,
            route_fork: Some(bbs_link_pb::RouteFork {
                source_route_id: "public-route-1".to_string(),
                source_route_version: 4,
                source_route_title: "四周主题阅读".to_string(),
                source_route_author_id: "author-1".to_string(),
                forked_at: "2026-08-18T00:00:00Z".to_string(),
            }),
            ..Default::default()
        })
        .expect("valid route fork response");
        let json = serde_json::to_value(content).expect("route fork JSON");
        assert_eq!(json["route_fork"]["source_route_id"], "public-route-1");
        assert_eq!(json["route_fork"]["source_route_version"], 4);
        assert_eq!(json["route_fork"]["source_route_title"], "四周主题阅读");
    }

    #[test]
    fn merchant_order_view_never_carries_buyer_identity() {
        let order = mall_order_pb::Order {
            id: "order-1".to_string(),
            user_id: "buyer-secret".to_string(),
            payment_reference: Some("pay-secret".to_string()),
            affiliate_creator_id: "creator-1".to_string(),
            items: vec![mall_order_pb::OrderLine {
                title: "轻量背包".to_string(),
                quantity: 1,
                line_total_cents: 32_900,
                ..Default::default()
            }],
            ..Default::default()
        };

        let view = serde_json::to_value(
            MerchantOrderView::try_from(order).expect("merchant projection"),
        )
        .expect("serializable view");
        assert!(
            view.get("user_id").is_none() && view.get("payment_reference").is_none(),
            "buyer identity fields must not leak into merchant payloads"
        );
        assert_eq!(view["items"][0]["title"], "轻量背包");
        assert_eq!(view["affiliate_creator_id"], "creator-1");
    }

    #[test]
    fn public_node_offers_never_leak_commission_or_merchant_identity() {
        let offers = PublicNodeOfferList::try_from(mall_pb::NodeOfferList {
            items: vec![mall_pb::NodeOffer {
                id: "offer-1".to_string(),
                product_id: "product-1".to_string(),
                sku_id: "sku-1".to_string(),
                route_id: "route-1".to_string(),
                action_node_id: "node-1".to_string(),
                creator_id: "creator-secret".to_string(),
                commission_bps: 800,
                merchant_id: "merchant-secret".to_string(),
                scene_equipment: "trail".to_string(),
                created_at: "2026-08-16T00:00:00Z".to_string(),
                product: Some(mall_pb::MallProduct {
                    id: "product-1".to_string(),
                    title: "轻量背包".to_string(),
                    description: "适合周末徒步".to_string(),
                    image_url: "https://cdn.example/backpack.jpg".to_string(),
                    product_kind: mall_pb::MallProductKind::Physical as i32,
                    skus: vec![mall_pb::MallSku {
                        id: "sku-1".to_string(),
                        title: "标准款".to_string(),
                        price_cents: 32_900,
                        currency: "CNY".to_string(),
                        ..Default::default()
                    }],
                    course_resource_id: String::new(),
                    ..Default::default()
                }),
            }],
        })
        .expect("public offer projection");

        let json = serde_json::to_value(offers).expect("serializable offers");
        let offer = &json["items"][0];
        for secret_field in ["creator_id", "merchant_id", "commission_bps"] {
            assert!(
                offer.get(secret_field).is_none(),
                "public offers must not leak {secret_field}"
            );
        }
        // The storefront keeps the facts it needs to sell.
        assert_eq!(offer["sku_id"], "sku-1");
        assert_eq!(offer["scene_equipment"], "trail");
        assert_eq!(offer["product"]["title"], "轻量背包");
        assert_eq!(offer["product"]["skus"][0]["price_cents"], 32_900);
        assert_eq!(offer["product"]["skus"][0]["currency"], "CNY");
    }

    #[test]
    fn creator_settlement_queries_take_filters_but_never_the_identity() {
        let query: mall_order_pb::CreatorSettlementRequest =
            serde_json::from_value(serde_json::json!({
                "creator_id": "spoofed-creator",
                "status": 1,
                "cursor": "settle-10",
                "limit": 20
            }))
            .expect("creator settlement query");
        // Filters travel through the query string; the handler overwrites
        // creator_id from the authenticated identity, so a spoofed value in
        // the request is dead on arrival.
        assert_eq!(query.creator_id, "spoofed-creator");
        assert_eq!(query.status, Some(1));
        assert_eq!(query.cursor.as_deref(), Some("settle-10"));
        assert_eq!(query.limit, Some(20));
    }

    #[test]
    fn content_response_serializes_named_enums() {
        let content = Content::try_from(bbs_link_pb::Content {
            id: "post-1".to_string(),
            post: Some(bbs_link_pb::PostSummary {
                id: "post-1".to_string(),
                author_name: "阿明".to_string(),
                author_avatar_url: String::new(),
                title: "一条路线".to_string(),
                summary: "从这里开始".to_string(),
                domain: bbs_link_pb::GrowthDomain::Learning as i32,
                cover_url: String::new(),
                route_title: "一条路线".to_string(),
                route_duration: "4 周".to_string(),
                join_count: 3,
                like_count: 5,
                freshness: 0.8,
                tags: vec!["专注".to_string()],
                is_route: true,
                is_milestone: false,
                is_question: false,
                fork_count: 2,
            }),
            author_id: "user-1".to_string(),
            content_type: bbs_link_pb::ContentType::Route as i32,
            status: bbs_link_pb::ContentStatus::Published as i32,
            body: "内容".to_string(),
            media: Vec::new(),
            topics: Vec::new(),
            created_at: "2026-08-16T00:00:00Z".to_string(),
            published_at: None,
            version: 1,
            quality_score: 1.0,
            route_template: Some(bbs_link_pb::RouteTemplate {
                intent: "完成".to_string(),
                completion_criteria: "完成全部".to_string(),
                stages: Vec::new(),
                actions: Vec::new(),
                journey_type: bbs_link_pb::RouteTemplateKind::Project as i32,
            }),
            milestone: None,
            accepted_answer_id: None,
            question_context: None,
            route_fork: None,
        })
        .expect("valid protobuf content");

        let json = serde_json::to_value(content).expect("content JSON");
        assert_eq!(json["content_type"], "route");
        assert_eq!(json["status"], "published");
        assert_eq!(json["post"]["domain"], "learning");
        assert_eq!(json["route_template"]["journey_type"], "project");
    }

    #[test]
    fn feed_and_search_queries_accept_mobile_enum_names() {
        let feed: FeedQuery = serde_json::from_value(serde_json::json!({
            "interests": "learning,movement,travel",
            "session_id": "session-1",
            "surface": "home"
        }))
        .expect("feed query JSON");
        let feed = feed.into_pb("user-1".to_string()).expect("feed request");
        assert_eq!(
            feed.interests,
            vec![
                bbs_link_pb::GrowthDomain::Learning as i32,
                bbs_link_pb::GrowthDomain::Movement as i32,
                bbs_link_pb::GrowthDomain::Travel as i32,
            ]
        );

        let contextual_feed: FeedQuery = serde_json::from_value(serde_json::json!({
            "route_id": "route-1",
            "action_node_id": "node-1",
            "placement": "route_feed",
            "action_domain": "movement"
        }))
        .expect("contextual feed query JSON");
        let contextual_feed = contextual_feed
            .into_pb("user-1".to_string())
            .expect("contextual feed request");
        assert_eq!(
            contextual_feed
                .action_context
                .as_ref()
                .map(|context| context.action_node_id.as_str()),
            Some("node-1")
        );

        let incomplete_context: FeedQuery = serde_json::from_value(serde_json::json!({
            "route_id": "route-1"
        }))
        .expect("incomplete feed query JSON");
        assert!(incomplete_context.into_pb("user-1".to_string()).is_err());

        let search: SearchQuery = serde_json::from_value(serde_json::json!({
            "q": "晨跑",
            "search_type": "all"
        }))
        .expect("search query JSON");
        assert_eq!(search.into_pb("user-1".to_string()).search_type, 0);
    }

    #[test]
    fn named_enums_are_exhaustive_at_the_rest_boundary() {
        assert_eq!(GrowthDomain::Learning.into_link(), 0);
        assert_eq!(ContentType::Route.into_link(), 3);
        assert_eq!(RouteTemplateKind::Project.into_link(), 0);
        assert!(
            RouteTemplate::try_from(bbs_link_pb::RouteTemplate {
                intent: String::new(),
                completion_criteria: String::new(),
                stages: Vec::new(),
                actions: Vec::new(),
                journey_type: 99,
            })
            .is_err()
        );
    }

    #[test]
    fn journey_creation_accepts_named_enums_and_recurrence() {
        let request: CreateJourneyRequest = serde_json::from_value(serde_json::json!({
            "title": "晨间阅读",
            "intent": "每天留出安静阅读时间",
            "domain": "learning",
            "journey_type": "habit",
            "completion_criteria": "连续完成 21 天",
            "stages": [{
                "title": "第一周",
                "detail": "每天阅读",
                "completion_criteria": "完成 7 天"
            }],
            "duration_label": "21 天",
            "first_action_title": "读十分钟",
            "first_action_detail": "从手边的书开始",
            "estimated_minutes": 10,
            "first_action_recurrence": {
                "frequency": "weekly",
                "interval": 1,
                "weekdays": ["monday", "wednesday"],
                "anchor_date": "2026-08-16"
            }
        }))
        .expect("mobile journey JSON should deserialize");

        let request = request
            .into_pb("user-1".to_string(), Some("journey-create-1".to_string()))
            .expect("valid recurrence");
        assert_eq!(request.domain, growth_pb::GrowthDomain::Learning as i32);
        assert_eq!(request.journey_type, growth_pb::JourneyType::Habit as i32);
        assert_eq!(request.idempotency_key.as_deref(), Some("journey-create-1"));
        let recurrence = request.first_action_recurrence.expect("recurrence");
        assert_eq!(
            recurrence.frequency,
            growth_pb::ActionRecurrenceFrequency::Weekly as i32
        );
        assert_eq!(
            recurrence.weekdays,
            vec![
                growth_pb::Weekday::Monday as i32,
                growth_pb::Weekday::Wednesday as i32
            ]
        );
    }

    #[test]
    fn knowledge_journey_creation_uses_the_path_resource_and_gateway_identity() {
        let request: StartKnowledgeJourneyRequest = serde_json::from_value(serde_json::json!({
            "title": "把阅读变成步行观察",
            "intent": "用一周验证一个观察方法",
            "domain": "learning",
            "journey_type": "project",
            "completion_criteria": "完成三次观察记录",
            "stages": [],
            "duration_label": "一周",
            "first_action_title": "步行观察 15 分钟",
            "first_action_detail": "带着一个问题出门。",
            "estimated_minutes": 15
        }))
        .expect("mobile knowledge journey JSON should deserialize");

        let request = request
            .into_pb("user-1".to_string(), "resource-1".to_string())
            .expect("valid knowledge journey request");
        assert_eq!(request.user_id, "user-1");
        assert_eq!(request.resource_id, "resource-1");
        assert_eq!(request.domain, growth_pb::GrowthDomain::Learning as i32);
        assert_eq!(request.journey_type, growth_pb::JourneyType::Project as i32);
        assert_eq!(request.first_action_title, "步行观察 15 分钟");
    }

    #[test]
    fn action_response_serializes_named_state_and_recurrence() {
        let action = Action::try_from(growth_pb::Action {
            id: "action-1".to_string(),
            journey_id: "journey-1".to_string(),
            stage_id: None,
            title: "读十分钟".to_string(),
            detail: "从手边的书开始".to_string(),
            estimated_minutes: 10,
            scheduled_label: "今天".to_string(),
            scheduled_for: None,
            scheduled_timezone: None,
            recurrence: Some(growth_pb::ActionRecurrence {
                frequency: growth_pb::ActionRecurrenceFrequency::Daily as i32,
                interval: 1,
                weekdays: vec![growth_pb::Weekday::Monday as i32],
                ends_on: None,
                anchor_date: None,
            }),
            state: growth_pb::ActionState::Pending as i32,
        })
        .expect("valid action");

        let json = serde_json::to_value(action).expect("action JSON");
        assert_eq!(json["state"], "pending");
        assert_eq!(json["recurrence"]["frequency"], "daily");
        assert_eq!(json["recurrence"]["weekdays"][0], "monday");
    }

    #[test]
    fn knowledge_response_serializes_named_kind_and_status() {
        let resource = KnowledgeResource::try_from(growth_pb::KnowledgeResource {
            id: "resource-1".to_string(),
            title: "一本书".to_string(),
            creator: "作者".to_string(),
            summary: "摘要".to_string(),
            kind: growth_pb::KnowledgeResourceKind::Book as i32,
            status: growth_pb::KnowledgeResourceStatus::Active as i32,
            source_url: None,
            body: None,
            tags: vec!["阅读".to_string()],
            journey_id: None,
            progress: 20,
            current_position: 3,
            reading_seconds: 120,
            bookmarks: Vec::new(),
            created_at: "2026-08-16T00:00:00Z".to_string(),
            updated_at: "2026-08-16T00:00:00Z".to_string(),
            last_opened_at: None,
            source_content_id: Some("post-1".to_string()),
        })
        .expect("valid knowledge resource");

        let json = serde_json::to_value(resource).expect("knowledge JSON");
        assert_eq!(json["kind"], "book");
        assert_eq!(json["status"], "active");
        assert_eq!(json["source_content_id"], "post-1");
    }

    #[test]
    fn feedback_response_serializes_named_category_and_status() {
        let feedback = FeedbackItem::try_from(feedback_pb::FeedbackItem {
            id: "feedback-1".to_string(),
            user_id: "user-1".to_string(),
            category: feedback_pb::FeedbackCategory::Experience as i32,
            content: "体验反馈".to_string(),
            contact: String::new(),
            platform: "ios".to_string(),
            app_version: "1.0".to_string(),
            status: feedback_pb::FeedbackStatus::Processing as i32,
            resolution: None,
            created_at: "2026-08-16T00:00:00Z".to_string(),
            updated_at: "2026-08-16T00:00:00Z".to_string(),
        })
        .expect("valid feedback");

        let json = serde_json::to_value(feedback).expect("feedback JSON");
        assert_eq!(json["category"], "experience");
        assert_eq!(json["status"], "processing");
    }

    #[test]
    fn reaction_and_comment_requests_do_not_trust_client_owned_ids() {
        let reaction: SetReactionRequest = serde_json::from_value(serde_json::json!({
            "reaction": "bookmark",
            "active": true
        }))
        .expect("mobile reaction JSON should deserialize");
        let (reaction, reason, attribution) = reaction
            .into_pb("user-1".to_string(), "post-1".to_string())
            .expect("bookmark request should be valid");
        assert_eq!(reaction.user_id, "user-1");
        assert_eq!(reaction.post_id, "post-1");
        assert_eq!(reaction.reaction, like_pb::ReactionType::Bookmark as i32);
        assert_eq!(reason, None);
        assert!(attribution.is_none());

        let hide: SetReactionRequest = serde_json::from_value(serde_json::json!({
            "reaction": "hide",
            "active": true,
            "negative_feedback_reason": "low_quality",
            "attribution": {
                "session_id": "01980000-0000-7000-8000-000000000001",
                "request_id": "01980000-0000-7000-8000-000000000002",
                "position": 2,
                "attribution_source": "search"
            }
        }))
        .expect("typed hide feedback should deserialize");
        let (hide, reason, attribution) = hide
            .into_pb("user-1".to_string(), "post-1".to_string())
            .expect("hide feedback should be valid");
        assert_eq!(hide.reaction, like_pb::ReactionType::Hide as i32);
        assert_eq!(
            reason,
            Some(bookway_user_event_api::pb::NegativeFeedbackReason::LowQuality)
        );
        let attribution = attribution.expect("served result context is retained for validation");
        assert_eq!(attribution.position, 2);
        assert_eq!(
            attribution.source,
            bookway_user_event_api::pb::AttributionSource::Search
        );

        let invalid: SetReactionRequest = serde_json::from_value(serde_json::json!({
            "reaction": "like",
            "active": true,
            "negative_feedback_reason": "not_relevant"
        }))
        .expect("the transport shape itself is valid");
        assert!(
            invalid
                .into_pb("user-1".to_string(), "post-1".to_string())
                .is_err()
        );

        let comment: CreateCommentRequest = serde_json::from_value(serde_json::json!({
            "body": "很有帮助",
            "parent_id": "comment-1"
        }))
        .expect("mobile comment JSON should deserialize");
        let comment = comment.into_pb(
            "user-1".to_string(),
            "post-1".to_string(),
            Some("request-1".to_string()),
        );
        assert_eq!(comment.user_id, "user-1");
        assert_eq!(comment.post_id, "post-1");
        assert_eq!(comment.parent_id.as_deref(), Some("comment-1"));
    }

    #[test]
    fn moderation_comment_review_uses_the_trusted_reviewer_and_path_comment() {
        let review: ReviewCommentRequest = serde_json::from_value(serde_json::json!({
            "decision": "approve"
        }))
        .expect("moderation review JSON should deserialize");
        let review = review.into_pb("moderator-1".to_string(), "comment-1".to_string());

        assert_eq!(review.reviewer_id, "moderator-1");
        assert_eq!(review.comment_id, "comment-1");
        assert_eq!(
            review.decision,
            comment_pb::CommentModerationDecision::Approve as i32
        );

        let query: ModerationCommentQuery = serde_json::from_value(serde_json::json!({
            "cursor": "2026-08-16T00:00:00Z|comment-1",
            "limit": 20
        }))
        .expect("moderation queue query should deserialize");
        let query = query.into_pb();
        assert_eq!(query.limit, Some(20));
        assert_eq!(
            query.cursor.as_deref(),
            Some("2026-08-16T00:00:00Z|comment-1")
        );
    }

    #[test]
    fn action_creation_uses_gateway_identity_and_idempotency_header() {
        let action: CreateActionRequest = serde_json::from_value(serde_json::json!({
            "title": "整理读书笔记",
            "detail": "只保留一个可执行的结论",
            "estimated_minutes": 15,
            "scheduled_label": "今晚"
        }))
        .expect("mobile action JSON should deserialize");
        let action = action
            .into_pb(
                "user-1".to_string(),
                "journey-1".to_string(),
                Some("action-create-1".to_string()),
            )
            .expect("valid action request");

        assert_eq!(action.user_id, "user-1");
        assert_eq!(action.journey_id, "journey-1");
        assert_eq!(action.idempotency_key.as_deref(), Some("action-create-1"));
    }

    #[test]
    fn weekly_review_save_uses_gateway_identity() {
        let request: SaveWeeklyReviewRequest = serde_json::from_value(serde_json::json!({
            "reflection": "午间阅读更容易坚持",
            "nextFocus": "每天先完成二十分钟阅读"
        }))
        .expect("mobile review JSON should deserialize");
        let request = request.into_pb("user-1".to_string());

        assert_eq!(request.user_id, "user-1");
        assert_eq!(request.reflection, "午间阅读更容易坚持");
        assert_eq!(request.next_focus, "每天先完成二十分钟阅读");
    }

    #[test]
    fn entry_creation_uses_gateway_identity_and_idempotency_header() {
        let entry: CreateEntryRequest = serde_json::from_value(serde_json::json!({
            "body": "今天把一个困惑写清楚了。",
            "mood": "clear",
            "published": true
        }))
        .expect("mobile entry JSON should deserialize");
        let entry = entry.into_pb("user-1".to_string(), Some("entry-create-1".to_string()));

        assert_eq!(entry.user_id, "user-1");
        assert_eq!(entry.idempotency_key.as_deref(), Some("entry-create-1"));
    }

    #[test]
    fn community_responses_serialize_stable_named_values() {
        let reaction = Reaction::try_from(like_pb::Reaction {
            target_id: "post-1".to_string(),
            target_type: "post".to_string(),
            reaction: like_pb::ReactionType::Hide as i32,
            active: true,
            count: 1,
        })
        .expect("valid reaction");
        assert_eq!(
            serde_json::to_value(reaction).expect("reaction JSON")["reaction"],
            "hide"
        );

        let appeal = ContentAppeal::try_from(audit_pb::ContentAppeal {
            id: "appeal-1".to_string(),
            content_id: "post-1".to_string(),
            appellant_id: "user-1".to_string(),
            details: "请复核".to_string(),
            status: audit_pb::AppealStatus::Reviewing as i32,
            assignee_id: None,
            resolution: None,
            action: audit_pb::ContentAction::Restrict as i32,
            created_at: "2026-08-16T00:00:00Z".to_string(),
            updated_at: "2026-08-16T00:00:00Z".to_string(),
        })
        .expect("valid appeal");
        let json = serde_json::to_value(appeal).expect("appeal JSON");
        assert_eq!(json["status"], "reviewing");
        assert_eq!(json["action"], "restrict_content");
    }

    #[test]
    fn social_and_event_requests_are_scoped_by_gateway_identity() {
        let follow: FollowRequest = serde_json::from_value(serde_json::json!({ "active": true }))
            .expect("mobile follow JSON should deserialize");
        let follow = follow.into_pb("user-1".to_string(), "author-1".to_string());
        assert_eq!(follow.user_id, "user-1");
        assert_eq!(follow.target_user_id, "author-1");
        assert_eq!(
            follow.edge,
            bookway_bbs_api::pb::SocialEdgeType::Follow as i32
        );

        let relationship: SetRelationshipRequest = serde_json::from_value(serde_json::json!({
            "edge": "block",
            "active": true
        }))
        .expect("mobile relationship JSON should deserialize");
        let relationship = relationship.into_pb("user-1".to_string(), "author-1".to_string());
        assert_eq!(
            relationship.edge,
            bookway_bbs_api::pb::SocialEdgeType::Block as i32
        );

        let route: RouteParticipationRequest = serde_json::from_value(serde_json::json!({
            "active": true,
            "private_journey_id": "journey-1"
        }))
        .expect("route participation JSON should deserialize");
        let route = route.into_pb("user-1".to_string(), "route-1".to_string());
        assert_eq!(route.user_id, "user-1");
        assert_eq!(route.route_id, "route-1");

        let events: IngestEventsRequest = serde_json::from_value(serde_json::json!({
            "events": [{
                "event_id": "event-1",
                "event_type": "hide",
                "session_id": "session-1",
                "request_id": "request-1",
                "component_id": "feed-card",
                "content_id": "post-1",
                "position": 2,
                "occurred_at": "2026-08-16T00:00:00Z",
                "source": "mobile",
                "attribution_source": "recommendation",
                "negative_feedback_reason": "already_seen"
            }]
        }))
        .expect("mobile event JSON should deserialize");
        let events = events
            .into_pb("user-1".to_string())
            .expect("client event request should convert");
        assert_eq!(events.user_id, "user-1");
        assert_eq!(
            events.events[0].attribution_source,
            bookway_user_event_api::pb::AttributionSource::Recommendation as i32
        );
        assert_eq!(
            events.events[0].negative_feedback_reason,
            Some(bookway_user_event_api::pb::NegativeFeedbackReason::AlreadySeen as i32)
        );

        let reserved_source = serde_json::from_value::<IngestEventsRequest>(serde_json::json!({
            "events": [{
                "event_id": "event-reserved-source",
                "event_type": "complete",
                "session_id": "session-1",
                "component_id": "route-action",
                "content_id": "route-1",
                "occurred_at": "2026-08-16T00:00:00Z",
                "source": " gateway-route-completion "
            }]
        }))
        .expect("reserved-source payload should deserialize");
        assert!(reserved_source.into_pb("user-1".to_string()).is_err());

        let fabricated_purchase =
            serde_json::from_value::<IngestEventsRequest>(serde_json::json!({
                "events": [{
                    "event_id": "event-purchase",
                    "event_type": "purchase",
                    "session_id": "session-1",
                    "component_id": "checkout",
                    "occurred_at": "2026-08-16T00:00:00Z",
                    "source": "mobile"
                }]
            }));
        assert!(fabricated_purchase.is_err());
    }

    #[test]
    fn account_and_reminder_requests_use_gateway_identity() {
        let profile: UpdateAccountProfileRequest = serde_json::from_value(serde_json::json!({
            "display_name": "阿明"
        }))
        .expect("profile JSON should deserialize");
        assert_eq!(profile.into_pb("user-1".to_string()).user_id, "user-1");

        let reminder: UpdateReminderPreferencesRequest =
            serde_json::from_value(serde_json::json!({
                "enabled": true,
                "lead_minutes": 15,
                "timezone": "Asia/Shanghai"
            }))
            .expect("reminder JSON should deserialize");
        assert_eq!(reminder.into_pb("user-1".to_string()).user_id, "user-1");

        let upload: CreateMediaUploadRequest = serde_json::from_value(serde_json::json!({
            "mime_type": "image/jpeg",
            "size_bytes": 1024
        }))
        .expect("upload JSON should deserialize");
        let upload = upload.into_pb("user-1".to_string());
        assert_eq!(upload.user_id, "user-1");
        assert_eq!(upload.mime_type, "image/jpeg");
    }

    #[test]
    fn notification_response_serializes_named_kind() {
        let page = NotificationPage::try_from(growth_pb::NotificationPage {
            items: vec![growth_pb::UserNotification {
                id: "notice-1".to_string(),
                kind: growth_pb::NotificationKind::Community as i32,
                source_id: "post-1".to_string(),
                title: "收到一个赞".to_string(),
                body: "有人赞了你的内容".to_string(),
                data: Default::default(),
                read_at: None,
                created_at: "2026-08-16T00:00:00Z".to_string(),
            }],
            next_cursor: None,
            unread_count: 1,
        })
        .expect("valid notification page");
        assert_eq!(
            serde_json::to_value(page).expect("notification JSON")["items"][0]["kind"],
            "community"
        );
    }

    #[test]
    fn direct_message_reports_use_gateway_identity_and_hide_message_content_from_receipts() {
        let request: CreateDirectMessageReportRequest = serde_json::from_value(serde_json::json!({
            "reason": "harassment",
            "details": "unwanted contact"
        }))
        .expect("report JSON should deserialize");
        let request = request
            .into_pb(
                "recipient-1".to_string(),
                "message-1".to_string(),
                Some("report-key-1".to_string()),
            )
            .expect("report request should require idempotency");
        assert_eq!(request.reporter_user_id, "recipient-1");
        assert_eq!(request.message_id, "message-1");
        assert_eq!(request.idempotency_key.as_deref(), Some("report-key-1"));

        let receipt = DirectMessageReportReceipt::try_from(message_pb::DirectMessageReport {
            id: "report-1".to_string(),
            reporter_user_id: "recipient-1".to_string(),
            reported_user_id: "sender-1".to_string(),
            reported_message: Some(message_pb::DirectMessage {
                id: "message-1".to_string(),
                conversation_id: "conversation-1".to_string(),
                sender_user_id: "sender-1".to_string(),
                recipient_user_id: "recipient-1".to_string(),
                kind: message_pb::DirectMessageKind::Text as i32,
                body: "private message body".to_string(),
                created_at: "2026-08-16T00:00:00Z".to_string(),
                read_at: None,
            }),
            reason: message_pb::DirectMessageReportReason::Harassment as i32,
            details: "unwanted contact".to_string(),
            status: message_pb::DirectMessageReportStatus::Pending as i32,
            reviewer_user_id: None,
            resolution: None,
            action: message_pb::DirectMessageModerationAction::NoAction as i32,
            created_at: "2026-08-16T00:00:00Z".to_string(),
            updated_at: "2026-08-16T00:00:00Z".to_string(),
        })
        .expect("report receipt");
        let json = serde_json::to_value(receipt).expect("receipt JSON");
        assert_eq!(json["reason"], "harassment");
        assert!(json.get("reported_message").is_none());
        assert!(!json.to_string().contains("private message body"));
    }

    #[test]
    fn direct_message_moderation_review_uses_trusted_reviewer_and_path_report() {
        let review: ReviewDirectMessageReportRequest = serde_json::from_value(serde_json::json!({
            "status": "resolved",
            "resolution": "restricted after corroboration",
            "action": "restrict_sender"
        }))
        .expect("moderation review JSON should deserialize");
        let review = review.into_pb("moderator-1".to_string(), "report-1".to_string());
        assert_eq!(review.reviewer_user_id, "moderator-1");
        assert_eq!(review.report_id, "report-1");
        assert_eq!(
            review.action,
            message_pb::DirectMessageModerationAction::RestrictSender as i32
        );
    }

    #[test]
    fn comment_reports_hide_body_from_receipts_and_require_gateway_identity() {
        let request: CreateCommentReportRequest = serde_json::from_value(serde_json::json!({
            "reason": "harassment",
            "details": "repeated insults"
        }))
        .expect("comment report JSON");
        let request = request
            .into_pb(
                "post-1".to_string(),
                "comment-1".to_string(),
                Some("comment-report-key-1".to_string()),
            )
            .expect("report request should require idempotency");
        assert!(request.reporter_id.is_empty());
        assert_eq!(request.post_id, "post-1");
        assert_eq!(request.comment_id, "comment-1");
        assert_eq!(
            request.idempotency_key.as_deref(),
            Some("comment-report-key-1")
        );

        let receipt = CommentReportReceipt::try_from(comment_pb::CommentReport {
            id: "report-1".to_string(),
            reporter_id: "reader-1".to_string(),
            reported_comment: Some(comment_pb::CommentItem {
                id: "comment-1".to_string(),
                post_id: "post-1".to_string(),
                author_id: "author-1".to_string(),
                author_name: "author-1".to_string(),
                body: "comment body that reporters must not receive".to_string(),
                parent_id: None,
                like_count: 0,
                created_at: "2026-08-16T00:00:00Z".to_string(),
                status: comment_pb::CommentStatus::Published as i32,
            }),
            reason: comment_pb::CommentReportReason::Harassment as i32,
            details: "repeated insults".to_string(),
            status: comment_pb::CommentReportStatus::Pending as i32,
            reviewer_id: None,
            resolution: None,
            action: comment_pb::CommentReportAction::NoAction as i32,
            created_at: "2026-08-16T00:00:00Z".to_string(),
            updated_at: "2026-08-16T00:00:00Z".to_string(),
        })
        .expect("comment report receipt");
        let json = serde_json::to_value(receipt).expect("receipt JSON");
        assert_eq!(json["reason"], "harassment");
        assert!(json.get("reported_comment").is_none());
        assert!(
            !json
                .to_string()
                .contains("comment body that reporters must not receive")
        );
    }

    #[test]
    fn comment_moderation_and_own_appeal_requests_only_use_trusted_ids() {
        let review: ReviewCommentReportRequest = serde_json::from_value(serde_json::json!({
            "status": "resolved",
            "resolution": "restricted after corroboration",
            "action": "restrict_comment"
        }))
        .expect("comment report review JSON");
        let review = review.into_pb("moderator-1".to_string(), "report-1".to_string());
        assert_eq!(review.reviewer_id, "moderator-1");
        assert_eq!(review.report_id, "report-1");
        assert_eq!(
            review.action,
            comment_pb::CommentReportAction::RestrictComment as i32
        );

        let appeal: CreateCommentAppealRequest = serde_json::from_value(serde_json::json!({
            "details": "please reconsider",
            "author_id": "attacker-supplied-user"
        }))
        .expect("comment appeal JSON");
        let appeal = appeal
            .into_pb("comment-1".to_string(), Some("appeal-key-1".to_string()))
            .expect("appeal request should require idempotency");
        assert!(appeal.author_id.is_empty());
        assert_eq!(appeal.comment_id, "comment-1");

        let own_query: OwnCommentAppealQuery = serde_json::from_value(serde_json::json!({
            "author_id": "attacker-supplied-user",
            "status": "pending"
        }))
        .expect("own appeal query JSON");
        assert!(own_query.into_pb().author_id.is_none());
    }

    #[test]
    fn public_resource_query_accepts_bounded_named_filters() {
        let query: ResourceSearchQuery = serde_json::from_value(serde_json::json!({
            "query": "开放课程",
            "kind": "course",
            "topic": "学习",
            "limit": 12
        }))
        .expect("resource query JSON");
        let request = query.into_pb().expect("resource query should map");
        assert_eq!(request.query, "开放课程");
        assert_eq!(request.kind, Some(catalog_pb::ResourceKind::Course as i32));
        assert_eq!(request.topic, "学习");
        assert_eq!(request.limit, Some(12));
    }

    #[test]
    fn rag_context_transports_plain_questions_without_client_vectors() {
        let request: RouteNodeRagContextRequest = serde_json::from_value(serde_json::json!({
            "question": "如何开始这一步？",
            "limit": 4,
            "scene_equipment": "阅读清单"
        }))
        .expect("RAG question JSON should deserialize");
        let request = request.into_pb("route-1".to_string(), "node-1".to_string());
        assert_eq!(request.route_id, "route-1");
        assert_eq!(request.action_node_id, "node-1");
        assert_eq!(request.question, "如何开始这一步？");
        assert_eq!(request.limit, Some(4));
        assert_eq!(request.scene_equipment.as_deref(), Some("阅读清单"));
        assert!(request.embedding_model.is_empty());
        assert!(request.query_embedding.is_empty());
    }

    #[test]
    fn route_node_resource_attach_uses_gateway_identity_and_idempotency_header() {
        let request: AttachRouteNodeResourceRequest = serde_json::from_value(serde_json::json!({
            "resource_id": "resource-mdn-web",
            "kind": "ai_action_guide",
            "note": "先完成官方示例，再记录复盘",
            "rag_enabled": true,
            "retrieval_scope": "当前行动节点",
            "scene_equipment": "开发环境"
        }))
        .expect("attachment JSON should deserialize");
        let request = request
            .into_pb(
                "route-1".to_string(),
                "node-1".to_string(),
                "creator-1".to_string(),
                Some("attach-key-1".to_string()),
            )
            .expect("attachment request should map");

        assert_eq!(request.route_id, "route-1");
        assert_eq!(request.action_node_id, "node-1");
        assert_eq!(request.created_by, "creator-1");
        assert_eq!(request.idempotency_key, "attach-key-1");
        assert_eq!(
            request.kind,
            catalog_pb::AttachmentKind::AiActionGuide as i32
        );
        assert!(request.rag_enabled);
        assert_eq!(request.scene_equipment, "开发环境");
    }

    #[test]
    fn route_node_resource_attach_accepts_a_typed_resource_package() {
        let request: AttachRouteNodeResourceRequest = serde_json::from_value(serde_json::json!({
            "resource_id": "resource-ocw-learning",
            "kind": "resource_package",
            "scene_equipment": "学习计划"
        }))
        .expect("resource package JSON should deserialize");
        let request = request
            .into_pb(
                "route-1".to_string(),
                "node-1".to_string(),
                "creator-1".to_string(),
                Some("resource-package-key-1".to_string()),
            )
            .expect("resource package should map to the catalog contract");
        assert_eq!(
            request.kind,
            catalog_pb::AttachmentKind::ResourcePackage as i32
        );
    }

    #[test]
    fn route_node_resource_attach_rejects_unknown_kind_and_missing_idempotency() {
        let invalid_kind = AttachRouteNodeResourceRequest {
            resource_id: "resource-1".to_string(),
            kind: "unknown".to_string(),
            scene_equipment: "装备".to_string(),
            title_override: String::new(),
            note: String::new(),
            sort_rank: 0,
            rag_enabled: false,
            retrieval_scope: String::new(),
        };
        assert!(
            invalid_kind
                .into_pb(
                    "route-1".to_string(),
                    "node-1".to_string(),
                    "user-1".to_string(),
                    Some("key-1".to_string()),
                )
                .is_err()
        );

        let missing_key = AttachRouteNodeResourceRequest {
            resource_id: "resource-1".to_string(),
            kind: "document".to_string(),
            scene_equipment: "装备".to_string(),
            title_override: String::new(),
            note: String::new(),
            sort_rank: 0,
            rag_enabled: false,
            retrieval_scope: String::new(),
        };
        assert!(
            missing_key
                .into_pb(
                    "route-1".to_string(),
                    "node-1".to_string(),
                    "user-1".to_string(),
                    None,
                )
                .is_err()
        );
    }

    #[test]
    fn route_node_resource_response_preserves_public_resource_metadata() {
        let attachment =
            RouteNodeResourceAttachment::try_from(catalog_pb::RouteNodeResourceAttachment {
                id: "attachment-1".to_string(),
                route_id: "route-1".to_string(),
                action_node_id: "node-1".to_string(),
                resource_id: "resource-1".to_string(),
                kind: catalog_pb::AttachmentKind::Pdf as i32,
                scene_equipment: "行动装备".to_string(),
                title_override: "行动手册".to_string(),
                note: "完成后复盘".to_string(),
                sort_rank: 2,
                rag_enabled: false,
                embedding_collection: String::new(),
                retrieval_scope: String::new(),
                created_by: "creator-1".to_string(),
                created_at: "2026-08-18T00:00:00Z".to_string(),
                updated_at: "2026-08-18T00:00:00Z".to_string(),
                resource: Some(catalog_pb::Resource {
                    id: "resource-1".to_string(),
                    title: "开放课程".to_string(),
                    kind: catalog_pb::ResourceKind::Course as i32,
                    provider: "官方机构".to_string(),
                    summary: "课程简介".to_string(),
                    url: "https://example.com/course".to_string(),
                    license: "CC BY 4.0".to_string(),
                    version: "1.0".to_string(),
                    citation: "Official course. 2026.".to_string(),
                    topics: vec!["学习".to_string()],
                    status: catalog_pb::ResourceStatus::Published as i32,
                    published_at: "2026-08-01T00:00:00Z".to_string(),
                    updated_at: "2026-08-18T00:00:00Z".to_string(),
                }),
            })
            .expect("attachment response should convert");
        let json = serde_json::to_value(attachment).expect("attachment JSON");
        assert_eq!(json["kind"], "pdf");
        assert_eq!(json["scene_equipment"], "行动装备");
        assert_eq!(json["resource"]["license"], "CC BY 4.0");
    }

    #[test]
    fn public_resource_response_requires_citation_and_license_metadata() {
        let resource = PublicResource::try_from(catalog_pb::Resource {
            id: "resource-1".to_string(),
            title: "公开课程".to_string(),
            kind: catalog_pb::ResourceKind::Course as i32,
            provider: "provider".to_string(),
            summary: "summary".to_string(),
            url: "https://example.com/course".to_string(),
            license: "CC BY 4.0".to_string(),
            version: "2026".to_string(),
            citation: "Provider. Course. 2026.".to_string(),
            topics: vec!["学习".to_string()],
            status: catalog_pb::ResourceStatus::Published as i32,
            published_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-08-01T00:00:00Z".to_string(),
        })
        .expect("complete resource metadata");
        assert_eq!(resource.kind, "course");
        assert_eq!(resource.license, "CC BY 4.0");
        assert!(
            PublicResource::try_from(catalog_pb::Resource {
                license: String::new(),
                ..catalog_pb::Resource::default()
            })
            .is_err()
        );
    }

    #[test]
    fn admin_resource_upsert_maps_named_kind_and_optional_status() {
        let request: UpsertResourceRequest = serde_json::from_value(serde_json::json!({
            "title": "开放工具目录",
            "kind": "tool",
            "provider": "Open Tools",
            "summary": "工具简介",
            "url": "https://tools.example/catalog",
            "license": "MIT",
            "version": "2.0",
            "citation": "Open Tools. 2026.",
            "topics": ["工具", "学习"],
            "status": "archived"
        }))
        .expect("admin upsert JSON should deserialize");
        let request = request
            .into_pb("resource-1".to_string(), "admin-1".to_string())
            .expect("admin upsert should map");
        assert_eq!(request.resource_id, "resource-1");
        assert_eq!(request.operator_id, "admin-1");
        assert_eq!(request.kind, catalog_pb::ResourceKind::Tool as i32);
        assert_eq!(request.status, catalog_pb::ResourceStatus::Archived as i32);
        assert_eq!(request.topics.len(), 2);

        // Create path: no id, absent status defaults to UNSPECIFIED so the
        // catalog decides the publish default.
        let create: UpsertResourceRequest = serde_json::from_value(serde_json::json!({
            "title": "开放课程",
            "kind": "course",
            "provider": "Open Course",
            "url": "https://course.example",
            "license": "CC BY 4.0",
            "version": "1.0",
            "citation": "Open Course. 2026."
        }))
        .expect("create JSON should deserialize");
        let create = create
            .into_pb(String::new(), "admin-1".to_string())
            .expect("create should map");
        assert!(create.resource_id.is_empty());
        assert_eq!(create.status, catalog_pb::ResourceStatus::Unspecified as i32);
    }

    #[test]
    fn admin_resource_upsert_rejects_unknown_kind_and_status() {
        let bad_kind: UpsertResourceRequest = serde_json::from_value(serde_json::json!({
            "title": "开放资源",
            "kind": "dataset",
            "provider": "Open",
            "url": "https://open.example",
            "license": "MIT",
            "version": "1",
            "citation": "Open. 2026."
        }))
        .expect("JSON should deserialize");
        assert!(bad_kind
            .into_pb(String::new(), "admin-1".to_string())
            .is_err());

        let bad_status: UpsertResourceRequest = serde_json::from_value(serde_json::json!({
            "title": "开放资源",
            "kind": "book",
            "provider": "Open",
            "url": "https://open.example",
            "license": "MIT",
            "version": "1",
            "citation": "Open. 2026.",
            "status": "draft"
        }))
        .expect("JSON should deserialize");
        assert!(bad_status
            .into_pb(String::new(), "admin-1".to_string())
            .is_err());
    }
}
