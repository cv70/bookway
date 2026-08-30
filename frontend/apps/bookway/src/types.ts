export type GrowthDomain = 'learning' | 'movement' | 'wellness' | 'travel' | 'leisure';

export type AccountProfile = {
  user_id: string;
  display_name: string;
  avatar_url: string;
  bio: string;
  created_at: string;
  updated_at: string;
};

export type UpdateAccountProfileInput = Partial<Pick<AccountProfile, 'display_name' | 'avatar_url' | 'bio'>>;

export type ActionState = 'pending' | 'completed' | 'skipped';
export type JourneyStatus = 'active' | 'paused' | 'completed';
export type JourneyType = 'habit' | 'project' | 'quantity' | 'travel' | 'challenge';
export type Weekday = 'monday' | 'tuesday' | 'wednesday' | 'thursday' | 'friday' | 'saturday' | 'sunday';
export type ActionRecurrence = {
  frequency: 'daily' | 'weekly';
  interval: number;
  weekdays: Weekday[];
  ends_on?: string;
  anchor_date?: string;
};

export type JourneyStage = {
  id: string;
  title: string;
  detail: string;
  completion_criteria: string;
  position: number;
};

export type CreateJourneyStageInput = Pick<JourneyStage, 'title' | 'detail' | 'completion_criteria'>;

export type Action = {
  id: string;
  journey_id: string;
  stage_id?: string;
  title: string;
  detail: string;
  estimated_minutes: number;
  scheduled_label: string;
  scheduled_for?: string;
  scheduled_timezone?: string;
  recurrence?: ActionRecurrence;
  state: ActionState;
};

export type ActionUpdate = Partial<Pick<Action, 'title' | 'detail' | 'estimated_minutes' | 'scheduled_label' | 'scheduled_for' | 'scheduled_timezone' | 'state'>>;

export type ReminderPreferences = {
  enabled: boolean;
  lead_minutes: number;
  timezone: string;
  quiet_hours_start?: string;
  quiet_hours_end?: string;
  updated_at: string;
};

export type ReminderPreferencesInput = Omit<ReminderPreferences, 'updated_at'>;

export type NotificationKind = 'action_reminder' | 'community' | 'system';

export type UserNotification = {
  id: string;
  kind: NotificationKind;
  source_id: string;
  title: string;
  body: string;
  data: Record<string, unknown>;
  read_at?: string | null;
  created_at: string;
};

export type NotificationPage = {
  items: UserNotification[];
  next_cursor?: string | null;
  unread_count: number;
};

export type DirectMessage = {
  id: string;
  conversation_id: string;
  sender_user_id: string;
  recipient_user_id: string;
  kind: 'text';
  body: string;
  created_at: string;
  read_at?: string | null;
};

export type DirectConversation = {
  id: string;
  peer_user_id: string;
  last_message_preview: string;
  last_message_at: string;
  unread_count: number;
};

export type DirectConversationPage = {
  items: DirectConversation[];
  next_cursor?: string | null;
};

export type DirectMessagePage = {
  items: DirectMessage[];
  next_cursor?: string | null;
};

export type DirectMessagePreferences = {
  allow_direct_messages: boolean;
  updated_at: string;
};

export type PublicResource = {
  id: string;
  title: string;
  kind: 'book' | 'course' | 'tool' | 'article' | 'podcast';
  provider: string;
  summary: string;
  url: string;
  license: string;
  version: string;
  citation: string;
  topics: string[];
  published_at: string;
  updated_at: string;
};

export type PublicResourcePage = {
  items: PublicResource[];
  next_cursor?: string | null;
};

export type Today = {
  completed: number;
  total: number;
  focus_minutes: number;
  actions: Action[];
};

export type CompanionBrief = {
  mode: 'start_small' | 'keep_going' | 'celebrate' | 'plan_next';
  headline: string;
  message: string;
  reason: string;
  suggested_action: Action | null;
  suggested_minutes: number | null;
  completed_actions: number;
  total_actions: number;
  active_journeys: number;
  reflection_prompt: string;
};

