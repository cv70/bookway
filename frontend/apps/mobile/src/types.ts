export type GrowthDomain = 'learning' | 'movement' | 'wellness' | 'travel' | 'leisure';
export type ActionState = 'pending' | 'completed' | 'skipped';
export type JourneyStatus = 'active' | 'paused' | 'completed';

export type Action = {
  id: string;
  journey_id: string;
  title: string;
  detail: string;
  estimated_minutes: number;
  scheduled_label: string;
  state: ActionState;
};

export type ActionUpdate = Partial<Pick<Action, 'title' | 'detail' | 'estimated_minutes' | 'scheduled_label' | 'state'>>;

export type Today = {
  completed: number;
  total: number;
  focus_minutes: number;
  actions: Action[];
};

export type Journey = {
  id: string;
  title: string;
  intent: string;
  domain: GrowthDomain;
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

export type FeedItem = {
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
  };
};

export type SearchResult = {
  id: string;
  result_type: 'post' | 'journey' | 'user' | 'topic';
  title: string;
  snippet: string;
  cover_url?: string;
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
  duration_label: string;
  first_action_title: string;
  first_action_detail: string;
  estimated_minutes: number;
};

export type CreateActionInput = {
  title: string;
  detail: string;
  estimated_minutes: number;
  scheduled_label: string;
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
  author_name: string;
  body: string;
  created_at: string;
};

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
