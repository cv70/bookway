use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentTypeDto {
    #[default]
    Note,
    Article,
    Video,
    Route,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentStatusDto {
    Draft,
    Reviewing,
    #[default]
    Published,
    Restricted,
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditDecisionDto {
    Approved,
    Reviewing,
    Restricted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentAuditRequest {
    pub content_id: String,
    pub version: u32,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentAuditResponse {
    pub decision: AuditDecisionDto,
    pub risk_score: f64,
    pub reasons: Vec<String>,
    pub provider: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchTypeDto {
    #[default]
    All,
    Posts,
    Journeys,
    Users,
    Topics,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchResultTypeDto {
    #[default]
    Post,
    Journey,
    User,
    Topic,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactionTypeDto {
    #[default]
    Like,
    Bookmark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialEdgeTypeDto {
    #[default]
    Follow,
    Block,
    Mute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrowthDomainDto {
    Learning,
    Movement,
    Wellness,
    Travel,
    Leisure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JourneyStatusDto {
    Active,
    Paused,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStateDto {
    Pending,
    Completed,
    Skipped,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JourneyDto {
    pub id: String,
    pub title: String,
    pub intent: String,
    pub domain: GrowthDomainDto,
    pub status: JourneyStatusDto,
    pub progress: u8,
    pub duration_label: String,
    pub next_action: String,
    pub participant_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionDto {
    pub id: String,
    pub journey_id: String,
    pub title: String,
    pub detail: String,
    pub estimated_minutes: u16,
    pub scheduled_label: String,
    pub state: ActionStateDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TodayDto {
    pub completed: usize,
    pub total: usize,
    pub focus_minutes: u32,
    pub actions: Vec<ActionDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateJourneyRequest {
    pub title: String,
    pub intent: String,
    pub domain: GrowthDomainDto,
    pub duration_label: String,
    pub first_action_title: String,
    pub first_action_detail: String,
    pub estimated_minutes: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentMediaDto {
    pub id: String,
    pub url: String,
    pub kind: String,
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MediaUploadRequest {
    pub mime_type: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MediaUploadResponse {
    pub id: String,
    pub object_key: String,
    pub upload_url: String,
    pub cdn_url: String,
    pub expires_in_seconds: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MediaDto {
    pub id: String,
    pub object_key: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub status: String,
    pub cdn_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentDto {
    pub id: String,
    pub post: PostSummaryDto,
    pub author_id: String,
    pub content_type: ContentTypeDto,
    pub status: ContentStatusDto,
    pub body: String,
    pub media: Vec<ContentMediaDto>,
    pub topics: Vec<String>,
    pub created_at: String,
    pub published_at: Option<String>,
    pub version: u32,
    pub quality_score: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateContentRequest {
    pub title: String,
    pub summary: String,
    pub body: String,
    pub domain: GrowthDomainDto,
    pub content_type: ContentTypeDto,
    pub cover_url: Option<String>,
    pub tags: Vec<String>,
    pub topics: Vec<String>,
    pub route_title: Option<String>,
    pub route_duration: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateContentRequest {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub body: Option<String>,
    pub tags: Option<Vec<String>>,
    pub topics: Option<Vec<String>>,
    pub cover_url: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContentQueryRequest {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub status: Option<ContentStatusDto>,
    pub strategy: Option<String>,
    pub ids: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentPageDto {
    pub items: Vec<ContentDto>,
    pub next_cursor: Option<String>,
    pub total_estimate: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PostSummaryDto {
    pub id: String,
    pub author_name: String,
    pub author_avatar_url: String,
    pub title: String,
    pub summary: String,
    pub domain: GrowthDomainDto,
    pub cover_url: String,
    pub route_title: String,
    pub route_duration: String,
    pub join_count: u32,
    pub like_count: u32,
    pub freshness: f64,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FeedQueryRequest {
    pub interests: Option<String>,
    pub seen: Option<String>,
    pub limit: Option<usize>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub surface: Option<String>,
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeedItemDto {
    pub post: PostSummaryDto,
    pub score: f64,
    pub source: String,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeedDto {
    pub request_id: String,
    pub items: Vec<FeedItemDto>,
    pub meta: FeedMetaDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeedMetaDto {
    pub sourced: usize,
    pub filtered: usize,
    pub selected: usize,
    pub next_cursor: Option<String>,
    pub pipeline_id: String,
    pub degraded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReactionDto {
    pub target_id: String,
    pub target_type: String,
    pub reaction: ReactionTypeDto,
    pub active: bool,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReactionRequest {
    pub reaction: ReactionTypeDto,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommentDto {
    pub id: String,
    pub post_id: String,
    pub author_id: String,
    pub author_name: String,
    pub body: String,
    pub parent_id: Option<String>,
    pub like_count: u64,
    pub created_at: String,
    pub status: ContentStatusDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SocialContextDto {
    pub followed_author_ids: Vec<String>,
    pub blocked_author_ids: Vec<String>,
    pub muted_author_ids: Vec<String>,
    pub liked_post_ids: Vec<String>,
    pub bookmarked_post_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SocialContextRequest {
    pub user_id: Option<String>,
    pub post_ids: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReactionContextDto {
    pub liked_post_ids: Vec<String>,
    pub bookmarked_post_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReactionContextRequest {
    pub user_id: Option<String>,
    pub post_ids: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserEventDto {
    pub event_id: String,
    pub event_type: String,
    pub session_id: String,
    pub request_id: Option<String>,
    pub component_id: String,
    pub content_id: Option<String>,
    pub position: Option<u32>,
    pub occurred_at: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserEventBatchRequest {
    pub events: Vec<UserEventDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserEventIngestResponse {
    pub accepted: usize,
    pub duplicate: usize,
    pub rejected: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FollowRequest {
    pub edge: SocialEdgeTypeDto,
    pub active: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchQueryRequest {
    pub q: String,
    pub search_type: SearchTypeDto,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResultDto {
    pub id: String,
    pub result_type: SearchResultTypeDto,
    pub title: String,
    pub snippet: String,
    pub cover_url: Option<String>,
    pub author_name: Option<String>,
    pub domain: Option<GrowthDomainDto>,
    pub score: f64,
    pub highlights: Vec<String>,
    pub post: Option<PostSummaryDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResponseDto {
    pub query: String,
    pub items: Vec<SearchResultDto>,
    pub next_cursor: Option<String>,
    pub total_estimate: usize,
    pub took_ms: u64,
    pub degraded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuggestionDto {
    pub text: String,
    pub result_type: SearchResultTypeDto,
    pub score: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuggestionResponseDto {
    pub query: String,
    pub items: Vec<SuggestionDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

impl<T> ApiResponse<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: ApiError,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ErrorResponse {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ApiError {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
    pub version: String,
}