export type Journey = {
  id: string;
  title: string;
  intent: string;
  domain: GrowthDomain;
  journey_type: JourneyType;
  completion_criteria: string;
  stages: JourneyStage[];
  status: JourneyStatus;
  progress: number;
  duration_label: string;
  next_action: string;
  participant_count: number;
};

export type JourneyDetail = {
  journey: Journey;
  actions: Action[];
};

// A resource becomes active together with its first private action. Keeping
// both results lets the app update the Inbox and Journey views atomically.
export type KnowledgeJourney = {
  resource: KnowledgeResource;
  journey: JourneyDetail;
};

export type JourneyUpdate = Partial<Pick<Journey, 'title' | 'intent' | 'duration_label' | 'status'>>;

export type CommunityPost = {
  id: string;
  author_name: string;
  author_avatar_url: string;
  title: string;
  summary: string;
  domain: GrowthDomain;
  cover_url: string;
  route_title: string;
  route_duration: string;
  // Absent when no service read the live participation fact. Never render it
  // as 0 人加入 — that is a claim the server did not make.
  join_count?: number | null;
  like_count: number;
  freshness: number;
  tags: string[];
  // Older servers omit this. New responses use the canonical content type.
  is_route?: boolean;
  is_milestone?: boolean;
  is_question?: boolean;
  // Search responses may include the public action nodes that matched a
  // route title or scene-equipment query.
  route_actions?: RouteTemplateAction[];
};

export type ContentMedia = {
  id: string;
  url: string;
  kind: string;
  width: number;
  height: number;
  duration_ms?: number | null;
};

export type ContentDetail = {
  id: string;
  // The public detail endpoint is authoritative, but tolerate a partial item
  // so an unavailable summary never crashes an already-open detail view.
  post?: CommunityPost | null;
  author_id: string;
  body: string;
  media: ContentMedia[];
  topics: string[];
  created_at: string;
  published_at?: string | null;
  content_type?: ContentType;
  status?: ContentStatus;
  version?: number;
  route_template?: RouteTemplate | null;
  route_fork?: RouteFork | null;
  milestone?: Milestone | null;
  accepted_answer_id?: string | null;
  question_context?: QuestionContext | null;
};

export type Milestone = {
  route_id: string;
  route_title: string;
  stage_id: string;
  stage_title: string;
  effort_summary: string;
  outcome_summary: string;
  adjustment_summary: string;
  evidence_scope: string;
};

export type QuestionContext = {
  route_id: string;
  route_title: string;
  stage_id?: string | null;
  stage_title?: string | null;
};

export type PublicContentPage = {
  items: ContentDetail[];
  next_cursor?: string | null;
  total_estimate: number;
};

export type PublicAuthor = {
  id: string;
  name: string;
  avatar_url?: string;
};

export type ContentStatus = 'draft' | 'reviewing' | 'published' | 'restricted' | 'deleted';

export type RouteFork = {
  source_route_id: string;
  source_route_version: number;
  source_route_title: string;
  forked_at: string;
};

export type OwnedContent = {
  id: string;
  post: CommunityPost;
  author_id: string;
  status: ContentStatus;
  body: string;
  topics: string[];
  created_at: string;
  published_at?: string | null;
  content_type?: ContentType;
  version?: number;
  route_template?: RouteTemplate | null;
  route_fork?: RouteFork | null;
};

export type OwnedContentPage = {
  items: OwnedContent[];
  next_cursor?: string | null;
  total_estimate: number;
};

export type ContentAppealStatus = 'pending' | 'reviewing' | 'resolved' | 'rejected';
export type ContentAppealAction = 'no_action' | 'restrict_content' | 'restore_content';

export type ContentAppeal = {
  id: string;
  content_id: string;
  appellant_id: string;
  details: string;
  status: ContentAppealStatus;
  assignee_id?: string | null;
  resolution?: string | null;
  action: ContentAppealAction;
  created_at: string;
  updated_at: string;
};

export type ContentAppealPage = {
  items: ContentAppeal[];
  next_cursor?: string | null;
};

export type FeedbackCategory = 'bug' | 'feature' | 'experience' | 'content' | 'other';
export type FeedbackStatus = 'pending' | 'processing' | 'resolved' | 'closed';

