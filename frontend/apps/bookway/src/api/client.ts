import { Platform } from 'react-native';

import {
  AccountProfile,
  CreateJourneyInput,
  Action,
  ActionUpdate,
  Comment,
  CommentPage,
  CompanionBrief,
  ContentAppeal,
  ContentAppealPage,
  ContentDetail,
  DirectConversationPage,
  DirectMessagePage,
  DirectMessage,
  DirectMessagePreferences,
  PublicResourcePage,
  ContentAppealStatus,
  ContentStatus,
  CreateFeedbackInput,
  CreateActionInput,
  CreateEntryInput,
  CreateKnowledgeResourceInput,
  CreatePostInput,
  Feed,
  GrowthEntry,
  Journey,
  JourneyDetail,
  JourneyUpdate,
  KnowledgeResource,
  KnowledgeJourney,
  KnowledgeResourceKind,
  KnowledgeResourceStatus,
  NotificationPage,
  OwnedContentPage,
  PublicContentPage,
  ReportReason,
  RouteParticipation,
  RouteJoinResult,
  RouteParticipationState,
  ReminderPreferences,
  ReminderPreferencesInput,
  RecommendationEventContext,
  SearchResponse,
  SuggestionResponse,
  SocialContext,
  Today,
  UserFeedback,
  UserNotification,
  WeeklyReview,
  UpdateKnowledgeResourceInput,
  UpdateAccountProfileInput,
  NegativeFeedbackReason,
} from '../types';
import { localScheduleContext } from '../utils/scheduling';
import { analyticsSessionId } from '../analytics/session';

type ApiResponse<T> = { data: T };
type ApiErrorResponse = { error?: { code?: string; message?: string } };

export type MediaResource = {
  id: string;
  object_key: string;
  mime_type: string;
  size_bytes: number;
  status: 'pending' | 'processing' | 'ready' | 'blocked' | 'deleted';
  cdn_url: string;
  width: number;
  height: number;
  duration_ms?: number | null;
};

type UploadResponse = {
  id: string;
  object_key: string;
  upload_url: string;
  cdn_url: string;
  expires_in_seconds: number;
};

export class ApiRequestError extends Error {
  readonly status: number;
  readonly code?: string;

  constructor(status: number, code?: string, message?: string) {
    super(message ?? `API request failed with ${status}`);
    this.name = 'ApiRequestError';
    this.status = status;
    this.code = code;
  }
}

const defaultBaseUrl = Platform.select({
  android: 'http://10.0.2.2:8080',
  default: 'http://127.0.0.1:8080',
});

const API_BASE_URL = process.env.EXPO_PUBLIC_API_URL ?? defaultBaseUrl;
const AUTH_TOKEN = process.env.EXPO_PUBLIC_AUTH_TOKEN;
const reportIdempotencyKeys = new Map<string, string>();
const appealIdempotencyKeys = new Map<string, string>();
const knowledgeIdempotencyKeys = new Map<string, string>();
const journeyIdempotencyKeys = new Map<string, string>();
const actionIdempotencyKeys = new Map<string, string>();
const entryIdempotencyKeys = new Map<string, string>();
const commentIdempotencyKeys = new Map<string, string>();
const feedbackIdempotencyKeys = new Map<string, string>();
const contentSubmissionStates = new Map<string, { idempotencyKey: string; postId?: string }>();

type ViewerClaims = { sub?: unknown; roles?: unknown };

// This only controls client-side affordances; Gateway derives identity from
// the verified request and never trusts this decoded JWT claim.
function viewerClaims(): ViewerClaims | undefined {
  if (!AUTH_TOKEN) return undefined;
  const payload = AUTH_TOKEN.split('.')[1];
  if (!payload || typeof atob !== 'function') return undefined;
  try {
    const base64 = payload.replace(/-/g, '+').replace(/_/g, '/');
    const padded = `${base64}${'='.repeat((4 - (base64.length % 4)) % 4)}`;
    return JSON.parse(atob(padded)) as ViewerClaims;
  } catch {
    return undefined;
  }
}

export function viewerUserId(): string | undefined {
  if (!AUTH_TOKEN) return 'demo-user';
  const claims = viewerClaims();
  return typeof claims?.sub === 'string' && claims.sub.trim() ? claims.sub : undefined;
}

