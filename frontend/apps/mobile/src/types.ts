export type GrowthDomain = 'learning' | 'movement' | 'wellness' | 'travel' | 'leisure';
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
  join_count: number;
  like_count: number;
  freshness: number;
  tags: string[];
};

export type ContentDetail = {
  id: string;
  post: CommunityPost;
  author_id: string;
};

export type ContentStatus = 'draft' | 'reviewing' | 'published' | 'restricted' | 'deleted';

export type OwnedContent = {
  id: string;
  post: CommunityPost;
  author_id: string;
  status: ContentStatus;
  created_at: string;
  published_at?: string | null;
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

export type FeedItem = {
  author_id: string;
  post: CommunityPost;
  score: number;
  source: string;
  reasons: string[];
};

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
  result_type: 'post' | 'journey' | 'user' | 'topic';
  title: string;
  snippet: string;
  cover_url?: string;
  author_id?: string;
  author_name?: string;
  domain?: GrowthDomain;
  score: number;
  highlights: string[];
  post?: CommunityPost;
};

export type SearchResponse = {
  query: string;
  items: SearchResult[];
  next_cursor?: string;
  total_estimate: number;
  took_ms: number;
  degraded: boolean;
};

export type SuggestionResponse = {
  query: string;
  items: Array<{ text: string; result_type: 'post' | 'journey' | 'user' | 'topic'; score: number }>;
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

export type GrowthEntry = {
  id: string;
  action_id?: string;
  journey_id?: string;
  body: string;
  mood: EntryMood;
  duration_minutes?: number;
  quantity?: string;
  location?: string;
  photo_url?: string;
  created_at: string;
  published?: boolean;
};

export type CreateEntryInput = Omit<GrowthEntry, 'id' | 'created_at'>;

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

export type CreateReadingBookInput = {
  title: string;
  author: string;
  content?: string;
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

export type ContentType = 'note' | 'article' | 'route';

export type CreatePostInput = {
  title: string;
  summary: string;
  body: string;
  domain: GrowthDomain;
  content_type: ContentType;
  cover_url?: string;
  tags: string[];
  topics: string[];
  route_title?: string;
  route_duration?: string;
};

export type TabKey = 'today' | 'discover' | 'journeys' | 'profile';