export type CreateFeedbackInput = {
  category: FeedbackCategory;
  content: string;
  contact?: string;
};

export type UserFeedback = {
  id: string;
  user_id: string;
  category: FeedbackCategory;
  content: string;
  contact: string;
  platform: string;
  app_version: string;
  status: FeedbackStatus;
  resolution?: string | null;
  created_at: string;
  updated_at: string;
};

export type FeedItem = {
  author_id: string;
  // Organic items carry a post; contextual commercial items carry only `ad`.
  // The transport keeps this as an optional field so the feed can mix both
  // kinds without inventing a fake community post.
  post?: CommunityPost | null;
  ad?: FeedAd | null;
  score: number;
  source: string;
  reasons: string[];
  // Client-only context from the recommendation response that served this item.
  recommendation_context?: RecommendationEventContext;
};

export type FeedAd = {
  request_id: string;
  campaign_id: string;
  placement: string;
  title: string;
  body: string;
  image_url: string;
  landing_url: string;
  ecpm: number;
  model_version: string;
  route_id: string;
  action_node_id: string;
  scene_equipment: string;
};

export type RecommendationSurface = 'home' | 'following' | 'search';

// Optional route-node context used to request contextual feed ads. The Gateway
// validates the route and action-node pair before any campaign is considered.
export type FeedActionContext = {
  route_id: string;
  action_node_id: string;
  placement?: string;
  action_domain?: GrowthDomain;
  scene_equipment?: string;
};

export type MallSku = {
  id: string;
  product_id: string;
  title: string;
  price_cents: number;
  currency: string;
  attributes: Record<string, string>;
  saleable: boolean;
};

export type MallProduct = {
  id: string;
  title: string;
  description: string;
  image_url: string;
  status: number;
  skus: MallSku[];
  created_at: string;
  updated_at: string;
};

// Public storefront projection. Merchant ownership and affiliate rates stay
// server-side; checkout addresses this offer by ID and the order service
// snapshots the private fields internally.
export type NodeOffer = {
  id: string;
  product_id: string;
  sku_id: string;
  route_id: string;
  action_node_id: string;
  created_at: string;
  scene_equipment: string;
  product?: MallProduct | null;
};

export type NodeOfferList = { items: NodeOffer[] };

export type MallOrderItemInput = { sku_id: string; quantity: number };

export type MallOrder = {
  id: string;
  user_id: string;
  status: number;
  currency: string;
  total_cents: number;
  items: Array<{
    sku_id: string;
    product_id: string;
    title: string;
    quantity: number;
    unit_price_cents: number;
    currency: string;
    line_total_cents: number;
  }>;
  payment_reference?: string | null;
  expires_at: string;
  created_at: string;
  updated_at: string;
  node_offer_id: string;
  affiliate_creator_id: string;
  commission_cents: number;
  merchant_id: string;
  fulfillment_status: number;
  tracking_number: string;
};

export type MallOrderList = { items: MallOrder[] };

export type RouteNodeResourceKind =
  | 'document'
  | 'pdf'
  | 'external_link'
  | 'tool_checklist'
  | 'ai_action_guide'
  | 'resource_package'
  | 'rag_corpus';

export type RouteNodeResourceAttachment = {
  id: string;
  route_id: string;
  action_node_id: string;
  resource_id: string;
  kind: RouteNodeResourceKind;
  title_override: string;
  note: string;
  sort_rank: number;
  rag_enabled: boolean;
  embedding_collection: string;
  retrieval_scope: string;
  scene_equipment: string;
  created_by: string;
  created_at: string;
  updated_at: string;
  resource?: PublicResource | null;
};

export type RouteNodeResourcePage = { items: RouteNodeResourceAttachment[] };

export type RecommendationEventContext = {
  request_id?: string;
  position: number;
  surface: RecommendationSurface;
};

export type NegativeFeedbackReason = 'not_relevant' | 'already_seen' | 'low_quality';

export type Feed = {
  request_id: string;
  items: FeedItem[];
  meta: {
    sourced: number;
    filtered: number;
    selected: number;
    next_cursor?: string;
    pipeline_id?: string;
    degraded?: boolean;
    model_version?: string;
    experiment_bucket?: string;
  };
};