// This merely decides whether to render the workbench. Gateway reconstructs
// roles from the verified JWT and still rejects any unauthorized request.
export function viewerCanModerate(): boolean {
  const claims = viewerClaims();
  if (!claims || !Array.isArray(claims.roles)) return false;
  return claims.roles.some((role) => (
    typeof role === 'string'
    && ['moderator', 'admin', 'trust_safety'].includes(role.trim().toLowerCase())
  ));
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE_URL}${path}`, {
    ...init,
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/json',
      ...(AUTH_TOKEN ? { Authorization: `Bearer ${AUTH_TOKEN}` } : { 'x-user-id': 'demo-user' }),
      ...init?.headers,
    },
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as ApiErrorResponse | null;
    throw new ApiRequestError(response.status, body?.error?.code, body?.error?.message);
  }
  if (response.status === 204) return undefined as T;
  const body = (await response.json()) as ApiResponse<T>;
  return body.data;
}

export function sendEvents(events: unknown[]): Promise<{ accepted: number; duplicate: number; rejected: number }> {
  return request('/v1/events', { method: 'POST', body: JSON.stringify({ events }) });
}

export function getAccountProfile(): Promise<AccountProfile> {
  return request('/v1/me/profile');
}

export function updateAccountProfile(input: UpdateAccountProfileInput): Promise<AccountProfile> {
  return request('/v1/me/profile', {
    method: 'PATCH',
    body: JSON.stringify(input),
  });
}

export function getToday(): Promise<Today> {
  const context = localScheduleContext();
  return request(`/v1/today?date=${encodeURIComponent(context.date)}&timezone=${encodeURIComponent(context.timezone)}`);
}

export function getReminderPreferences(): Promise<ReminderPreferences> {
  return request('/v1/reminder-preferences');
}

export function updateReminderPreferences(input: ReminderPreferencesInput): Promise<ReminderPreferences> {
  return request('/v1/reminder-preferences', {
    method: 'PUT',
    body: JSON.stringify(input),
  });
}

export function getNotifications(cursor?: string, unreadOnly = false): Promise<NotificationPage> {
  const query = new URLSearchParams({ limit: '30' });
  if (cursor) query.set('cursor', cursor);
  if (unreadOnly) query.set('unread_only', 'true');
  return request(`/v1/notifications?${query.toString()}`);
}

export function getDirectConversations(cursor?: string): Promise<DirectConversationPage> {
  const query = new URLSearchParams({ limit: '30' });
  if (cursor) query.set('cursor', cursor);
  return request(`/v1/messages/conversations?${query.toString()}`);
}

export function getDirectMessages(conversationId: string, cursor?: string): Promise<DirectMessagePage> {
  const query = new URLSearchParams({ limit: '50' });
  if (cursor) query.set('cursor', cursor);
  return request(`/v1/messages/conversations/${encodeURIComponent(conversationId)}?${query.toString()}`);
}

export function sendDirectMessage(recipientUserId: string, body: string): Promise<DirectMessage> {
  return request('/v1/messages', {
    method: 'POST',
    headers: { 'Idempotency-Key': `mobile-${Date.now()}-${Math.random().toString(36).slice(2, 10)}` },
    body: JSON.stringify({ recipient_user_id: recipientUserId, body }),
  });
}

export function markDirectConversationRead(conversationId: string, throughMessageId?: string): Promise<{ marked_count: number; read_at: string }> {
  return request(`/v1/messages/conversations/${encodeURIComponent(conversationId)}/read`, {
    method: 'POST',
    body: JSON.stringify({ through_message_id: throughMessageId }),
  });
}

export function reportDirectMessage(messageId: string, reason: 'spam' | 'harassment' | 'unsafe' | 'fraud' | 'privacy' | 'other' = 'other'): Promise<{ id: string; message_id: string; status: string }> {
  return request(`/v1/messages/${encodeURIComponent(messageId)}/report`, {
    method: 'POST',
    headers: { 'Idempotency-Key': `mobile-report-${Date.now()}-${Math.random().toString(36).slice(2, 10)}` },
    body: JSON.stringify({ reason, details: '' }),
  });
}

export function getDirectMessagePreferences(): Promise<DirectMessagePreferences> {
  return request('/v1/message-preferences');
}

export function updateDirectMessagePreferences(allowDirectMessages: boolean): Promise<DirectMessagePreferences> {
  return request('/v1/message-preferences', {
    method: 'PUT',
    body: JSON.stringify({ allow_direct_messages: allowDirectMessages }),
  });
}

export function getPublicResources(params: { query?: string; kind?: PublicResourcePage['items'][number]['kind']; topic?: string; cursor?: string; limit?: number } = {}): Promise<PublicResourcePage> {
  const query = new URLSearchParams();
  if (params.query) query.set('query', params.query);
  if (params.kind) query.set('kind', params.kind);
  if (params.topic) query.set('topic', params.topic);
  if (params.cursor) query.set('cursor', params.cursor);
  query.set('limit', String(params.limit ?? 30));
  return request(`/v1/resources?${query.toString()}`);
}

export function markNotificationRead(notificationId: string): Promise<UserNotification> {
  return request(`/v1/notifications/${encodeURIComponent(notificationId)}/read`, {
    method: 'PATCH',
  });
}

export function getJourneys(): Promise<Journey[]> {
  return request('/v1/journeys');
}

export function getFeed(interests = 'learning,movement,travel', cursor?: string, surface: 'home' | 'following' = 'home'): Promise<Feed> {
  const query = new URLSearchParams({ interests, limit: '10', session_id: analyticsSessionId(), surface });
  if (cursor) query.set('cursor', cursor);
  return request(
    `/v1/feed?${query.toString()}`,
  );
}

export function getPost(postId: string): Promise<ContentDetail> {
  return request(`/v1/posts/${encodeURIComponent(postId)}`);
}

export function getAuthorPosts(authorId: string, cursor?: string): Promise<PublicContentPage> {
  const query = new URLSearchParams({ limit: '20' });
  if (cursor) query.set('cursor', cursor);
  return request(`/v1/users/${encodeURIComponent(authorId)}/posts?${query.toString()}`);
}

export function getMyPosts(cursor?: string, status?: ContentStatus): Promise<OwnedContentPage> {
  const query = new URLSearchParams({ limit: '50', strategy: 'fresh' });
  if (cursor) query.set('cursor', cursor);
  if (status) query.set('status', status);
  return request(`/v1/me/posts?${query.toString()}`);
}

export function getMyAppeals(cursor?: string, status?: ContentAppealStatus): Promise<ContentAppealPage> {
  const query = new URLSearchParams({ limit: '50' });
  if (cursor) query.set('cursor', cursor);
  if (status) query.set('status', status);
  return request(`/v1/me/appeals?${query.toString()}`);
}

export function getMyFeedback(): Promise<UserFeedback[]> {
  return request('/v1/me/feedback?limit=50');
}

export function submitFeedback(input: CreateFeedbackInput): Promise<UserFeedback> {
  const content = input.content.trim();
  const contact = input.contact?.trim() ?? '';
  const fingerprint = JSON.stringify([input.category, content, contact]);
  const idempotencyKey = feedbackIdempotencyKeys.get(fingerprint)
    ?? `feedback-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  feedbackIdempotencyKeys.set(fingerprint, idempotencyKey);
  return request<UserFeedback>('/v1/feedback', {
    method: 'POST',
    headers: { 'Idempotency-Key': idempotencyKey },
    body: JSON.stringify({
      category: input.category,
      content,
      contact,
      platform: Platform.OS,
      app_version: 'mobile',
    }),
  }).then((feedback) => {
    feedbackIdempotencyKeys.delete(fingerprint);
    return feedback;
  });
}

