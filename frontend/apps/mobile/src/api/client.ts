import { Platform } from 'react-native';

import {
  CreateJourneyInput,
  Action,
  ActionUpdate,
  Comment,
  CreateActionInput,
  CreatePostInput,
  Feed,
  Journey,
  JourneyDetail,
  JourneyUpdate,
  SearchResponse,
  SuggestionResponse,
  Today,
} from '../types';

type ApiResponse<T> = { data: T };

const defaultBaseUrl = Platform.select({
  android: 'http://10.0.2.2:8080',
  default: 'http://127.0.0.1:8080',
});

const API_BASE_URL = process.env.EXPO_PUBLIC_API_URL ?? defaultBaseUrl;
const AUTH_TOKEN = process.env.EXPO_PUBLIC_AUTH_TOKEN;

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
    throw new Error(`API request failed with ${response.status}`);
  }
  const body = (await response.json()) as ApiResponse<T>;
  return body.data;
}

export function sendEvents(events: unknown[]): Promise<{ accepted: number; duplicate: number; rejected: number }> {
  return request('/v1/events', { method: 'POST', body: JSON.stringify({ events }) });
}

export function getToday(): Promise<Today> {
  return request('/v1/today');
}

export function getJourneys(): Promise<Journey[]> {
  return request('/v1/journeys');
}

export function getFeed(interests = 'learning,movement,travel', cursor?: string): Promise<Feed> {
  const query = new URLSearchParams({ interests, limit: '10', surface: 'home' });
  if (cursor) query.set('cursor', cursor);
  return request(
    `/v1/feed?${query.toString()}`,
  );
}

export function search(query: string): Promise<SearchResponse> {
  return request(
    `/v1/search?q=${encodeURIComponent(query)}&search_type=all&limit=20`,
  );
}

export function getSuggestions(query: string): Promise<SuggestionResponse> {
  return request(`/v1/search/suggestions?q=${encodeURIComponent(query)}`);
}

export function setPostReaction(
  postId: string,
  reaction: 'like' | 'bookmark',
  active: boolean,
): Promise<unknown> {
  return request(`/v1/posts/${encodeURIComponent(postId)}/reactions`, {
    method: 'PUT',
    body: JSON.stringify({ reaction, active }),
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

export function getComments(postId: string): Promise<Comment[]> {
  return request(`/v1/posts/${encodeURIComponent(postId)}/comments`);
}

export function createComment(postId: string, body: string): Promise<Comment> {
  return request(`/v1/posts/${encodeURIComponent(postId)}/comments`, {
    method: 'POST',
    body: JSON.stringify({ body, parent_id: null }),
  });
}

export function setFollow(userId: string, active: boolean): Promise<unknown> {
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
