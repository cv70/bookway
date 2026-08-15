import { Platform } from 'react-native';

import {
  CreateJourneyInput,
  Action,
  ActionUpdate,
  Comment,
  CommentPage,
  CompanionBrief,
  ContentAppeal,
  ContentAppealPage,
  ContentDetail,
  ContentAppealStatus,
  ContentStatus,
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
  KnowledgeResourceKind,
  KnowledgeResourceStatus,
  NotificationPage,
  OwnedContentPage,
  ReportReason,
  RouteParticipation,
  RouteJoinResult,
  RouteParticipationState,
  ReminderPreferences,
  ReminderPreferencesInput,
  SearchResponse,
  SuggestionResponse,
  SocialContext,
  Today,
  UserNotification,
  WeeklyReview,
  UpdateKnowledgeResourceInput,
} from '../types';
import { localScheduleContext } from '../utils/scheduling';

type ApiResponse<T> = { data: T };
type ApiErrorResponse = { error?: { code?: string; message?: string } };

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
const commentIdempotencyKeys = new Map<string, string>();

// This only controls client-side affordances; Gateway derives identity from
// the verified request and never trusts this decoded JWT claim.
export function viewerUserId(): string | undefined {
  if (!AUTH_TOKEN) return 'demo-user';
  const payload = AUTH_TOKEN.split('.')[1];
  if (!payload || typeof atob !== 'function') return undefined;
  try {
    const base64 = payload.replace(/-/g, '+').replace(/_/g, '/');
    const padded = `${base64}${'='.repeat((4 - (base64.length % 4)) % 4)}`;
    const claims = JSON.parse(atob(padded)) as { sub?: unknown };
    return typeof claims.sub === 'string' && claims.sub.trim() ? claims.sub : undefined;
  } catch {
    return undefined;
  }
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

export function markNotificationRead(notificationId: string): Promise<UserNotification> {
  return request(`/v1/notifications/${encodeURIComponent(notificationId)}/read`, {
    method: 'PATCH',
  });
}

export function getJourneys(): Promise<Journey[]> {
  return request('/v1/journeys');
}

export function getFeed(interests = 'learning,movement,travel', cursor?: string, surface: 'home' | 'following' = 'home'): Promise<Feed> {
  const query = new URLSearchParams({ interests, limit: '10', surface });
  if (cursor) query.set('cursor', cursor);
  return request(
    `/v1/feed?${query.toString()}`,
  );
}

export function getPost(postId: string): Promise<ContentDetail> {
  return request(`/v1/posts/${encodeURIComponent(postId)}`);
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

export function search(query: string, cursor?: string): Promise<SearchResponse> {
  const params = new URLSearchParams({ q: query, search_type: 'all', limit: '20' });
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
): Promise<unknown> {
  return request(`/v1/posts/${encodeURIComponent(postId)}/reactions`, {
    method: 'PUT',
    body: JSON.stringify({ reaction, active }),
  });
}

export function getEntries(): Promise<GrowthEntry[]> {
  return request('/v1/entries');
}

export function createEntry(input: CreateEntryInput): Promise<GrowthEntry> {
  return request('/v1/entries', {
    method: 'POST',
    body: JSON.stringify(input),
  });
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

export function joinRoute(routeId: string): Promise<RouteJoinResult> {
  return request(`/v1/routes/${encodeURIComponent(routeId)}/join`, {
    method: 'POST',
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
  return request(`/v1/journeys/${encodeURIComponent(journeyId)}/actions`, {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export function updateAction(actionId: string, input: ActionUpdate): Promise<Action> {
  return request(`/v1/actions/${encodeURIComponent(actionId)}`, {
    method: 'PATCH',
    body: JSON.stringify(input),
  });
}

export function createJourney(input: CreateJourneyInput): Promise<Journey> {
  return request('/v1/journeys', {
    method: 'POST',
    body: JSON.stringify(input),
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

export function createPost(input: CreatePostInput): Promise<{ id: string }> {
  return request('/v1/posts', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export function publishPost(postId: string): Promise<{ id: string }> {
  return request(`/v1/posts/${encodeURIComponent(postId)}/publish`, { method: 'POST' });
}