export function search(query: string, cursor?: string): Promise<SearchResponse> {
  const params = new URLSearchParams({ q: query, search_type: 'all', limit: '20', session_id: analyticsSessionId() });
  if (cursor) params.set('cursor', cursor);
  return request(`/v1/search?${params.toString()}`);
}

export function getSuggestions(query: string): Promise<SuggestionResponse> {
  return request(`/v1/search/suggestions?q=${encodeURIComponent(query)}`);
}

export function setPostReaction(
  postId: string,
  reaction: 'like' | 'bookmark' | 'hide',
  active: boolean,
  negativeFeedbackReason?: NegativeFeedbackReason,
  context?: RecommendationEventContext,
): Promise<unknown> {
  return request(`/v1/posts/${encodeURIComponent(postId)}/reactions`, {
    method: 'PUT',
    body: JSON.stringify({
      reaction,
      active,
      negative_feedback_reason: negativeFeedbackReason,
      attribution: contentAttribution(context),
    }),
  });
}

export function getEntries(): Promise<GrowthEntry[]> {
  return request('/v1/entries');
}

export function createEntry(input: CreateEntryInput): Promise<GrowthEntry> {
  const fingerprint = JSON.stringify(input);
  const idempotencyKey = entryIdempotencyKeys.get(fingerprint)
    ?? `entry-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  entryIdempotencyKeys.set(fingerprint, idempotencyKey);
  return request<GrowthEntry>('/v1/entries', {
    method: 'POST',
    headers: { 'Idempotency-Key': idempotencyKey },
    body: JSON.stringify(input),
  }).then((entry) => {
    entryIdempotencyKeys.delete(fingerprint);
    return entry;
  });
}

export function retryEntryPublication(entryId: string): Promise<GrowthEntry> {
  return request(`/v1/entries/${encodeURIComponent(entryId)}/publication/retry`, {
    method: 'POST',
  });
}

async function uploadMediaAsset(
  uri: string,
  declaredMimeType: string | null | undefined,
  acceptedMimeTypes: readonly string[],
  fallbackMimeType: string,
  label: string,
): Promise<MediaResource> {
  // Read exactly once so the declared upload size matches the PUT body on all
  // Expo platforms, including web's blob-backed picker URI.
  const source = await fetch(uri);
  if (!source.ok) throw new Error(`无法读取所选${label}`);
  const body = await source.blob();
  const mimeType = declaredMimeType?.trim().toLowerCase() || body.type.toLowerCase() || fallbackMimeType;
  if (!acceptedMimeTypes.includes(mimeType) || body.size <= 0 || body.size > 512 * 1024 * 1024) {
    throw new Error(`请选择不超过 512MB 的 ${acceptedMimeTypes.join('、')} ${label}`);
  }
  const upload = await request<UploadResponse>('/v1/media/upload-url', {
    method: 'POST',
    body: JSON.stringify({ mime_type: mimeType, size_bytes: body.size }),
  });
  const response = await fetch(upload.upload_url, {
    method: 'PUT',
    headers: { 'Content-Type': mimeType },
    body,
  });
  if (!response.ok) throw new Error(`${label}上传失败，请重试`);
  return request<MediaResource>(`/v1/media/${encodeURIComponent(upload.id)}/complete`, {
    method: 'POST',
  });
}

export function uploadImageAsset(uri: string, declaredMimeType?: string | null): Promise<MediaResource> {
  return uploadMediaAsset(uri, declaredMimeType, ['image/jpeg', 'image/png', 'image/webp'], 'image/jpeg', '图片');
}

export function uploadVideoAsset(uri: string, declaredMimeType?: string | null): Promise<MediaResource> {
  return uploadMediaAsset(uri, declaredMimeType, ['video/mp4'], 'video/mp4', '视频');
}

export function getMediaAsset(mediaId: string): Promise<MediaResource> {
  return request(`/v1/media/${encodeURIComponent(mediaId)}`);
}

export function getWeeklyReview(): Promise<WeeklyReview> {
  return request('/v1/reviews/weekly');
}

export function getCompanion(): Promise<CompanionBrief> {
  const context = localScheduleContext();
  return request(`/v1/companion?date=${encodeURIComponent(context.date)}&timezone=${encodeURIComponent(context.timezone)}`);
}

export function getSocialContext(): Promise<SocialContext> {
  return request('/v1/social/context');
}

export function getRouteParticipations(): Promise<RouteParticipation[]> {
  return request('/v1/route-participations');
}

export function setRouteParticipation(
  routeId: string,
  active: boolean,
  privateJourneyId?: string,
): Promise<RouteParticipationState> {
  return request(`/v1/routes/${encodeURIComponent(routeId)}/participation`, {
    method: 'PUT',
    body: JSON.stringify({ active, private_journey_id: privateJourneyId }),
  });
}

export function joinRoute(routeId: string, context?: RecommendationEventContext): Promise<RouteJoinResult> {
  return request(`/v1/routes/${encodeURIComponent(routeId)}/join`, {
    method: 'POST',
    body: JSON.stringify({ attribution: contentAttribution(context) }),
  });
}

export function getKnowledge(filters: { q?: string; kind?: KnowledgeResourceKind; status?: KnowledgeResourceStatus; tag?: string } = {}): Promise<KnowledgeResource[]> {
  const query = new URLSearchParams();
  Object.entries(filters).forEach(([key, value]) => {
    if (value) query.set(key, value);
  });
  const suffix = query.size ? `?${query.toString()}` : '';
  return request(`/v1/knowledge${suffix}`);
}

export function createKnowledge(input: CreateKnowledgeResourceInput): Promise<KnowledgeResource> {
  const fingerprint = JSON.stringify(input);
  const idempotencyKey = knowledgeIdempotencyKeys.get(fingerprint)
    ?? `knowledge-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  knowledgeIdempotencyKeys.set(fingerprint, idempotencyKey);
  return request<KnowledgeResource>('/v1/knowledge', {
    method: 'POST',
    headers: { 'Idempotency-Key': idempotencyKey },
    body: JSON.stringify(input),
  }).then((resource) => {
    knowledgeIdempotencyKeys.delete(fingerprint);
    return resource;
  });
}