export type SearchResult = {
  id: string;
  result_type: 'post' | 'journey' | 'user' | 'topic' | 'resource' | 'ad' | 'action_node' | 'scene_equipment';
  title: string;
  snippet: string;
  cover_url?: string;
  author_id?: string;
  author_name?: string;
  domain?: GrowthDomain;
  score: number;
  highlights: string[];
  post?: CommunityPost;
  resource?: SearchResourceSummary | null;
  ad?: FeedAd | null;
  // Client-only position and page request that produced this search result.
  event_context?: RecommendationEventContext;
};

export type SearchResourceSummary = {
  id: string;
  kind: string;
  provider: string;
  url: string;
  license: string;
  version: string;
  citation: string;
  topics: string[];
  published_at: string;
  updated_at: string;
};

export type SearchResponse = {
  request_id: string;
  query: string;
  items: SearchResult[];
  next_cursor?: string;
  total_estimate: number;
  took_ms: number;
  degraded: boolean;
};

export type SuggestionResponse = {
  query: string;
  items: Array<{ text: string; result_type: SearchResult['result_type']; score: number; personal?: boolean }>;
};

export type CreateJourneyInput = {
  title: string;
  intent: string;
  domain: GrowthDomain;
  journey_type: JourneyType;
  completion_criteria: string;
  stages: CreateJourneyStageInput[];
  duration_label: string;
  first_action_title: string;
  first_action_detail: string;
  estimated_minutes: number;
  first_action_scheduled_label?: string;
  first_action_scheduled_for?: string;
  first_action_scheduled_timezone?: string;
  first_action_stage_index?: number;
  first_action_recurrence?: ActionRecurrence;
};

export type CreateActionInput = {
  stage_id?: string;
  title: string;
  detail: string;
  estimated_minutes: number;
  scheduled_label: string;
  scheduled_for: string;
  scheduled_timezone: string;
  recurrence?: ActionRecurrence;
};

export type EntryMood = 'clear' | 'steady' | 'tired' | 'energized' | 'calm';
export type EntryPublicationStatus = 'private' | 'pending' | 'reviewing' | 'published' | 'restricted' | 'failed';

export type GrowthEntry = {
  id: string;
  action_id?: string;
  journey_id?: string;
  body: string;
  mood: EntryMood;
  duration_minutes?: number;
  quantity?: string;
  location?: string;
  photo_media_id?: string;
  created_at: string;
  published: boolean;
  publication_status?: EntryPublicationStatus;
  public_content_id?: string;
  publication_error?: string;
};

export type CreateEntryInput = Omit<GrowthEntry, 'id' | 'created_at' | 'publication_status' | 'public_content_id' | 'publication_error'>;

export type WeeklyReview = {
  period_start: string;
  period_end: string;
  completed_actions: number;
  skipped_actions: number;
  focus_minutes: number;
  entry_count: number;
  active_journeys: number;
  completion_rate: number;
  domains: Array<{
    domain: GrowthDomain;
    completed_actions: number;
    total_actions: number;
  }>;
  reflection_prompts: string[];
  adjustment_suggestions: ReviewAdjustmentSuggestion[];
};

export type ReviewAdjustmentSuggestion = {
  kind: 'reduce_action_duration' | 'reschedule_action' | 'pause_journey';
  title: string;
  rationale: string;
  action_patch?: {
    action_id: string;
    estimated_minutes?: number;
    scheduled_label?: string;
  };
  journey_patch?: {
    journey_id: string;
    status: JourneyStatus;
  };
};

export type KnowledgeResourceKind = 'book' | 'article' | 'course' | 'video' | 'link' | 'note';
export type KnowledgeResourceStatus = 'inbox' | 'active' | 'completed' | 'archived';

export type KnowledgeResource = {
  id: string;
  title: string;
  creator: string;
  summary: string;
  kind: KnowledgeResourceKind;
  status: KnowledgeResourceStatus;
  source_url?: string;
  body?: string;
  tags: string[];
  journey_id?: string;
  progress: number;
  current_position: number;
  reading_seconds: number;
  bookmarks: string[];
  created_at: string;
  updated_at: string;
  last_opened_at?: string;
  // Present only for the private metadata reference created from a community post.
  source_content_id?: string;
};

