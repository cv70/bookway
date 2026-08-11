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

export type CreateJourneyInput = {
  title: string;
  intent: string;
  domain: GrowthDomain;
  duration_label: string;
  first_action_title: string;
  first_action_detail: string;
  estimated_minutes: number;
};

export type TabKey = 'today' | 'discover' | 'journeys' | 'profile';
