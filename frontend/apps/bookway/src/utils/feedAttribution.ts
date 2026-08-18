import { Feed, RecommendationEventContext, RecommendationSurface, SearchResponse } from '../types';

export function attachFeedAttribution(feed: Feed, surface: Exclude<RecommendationSurface, 'search'>): Feed {
  return {
    ...feed,
    // Position is the original server-ranked order, before client filtering or
    // pagination de-duplication changes the visual order.
    items: feed.items.map((item, position) => ({
      ...item,
      recommendation_context: {
        request_id: feed.request_id,
        position,
        surface,
      },
    })),
  };
}

export function attachSearchAttribution(response: SearchResponse): SearchResponse {
  return {
    ...response,
    items: response.items.map((item, position) => ({
      ...item,
      event_context: searchAttribution(position, response.request_id),
    })),
  };
}

export function searchAttribution(position: number, requestId?: string): RecommendationEventContext {
  return { request_id: requestId || undefined, position, surface: 'search' };
}
