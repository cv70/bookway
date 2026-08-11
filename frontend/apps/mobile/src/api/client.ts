import { Platform } from 'react-native';

import {
  CreateJourneyInput,
  Feed,
  Journey,
  SearchResponse,
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

export function getFeed(interests = 'learning,movement,travel'): Promise<Feed> {
  return request(
    `/v1/feed?interests=${encodeURIComponent(interests)}&limit=10&surface=home`,
  );
}

export function search(query: string): Promise<SearchResponse> {
  return request(
    `/v1/search?q=${encodeURIComponent(query)}&search_type=all&limit=20`,
  );
}

export function setPostReaction(postId: string, active: boolean): Promise<unknown> {
  return request(`/v1/posts/${encodeURIComponent(postId)}/reactions`, {
    method: 'PUT',
    body: JSON.stringify({ reaction: 'like', active }),
    headers: { 'x-user-id': 'demo-user' },
  });
}

export function completeAction(actionId: string): Promise<void> {
  return request(`/v1/actions/${encodeURIComponent(actionId)}/complete`, { method: 'POST' });
}

export function createJourney(input: CreateJourneyInput): Promise<Journey> {
  return request('/v1/journeys', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}
