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
pub enum ReportReasonDto {
    #[default]
    Spam,
    Harassment,
    Unsafe,
    Misinformation,
    Copyright,
    Privacy,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentReportStatusDto {
    #[default]
    Pending,
    Reviewing,
    Resolved,
    Rejected,
}

/// A content-state operation requested as part of a human report decision.
/// The report remains the durable audit trail; content ownership stays in bbs-link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentReportActionDto {
    #[default]
    NoAction,
    RestrictContent,
    RestoreContent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentAppealStatusDto {
    #[default]
    Pending,
    Reviewing,
    Resolved,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateContentReportRequest {
    pub reason: ReportReasonDto,
    #[serde(default)]
    pub details: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentReportDto {
    pub id: String,
    pub reporter_id: String,
    pub content_id: String,
    pub reason: ReportReasonDto,
    pub details: String,
    pub status: ContentReportStatusDto,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default)]
    pub action: ContentReportActionDto,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContentReportQueryRequest {
    pub status: Option<ContentReportStatusDto>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentReportPageDto {
    pub items: Vec<ContentReportDto>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewContentReportRequest {
    pub status: ContentReportStatusDto,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub action: ContentReportActionDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateContentAppealRequest {
    #[serde(default)]
    pub details: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentAppealDto {
    pub id: String,
    pub content_id: String,
    pub appellant_id: String,
    pub details: String,
    pub status: ContentAppealStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default)]
    pub action: ContentReportActionDto,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContentAppealQueryRequest {
    pub status: Option<ContentAppealStatusDto>,
    /// Internal owner filter. Gateway sets this from the authenticated user;
    /// it is never accepted as a public identity claim.
    pub appellant_id: Option<String>,
    /// Internal content filter used by moderation and owner-scoped lookups.
    pub content_id: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentAppealPageDto {
    pub items: Vec<ContentAppealDto>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewContentAppealRequest {
    pub status: ContentAppealStatusDto,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub action: ContentReportActionDto,
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
    Hide,
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

/// How a journey reaches its user-defined completion condition. The type is
/// deliberately separate from the life domain: a learning journey, for
/// example, can be a repeatable habit or a finite project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JourneyTypeDto {
    Habit,
    #[default]
    Project,
    Quantity,
    Travel,
    Challenge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStateDto {
    Pending,
    Completed,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionRecurrenceFrequencyDto {
    Daily,
    Weekly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeekdayDto {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

/// Structured local-calendar recurrence for a single action occurrence.
/// Completing or skipping an occurrence materializes the next one; completed
/// occurrences remain available as evidence for reviews.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRecurrenceDto {
    pub frequency: ActionRecurrenceFrequencyDto,
    #[serde(default = "default_recurrence_interval")]
    pub interval: u16,
    #[serde(default)]
    pub weekdays: Vec<WeekdayDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_on: Option<String>,
    /// Server-filled date that keeps a multi-week rule stable across
    /// successive occurrences. Clients may omit it when creating a plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_date: Option<String>,
}

fn default_recurrence_interval() -> u16 {
    1
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyStageDto {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub completion_criteria: String,
    pub position: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateJourneyStageRequest {
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub completion_criteria: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JourneyDto {
    pub id: String,
    pub title: String,
    pub intent: String,
    pub domain: GrowthDomainDto,
    #[serde(default)]
    pub journey_type: JourneyTypeDto,
    #[serde(default)]
    pub completion_criteria: String,
    #[serde(default)]
    pub stages: Vec<JourneyStageDto>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_id: Option<String>,
    pub title: String,
    pub detail: String,
    pub estimated_minutes: u16,
    pub scheduled_label: String,
    /// RFC 3339 timestamp with an explicit offset for a user-chosen schedule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_for: Option<String>,
    /// IANA timezone selected when the schedule was created, retained for display and reminders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<ActionRecurrenceDto>,
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
    #[serde(default)]
    pub journey_type: JourneyTypeDto,
    #[serde(default)]
    pub completion_criteria: String,
    #[serde(default)]
    pub stages: Vec<CreateJourneyStageRequest>,
    pub duration_label: String,
    pub first_action_title: String,
    pub first_action_detail: String,
    pub estimated_minutes: u16,
    #[serde(default)]
    pub first_action_scheduled_label: Option<String>,
    #[serde(default)]
    pub first_action_scheduled_for: Option<String>,
    #[serde(default)]
    pub first_action_scheduled_timezone: Option<String>,
    /// Zero-based index into `stages`; omitted when the first action does not
    /// belong to a stage.
    #[serde(default)]
    pub first_action_stage_index: Option<u16>,
    #[serde(default)]
    pub first_action_recurrence: Option<ActionRecurrenceDto>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateJourneyRequest {
    pub title: Option<String>,
    pub intent: Option<String>,
    pub duration_label: Option<String>,
    pub status: Option<JourneyStatusDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JourneyDetailDto {
    pub journey: JourneyDto,
    pub actions: Vec<ActionDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateActionRequest {
    pub journey_id: String,
    #[serde(default)]
    pub stage_id: Option<String>,
    pub title: String,
    pub detail: String,
    pub estimated_minutes: u16,
    pub scheduled_label: String,
    #[serde(default)]
    pub scheduled_for: Option<String>,
    #[serde(default)]
    pub scheduled_timezone: Option<String>,
    #[serde(default)]
    pub recurrence: Option<ActionRecurrenceDto>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateActionRequest {
    pub title: Option<String>,
    pub detail: Option<String>,
    pub estimated_minutes: Option<u16>,
    pub scheduled_label: Option<String>,
    pub scheduled_for: Option<String>,
    pub scheduled_timezone: Option<String>,
    pub state: Option<ActionStateDto>,
}

/// Per-user policy used by the reminder dispatcher. Times are stored in the
/// user's selected IANA timezone, rather than the timezone of a worker host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderPreferencesDto {
    pub enabled: bool,
    pub lead_minutes: u16,
    pub timezone: String,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateReminderPreferencesRequest {
    pub enabled: bool,
    #[serde(default)]
    pub lead_minutes: u16,
    pub timezone: String,
    #[serde(default)]
    pub quiet_hours_start: Option<String>,
    #[serde(default)]
    pub quiet_hours_end: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushProviderDto {
    Expo,
    Fcm,
    Apns,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterPushDeviceRequest {
    /// A stable installation identifier generated by the client.
    pub device_id: String,
    pub provider: PushProviderDto,
    /// An opaque provider endpoint. It is never returned by the API or sent in events.
    pub endpoint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushDeviceDto {
    pub device_id: String,
    pub provider: PushProviderDto,
    pub active: bool,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKindDto {
    #[default]
    ActionReminder,
    Community,
    System,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserNotificationDto {
    pub id: String,
    pub kind: NotificationKindDto,
    /// Stable producer-side identifier, such as a reminder delivery ID.
    pub source_id: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub data: serde_json::Value,
    pub read_at: Option<String>,
    pub created_at: String,
}

/// An internal producer request for a durable, user-visible notification.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateUserNotificationRequest {
    pub kind: NotificationKindDto,
    /// Stable producer-side idempotency key, scoped by `kind`.
    pub source_id: String,
    pub title: String,
    pub body: String,
    pub data: serde_json::Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NotificationQueryRequest {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub unread_only: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotificationPageDto {
    pub items: Vec<UserNotificationDto>,
    pub next_cursor: Option<String>,
    pub unread_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryMoodDto {
    Clear,
    #[default]
    Steady,
    Tired,
    Energized,
    Calm,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GrowthEntryDto {
    pub id: String,
    pub action_id: Option<String>,
    pub journey_id: Option<String>,
    pub body: String,
    pub mood: EntryMoodDto,
    pub duration_minutes: Option<u16>,
    pub quantity: Option<String>,
    pub location: Option<String>,
    pub photo_url: Option<String>,
    pub created_at: String,
    pub published: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGrowthEntryRequest {
    pub action_id: Option<String>,
    pub journey_id: Option<String>,
    pub body: String,
    pub mood: EntryMoodDto,
    pub duration_minutes: Option<u16>,
    pub quantity: Option<String>,
    pub location: Option<String>,
    pub photo_url: Option<String>,
    #[serde(default)]
    pub published: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewDomainProgressDto {
    pub domain: GrowthDomainDto,
    pub completed_actions: usize,
    pub total_actions: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAdjustmentKindDto {
    ReduceActionDuration,
    RescheduleAction,
    PauseJourney,
}

/// A user-controlled adjustment surfaced by a review. `action_patch` and
/// `journey_patch` use the existing PATCH endpoints; the service never applies
/// them implicitly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewAdjustmentSuggestionDto {
    pub kind: ReviewAdjustmentKindDto,
    pub title: String,
    pub rationale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_patch: Option<ReviewActionPatchDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journey_patch: Option<ReviewJourneyPatchDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewActionPatchDto {
    pub action_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_minutes: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_label: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewJourneyPatchDto {
    pub journey_id: String,
    pub status: JourneyStatusDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeeklyReviewDto {
    pub period_start: String,
    pub period_end: String,
    pub completed_actions: usize,
    pub skipped_actions: usize,
    pub focus_minutes: u32,
    pub entry_count: usize,
    pub active_journeys: usize,
    pub completion_rate: f64,
    pub domains: Vec<ReviewDomainProgressDto>,
    pub reflection_prompts: Vec<String>,
    #[serde(default)]
    pub adjustment_suggestions: Vec<ReviewAdjustmentSuggestionDto>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanionModeDto {
    #[default]
    StartSmall,
    KeepGoing,
    Celebrate,
    PlanNext,
}

/// A non-prescriptive next-step recommendation based only on the user's own
/// routes, actions and records. The service never applies the suggestion.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompanionBriefDto {
    pub mode: CompanionModeDto,
    pub headline: String,
    pub message: String,
    pub reason: String,
    pub suggested_action: Option<ActionDto>,
    pub suggested_minutes: Option<u16>,
    pub completed_actions: usize,
    pub total_actions: usize,
    pub active_journeys: usize,
    pub reflection_prompt: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeResourceKindDto {
    #[default]
    Book,
    Article,
    Course,
    Video,
    Link,
    Note,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeResourceStatusDto {
    #[default]
    Inbox,
    Active,
    Completed,
    Archived,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeResourceDto {
    pub id: String,
    pub title: String,
    pub creator: String,
    pub summary: String,
    pub kind: KnowledgeResourceKindDto,
    pub status: KnowledgeResourceStatusDto,
    pub source_url: Option<String>,
    pub body: Option<String>,
    pub tags: Vec<String>,
    pub journey_id: Option<String>,
    pub progress: u8,
    pub current_position: u32,
    pub reading_seconds: u64,
    pub bookmarks: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_opened_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct KnowledgeQueryRequest {
    pub q: Option<String>,
    pub kind: Option<KnowledgeResourceKindDto>,
    pub status: Option<KnowledgeResourceStatusDto>,
    pub tag: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateKnowledgeResourceRequest {
    pub title: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub kind: KnowledgeResourceKindDto,
    #[serde(default)]
    pub status: KnowledgeResourceStatusDto,
    pub source_url: Option<String>,
    pub body: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub journey_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateKnowledgeResourceRequest {
    pub title: Option<String>,
    pub creator: Option<String>,
    pub summary: Option<String>,
    pub kind: Option<KnowledgeResourceKindDto>,
    pub status: Option<KnowledgeResourceStatusDto>,
    pub source_url: Option<String>,
    pub body: Option<String>,
    pub tags: Option<Vec<String>>,
    pub journey_id: Option<String>,
    pub progress: Option<u8>,
    pub current_position: Option<u32>,
    pub reading_seconds: Option<u64>,
    pub bookmarks: Option<Vec<String>>,
    pub last_opened_at: Option<String>,
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
    /// Internal author filter for private creator-management views.
    pub author_id: Option<String>,
    /// Optional indexed filters used by search and discovery surfaces.
    pub content_type: Option<ContentTypeDto>,
    pub domain: Option<GrowthDomainDto>,
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
    pub author_id: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_bucket: Option<String>,
}

/// Candidate exchanged between the recommendation stages. Keeping this
/// contract in the shared API crate lets recall, filtering, scoring and
/// ranking services evolve independently from the feed product response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecommendationCandidateDto {
    pub content_id: String,
    pub post: PostSummaryDto,
    pub author_id: String,
    pub status: ContentStatusDto,
    pub quality_score: f64,
    pub freshness: f64,
    pub recall_score: f64,
    pub score: f64,
    pub source: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecallRequestDto {
    pub user_id: String,
    pub interests: Vec<GrowthDomainDto>,
    pub seen: Vec<String>,
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecallResponseDto {
    pub candidates: Vec<RecommendationCandidateDto>,
    pub next_cursor: Option<String>,
    pub sources: Vec<String>,
    pub degraded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecommendRankRequestDto {
    pub user_id: String,
    pub features: serde_json::Value,
    pub candidates: Vec<RecommendationCandidateDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecommendRankResponseDto {
    pub candidates: Vec<RecommendationCandidateDto>,
    pub model_version: String,
    pub experiment_bucket: String,
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

/// Internal result for the comment create RPC. The flattened comment preserves
/// the legacy response shape while allowing trusted callers to notify the
/// parent-comment author without adding that relationship to public comments.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateCommentResult {
    #[serde(flatten)]
    pub comment: CommentDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_author_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CommentQueryRequest {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    /// Internal viewer policy injected by Gateway before the request reaches
    /// the cursor-owning comment service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_author_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommentPageDto {
    pub items: Vec<CommentDto>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
    pub parent_id: Option<String>,
    /// Internal viewer policy injected by Gateway so replies cannot target a
    /// comment hidden from the current viewer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_author_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SocialContextDto {
    pub followed_author_ids: Vec<String>,
    pub blocked_author_ids: Vec<String>,
    pub muted_author_ids: Vec<String>,
    pub liked_post_ids: Vec<String>,
    pub bookmarked_post_ids: Vec<String>,
}

/// Internal-only author visibility policy. It intentionally has no public
/// HTTP surface: incoming blocks are enforced without disclosing who blocked
/// the current viewer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SocialVisibilityDto {
    pub excluded_author_ids: Vec<String>,
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
    #[serde(default)]
    pub hidden_post_ids: Vec<String>,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteParticipationDto {
    pub route_id: String,
    pub private_journey_id: Option<String>,
    pub joined_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetRouteParticipationRequest {
    pub active: bool,
    pub private_journey_id: Option<String>,
    pub intent_version: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteParticipationIntentDto {
    pub route_id: String,
    pub desired_active: bool,
    pub private_journey_id: Option<String>,
    pub version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteParticipationStateDto {
    pub route_id: String,
    pub joined: bool,
    pub private_journey_id: Option<String>,
    pub joined_at: Option<String>,
    pub participant_count: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteParticipationContextDto {
    pub joined_route_ids: Vec<String>,
    pub participant_counts: std::collections::HashMap<String, u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteJoinResultDto {
    pub journey: JourneyDto,
    pub participation: RouteParticipationStateDto,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchQueryRequest {
    pub q: String,
    pub search_type: SearchTypeDto,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    /// Trusted viewer identity injected by Gateway for personalization and cursor binding.
    pub user_id: Option<String>,
    /// Author IDs excluded by the Gateway-derived social visibility policy.
    /// This is an internal propagation field, never a client identity or authorization claim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_author_ids: Vec<String>,
}

/// Internal suggestion request propagated from Gateway after it derives the
/// viewer's social visibility policy. It is not a client authorization claim.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SuggestionQueryRequest {
    pub q: String,
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_author_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResultDto {
    pub id: String,
    pub result_type: SearchResultTypeDto,
    pub title: String,
    pub snippet: String,
    pub cover_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
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