export type CreateKnowledgeResourceInput = Pick<KnowledgeResource, 'title' | 'creator' | 'summary' | 'kind' | 'status' | 'tags'> &
  Partial<Pick<KnowledgeResource, 'source_url' | 'body' | 'journey_id'>>;

export type UpdateKnowledgeResourceInput = Partial<Pick<KnowledgeResource,
  'title' | 'creator' | 'summary' | 'kind' | 'status' | 'source_url' | 'body' | 'tags' |
  'journey_id' | 'progress' | 'current_position' | 'reading_seconds' | 'bookmarks' | 'last_opened_at'
>>;

export type SocialContext = {
  followed_author_ids: string[];
  blocked_author_ids: string[];
  muted_author_ids: string[];
};

export type SocialStats = {
  followers: number;
  following: number;
};

export type RoutePeerPage = {
  items: { user_id: string; joined_at: string }[];
  next_cursor?: string | null;
};

export type RouteParticipation = {
  route_id: string;
  private_journey_id?: string | null;
  joined_at: string;
};

export type RouteParticipationState = {
  route_id: string;
  joined: boolean;
  private_journey_id?: string | null;
  joined_at?: string | null;
  participant_count: number;
};

export type RouteJoinResult = {
  journey: Journey;
  participation: RouteParticipationState;
};

export type ReadingChapter = {
  id: string;
  title: string;
  body: string[];
};

export type ReadingBook = {
  id: string;
  title: string;
  author: string;
  summary: string;
  journey_id?: string;
  progress: number;
  current_chapter: number;
  reading_seconds: number;
  added_at: string;
  last_opened_at?: string;
  accent: string;
  chapters: ReadingChapter[];
};

export type ReadingBookmark = {
  book_id: string;
  chapter_id: string;
  created_at: string;
};

export type ReaderTheme = 'light' | 'night';

export type ReaderSettings = {
  font_size: number;
  line_height: number;
  theme: ReaderTheme;
};

export type Comment = {
  id: string;
  post_id: string;
  author_id: string;
  author_name: string;
  body: string;
  parent_id?: string | null;
  like_count: number;
  created_at: string;
  status?: 'reviewing' | 'published' | 'restricted' | 'deleted';
};

export type CommentPage = {
  items: Comment[];
  next_cursor?: string;
};

export type ReportReason = 'spam' | 'harassment' | 'unsafe' | 'misinformation' | 'copyright' | 'privacy' | 'other';

export type ContentType = 'note' | 'article' | 'video' | 'route' | 'milestone' | 'question';

export type RouteTemplateStage = Pick<JourneyStage, 'id' | 'title' | 'detail' | 'completion_criteria'>;

export type RouteTemplateAction = Pick<Action, 'title' | 'detail' | 'estimated_minutes' | 'scheduled_label'> & {
  id: string;
  stage_id?: string;
  scene_equipment?: string[];
};

export type RouteTemplate = {
  intent: string;
  completion_criteria: string;
  stages: RouteTemplateStage[];
  actions: RouteTemplateAction[];
  journey_type: JourneyType;
};

export type CreatePostInput = {
  title: string;
  summary: string;
  body: string;
  domain: GrowthDomain;
  content_type: ContentType;
  media_asset_ids?: string[];
  tags: string[];
  topics: string[];
  route_title?: string;
  route_duration?: string;
  route_template?: RouteTemplate;
  milestone?: {
    route_id: string;
    stage_id?: string;
    effort_summary: string;
    outcome_summary: string;
    adjustment_summary: string;
    evidence_scope: string;
  };
  question_context?: {
    route_id: string;
    stage_id?: string;
  };
};

export type UpdatePostInput = {
  title?: string;
  summary?: string;
  body?: string;
  tags?: string[];
  topics?: string[];
  route_template?: RouteTemplate;
};

export type TabKey = 'today' | 'discover' | 'journeys' | 'profile';