export function updateKnowledge(resourceId: string, input: UpdateKnowledgeResourceInput): Promise<KnowledgeResource> {
  return request(`/v1/knowledge/${encodeURIComponent(resourceId)}`, {
    method: 'PATCH',
    body: JSON.stringify(input),
  });
}

export function capturePostAsKnowledge(postId: string, context?: RecommendationEventContext): Promise<KnowledgeResource> {
  return request(`/v1/posts/${encodeURIComponent(postId)}/knowledge`, {
    method: 'POST',
    body: JSON.stringify({ attribution: contentAttribution(context) }),
  });
}

function contentAttribution(context?: RecommendationEventContext) {
  const requestId = context?.request_id?.trim();
  if (!context || !requestId) return undefined;
  const { position, surface } = context;
  return {
    session_id: analyticsSessionId(),
    request_id: requestId,
    position,
    attribution_source: surface === 'search' ? 'search' : 'recommendation',
  } as const;
}

export function startKnowledgeJourney(resourceId: string, input: CreateJourneyInput): Promise<KnowledgeJourney> {
  return request(`/v1/knowledge/${encodeURIComponent(resourceId)}/journey`, {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export function completeAction(actionId: string): Promise<Action> {
  return request(`/v1/actions/${encodeURIComponent(actionId)}/complete`, { method: 'POST' });
}

export function getJourney(journeyId: string): Promise<JourneyDetail> {
  return request(`/v1/journeys/${encodeURIComponent(journeyId)}`);
}

export function updateJourney(journeyId: string, input: JourneyUpdate): Promise<Journey> {
  return request(`/v1/journeys/${encodeURIComponent(journeyId)}`, {
    method: 'PATCH',
    body: JSON.stringify(input),
  });
}

export function createAction(journeyId: string, input: CreateActionInput): Promise<Action> {
  const fingerprint = JSON.stringify([journeyId, input]);
  const idempotencyKey = actionIdempotencyKeys.get(fingerprint)
    ?? `action-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  actionIdempotencyKeys.set(fingerprint, idempotencyKey);
  return request<Action>(`/v1/journeys/${encodeURIComponent(journeyId)}/actions`, {
    method: 'POST',
    headers: { 'Idempotency-Key': idempotencyKey },
    body: JSON.stringify(input),
  }).then((action) => {
    actionIdempotencyKeys.delete(fingerprint);
    return action;
  });
}

export function updateAction(actionId: string, input: ActionUpdate): Promise<Action> {
  return request(`/v1/actions/${encodeURIComponent(actionId)}`, {
    method: 'PATCH',
    body: JSON.stringify(input),
  });
}

export function createJourney(input: CreateJourneyInput): Promise<Journey> {
  const fingerprint = JSON.stringify(input);
  const idempotencyKey = journeyIdempotencyKeys.get(fingerprint)
    ?? `journey-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  journeyIdempotencyKeys.set(fingerprint, idempotencyKey);
  return request<Journey>('/v1/journeys', {
    method: 'POST',
    headers: { 'Idempotency-Key': idempotencyKey },
    body: JSON.stringify(input),
  }).then((journey) => {
    journeyIdempotencyKeys.delete(fingerprint);
    return journey;
  });
}

export function getComments(postId: string, cursor?: string): Promise<CommentPage> {
  const query = new URLSearchParams({ limit: '30' });
  if (cursor) query.set('cursor', cursor);
  return request<CommentPage | Comment[]>(`/v1/posts/${encodeURIComponent(postId)}/comments?${query.toString()}`)
    .then((response) => Array.isArray(response) ? { items: response } : response);
}

export function createComment(postId: string, body: string, parentId?: string): Promise<Comment> {
  const fingerprint = JSON.stringify([postId, body, parentId ?? null]);
  const idempotencyKey = commentIdempotencyKeys.get(fingerprint)
    ?? `comment-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  commentIdempotencyKeys.set(fingerprint, idempotencyKey);
  return request<Comment>(`/v1/posts/${encodeURIComponent(postId)}/comments`, {
    method: 'POST',
    headers: { 'Idempotency-Key': idempotencyKey },
    body: JSON.stringify({ body, parent_id: parentId ?? null }),
  }).then((comment) => {
    commentIdempotencyKeys.delete(fingerprint);
    return comment;
  });
}

export function deleteComment(postId: string, commentId: string): Promise<void> {
  return request<unknown>(
    `/v1/posts/${encodeURIComponent(postId)}/comments/${encodeURIComponent(commentId)}`,
    { method: 'DELETE' },
  ).then(() => undefined);
}

export function acceptQuestionAnswer(postId: string, commentId: string): Promise<ContentDetail> {
  return request<ContentDetail>(
    `/v1/posts/${encodeURIComponent(postId)}/comments/${encodeURIComponent(commentId)}/accept`,
    { method: 'POST' },
  );
}

export function getModerationComments(cursor?: string): Promise<CommentPage> {
  const query = new URLSearchParams({ limit: '30' });
  if (cursor) query.set('cursor', cursor);
  return request(`/v1/moderation/comments?${query.toString()}`);
}

export function reviewModerationComment(
  commentId: string,
  decision: 'approve' | 'restrict',
): Promise<Comment> {
  return request(`/v1/moderation/comments/${encodeURIComponent(commentId)}`, {
    method: 'PATCH',
    body: JSON.stringify({ decision }),
  });
}

export function reportPost(postId: string, reason: ReportReason, details = ''): Promise<unknown> {
  const reportKey = `${postId}:${reason}`;
  const idempotencyKey = reportIdempotencyKeys.get(reportKey)
    ?? `report-${postId}-${reason}-${Date.now()}`;
  reportIdempotencyKeys.set(reportKey, idempotencyKey);
  return request(`/v1/posts/${encodeURIComponent(postId)}/report`, {
    method: 'POST',
    headers: { 'Idempotency-Key': idempotencyKey },
    body: JSON.stringify({ reason, details }),
  });
}

export function appealPost(postId: string, details: string): Promise<ContentAppeal> {
  const normalizedDetails = details.trim();
  const appealKey = `${postId}:${normalizedDetails}`;
  const idempotencyKey = appealIdempotencyKeys.get(appealKey)
    ?? `appeal-${postId}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  appealIdempotencyKeys.set(appealKey, idempotencyKey);
  return request<ContentAppeal>(`/v1/posts/${encodeURIComponent(postId)}/appeals`, {
    method: 'POST',
    headers: { 'Idempotency-Key': idempotencyKey },
    body: JSON.stringify({ details: normalizedDetails }),
  }).then((appeal) => {
    appealIdempotencyKeys.delete(appealKey);
    return appeal;
  });
}

export function setFollow(userId: string, active: boolean): Promise<SocialContext> {
  return request(`/v1/users/${encodeURIComponent(userId)}/follow`, {
    method: 'PUT',
    body: JSON.stringify({ edge: 'follow', active }),
  });
}

export function setCreatorRelationship(
  userId: string,
  edge: 'mute' | 'block',
  active: boolean,
): Promise<SocialContext> {
  return request(`/v1/users/${encodeURIComponent(userId)}/relationship`, {
    method: 'PUT',
    body: JSON.stringify({ edge, active }),
  });
}

export function publishPost(postId: string, idempotencyKey?: string): Promise<{ id: string }> {
  return request(`/v1/posts/${encodeURIComponent(postId)}/publish`, {
    method: 'POST',
    ...(idempotencyKey ? { headers: { 'Idempotency-Key': idempotencyKey } } : {}),
  });
}

export async function submitPostForReview(input: CreatePostInput): Promise<{ id: string }> {
  const fingerprint = JSON.stringify(input);
  const state = contentSubmissionStates.get(fingerprint)
    ?? { idempotencyKey: `content-submit-${Date.now()}-${Math.random().toString(36).slice(2)}` };
  contentSubmissionStates.set(fingerprint, state);
  if (!state.postId) {
    const draft = await request<{ id: string }>('/v1/posts', {
      method: 'POST',
      headers: { 'Idempotency-Key': state.idempotencyKey },
      body: JSON.stringify(input),
    });
    state.postId = draft.id;
  }
  try {
    const published = await publishPost(state.postId, state.idempotencyKey);
    contentSubmissionStates.delete(fingerprint);
    return published;
  } catch (error) {
    // Keep the draft ID and the original key so a retry publishes this draft
    // instead of creating a second copy after a transient failure.
    throw error;
  }
}
