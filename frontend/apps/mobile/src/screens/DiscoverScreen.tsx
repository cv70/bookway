import { Search, X } from 'lucide-react-native';
import { useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import { ApiRequestError, getSuggestions, search as searchApi } from '../api/client';
import { eventReporter } from '../analytics/eventReporter';
import { FeedCard } from '../components/FeedCard';
import { ScreenHeader } from '../components/ScreenHeader';
import { colors, domainMeta } from '../theme';
import { Feed, FeedItem, GrowthDomain, PublicAuthor, SearchResponse, SearchResult } from '../types';
import { attachSearchAttribution, searchAttribution } from '../utils/feedAttribution';

type Filter = 'all' | GrowthDomain;
type FeedMode = 'recommend' | 'following';
type FeedSurface = 'home' | 'following';

const filters: Array<{ key: Filter; label: string }> = [
  { key: 'all', label: '为你推荐' },
  ...Object.entries(domainMeta).map(([key, value]) => ({ key: key as GrowthDomain, label: value.label })),
];

export function DiscoverScreen({
  feed,
  followingFeed,
  feedLoadingMore = false,
  followingFeedLoadingMore = false,
  likedPostIds,
  bookmarkedPostIds,
  joinedRouteIds,
  joiningRouteIds,
  routeParticipantCounts,
  offline = false,
  onLike,
  onBookmark,
  onHide,
  onJoin,
  onLoadMoreFeed,
  onOpen,
  onOpenAuthor,
}: {
  feed: Feed;
  followingFeed: Feed;
  feedLoadingMore?: boolean;
  followingFeedLoadingMore?: boolean;
  likedPostIds?: Set<string>;
  bookmarkedPostIds?: Set<string>;
  joinedRouteIds?: Set<string>;
  joiningRouteIds?: Set<string>;
  routeParticipantCounts?: Record<string, number>;
  offline?: boolean;
  onLike?: (postId: string, context?: FeedItem['recommendation_context']) => void;
  onBookmark?: (postId: string, context?: FeedItem['recommendation_context']) => void;
  onHide?: (postId: string, context?: FeedItem['recommendation_context']) => void;
  onJoin?: (post: Feed['items'][number]['post'], context?: FeedItem['recommendation_context']) => void;
  onLoadMoreFeed?: (surface: FeedSurface) => void;
  onOpen?: (item: Feed['items'][number]) => void;
  onOpenAuthor?: (author: PublicAuthor) => void;
}) {
  const [filter, setFilter] = useState<Filter>('all');
  const [mode, setMode] = useState<FeedMode>('recommend');
  const [query, setQuery] = useState('');
  const [searchResponse, setSearchResponse] = useState<SearchResponse | null>(null);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [recentQueries, setRecentQueries] = useState<string[]>([]);
  const [searching, setSearching] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const searchRequestId = useRef(0);
  const suggestionRequestId = useRef(0);
  const loadingMoreRef = useRef(false);
  const activeFeed = mode === 'following' ? followingFeed : feed;
  const visibleFeed = activeFeed.items;
  const feedLoading = mode === 'following' ? followingFeedLoadingMore : feedLoadingMore;
  const items = filter === 'all' ? visibleFeed : visibleFeed.filter((item) => item.post.domain === filter);
  const searchResults = searchResponse?.items ?? null;
  const visibleSearchResults = (searchResults?.map((result, position) => ({ result, attribution: result.event_context ?? searchAttribution(position, searchResponse?.request_id) })) ?? null)
    ?.filter(({ result }) => filter === 'all' || (result.post?.domain ?? result.domain) === filter);

  const openSearchResult = (result: SearchResult) => {
    if (result.result_type === 'user') {
      const authorId = result.author_id ?? result.id;
      if (authorId) onOpenAuthor?.({ id: authorId, name: result.author_name ?? result.title, avatar_url: result.cover_url });
      return;
    }
    if (result.result_type === 'topic') {
      setQuery(result.title);
      rememberQuery(result.title);
    }
  };

  useEffect(() => {
    const trimmed = query.trim();
    const requestId = ++searchRequestId.current;
    loadingMoreRef.current = false;
    setLoadingMore(false);
    if (!trimmed) {
      setSearchResponse(null);
      setSearching(false);
      return;
    }
    setSearchResponse(null);
    setSearching(true);
    const timer = setTimeout(() => {
      eventReporter.track({ event_type: 'search_submit', component_id: 'discover-search', source: 'mobile-search', content_id: undefined });
      searchApi(trimmed)
        .then((response) => {
          if (requestId !== searchRequestId.current) return;
          setRecentQueries((current) => [trimmed, ...current.filter((item) => item !== trimmed)].slice(0, 6));
          setSearchResponse(attachSearchAttribution(response));
        })
        .catch(() => {
          if (requestId !== searchRequestId.current) return;
          const lower = trimmed.toLowerCase();
          setSearchResponse({
            request_id: '',
            query: trimmed,
            items: feed.items
              .filter(({ post }) => `${post.title} ${post.summary} ${post.tags.join(' ')}`.toLowerCase().includes(lower))
              .map(({ author_id, post, score }) => ({
                id: post.id,
                result_type: 'post' as const,
                title: post.title,
                snippet: post.summary,
                cover_url: post.cover_url,
                author_id,
                author_name: post.author_name,
                domain: post.domain,
                score,
                highlights: [post.title],
                post,
              })),
            total_estimate: feed.items.length,
            took_ms: 0,
            degraded: true,
          });
        })
        .finally(() => {
          if (requestId === searchRequestId.current) setSearching(false);
        });
    }, 260);
    return () => clearTimeout(timer);
  }, [feed.items, query]);

  useEffect(() => {
    const trimmed = query.trim();
    const requestId = ++suggestionRequestId.current;
    if (!trimmed) {
      setSuggestions([]);
      return;
    }
    const timer = setTimeout(() => {
      getSuggestions(trimmed)
        .then((response) => {
          if (requestId === suggestionRequestId.current) setSuggestions(response.items.map((item) => item.text));
        })
        .catch(() => {
          if (requestId === suggestionRequestId.current) {
            setSuggestions(feed.items.flatMap((item) => item.post.tags).filter((tag) => tag.includes(trimmed)).slice(0, 6));
          }
        });
    }, 140);
    return () => clearTimeout(timer);
  }, [feed.items, query]);

  const rememberQuery = (value: string) => {
    const trimmed = value.trim();
    if (!trimmed) return;
    setRecentQueries((current) => [trimmed, ...current.filter((item) => item !== trimmed)].slice(0, 6));
  };

  const loadMoreSearch = () => {
    const trimmed = query.trim();
    const cursor = searchResponse?.next_cursor;
    if (!trimmed || searchResponse?.query !== trimmed || !cursor || loadingMoreRef.current) return;

    const requestId = searchRequestId.current;
    loadingMoreRef.current = true;
    setLoadingMore(true);
    searchApi(trimmed, cursor)
      .then((response) => {
        if (requestId !== searchRequestId.current) return;
        const attributedResponse = attachSearchAttribution(response);
        setSearchResponse((current) => {
          if (!current || current.query !== trimmed) return current;
          const seenIds = new Set(current.items.map((item) => item.id));
          return {
            ...attributedResponse,
            items: [...current.items, ...attributedResponse.items.filter((item) => !seenIds.has(item.id))],
            total_estimate: Math.max(current.total_estimate, attributedResponse.total_estimate),
            degraded: current.degraded || attributedResponse.degraded,
          };
        });
      })
      .catch((error: unknown) => {
        if (!(error instanceof ApiRequestError) || error.code !== 'failed_precondition') {
          // Retain the already-rendered page: an intermittent next-page failure is recoverable.
          return;
        }
        // PIT-backed cursors are deliberately short-lived. Restart from page one so a user
        // can continue searching instead of retrying an expired cursor forever.
        return searchApi(trimmed).then((response) => {
          if (requestId === searchRequestId.current) setSearchResponse(attachSearchAttribution(response));
        }).catch(() => {
          // Keep already rendered results if the refresh itself cannot be completed.
        });
      })
      .finally(() => {
        if (requestId === searchRequestId.current) {
          loadingMoreRef.current = false;
          setLoadingMore(false);
        }
      });
  };

  return (
    <ScrollView
      contentContainerStyle={styles.content}
      onScroll={({ nativeEvent }) => {
        if (nativeEvent.layoutMeasurement.height + nativeEvent.contentOffset.y < nativeEvent.contentSize.height - 180) return;
        if (query) loadMoreSearch();
        else onLoadMoreFeed?.(mode === 'following' ? 'following' : 'home');
      }}
      scrollEventThrottle={200}
      showsVerticalScrollIndicator={false}
    >
      <ScreenHeader title="发现" />
      {offline ? <Text style={styles.offline}>当前展示离线预览，连接恢复后会自动更新</Text> : null}
      <View style={styles.searchBox}>
        <Search color={colors.muted} size={18} />
        <TextInput
          accessibilityLabel="搜索万卷行"
          onChangeText={setQuery}
          placeholder="搜索行记、路线和主题"
          placeholderTextColor={colors.faint}
          returnKeyType="search"
          style={styles.searchInput}
          value={query}
        />
        {query ? (
          <Pressable accessibilityLabel="清空搜索" hitSlop={8} onPress={() => setQuery('')}>
            <X color={colors.muted} size={17} />
          </Pressable>
        ) : null}
      </View>
      {query ? (
        <View style={styles.searchState}>
          <Text style={styles.searchTitle}>搜索结果</Text>
          {searching ? <ActivityIndicator color={colors.evergreen} size="small" /> : null}
        </View>
      ) : null}
      {query && searchResults === null && suggestions.length ? <View style={styles.suggestions}>{suggestions.map((suggestion) => <Pressable key={suggestion} onPress={() => { setQuery(suggestion); rememberQuery(suggestion); }} style={styles.suggestion}><Search color={colors.faint} size={15} /><Text style={styles.suggestionText}>{suggestion}</Text></Pressable>)}</View> : null}
      {!query && recentQueries.length ? <View style={styles.recent}><View style={styles.recentHeader}><Text style={styles.recentTitle}>最近搜索</Text><Pressable accessibilityLabel="清除搜索历史" onPress={() => setRecentQueries([])}><X color={colors.faint} size={15} /></Pressable></View><View style={styles.recentItems}>{recentQueries.map((item) => <Pressable key={item} onPress={() => setQuery(item)} style={styles.recentItem}><Text style={styles.recentText}>{item}</Text></Pressable>)}</View></View> : null}
      {!query ? (
        <View style={styles.modeTabs}>
          {([['recommend', '推荐'], ['following', '关注']] as const).map(([key, label]) => {
            const selected = mode === key;
            return <Pressable accessibilityRole="tab" accessibilityState={{ selected }} key={key} onPress={() => setMode(key)} style={[styles.modeTab, selected && styles.modeTabSelected]}><Text style={[styles.modeText, selected && styles.modeTextSelected]}>{label}</Text></Pressable>;
          })}
        </View>
      ) : null}
      <ScrollView
        contentContainerStyle={styles.filters}
        horizontal
        showsHorizontalScrollIndicator={false}
      >
        {filters.map((item) => {
          const selected = filter === item.key;
          return (
            <Pressable
              accessibilityRole="tab"
              accessibilityState={{ selected }}
              key={item.key}
              onPress={() => setFilter(item.key)}
              style={[styles.filter, selected && styles.filterSelected]}
            >
              <Text style={[styles.filterText, selected && styles.filterTextSelected]}>{item.label}</Text>
            </Pressable>
          );
        })}
      </ScrollView>
      <View>
        {query
          ? visibleSearchResults?.map(({ result, attribution }) =>
              result.post ? (
                <FeedCard
                  item={{ author_id: result.author_id ?? '', post: result.post, score: result.score, source: 'search', reasons: result.highlights, recommendation_context: attribution }}
                  bookmarked={bookmarkedPostIds?.has(result.id)}
                  key={result.id}
                  joined={joinedRouteIds?.has(result.id)}
                  joining={joiningRouteIds?.has(result.id)}
                  joinCount={routeParticipantCounts?.[result.id]}
                  liked={likedPostIds?.has(result.id)}
                  onBookmark={onBookmark}
                  onHide={onHide}
                  onJoin={onJoin}
                  onLike={onLike}
                  onOpen={onOpen}
                />
              ) : (
                <Pressable accessibilityLabel={result.result_type === 'user' ? `查看创作者${result.title}` : `搜索话题${result.title}`} key={result.id} onPress={() => openSearchResult(result)} style={({ pressed }) => [styles.resultRow, pressed && styles.pressed]}>
                  <Text style={styles.resultTitle}>{result.title}</Text>
                  <Text style={styles.resultSnippet}>{result.snippet}</Text>
                </Pressable>
              ),
            )
          : items.map((item) => (
              <FeedCard
                item={item}
                bookmarked={bookmarkedPostIds?.has(item.post.id)}
                key={item.post.id}
                joined={joinedRouteIds?.has(item.post.id)}
                joining={joiningRouteIds?.has(item.post.id)}
                joinCount={routeParticipantCounts?.[item.post.id]}
                liked={likedPostIds?.has(item.post.id)}
                onBookmark={onBookmark}
                onHide={onHide}
                onJoin={onJoin}
                onLike={onLike}
                onOpen={onOpen}
              />
            ))}
      </View>
      {query && searchResponse?.degraded ? <Text style={styles.degraded}>搜索服务暂不可用，已在当前已加载内容中查找</Text> : null}
      {query && searchResponse?.next_cursor ? (
        <Pressable accessibilityRole="button" disabled={loadingMore} onPress={loadMoreSearch} style={[styles.loadMore, loadingMore && styles.loadMoreDisabled]}>
          {loadingMore ? <ActivityIndicator color={colors.evergreen} size="small" /> : <Text style={styles.loadMoreText}>加载更多结果</Text>}
        </Pressable>
      ) : null}
      {!query && activeFeed.meta.next_cursor ? (
        <Pressable accessibilityRole="button" disabled={feedLoading} onPress={() => onLoadMoreFeed?.(mode === 'following' ? 'following' : 'home')} style={[styles.loadMore, feedLoading && styles.loadMoreDisabled]}>
          {feedLoading ? <ActivityIndicator color={colors.evergreen} size="small" /> : <Text style={styles.loadMoreText}>加载更多{mode === 'following' ? '关注内容' : '推荐内容'}</Text>}
        </Pressable>
      ) : null}
      {((query && visibleSearchResults?.length === 0) || (!query && items.length === 0)) ? (
        <View style={styles.empty}><Text style={styles.emptyText}>{mode === 'following' && !query ? '关注创作者后，他们的行记会出现在这里' : '这一页还在生长'}</Text></View>
      ) : null}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  searchBox: { height: 44, marginHorizontal: 16, marginBottom: 14, paddingHorizontal: 12, flexDirection: 'row', alignItems: 'center', gap: 8, borderWidth: 1, borderColor: colors.line, borderRadius: 7, backgroundColor: colors.surface },
  offline: { marginHorizontal: 16, marginBottom: 12, paddingHorizontal: 12, paddingVertical: 9, color: colors.muted, backgroundColor: colors.goldSoft, borderRadius: 6, fontSize: 12, lineHeight: 17 },
  searchInput: { flex: 1, minWidth: 0, color: colors.ink, fontSize: 14, paddingVertical: 0, letterSpacing: 0 },
  searchState: { height: 42, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  searchTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  modeTabs: { height: 43, marginHorizontal: 16, marginBottom: 10, flexDirection: 'row', borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  modeTab: { flex: 1, alignItems: 'center', justifyContent: 'center', borderBottomWidth: 2, borderBottomColor: 'transparent' },
  modeTabSelected: { borderBottomColor: colors.evergreen },
  modeText: { color: colors.faint, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  modeTextSelected: { color: colors.evergreen },
  suggestions: { marginHorizontal: 16, marginBottom: 10, borderRadius: 6, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  suggestion: { minHeight: 40, paddingHorizontal: 12, flexDirection: 'row', alignItems: 'center', gap: 8, borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  suggestionText: { color: colors.ink, fontSize: 13, letterSpacing: 0 },
  recent: { marginHorizontal: 16, marginBottom: 12 },
  recentHeader: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  recentTitle: { color: colors.muted, fontSize: 11, fontWeight: '700', letterSpacing: 0 },
  recentItems: { flexDirection: 'row', flexWrap: 'wrap', gap: 7, marginTop: 8 },
  recentItem: { minHeight: 30, paddingHorizontal: 10, alignItems: 'center', justifyContent: 'center', borderRadius: 5, backgroundColor: colors.surface, borderWidth: 1, borderColor: colors.line },
  recentText: { color: colors.muted, fontSize: 11, letterSpacing: 0 },
  filters: { paddingHorizontal: 16, gap: 7, paddingBottom: 14 },
  filter: { height: 34, justifyContent: 'center', paddingHorizontal: 13, borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  filterSelected: { backgroundColor: colors.ink, borderColor: colors.ink },
  filterText: { color: colors.muted, fontSize: 12, fontWeight: '600', letterSpacing: 0 },
  filterTextSelected: { color: colors.surface },
  empty: { height: 200, alignItems: 'center', justifyContent: 'center' },
  emptyText: { color: colors.faint, fontSize: 14, letterSpacing: 0 },
  resultRow: { paddingHorizontal: 20, paddingVertical: 15, backgroundColor: colors.surface, borderBottomColor: colors.line, borderBottomWidth: StyleSheet.hairlineWidth },
  resultTitle: { color: colors.ink, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  resultSnippet: { color: colors.muted, fontSize: 13, lineHeight: 19, marginTop: 4, letterSpacing: 0 },
  degraded: { marginHorizontal: 20, marginTop: 12, color: colors.muted, fontSize: 12, lineHeight: 18, letterSpacing: 0 },
  loadMore: { minHeight: 42, marginHorizontal: 16, marginTop: 14, alignItems: 'center', justifyContent: 'center', borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.surface },
  loadMoreDisabled: { opacity: 0.65 },
  loadMoreText: { color: colors.evergreen, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  pressed: { opacity: 0.62 },
});
